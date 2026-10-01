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
from unittest.mock import MagicMock, Mock, patch
from urllib.error import HTTPError
from urllib.request import Request, urlopen

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))
sys.path.insert(0, str(ROOT / "GenericScripts"))
from server import mattos_repository as backend
from server.mattos_repository import RepositoryManager, ServerConfig
from server.cloudflare_ingress import CloudflareIngress, CloudflareIngressError
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
                self.assertEqual(config.server_url, "http://hunglosvr.tail30f889.ts.net:8790")
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

    def test_shared_service_starts_both_public_archives(self):
        with tempfile.TemporaryDirectory() as directory:
            configs = configurations(Path(directory))
            api = MagicMock()
            public = {name: MagicMock() for name in backend.REPOSITORIES}
            api.__enter__.return_value = api
            for server in public.values():
                server.__enter__.return_value = server
            with patch.object(backend, "create_server", return_value=api), \
                 patch.object(backend, "create_public_server", side_effect=lambda _configs, name, _bind, _port: public[name]) as create_public:
                backend.serve(configs, "127.0.0.1", 0)
            self.assertEqual(create_public.call_count, 2)
            for name in backend.REPOSITORIES:
                create_public.assert_any_call(configs, name, "127.0.0.1", backend.PUBLIC_PORTS[name])
                public[name].shutdown.assert_called_once()

    def test_setup_can_persist_local_only_publication(self):
        with tempfile.TemporaryDirectory() as directory:
            configs = configurations(Path(directory))
            configs["mattpackages"] = replace(configs["mattpackages"], r2_enabled=True)
            with patch.object(backend, "load_configs", return_value=configs), patch.object(backend, "setup_with_publication") as setup, \
                 contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(backend.main(["--repo", "mattpackages", "setup", "--publication", "local"]), 0)
            selected = setup.call_args.args[0]
            self.assertFalse(selected.r2_enabled)
            self.assertFalse(setup.call_args.args[1]["mattpackages"].r2_enabled)
            self.assertFalse(configs["mattos"].r2_enabled)

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
                 patch.object(backend, "provision_client_token"), patch.object(backend, "remove_legacy_public_services") as cleanup, \
                 patch.object(backend, "install_service") as service:
                path = Path(directory) / "server.json"
                backend.setup_server(configs["mattpackages"], configs, path)
                manager.assert_called_once_with(configs["mattpackages"])
                save.assert_called_once_with(configs, path)
                cleanup.assert_called_once_with()
                self.assertEqual(service.call_args.args[0], path)

    def test_local_setup_checks_cloudflare_before_changing_the_server(self):
        with tempfile.TemporaryDirectory() as directory:
            configs = configurations(Path(directory))
            ingress = Mock()
            ingress.ensure_tunnel.side_effect = backend.CloudflareIngressError("Tunnel Edit permission missing")
            with patch.object(backend.CloudflareIngress, "from_vault", return_value=ingress), \
                 patch.object(backend, "setup_server") as setup:
                with self.assertRaisesRegex(backend.CloudflareIngressError, "Tunnel Edit"):
                    backend.setup_with_publication(configs["mattpackages"], configs, Path(directory) / "server.json")
            setup.assert_not_called()


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
        cls.public_servers = {name: backend.create_public_server(cls.configs, name, "127.0.0.1", 0) for name in client.REPOSITORIES}
        cls.public_threads = [threading.Thread(target=server.serve_forever, daemon=True) for server in cls.public_servers.values()]
        for thread in cls.public_threads:
            thread.start()

    @classmethod
    def tearDownClass(cls):
        cls.server.shutdown()
        cls.server.server_close()
        cls.thread.join()
        for server in cls.public_servers.values():
            server.shutdown()
            server.server_close()
        for thread in cls.public_threads:
            thread.join()
        cls.temporary.cleanup()

    def remote(self, name):
        return client.ServerRepository(client.Config(repository=name, server_url=self.url, token_file=self.root / "token"))

    def make_package(self, name, architecture="amd64", description="Repository isolation test"):
        root = self.root / f"input-{name}-{architecture}"
        (root / "DEBIAN").mkdir(parents=True, exist_ok=True)
        (root / "DEBIAN/control").write_text(
            f"Package: {name}\nVersion: 1.0\nPriority: optional\nArchitecture: {architecture}\nMaintainer: Test <test@example.invalid>\nDescription: {description}\n")
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

    def test_03_same_version_upload_replaces_payload_unless_disabled(self):
        remote = self.remote("mattpackages")
        neighbor = self.make_package("overwrite-neighbor")
        neighbor_payload = neighbor.read_bytes()
        remote.upload(neighbor)
        package = self.make_package("overwrite-example", description="first payload")
        first_payload = package.read_bytes()
        remote.upload(package)
        previous = RepositoryManager(self.configs["mattpackages"]).current.resolve()
        old_pool = next((previous / "pool").rglob("overwrite-example_1.0_amd64.deb"))
        self.assertEqual(old_pool.read_bytes(), first_payload)

        package = self.make_package("overwrite-example", description="second payload")
        second_payload = package.read_bytes()
        self.assertNotEqual(first_payload, second_payload)
        config = client.Config(repository="mattpackages", server_url=self.url, token_file=self.root / "token")
        with patch.object(client.Config, "from_env", return_value=config), contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
            self.assertEqual(client.main(["manager", "--repo", "mattpackages", "upload", "--no-overwrites", str(package)]), 40)
        with patch.object(backend, "load_configs", return_value=self.configs), contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
            self.assertEqual(backend.main(["--repo", "mattpackages", "upload", "--no-overwrites", str(package)]), 1)
        self.assertEqual(RepositoryManager(self.configs["mattpackages"]).current.resolve(), previous)
        self.assertEqual(old_pool.read_bytes(), first_payload)

        with patch.object(client.Config, "from_env", return_value=config), contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
            self.assertEqual(client.main(["manager", "--repo", "mattpackages", "upload", str(package)]), 0)
        current = RepositoryManager(self.configs["mattpackages"]).current.resolve()
        new_pool = next((current / "pool").rglob("overwrite-example_1.0_amd64.deb"))
        self.assertNotEqual(current, previous)
        self.assertEqual(new_pool.read_bytes(), second_payload)
        self.assertEqual(next((current / "pool").rglob("overwrite-neighbor_1.0_amd64.deb")).read_bytes(), neighbor_payload)
        self.assertEqual(old_pool.read_bytes(), first_payload)
        self.assertEqual([item for item in remote.request("GET", "/packages")["packages"] if item["name"] == "overwrite-example"],
                         [{"name": "overwrite-example", "version": "1.0", "architecture": "amd64"}])
        self.assertTrue(remote.request("GET", "/verify")["verified"])
        remote.remove("overwrite-example", "1.0")
        remote.remove("overwrite-neighbor", "1.0")

    def test_04_public_routes_and_private_file_protection(self):
        for path in ("/repository/dists/trixie/InRelease", "/repositories/mattos/dists/trixie/InRelease", "/repositories/mattpackages/dists/stable/InRelease"):
            with urlopen(self.url + path) as response:
                self.assertIn(b"BEGIN PGP SIGNED MESSAGE", response.read())
        for path in ("/repositories/other/dists/stable/InRelease", "/repositories/mattpackages/private-key.asc", "/repository/dists/../conf/distributions"):
            with self.assertRaises(HTTPError) as error:
                urlopen(self.url + path)
            self.assertEqual(error.exception.code, 404)
            error.exception.close()

    def test_04_read_only_public_server_uses_apt_root_paths(self):
        for name, suite in (("mattos", "trixie"), ("mattpackages", "stable")):
            url = f"http://127.0.0.1:{self.public_servers[name].server_port}"
            with urlopen(url + f"/dists/{suite}/InRelease") as response:
                self.assertIn(b"BEGIN PGP SIGNED MESSAGE", response.read())
                self.assertEqual(response.headers["Cache-Control"], "no-store")
                self.assertEqual(response.headers["Cloudflare-CDN-Cache-Control"], "no-store")
                self.assertEqual(response.headers["X-MattOS-Repository-Origin"], "home-server")
            # APT percent-encodes "+" and "~" in package file names.
            with urlopen(url + f"/dists/{suite}/In%52elease") as response:
                self.assertIn(b"BEGIN PGP SIGNED MESSAGE", response.read())
            with self.assertRaises(HTTPError) as error:
                urlopen(url + f"/dists/{suite}/%2e%2e/%2e%2e/conf/distributions")
            self.assertEqual(error.exception.code, 404)
            error.exception.close()
            with urlopen(Request(url + f"/dists/{suite}/InRelease", method="HEAD")) as response:
                self.assertEqual(response.status, 200)
                self.assertEqual(response.read(), b"")
            with self.assertRaises(HTTPError) as error:
                urlopen(url + f"/v2/repos/{name}/private-key")
            self.assertEqual(error.exception.code, 404)
            error.exception.close()
            with self.assertRaises(HTTPError) as error:
                urlopen(Request(url + f"/v2/repos/{name}/upload", data=b"payload", method="POST"))
            self.assertEqual(error.exception.code, 405)
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
                (config.root / "pool").mkdir()
                (config.root / "pool/test.deb").write_bytes(b"package payload")
                with patch.dict(sys.modules, {"boto3": boto}):
                    publisher = R2Publisher(config, Mock())
                publisher.call = Mock()
                publisher.publish(config.root, {"pool/stale.deb"})
                calls = publisher.call.call_args_list
                self.assertTrue(any(call.args[0] == "delete_object" for call in calls))
                self.assertTrue(any(call.args[0] == "put_object" for call in calls))
                self.assertTrue(all(call.kwargs["Bucket"] == config.bucket for call in calls))
                pool_upload = next(call for call in calls if call.args[0] == "put_object" and call.kwargs["Key"].endswith(".deb"))
                self.assertEqual(pool_upload.kwargs["CacheControl"], "no-cache, max-age=0, must-revalidate")


class CloudflareIngressTests(unittest.TestCase):
    def test_tunnel_permission_error_names_the_vault_item(self):
        ingress = CloudflareIngress("test-token", Mock())
        ingress.account_id = "account"
        ingress.request = Mock(side_effect=[[], CloudflareIngressError("HTTP 403")])
        with self.assertRaisesRegex(CloudflareIngressError, "MattPackages Cloudflare Setup.*Tunnel Edit"):
            ingress.ensure_tunnel({"mattos": "packages.mattsherfey.com"})

    def test_cache_rule_permission_falls_back_to_origin_headers(self):
        ingress = CloudflareIngress("test-token", Mock())
        ingress.account_id = "account"
        ingress.zone_id = "zone"
        ingress.request = Mock(side_effect=CloudflareIngressError("HTTP 403"))
        self.assertFalse(ingress.ensure_cache_rule("packages.mattsherfey.com", "mattos"))

    def test_domain_switch_detaches_only_selected_r2_domain(self):
        ingress = CloudflareIngress("test-token", Mock())
        ingress.account_id = "account"
        ingress.zone_id = "zone"
        ingress.request = Mock(side_effect=[
            {"domains": [{"domain": "mattpackages.mattsherfey.com"}]},
            [{"id": "record", "type": "CNAME", "content": "public.r2.dev", "proxied": True}],
            {"domain": "mattpackages.mattsherfey.com"},
            [{"id": "record", "type": "CNAME", "content": "public.r2.dev", "proxied": True}],
            {"id": "record"},
        ])
        ingress.switch_domain("mattpackages.mattsherfey.com", "mattpackages-apt-repo", "tunnel")
        calls = ingress.request.call_args_list
        self.assertEqual([call.args[0] for call in calls], ["GET", "GET", "DELETE", "GET", "PATCH"])
        self.assertTrue(all("mattpackages" in call.args[1] or "dns_records" in call.args[1] for call in calls))
        self.assertEqual(calls[-1].args[2]["content"], "tunnel.cfargotunnel.com")


if __name__ == "__main__":
    unittest.main()
