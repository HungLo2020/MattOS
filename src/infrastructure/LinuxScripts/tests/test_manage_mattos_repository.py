import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch


import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "GenericScripts"))
import ManageMattOSRepository as repo


class PackageMetadataTests(unittest.TestCase):
    def fake_package(self, name="demo", version="1.0~rc1-1", architecture="amd64"):
        path = Path(self.temp.name) / "directory with spaces" / "demo.deb"
        path.parent.mkdir()
        path.write_bytes(b"not a real deb; subprocess is mocked")
        result = SimpleNamespace(stdout=f"{name}\n{version}\n{architecture}\n", returncode=0, stderr="")
        return path, result

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()

    def tearDown(self):
        self.temp.cleanup()

    @patch.object(repo, "run_command")
    def test_normal_amd64_package(self, run):
        path, result = self.fake_package()
        run.return_value = result
        info = repo.package_info(path, ("amd64",))
        self.assertEqual((info.name, info.version, info.architecture), ("demo", "1.0~rc1-1", "amd64"))
        self.assertIn(str(path), run.call_args.args[0])
        self.assertIn("--show", run.call_args.args[0])

    @patch.object(repo, "run_command")
    def test_architecture_all_is_accepted(self, run):
        path, result = self.fake_package(architecture="all")
        run.return_value = result
        self.assertEqual(repo.package_info(path, ("amd64",)).architecture, "all")

    @patch.object(repo, "run_command")
    def test_missing_field_is_rejected(self, run):
        path, _ = self.fake_package()
        run.return_value = SimpleNamespace(stdout="demo\n\namd64\n", returncode=0, stderr="")
        with self.assertRaises(repo.PackageError):
            repo.package_info(path, ("amd64",))

    @patch.object(repo, "run_command")
    def test_labeled_output_is_rejected(self, run):
        path, _ = self.fake_package()
        run.return_value = SimpleNamespace(
            stdout="Package: demo\nVersion: 1.0\nArchitecture: amd64\n", returncode=0, stderr=""
        )
        with self.assertRaises(repo.PackageError):
            repo.package_info(path, ("amd64",))

    @patch.object(repo, "run_command")
    def test_malformed_package_is_rejected(self, run):
        path, _ = self.fake_package()
        run.side_effect = repo.PackageError("dpkg-deb failed")
        with self.assertRaises(repo.PackageError):
            repo.package_info(path, ("amd64",))

    @patch.object(repo, "run_command")
    def test_incompatible_architecture_is_rejected(self, run):
        path, result = self.fake_package(architecture="arm64")
        run.return_value = result
        with self.assertRaises(repo.PackageError):
            repo.package_info(path, ("amd64",))


class ConfigurationTests(unittest.TestCase):
    def test_all_is_not_a_reprepro_architecture(self):
        with self.assertRaises(repo.ConfigurationError):
            repo.validate_architectures("amd64,all")

    def test_architectures_are_normalized(self):
        self.assertEqual(repo.validate_architectures("amd64,amd64,arm64"), ("amd64", "arm64"))

    def test_remote_path_traversal_is_rejected(self):
        with self.assertRaises(repo.RemoteError):
            repo.safe_key("pool/../../etc/passwd")

    def test_release_hash_parser_uses_release_relative_paths(self):
        text = "SHA256:\n abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789 12 main/binary-amd64/Packages.gz\n"
        self.assertIn("main/binary-amd64/Packages.gz", repo.parse_sha256_release(text))


class BitwardenTests(unittest.TestCase):
    @patch.object(repo, "command_exists", return_value=True)
    @patch.object(repo, "bitwarden_status_payload", return_value=("unlocked", ""))
    def test_unlocked_session_is_reused(self, status, exists):
        client = repo.Bitwarden(non_interactive=True, yes=True)
        client.ensure_session()
        self.assertTrue(client.ready)

    @patch.object(repo, "command_exists", return_value=False)
    def test_missing_bitwarden_cli_is_distinct(self, exists):
        with self.assertRaises(repo.AuthenticationError) as error:
            repo.Bitwarden(non_interactive=True, yes=True).ensure_session()
        self.assertIn("not installed", str(error.exception))

    @patch.object(repo.Bitwarden, "ensure_session")
    @patch.object(repo, "run_command")
    def test_missing_item_is_distinct_from_inaccessible_item(self, run, ensure):
        run.return_value = SimpleNamespace(stdout="[]", stderr="", returncode=0)
        client = repo.Bitwarden(non_interactive=True, yes=True)
        self.assertIsNone(client.item("Missing", required=False))
        run.return_value = SimpleNamespace(stdout="", stderr="server unavailable", returncode=1)
        with self.assertRaises(repo.AuthenticationError):
            client.item("Unavailable", required=False)


class DependencyBootstrapTests(unittest.TestCase):
    @patch.object(repo, "tool_home")
    @patch.object(repo.os, "execve")
    @patch.object(repo.subprocess, "run")
    def test_bootstrap_reexecs_tool_owned_environment(self, run, execve, tool_home):
        with tempfile.TemporaryDirectory() as temporary:
            home = Path(temporary)
            tool_home.return_value = home
            run.side_effect = [
                SimpleNamespace(returncode=0, stdout="", stderr=""),
                SimpleNamespace(returncode=0, stdout="", stderr=""),
            ]
            repo.bootstrap_python(["tool.py", "doctor"])
            execve.assert_called_once()

    @patch.object(repo, "tool_home")
    @patch.object(repo.subprocess, "run")
    def test_missing_venv_support_is_actionable(self, run, tool_home):
        with tempfile.TemporaryDirectory() as temporary:
            tool_home.return_value = Path(temporary)
            run.return_value = SimpleNamespace(returncode=1, stdout="", stderr="ensurepip unavailable")
            with self.assertRaises(repo.DependencyError) as error:
                repo.bootstrap_python(["tool.py", "upload"])
            self.assertIn("python3-venv", str(error.exception))


class PublicationTests(unittest.TestCase):
    def test_reprepro_config_does_not_emit_all(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            config = repo.Config(architectures=("amd64",))
            repo.write_reprepro_config(root, config, "A" * 40)
            text = (root / "conf" / "distributions").read_text()
            self.assertIn("Architectures: amd64", text)
            self.assertNotIn("Architectures: amd64 all", text)

    @patch.object(repo.time, "sleep")
    @patch.object(repo.urllib.request, "urlopen")
    def test_public_fetch_retries_transient_failure(self, urlopen, sleep):
        class Response:
            def __enter__(self):
                return self

            def __exit__(self, *args):
                return None

            def read(self):
                return b"published"

        response = Response()
        urlopen.side_effect = [repo.urllib.error.URLError("not propagated"), response]
        self.assertEqual(repo.fetch_public_bytes("https://example.test/InRelease", "unreachable"), b"published")
        sleep.assert_called_once_with(1)
        request = urlopen.call_args_list[0].args[0]
        self.assertEqual(request.get_header("User-agent"), "MattOSRepositoryManager/1.0")


if __name__ == "__main__":
    unittest.main()
