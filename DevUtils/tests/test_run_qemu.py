#!/usr/bin/env python3
import os
import shutil
import subprocess
import sys
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import run_qemu
from common import RepoError, ensure_project_temp_root, mattos_build_environment
from run_qemu import (
    acceleration_arguments,
    cleanup_test_control_socket,
    ensure_iso_exists,
    graphical_gpu_device,
    image_build_commands,
    install_completion_marker,
    launch_qemu,
    network_arguments,
    prepare_install_disk,
    test_control_socket,
    uefi_firmware_arguments,
    validate_completed_install,
    write_install_completion,
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

    def test_qemu_uses_ovmf_for_the_installed_efi_boot_path(self) -> None:
        with TemporaryDirectory() as temporary:
            firmware = Path(temporary) / "OVMF.fd"
            firmware.write_bytes(b"OVMF")
            self.assertEqual(
                uefi_firmware_arguments((firmware,)),
                ["-bios", str(firmware)],
            )
            with self.assertRaises(RepoError):
                uefi_firmware_arguments((firmware.with_name("missing.fd"),))

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
            dispatcher = root / "DevUtils/cargo_source_owned.py"
            dispatcher.parent.mkdir(parents=True)
            dispatcher.write_text("#!/usr/bin/env python3\n", encoding="utf-8")
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

    def test_install_mode_recreates_only_the_dedicated_test_disk(self) -> None:
        with TemporaryDirectory() as temporary:
            root = Path(temporary)
            disk = root / "out/qemu/installed-test.qcow2"
            disk.parent.mkdir(parents=True)
            disk.write_bytes(b"old")
            args = type("Args", (), {"no_install_disk": False, "install_disk": None, "install": True, "run_installed": False})()
            def create_disk(*_args, **_kwargs):
                disk.write_bytes(b"new")
                return subprocess.CompletedProcess([], 0)
            with mock.patch("run_qemu.shutil.which", return_value="/usr/bin/qemu-img"), mock.patch(
                "run_qemu.subprocess.run", side_effect=create_disk
            ) as create:
                self.assertEqual(prepare_install_disk(root, args), disk.resolve())
            self.assertEqual(create.call_args.args[0][0:4], ["qemu-img", "create", "-f", "qcow2"])
            self.assertEqual(disk.read_bytes(), b"new")

    def test_run_installed_requires_existing_dedicated_disk(self) -> None:
        with TemporaryDirectory() as temporary:
            args = type("Args", (), {"no_install_disk": False, "install_disk": None, "install": False, "run_installed": True})()
            with self.assertRaises(RepoError):
                prepare_install_disk(Path(temporary), args)

    def test_run_installed_rejects_an_unmarked_disk(self) -> None:
        with TemporaryDirectory() as temporary:
            root = Path(temporary)
            disk = root / "out/qemu/installed-test.qcow2"
            disk.parent.mkdir(parents=True)
            disk.write_bytes(b"incomplete")
            args = type("Args", (), {"no_install_disk": False, "install_disk": None, "install": False, "run_installed": True})()
            with self.assertRaisesRegex(RepoError, "has not completed installation verification"):
                prepare_install_disk(root, args)

    def test_install_invalidates_the_completion_marker_before_recreating_disk(self) -> None:
        with TemporaryDirectory() as temporary:
            root = Path(temporary)
            disk = root / "out/qemu/installed-test.qcow2"
            disk.parent.mkdir(parents=True)
            disk.write_bytes(b"old")
            marker = install_completion_marker(root)
            marker.write_text('{"schema": 1}\n', encoding="utf-8")
            args = type("Args", (), {"no_install_disk": False, "install_disk": None, "install": True, "run_installed": False})()
            def create_disk(*_args, **_kwargs):
                disk.write_bytes(b"fresh")
                return subprocess.CompletedProcess([], 0)
            with mock.patch("run_qemu.shutil.which", return_value="/usr/bin/qemu-img"), mock.patch(
                "run_qemu.subprocess.run", side_effect=create_disk
            ):
                prepare_install_disk(root, args)
            self.assertFalse(marker.exists())

    def test_completion_metadata_is_written_only_after_explicit_boot_verification(self) -> None:
        with TemporaryDirectory() as temporary:
            root = Path(temporary)
            disk = root / "out/qemu/installed-test.qcow2"
            disk.parent.mkdir(parents=True)
            disk.write_bytes(b"qcow2")
            self.assertFalse(install_completion_marker(root).exists())
            with mock.patch("run_qemu.qemu_disk_virtual_size", return_value=16 * 1024**3):
                marker = write_install_completion(root, disk, {"uefi_grub_boot": True})
            self.assertTrue(marker.is_file())
            payload = __import__("json").loads(marker.read_text(encoding="utf-8"))
            self.assertTrue(payload["verification"]["uefi_grub_boot"])

    def test_installed_validation_rejects_changed_or_corrupt_disk(self) -> None:
        with TemporaryDirectory() as temporary:
            root = Path(temporary)
            disk = root / "out/qemu/installed-test.qcow2"
            disk.parent.mkdir(parents=True)
            disk.write_bytes(b"qcow2")
            marker = install_completion_marker(root)
            marker.write_text(
                '{"schema": 1, "disk": "' + str(disk.resolve()) + '", '
                '"virtual_size": 1, "verification": {"uefi_grub_boot": true}}\n',
                encoding="utf-8",
            )
            with mock.patch("run_qemu.qemu_disk_virtual_size", return_value=2):
                with self.assertRaisesRegex(RepoError, "changed after verification"):
                    validate_completed_install(root, disk)

    def test_installed_boot_omits_install_media_and_uses_disk_boot_order(self) -> None:
        with TemporaryDirectory() as temporary:
            root = Path(temporary)
            disk = root / "out/qemu/installed-test.qcow2"
            disk.parent.mkdir(parents=True)
            disk.write_bytes(b"qcow2")
            args = type(
                "Args", (), {
                    "no_install_disk": False, "install_disk": None, "install": False,
                    "run_installed": True, "no_network": True, "serial_console": True,
                    "dry_run": False, "headless": False, "qemu_arg": [], "memory": 1024,
                    "cpus": 1, "test_control": False, "qmp_socket": None,
                }
            )()
            process = mock.Mock()
            process.wait.return_value = 0
            with mock.patch("run_qemu.prepare_install_disk", return_value=disk.resolve()), mock.patch(
                "run_qemu.subprocess.Popen", return_value=process
            ) as launched, mock.patch(
                "run_qemu.mattos_build_environment", return_value={}
            ), mock.patch("run_qemu.choose_graphical_display", return_value="gtk,gl=on"), mock.patch(
                "run_qemu.graphical_gpu_device", return_value="virtio-vga-gl,blob=true,hostmem=256M"
            ):
                self.assertEqual(launch_qemu(root, None, args), 0)
            command = launched.call_args.args[0]
            self.assertIn("-boot", command)
            self.assertIn("order=c", command)
            self.assertNotIn("media=cdrom", " ".join(command))
            self.assertIn(f"file={disk.resolve()},if=virtio,format=qcow2", command)

    def test_test_control_uses_scoped_qmp_socket_and_removes_stale_socket(self) -> None:
        with TemporaryDirectory() as temporary:
            root = Path(temporary)
            stale = root / "out/qemu/test-control/qmp.sock"
            stale.parent.mkdir(parents=True)
            stale.write_text("stale", encoding="utf-8")
            args = type(
                "Args",
                (),
                {"test_control": True, "qmp_socket": None, "headless": False, "dry_run": False},
            )()
            self.assertEqual(test_control_socket(root, args), stale)
            self.assertFalse(stale.exists())
            stale.write_text("created", encoding="utf-8")
            cleanup_test_control_socket(stale)
            self.assertFalse(stale.exists())

    def test_test_control_rejects_headless_or_unscoped_socket(self) -> None:
        with TemporaryDirectory() as temporary:
            root = Path(temporary)
            headless = type(
                "Args",
                (),
                {"test_control": True, "qmp_socket": None, "headless": True, "dry_run": False},
            )()
            with self.assertRaises(RepoError):
                test_control_socket(root, headless)
            outside = type(
                "Args",
                (),
                {"test_control": True, "qmp_socket": root / "outside.sock", "headless": False, "dry_run": False},
            )()
            with self.assertRaises(RepoError):
                test_control_socket(root, outside)

    def test_qmp_socket_requires_explicit_test_control_mode(self) -> None:
        args = type(
            "Args",
            (),
            {"test_control": False, "qmp_socket": Path("out/qemu/test-control/qmp.sock"), "headless": False, "dry_run": False},
        )()
        with self.assertRaises(RepoError):
            test_control_socket(Path("/repo"), args)

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
                    "headless": False,
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
            ), mock.patch(
                "run_qemu.choose_graphical_display", return_value="gtk,gl=on"
            ):
                self.assertEqual(launch_qemu(root, root / "mattos.iso", args), 0)
            command = launched.call_args.args[0]
            self.assertIn(f"file={disk.resolve()},if=virtio,format=qcow2", command)
            self.assertIn("/usr/share/ovmf/OVMF.fd", command)
            self.assertIn("virtio-vga-gl,blob=true,hostmem=256M", command)
            self.assertNotIn("-vga", command)
            self.assertIn("qemu-xhci,id=mattos-xhci", command)
            self.assertIn("usb-tablet,bus=mattos-xhci.0", command)

    def test_test_control_adds_qmp_without_changing_normal_qemu_arguments(self) -> None:
        with TemporaryDirectory() as temporary:
            root = Path(temporary)
            args = type(
                "Args",
                (),
                {
                    "no_install_disk": True,
                    "install_disk": None,
                    "no_network": True,
                    "serial_console": True,
                    "dry_run": False,
                    "headless": False,
                    "qemu_arg": [],
                    "memory": 1024,
                    "cpus": 1,
                    "test_control": True,
                    "qmp_socket": None,
                },
            )()
            process = mock.Mock()
            process.wait.return_value = 0
            with mock.patch("run_qemu.subprocess.Popen", return_value=process) as launched, mock.patch(
                "run_qemu.mattos_build_environment", return_value={}
            ), mock.patch(
                "run_qemu.graphical_gpu_device",
                return_value="virtio-vga-gl,blob=true,hostmem=256M",
            ), mock.patch(
                "run_qemu.choose_graphical_display", return_value="gtk,gl=on"
            ):
                self.assertEqual(launch_qemu(root, root / "mattos.iso", args), 0)
            command = launched.call_args.args[0]
            self.assertIn("-qmp", command)
            self.assertIn("unix:" + str(root / "out/qemu/test-control/qmp.sock") + ",server=on,wait=off", command)
            self.assertIn("signal=off", command[command.index("-chardev") + 1])
            self.assertFalse((root / "out/qemu/test-control/qmp.sock").exists())


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
        repo_root = Path(__file__).resolve().parents[2]
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
