"""Repository enrollment tests use temporary files and mocked privilege commands."""

from pathlib import Path
import subprocess
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))
import mattpackages as repository
from system import LinuxDistro, PackageManager, detect_package_manager


class MattPackagesTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.source = self.root / "sources.list.d/mattpackages.sources"
        self.key = self.root / "keyrings/mattpackages.asc"
        self.source.parent.mkdir()
        for name, value in (("APT_DIRECTORY", self.root), ("SOURCE_PATH", self.source), ("KEY_PATH", self.key)):
            patcher = patch.object(repository, name, value)
            patcher.start()
            self.addCleanup(patcher.stop)

    def test_debian_and_ubuntu_derivatives_detect_apt(self):
        for identifier, parents in (("debian", ()), ("ubuntu", ("debian",)), ("kubuntu", ("ubuntu", "debian"))):
            with self.subTest(identifier=identifier):
                distro = LinuxDistro(identifier, identifier, None, parents)
                self.assertEqual(detect_package_manager(distro, lambda command: "/bin/apt-get" if command == "apt-get" else None), PackageManager.APT)

    def test_non_linux_and_non_apt_never_query_dpkg(self):
        for system, manager in (("windows", PackageManager.APT), ("linux", PackageManager.DNF)):
            with patch.object(repository, "detect_host", return_value=SimpleNamespace(system=system)), patch.object(
                repository, "detect_package_manager", return_value=manager
            ), patch.object(repository.subprocess, "run") as run:
                self.assertIsNotNone(repository.eligibility_problem())
                run.assert_not_called()

    def test_architecture_guard(self):
        for architecture in ("amd64", "arm64", "i386"):
            with patch.object(repository, "detect_host", return_value=SimpleNamespace(system="linux")), patch.object(
                repository, "detect_package_manager", return_value=PackageManager.APT
            ), patch.object(repository.subprocess, "run", return_value=SimpleNamespace(stdout=architecture + "\n")):
                self.assertEqual(repository.eligibility_problem() is None, architecture == "amd64")

    def test_existing_foreign_sources_abort_before_writes(self):
        for filename, contents in (
            ("sources.list", "deb https://mattpackages.mattsherfey.com stable main\n"),
            ("sources.list.d/custom.sources", "URIs: https://mattpackages.mattsherfey.com\nEnabled: no\n"),
            ("sources.list.d/mattpackages.sources", "Types: deb\nURIs: https://example.com\n"),
        ):
            with self.subTest(filename=filename):
                path = self.root / filename
                path.write_text(contents)
                with patch.object(repository, "eligibility_problem", return_value=None), patch.object(repository, "install_file") as install:
                    with self.assertRaises(RuntimeError):
                        repository.configure_repository()
                    install.assert_not_called()
                self.assertEqual(path.read_text(), contents)
                path.unlink()

    def test_commented_sources_are_ignored(self):
        (self.root / "sources.list").write_text("# deb https://mattpackages.mattsherfey.com stable main\n")
        repository.check_existing_sources()

    def test_symlinks_are_rejected(self):
        self.source.symlink_to(self.root / "missing")
        with self.assertRaisesRegex(RuntimeError, "symbolic link"):
            repository.check_existing_sources()

    def test_correct_file_is_not_rewritten(self):
        self.source.write_bytes(b"correct")
        self.source.chmod(0o644)
        status = SimpleNamespace(st_mode=0o100644, st_uid=0, st_gid=0)
        with patch.object(Path, "stat", return_value=status), patch.object(repository.subprocess, "run") as run:
            self.assertFalse(repository.install_file(self.source, b"correct"))
            run.assert_not_called()

    def test_failed_staging_preserves_existing_source(self):
        self.source.write_bytes(b"previous source")
        def run(command, **kwargs):
            if "0644" in command:
                raise subprocess.CalledProcessError(1, command)
        with patch.object(repository.subprocess, "run", side_effect=run) as commands:
            with self.assertRaises(subprocess.CalledProcessError):
                repository.install_file(self.source, b"replacement")
        self.assertEqual(self.source.read_bytes(), b"previous source")
        self.assertFalse(any("mv" in call.args[0] for call in commands.call_args_list))
        self.assertIn("rm", commands.call_args.args[0])

    def test_replacement_is_staged_before_rename(self):
        self.source.write_bytes(b"previous source")
        def run(command, **kwargs):
            if "0644" in command:
                self.assertEqual(self.source.read_bytes(), b"previous source")
                Path(command[-1]).write_bytes(Path(command[-2]).read_bytes())
            elif "mv" in command:
                Path(command[-2]).replace(Path(command[-1]))
        with patch.object(repository.subprocess, "run", side_effect=run):
            self.assertTrue(repository.install_file(self.source, b"replacement"))
        self.assertEqual(self.source.read_bytes(), b"replacement")
        self.assertEqual(list(self.source.parent.iterdir()), [self.source])

    def test_disabled_managed_source_and_missing_key_are_repaired(self):
        self.source.write_bytes(repository.source_contents().replace(b"Enabled: yes", b"Enabled: no"))
        def install(destination, contents):
            destination.parent.mkdir(exist_ok=True)
            destination.write_bytes(contents)
        with patch.object(repository, "eligibility_problem", return_value=None), patch.object(
            repository, "install_file", side_effect=install
        ), patch.object(repository.subprocess, "run") as run:
            repository.configure_repository()
        self.assertIn(b"Enabled: yes", self.source.read_bytes())
        self.assertIn(b"Suites: stable", self.source.read_bytes())
        self.assertEqual(self.key.read_bytes(), repository.BUNDLED_KEY.read_bytes())
        command = run.call_args.args[0]
        self.assertIn("Dir::Etc::sourceparts=-", command)
        self.assertIn("APT::Get::List-Cleanup=0", command)
        self.assertIn("APT::Update::Error-Mode=any", command)

    def test_modified_bundled_key_aborts_before_writes(self):
        bad_key = self.root / "bad.asc"
        bad_key.write_bytes(b"invalid")
        with patch.object(repository, "eligibility_problem", return_value=None), patch.object(
            repository, "BUNDLED_KEY", bad_key
        ), patch.object(repository, "install_file") as install:
            with self.assertRaisesRegex(RuntimeError, "modified"):
                repository.configure_repository()
            install.assert_not_called()

    def test_refresh_failure_is_not_reported_as_success(self):
        with patch.object(repository, "eligibility_problem", return_value=None), patch.object(
            repository, "install_file"
        ), patch.object(repository.subprocess, "run", side_effect=subprocess.CalledProcessError(100, "apt-get")):
            with self.assertRaisesRegex(RuntimeError, "could not refresh"):
                repository.configure_repository()


if __name__ == "__main__":
    unittest.main()
