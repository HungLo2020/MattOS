"""Repository selection, real signed archives, HTTP routing, and R2 isolation."""
from __future__ import annotations

import contextlib
import io
import json
import os
from dataclasses import asdict, replace
from pathlib import Path
import subprocess
import sys
import tempfile
import threading
import unittest
from unittest.mock import Mock, patch
from urllib.error import HTTPError
from urllib.request import Request, urlopen

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))
sys.path.insert(0, str(ROOT / "GenericScripts"))
from server import mattos_repository as backend
from server.mattos_repository import RepositoryManager, ServerConfig
from server.r2_repository import R2Publisher, R2Error
import ManageMattOSRepository as client


def configurations(root: Path) -> dict[str, ServerConfig]:
    os_config = ServerConfig(root=root / "os", token_file=root / "token", r2_enabled=False)
    apps_config = replace(os_config, repository="mattpackages", root=root / "apps",
                          suite="stable", bucket="mattpackages-apt-repo",
                          public_url="https://mattpackages.mattsherfey.com",
                          private_key_file=os_config.root / "private-key.asc")
    return {"mattos": os_config, "mattpackages": apps_config}


class RepositoryTests(unittest.TestCase):
    def test_selection_required_before_configuration_or_side_effects(self):
        commands = [["doctor"], ["list"], ["status"], ["init"], ["publish"], ["verify"],
                    ["upload", "missing.deb"], ["add", "missing.deb"], ["remove", "example"],
                    ["export-key", "--output", "key.asc"], ["export-private-key", "--output", "private.asc"]]
        with patch.object(client.Config, "from_env") as config, patch.object(client, "ServerRepository") as remote:
            for command in commands:
                with self.subTest(command=command), contextlib.redirect_stderr(io.StringIO()) as errors:
                    with self.assertRaises(SystemExit) as result:
                        client.main(["manager", *command])
                    self.assertEqual(result.exception.code, 2)
                    self.assertIn("--repo mattos or --repo mattpackages", errors.getvalue())
            config.assert_not_called()
            remote.assert_not_called()
        with patch.object(backend, "load_configs") as configs:
            with contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit):
                backend.main(["init"])
            configs.assert_not_called()

    def test_key_export_dry_run_does_not_request_or_create_files(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "not-created" / "key.asc"
            for repo in client.REPOSITORIES:
                for command in ("export-key", "export-private-key"):
                    with patch.object(client.ServerRepository, "request") as request, contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
                        self.assertEqual(client.main(["manager", "--repo", repo, "--dry-run", command, "--output", str(output)]), 0)
                        request.assert_not_called()
                        self.assertFalse(output.parent.exists())

    def test_saved_configuration_round_trip_uses_only_explicit_temp_paths(self):
        with tempfile.TemporaryDirectory() as directory, patch.dict(os.environ, {}, clear=True):
            configs = configurations(Path(directory))
            path = Path(directory) / "settings/server.json"
            def local_install(command):
                # Only temporary paths are passed by save_configs; no sudo/service operations.
                subprocess.run(command, check=True, capture_output=True)
            with patch.object(backend, "privileged", side_effect=local_install):
                backend.save_configs(configs, path)
            self.assertEqual(backend.load_configs(path), configs)
            self.assertEqual(path.stat().st_mode & 0o777, 0o644)

    def test_unknown_repo_and_environment_do_not_supply_selection(self):
        with patch.dict(os.environ, {"MATTOS_REPOSITORY_REPO": "mattos", "REPO": "mattpackages"}):
            for arguments in (["list"], ["--repo", "other", "list"]):
                with contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit):
                    client.parser().parse_args(arguments)
        with self.assertRaises(client.ConfigurationError):
            client.ServerRepository(client.Config())

    def test_configuration_defaults_and_saved_configuration(self):
        with tempfile.TemporaryDirectory() as directory, patch.dict(os.environ, {}, clear=True):
            path = Path(directory) / "config.json"
            configs = backend.load_configs(path)
            self.assertEqual(configs["mattos"].root, backend.DEFAULT_ROOT)
            self.assertEqual(configs["mattos"].bucket, "matt-apt-repo")
            self.assertEqual(configs["mattpackages"].suite, "stable")
            self.assertEqual(configs["mattpackages"].private_key_file, backend.DEFAULT_ROOT / "private-key.asc")
            configs["mattos"] = replace(configs["mattos"], root=Path(directory) / "custom-os", bucket="custom-os", endpoint="https://example.invalid", architectures=("amd64", "arm64"))
            path.write_text(json.dumps({name: asdict(value) for name, value in configs.items()}, default=str))
            restored = backend.load_configs(path)
            self.assertEqual(restored["mattos"], configs["mattos"])
            self.assertEqual(restored["mattpackages"].private_key_file, configs["mattos"].root / "private-key.asc")
            for name in client.REPOSITORIES:
                config = client.Config.from_env(name)
                self.assertEqual(config.server_url, "http://hunglosvr:8790")
                self.assertEqual(config.repository, name)
            self.assertEqual(client.Config.from_env("mattpackages").suite, "stable")

    def test_overlapping_storage_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            configs = configurations(Path(directory))
            for changes in ({"root": configs["mattos"].root},
                            {"root": configs["mattos"].root / "nested"},
                            {"bucket": configs["mattos"].bucket}):
                with self.subTest(changes=changes), self.assertRaises(backend.RepositoryError):
                    backend.validate_configs(configs | {"mattpackages": replace(configs["mattpackages"], **changes)})

    def test_shared_service_persists_config_path_and_tailscale_access(self):
        with patch.dict(os.environ, {"MATTOS_REPOSITORY_BIND": "100.1.2.3"}):
            unit = backend.service_definition(Path("/etc/mattos-repository/server.json"), "matt")
        self.assertIn('--config "/etc/mattos-repository/server.json" serve', unit)
        self.assertIn("--bind 100.1.2.3", unit)
        self.assertIn("MATTOS_REPOSITORY_ALLOW_ANONYMOUS=1", unit)
        self.assertNotIn("--repo", unit)

    def test_mattpackages_never_generates_a_new_shared_key(self):
        with tempfile.TemporaryDirectory() as directory:
            config = configurations(Path(directory))["mattpackages"]
            with patch.object(backend, "run") as run:
                with self.assertRaisesRegex(backend.RepositoryError, "Existing MattOS signing key"):
                    RepositoryManager(config).init()
                run.assert_not_called()

    def test_synchronization_does_not_restore_packages_into_empty_archive(self):
        with tempfile.TemporaryDirectory() as directory:
            config = replace(configurations(Path(directory))["mattpackages"], r2_enabled=True)
            manager = RepositoryManager(config)
            active = config.root / "current"
            active.mkdir(parents=True)
            r2 = Mock()
            r2.keys.return_value = {"pool/main/e/example.deb"}
            with patch.object(manager, "_r2", return_value=r2):
                manager.synchronize_r2()
            r2.download.assert_not_called()
            r2.publish.assert_called_once_with(active, r2.keys.return_value)

    def test_setup_changes_only_selected_repository(self):
        with tempfile.TemporaryDirectory() as directory:
            configs = configurations(Path(directory))
            with patch.object(backend, "RepositoryManager") as manager, \
                 patch.object(backend, "install_dependencies"), patch.object(backend, "privileged"), \
                 patch.object(backend, "ensure_tree_permissions"), patch.object(backend, "save_configs") as save, \
                 patch.object(backend, "provision_client_token"), patch.object(backend, "install_service") as service:
                path = Path(directory) / "server.json"
                backend.setup_server(configs["mattpackages"], configs, path)
                manager.assert_called_once_with(configs["mattpackages"])
                save.assert_called_once_with(configs, path)
                self.assertEqual(service.call_args.args[0], path)


class SignedHTTPTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temporary = tempfile.TemporaryDirectory()
        cls.root = Path(cls.temporary.name)
        cls.configs = configurations(cls.root)
        # Real GPG signing and reprepro export, with test-only faster key generation.
        with patch.dict(os.environ, {"MATTOS_GPG_ALGORITHM": "rsa2048"}):
            RepositoryManager(cls.configs["mattos"]).init()
        cls.original_os_release = (cls.configs["mattos"].root / "current").resolve()
        RepositoryManager(cls.configs["mattpackages"]).init()
        cls.server = backend.create_server(cls.configs, "127.0.0.1", 0)
        cls.thread = threading.Thread(target=cls.server.serve_forever, daemon=True)
        cls.thread.start()
        cls.url = f"http://127.0.0.1:{cls.server.server_port}"

    @classmethod
    def tearDownClass(cls):
        cls.server.shutdown()
        cls.server.server_close()
        cls.thread.join()
        cls.temporary.cleanup()

    def remote(self, name):
        return client.ServerRepository(client.Config(repository=name, server_url=self.url, token_file=self.root / "token"))

    def make_package(self, name, architecture="amd64"):
        root = self.root / f"input-{name}-{architecture}"
        (root / "DEBIAN").mkdir(parents=True, exist_ok=True)
        (root / "DEBIAN/control").write_text(
            f"Package: {name}\nVersion: 1.0\nPriority: optional\nArchitecture: {architecture}\nMaintainer: Test <test@example.invalid>\nDescription: Repository isolation test\n")
        artifact = self.root / f"{name}_{architecture}.deb"
        subprocess.run(["dpkg-deb", "--build", str(root), str(artifact)], check=True, capture_output=True)
        return artifact

    def test_01_empty_initialization_preserves_mattos_and_shares_key(self):
        os_repo, apps = self.remote("mattos"), self.remote("mattpackages")
        self.assertEqual(self.original_os_release, (self.configs["mattos"].root / "current").resolve())
        self.assertEqual(apps.request("GET", "/packages")["packages"], [])
        self.assertEqual(os_repo.request("GET", "/public-key"), apps.request("GET", "/public-key"))
        for name, suite in (("mattos", "trixie"), ("mattpackages", "stable")):
            manager = RepositoryManager(self.configs[name])
            release = manager.current / "dists" / suite / "InRelease"
            self.assertTrue(release.is_file())
            public_key = self.root / f"{name}.asc"
            public_key.write_text(manager.public_key())
            subprocess.run(["gpg", "--batch", "--yes", "--dearmor", "--output", str(public_key.with_suffix(".gpg")), str(public_key)], check=True, capture_output=True)
            subprocess.run(["gpgv", "--keyring", str(public_key.with_suffix(".gpg")), str(release)], check=True, capture_output=True)
            self.assertIn(f"Origin: {self.configs[name].label}", release.read_text())

    def test_02_legacy_and_unknown_requests_rejected_before_upload(self):
        for endpoint in ("/v1/upload", "/v1/init", "/v1/remove", "/v1/publish", "/v2/repos/other/upload", "/v2/repos//upload"):
            with self.subTest(endpoint=endpoint), self.assertRaises(HTTPError) as error:
                urlopen(Request(self.url + endpoint, data=b"not a package", method="POST"))
            self.assertEqual(error.exception.code, 400)
            self.assertIn(b"--repo", error.exception.read())
            error.exception.close()
        for endpoint in ("/v1/packages", "/v1/private-key"):
            with self.assertRaises(HTTPError) as error:
                urlopen(self.url + endpoint)
            self.assertEqual(error.exception.code, 400)
            error.exception.close()
        for name in client.REPOSITORIES:
            self.assertEqual(self.remote(name).request("GET", "/packages")["packages"], [])

    def test_03_upload_list_remove_publish_are_isolated_without_package_rules(self):
        os_repo, apps = self.remote("mattos"), self.remote("mattpackages")
        os_repo.upload(self.make_package("os-example"))
        os_snapshot = (self.configs["mattos"].root / "current").resolve()
        # Deliberately accept a system-like package name: no package ownership rules.
        apps.upload(self.make_package("linux-image-example", "all"))
        self.assertEqual([p["name"] for p in apps.request("GET", "/packages")["packages"]], ["linux-image-example"])
        self.assertEqual([p["name"] for p in os_repo.request("GET", "/packages")["packages"]], ["os-example"])
        apps.remove("linux-image-example", "1.0")
        self.assertEqual(apps.request("GET", "/packages")["packages"], [])
        self.assertEqual(os_snapshot, (self.configs["mattos"].root / "current").resolve())
        for remote in (os_repo, apps):
            self.assertTrue(remote.request("GET", "/verify")["verified"])
            self.assertTrue(remote.request("POST", "/publish")["published"])
            self.assertEqual(remote.request("GET", "/status")["repository"], remote.config.repository)

    def test_04_public_routes_and_private_file_protection(self):
        for path in ("/repository/dists/trixie/InRelease", "/repositories/mattos/dists/trixie/InRelease", "/repositories/mattpackages/dists/stable/InRelease"):
            with urlopen(self.url + path) as response:
                self.assertIn(b"BEGIN PGP SIGNED MESSAGE", response.read())
        for path in ("/repositories/other/dists/stable/InRelease", "/repositories/mattpackages/private-key.asc", "/repository/dists/../conf/distributions"):
            with self.assertRaises(HTTPError) as error:
                urlopen(self.url + path)
            self.assertEqual(error.exception.code, 404)
            error.exception.close()

    def test_05_cli_operations_select_both_repositories(self):
        for repo in client.REPOSITORIES:
            config = client.Config(repository=repo, server_url=self.url, token_file=self.root / "token")
            for command in ("doctor", "init", "list", "status", "verify", "publish"):
                with self.subTest(repo=repo, command=command), patch.object(client.Config, "from_env", return_value=config), contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
                    self.assertEqual(client.main(["manager", "--repo", repo, command]), 0)
            for command in ("export-key", "export-private-key"):
                output = self.root / f"{repo}-{command}.asc"
                with patch.object(client.Config, "from_env", return_value=config), contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
                    self.assertEqual(client.main(["manager", "--repo", repo, command, "--output", str(output)]), 0)
                self.assertIn("BEGIN PGP", output.read_text())
                self.assertEqual(output.stat().st_mode & 0o777, 0o600 if command == "export-private-key" else 0o644)

    def test_06_anonymous_tailscale_mode_still_requires_repository(self):
        with patch.dict(os.environ, {"MATTOS_REPOSITORY_ALLOW_ANONYMOUS": "1"}):
            for repo in client.REPOSITORIES:
                with urlopen(self.url + f"/v2/repos/{repo}/status") as response:
                    self.assertEqual(json.load(response)["repository"], repo)
            with self.assertRaises(HTTPError) as error:
                urlopen(self.url + "/v1/status")
            self.assertEqual(error.exception.code, 400)
            error.exception.close()

    def test_07_preexisting_mattpackages_bucket_is_not_imported_or_overwritten(self):
        with tempfile.TemporaryDirectory() as directory:
            config = replace(self.configs["mattpackages"], root=Path(directory), r2_enabled=True)
            remote = Mock()
            remote.keys.return_value = {"pool/main/e/example.deb"}
            manager = RepositoryManager(config)
            with patch.object(manager, "_r2", return_value=remote):
                with self.assertRaisesRegex(backend.RepositoryError, "must start empty"):
                    manager.init()
            remote.download.assert_not_called()
            remote.publish.assert_not_called()
            self.assertFalse(manager.current.exists())
    def test_08_bootstrapping_mattos_preserves_remote_package_payload(self):
        with tempfile.TemporaryDirectory() as directory:
            config = replace(self.configs["mattos"], root=Path(directory), r2_enabled=True,
                             private_key_file=self.configs["mattpackages"].private_key_file)
            remote = Mock()
            key = "pool/main/e/existing/existing_1.0_amd64.deb"
            remote.keys.return_value = {key}
            artifact = self.make_package("existing")
            def download(key, destination):
                destination.parent.mkdir(parents=True, exist_ok=True)
                destination.write_bytes(artifact.read_bytes())
            remote.download.side_effect = download
            def publish(active, keys):
                self.assertEqual((active / key).read_bytes(), artifact.read_bytes())
            remote.publish.side_effect = publish
            manager = RepositoryManager(config)
            with patch.object(manager, "_r2", return_value=remote):
                manager.init()
            remote.publish.assert_called_once()
            manager.verify()



class R2Tests(unittest.TestCase):
    def test_cached_or_vault_destination_cannot_override_selected_bucket(self):
        with tempfile.TemporaryDirectory() as directory:
            config = configurations(Path(directory))["mattpackages"]
            config.root.mkdir()
            cached = {"access_key": "test", "secret_key": "test", "endpoint": "https://r2.invalid", "bucket": "matt-apt-repo", "public_url": "https://packages.mattsherfey.com"}
            cache = config.root / "r2-credentials.json"
            cache.write_text(json.dumps(cached))
            boto = Mock()
            with patch.dict(sys.modules, {"boto3": boto}):
                with self.assertRaisesRegex(R2Error, "destination"):
                    R2Publisher(config, Mock())
                boto.client.assert_not_called()
                cache.unlink()
                vault = Mock()
                vault.item.return_value = {"login": {"username": "test", "password": "test"}, "fields": [{"name": "R2_ENDPOINT", "value": cached["endpoint"]}, {"name": "R2_BUCKET_NAME", "value": cached["bucket"]}]}
                with self.assertRaisesRegex(R2Error, "destination"):
                    R2Publisher(config, vault)
                self.assertFalse(cache.exists())

    def test_publish_and_delete_are_scoped_to_the_selected_bucket(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            configs = configurations(root)
            boto = Mock()
            for name, config in configs.items():
                config.root.mkdir()
                (config.root / "r2-credentials.json").write_text(json.dumps({"access_key": "test", "secret_key": "test", "endpoint": "https://r2.invalid", "bucket": config.bucket, "public_url": config.public_url}))
                (config.root / "dists").mkdir()
                (config.root / "dists/Release").write_text(name)
                with patch.dict(sys.modules, {"boto3": boto}):
                    publisher = R2Publisher(config, Mock())
                publisher.call = Mock()
                publisher.publish(config.root, {"pool/stale.deb"})
                calls = publisher.call.call_args_list
                self.assertTrue(any(call.args[0] == "delete_object" for call in calls))
                self.assertTrue(any(call.args[0] == "put_object" for call in calls))
                self.assertTrue(all(call.kwargs["Bucket"] == config.bucket for call in calls))


if __name__ == "__main__":
    unittest.main()
