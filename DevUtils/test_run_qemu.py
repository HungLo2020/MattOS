#!/usr/bin/env python3
import os
import shutil
import subprocess
import sys
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parent))
import run_qemu
from common import RepoError, ensure_project_temp_root, mattos_build_environment
from run_qemu import (
    acceleration_arguments,
    ensure_iso_exists,
    graphical_gpu_device,
    image_build_commands,
    launch_qemu,
    network_arguments,
    prepare_install_disk,
)


class QemuNetworkArgumentsTests(unittest.TestCase):
    def test_kvm_is_used_when_accessible(self) -> None:
        with mock.patch("run_qemu.Path.exists", return_value=True), mock.patch(
            "run_qemu.os.access", return_value=True
        ):
            self.assertEqual(acceleration_arguments(), ["-enable-kvm", "-cpu", "host"])

    def test_kvm_falls_back_to_tcg_when_inaccessible_or_disabled(self) -> None:
        with mock.patch("run_qemu.Path.exists", return_value=True), mock.patch(
            "run_qemu.os.access", return_value=False
        ):
            self.assertEqual(acceleration_arguments(), [])
        self.assertEqual(acceleration_arguments(disabled=True), [])

    def test_iso_guard_accepts_xorriso_listing_written_to_stderr(self) -> None:
        with TemporaryDirectory() as temporary:
            root = Path(temporary)
            iso = root / "out/images/mattos-x86_64.iso"
            iso.parent.mkdir(parents=True)
            iso.touch()
            completed = subprocess.CompletedProcess(
                args=[],
                returncode=0,
                stdout="",
                stderr="'/live/rootfs.squashfs'\n",
            )
            with mock.patch("run_qemu.shutil.which", return_value="/usr/bin/xorriso"), mock.patch(
                "run_qemu.subprocess.run", return_value=completed
            ) as invoked:
                self.assertEqual(ensure_iso_exists(root), iso)
                self.assertIn("-ls", invoked.call_args.args[0])

    def test_default_network_is_unprivileged_virtio(self) -> None:
        self.assertEqual(
            network_arguments(False),
            ["-netdev", "user,id=net0", "-device", "virtio-net-pci,netdev=net0"],
        )

    def test_no_network_omits_all_network_arguments(self) -> None:
        self.assertEqual(network_arguments(True), [])

    def test_graphical_display_enables_host_gl_for_virgl_scanout(self) -> None:
        with mock.patch("run_qemu.run_command_capture", return_value="gtk\nsdl\n"):
            self.assertEqual(run_qemu.choose_graphical_display(Path("/repo")), "gtk,gl=on")

    def test_graphical_gpu_requires_the_qemu_vga_virgl_variant(self) -> None:
        with mock.patch("run_qemu.run_command_capture", return_value='name "virtio-vga-gl", bus PCI'):
            self.assertEqual(
                graphical_gpu_device(Path("/repo")),
                "virtio-vga-gl,blob=true,hostmem=256M",
            )

    def test_graphical_gpu_fails_closed_without_virgl(self) -> None:
        with mock.patch("run_qemu.run_command_capture", return_value='name "virtio-gpu-pci", bus PCI'):
            with self.assertRaises(RepoError):
                graphical_gpu_device(Path("/repo"))

    def test_launcher_invokes_one_image_producing_build(self) -> None:
        commands = image_build_commands(False)
        self.assertEqual(
            commands,
            [["cargo", "run", "-p", "mattos-build", "--", "build", "all"]],
        )
        self.assertNotIn("image", [argument for command in commands for argument in command])

    def test_clean_build_still_has_one_image_producing_build(self) -> None:
        commands = image_build_commands(True)
        self.assertEqual(len(commands), 2)
        self.assertEqual(commands[-1][-2:], ["build", "all"])

    def test_launcher_environment_uses_repository_owned_tmpdir(self) -> None:
        with TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "src/tools/mattos-build").mkdir(parents=True)
            (root / "src/tools/mattos-build/Cargo.toml").touch()
            with mock.patch.dict(os.environ, {"TMPDIR": "/full/host/tmp"}, clear=False), mock.patch(
                "common.helpers.shutil.disk_usage",
                return_value=shutil.disk_usage(Path.cwd()),
            ):
                environment = mattos_build_environment(root)
            self.assertEqual(environment["TMPDIR"], str(root / "out/tmp"))
            self.assertTrue((root / "out/tmp").is_dir())

    def test_project_temp_preflight_rejects_insufficient_space(self) -> None:
        with TemporaryDirectory() as temporary:
            with self.assertRaises(Exception):
                ensure_project_temp_root(Path(temporary), minimum_free_bytes=10**18)

    def test_default_install_disk_is_created_once_and_persistent(self) -> None:
        with TemporaryDirectory() as temporary:
            root = Path(temporary)
            args = type("Args", (), {"no_install_disk": False, "install_disk": None})()
            disk = prepare_install_disk(root, args)
            self.assertEqual(disk, (root / "out/qemu/mattos-dev.qcow2").resolve())
            self.assertTrue(disk.is_file())
            first_bytes = disk.read_bytes()
            self.assertGreater(len(first_bytes), 0)
            self.assertEqual(prepare_install_disk(root, args), disk)
            self.assertEqual(disk.read_bytes(), first_bytes)

    def test_no_install_disk_option_disables_target_disk(self) -> None:
        with TemporaryDirectory() as temporary:
            args = type("Args", (), {"no_install_disk": True, "install_disk": None})()
            self.assertIsNone(prepare_install_disk(Path(temporary), args))

    def test_qemu_command_attaches_install_disk_as_virtio(self) -> None:
        with TemporaryDirectory() as temporary:
            root = Path(temporary)
            disk = root / "custom.qcow2"
            disk.write_bytes(b"existing qcow2 placeholder")
            args = type(
                "Args",
                (),
                {
                    "no_install_disk": False,
                    "install_disk": disk,
                    "no_network": True,
                    "serial_console": True,
                    "dry_run": False,
                    "qemu_arg": [],
                    "memory": 1024,
                    "cpus": 1,
                },
            )()
            process = mock.Mock()
            process.wait.return_value = 0
            with mock.patch("run_qemu.subprocess.Popen", return_value=process) as launched, mock.patch(
                "run_qemu.mattos_build_environment", return_value={}
            ), mock.patch(
                "run_qemu.graphical_gpu_device",
                return_value="virtio-vga-gl,blob=true,hostmem=256M",
            ):
                self.assertEqual(launch_qemu(root, root / "mattos.iso", args), 0)
            command = launched.call_args.args[0]
            self.assertIn(f"file={disk.resolve()},if=virtio,format=qcow2", command)
            self.assertIn("virtio-vga-gl,blob=true,hostmem=256M", command)
            self.assertNotIn("-vga", command)
            self.assertIn("qemu-xhci,id=mattos-xhci", command)
            self.assertIn("usb-tablet,bus=mattos-xhci.0", command)


@unittest.skipUnless(
    os.environ.get("MATTOS_RUN_FRESH_PROCESS_CACHE_TESTS") == "1",
    "set MATTOS_RUN_FRESH_PROCESS_CACHE_TESTS=1 for the full cache integration test",
)
class FreshProcessCacheIntegrationTests(unittest.TestCase):
    FOUNDATIONAL_STAGES = (
        "linux",
        "glibc",
        "linux-headers",
        "gcc-runtime",
        "binutils",
        "gcc-compiler",
        "make",
        "formal-sysroot",
    )

    def run_fresh(self, command: list[str], **environment: str) -> str:
        repo_root = Path(__file__).resolve().parents[1]
        child_environment = os.environ.copy()
        child_environment.update(environment)
        completed = subprocess.run(
            command,
            cwd=repo_root,
            env=child_environment,
            check=False,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
        )
        self.assertEqual(completed.returncode, 0, completed.stdout)
        return completed.stdout

    def assert_foundational_hits(self, output: str) -> None:
        for stage in self.FOUNDATIONAL_STAGES:
            self.assertIn(f"cache hit: {stage} ", output, output)
            self.assertNotIn(f"cache miss: {stage} ", output, output)

    def test_direct_launcher_and_cross_path_cache_stability(self) -> None:
        direct = ["cargo", "run", "-p", "mattos-build", "--", "build", "all"]
        launcher = [sys.executable, "DevUtils/run_qemu.py", "--build-only"]

        # The first direct process may perform the one-time schema migration.
        self.run_fresh(direct, TERM="dumb", COLUMNS="80", LINES="24")
        launcher_one = self.run_fresh(
            launcher,
            TERM="xterm-256color",
            COLORTERM="truecolor",
            COLUMNS="240",
            LINES="60",
            MATTOS_VERBOSE_BUILD_OUTPUT="",
            QEMU_AUDIO_DRV="none",
        )
        launcher_two = self.run_fresh(
            launcher,
            TERM="screen-256color",
            COLORTERM="24bit",
            COLUMNS="132",
            LINES="43",
            QEMU_AUDIO_DRV="pa",
        )
        direct_two = self.run_fresh(direct, TERM="dumb", COLUMNS="72", LINES="20")

        self.assert_foundational_hits(launcher_one)
        self.assert_foundational_hits(launcher_two)
        self.assert_foundational_hits(direct_two)


if __name__ == "__main__":
    unittest.main()
