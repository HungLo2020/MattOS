import subprocess
import sys
import tempfile
import unittest
import urllib.request
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import run_qemu
from common import RepoError


class UpgradeTestHelperTests(unittest.TestCase):
    def test_framed_output_survives_the_echoed_command(self) -> None:
        command = run_qemu.framed_command("printf 'libc6 2.43-1mattos1\\nbash 5.3-1mattos2\\n'", "installed")
        # The guest echoes the command line before running it; only the
        # printed frame may match.
        ran = subprocess.run(["sh", "-c", command], text=True, capture_output=True, check=True).stdout
        output = f"$ {command}\r\n" + ran.replace("\n", "\r\n")
        self.assertEqual(
            run_qemu.framed_output(output, "installed").split(),
            ["libc6", "2.43-1mattos1", "bash", "5.3-1mattos2"],
        )
        with self.assertRaises(RepoError):
            run_qemu.framed_output(f"$ {command}\r\n", "installed")
        # systemd's shell integration opens each command's output with an
        # OSC 3008 context frame on the same line as the first output.
        osc = "\x1b]3008;start=d2da;type=command;cwd=/home/mattos\x1b\\"
        marked = f"$ {command}\r\n{osc}" + ran.replace("\n", "\r\n") + "\x1b]3008;end=d2da;exit=success\x07"
        self.assertEqual(run_qemu.framed_output(marked, "installed").split()[0], "libc6")
        self.assertEqual(
            run_qemu.parse_upgrade_summary(f"{osc}\x1b[1m3 upgraded, 0 newly installed, 0 to remove and 1 not upgraded.\x1b[0m\r\n"),
            {"upgraded": 3, "installed": 0, "removed": 0, "held": 1},
        )

    def test_apt_summary_counts_come_from_the_last_summary_line(self) -> None:
        output = (
            "Calculating upgrade...\n"
            "12 upgraded, 1 newly installed, 0 to remove and 2 not upgraded.\n"
            "Setting up libc6 ...\n"
            "0 upgraded, 0 newly installed, 0 to remove and 0 not upgraded.\n"
        )
        self.assertEqual(
            run_qemu.parse_upgrade_summary(output),
            {"upgraded": 0, "installed": 0, "removed": 0, "held": 0},
        )
        self.assertEqual(run_qemu.parse_upgrade_summary(output.split("Setting")[0])["held"], 2)
        with self.assertRaises(RepoError):
            run_qemu.parse_upgrade_summary("E: Unable to locate package\n")

    def test_installed_packages_must_be_at_the_built_versions(self) -> None:
        installed = "libc6 2.43-1mattos2\nbash 5.3-1mattos1\nunrelated 1.0\n\ngarbage line here\n"
        inventory = {"libc6": "2.43-1mattos2", "bash": "5.3-1mattos2", "zlib1g": "1.3.2-1mattos1"}
        self.assertEqual(
            run_qemu.stale_installed_packages(installed, inventory),
            ["bash 5.3-1mattos1 (built 5.3-1mattos2)"],
        )

    def test_installed_check_results_are_named(self) -> None:
        log = (
            "[installed-check] PASS not-live\n\x1b]3008;end=x\x1b\\[installed-check] PASS toolchain\n"
            "noise [installed-check] FAIL compositor\n[installed-check] screenshot desktop=x\n"
            "\x1b[installed-check] PASS grub-menu\n"
        )
        self.assertEqual(
            run_qemu.installed_check_results(log),
            {"pass": ["not-live", "toolchain", "grub-menu"], "fail": ["compositor"]},
        )

    def test_the_temporary_source_trusts_only_the_served_build(self) -> None:
        source = run_qemu.upgrade_test_source(43210)
        self.assertIn("URIs: http://10.0.2.2:43210/\n", source)
        self.assertIn("Suites: trixie\n", source)
        self.assertIn("Trusted: yes\n", source)

    def test_the_build_repository_is_served_on_loopback(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            with self.assertRaises(RepoError):
                run_qemu.serve_repository(root)
            release = root / "dists/trixie/Release"
            release.parent.mkdir(parents=True)
            release.write_text("Origin: MattOS\nLabel: MattOS Local\n")
            server, port = run_qemu.serve_repository(root)
            try:
                self.assertEqual(server.server_address[0], "127.0.0.1")
                with urllib.request.urlopen(f"http://127.0.0.1:{port}/dists/trixie/Release") as response:
                    self.assertIn(b"MattOS Local", response.read())
            finally:
                server.shutdown()
                server.server_close()

    def test_saving_a_baseline_records_its_checksum_and_commit(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            iso = root / "out/images/mattos-x86_64.iso"
            iso.parent.mkdir(parents=True)
            iso.write_bytes(b"iso")
            outputs = {("git", "rev-parse", "HEAD"): "abc123\n", ("git", "status", "--porcelain"): ""}
            with mock.patch.object(run_qemu, "run_command_capture", side_effect=lambda args, _root: outputs[tuple(args)]):
                saved = run_qemu.save_upgrade_baseline(root, iso)
            self.assertEqual(saved.read_bytes(), b"iso")
            import json
            metadata = json.loads(run_qemu.upgrade_baseline_metadata(saved).read_text())
            self.assertEqual(metadata["git_commit"], "abc123")
            self.assertFalse(metadata["git_dirty"])
            import hashlib
            self.assertEqual(metadata["sha256"], hashlib.sha256(b"iso").hexdigest())

    def test_upgrade_test_arguments_are_validated(self) -> None:
        for argv in (
            ["--upgrade-test", "--install"],
            ["--upgrade-test", "--run-installed"],
            ["--upgrade-from", "x.iso"],
            ["--save-upgrade-baseline", "--upgrade-test"],
        ):
            with self.subTest(argv=argv), mock.patch.object(sys, "argv", ["run_qemu.py", *argv]):
                with self.assertRaises(SystemExit), mock.patch("sys.stderr"):
                    run_qemu.parse_args()
        with mock.patch.object(sys, "argv", ["run_qemu.py", "--upgrade-test", "--upgrade-from", "old.iso",
                                             "--install-profile", "cli"]):
            args = run_qemu.parse_args()
        self.assertTrue(args.upgrade_test)
        self.assertEqual(args.upgrade_from, Path("old.iso"))

    def test_a_missing_baseline_is_reported_with_how_to_save_one(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            args = mock.Mock(upgrade_from=None, install_profile="cli")
            with self.assertRaisesRegex(RepoError, "--save-upgrade-baseline"):
                run_qemu.run_upgrade_test(Path(directory), args)


if __name__ == "__main__":
    unittest.main()
