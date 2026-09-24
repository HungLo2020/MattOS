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
    installed_plasma_greeter_probe,
    installed_plasma_greeter_process_probe,
    installed_plasma_wayland_socket_probe,
    installed_plasma_user_desktop_absent_probe,
    installed_plasma_logout_command,
    installed_plasma_applet_runtime_probes,
    install_completion_marker,
    launch_qemu,
    network_arguments,
    prepare_install_disk,
    scheduled_poweroff_command,
    scheduled_reboot_command,
    schedule_and_wait_for_guest_reboot,
    submit_plasma_greeter_password,
    test_control_socket,
    _capture_plasma_verification_window,
    _select_installed_qemu_window,
    wake_installed_plasma_greeter,
    uefi_firmware_arguments,
    validate_completed_install,
    write_install_completion,
)


class QemuNetworkArgumentsTests(unittest.TestCase):
    def test_installed_plasma_verifier_checks_dynamic_applet_elf_and_qml_failures(self) -> None:
        probes = dict(installed_plasma_applet_runtime_probes())
        runtime = probes["plasma-applet-runtime-libraries"]
        self.assertIn("dpkg-query -W libicu78 libpulse0", runtime)
        self.assertIn("libkickerplugin.so", runtime)
        self.assertIn("libdigitalclockplugin.so", runtime)
        self.assertIn("libplasma-volume-declarative.so", runtime)
        self.assertEqual(runtime.count("grep -q 'not found'"), 3)
        loaded = probes["plasma-applets-loaded"]
        self.assertIn("journalctl -b _COMM=plasmashell", loaded)
        for applet in ("kickoff", "digitalclock", "pager", "volume"):
            self.assertIn(applet, loaded)
        self.assertIn("Cannot load library", loaded)

    def test_installed_reboot_waits_for_qmp_reset_before_returning_to_serial_probe(self) -> None:
        events: list[str] = []

        class FakeQmp:
            def __init__(self, socket: Path, timeout: float) -> None:
                events.append(f"qmp-open:{socket}:{timeout}")

            def __enter__(self) -> "FakeQmp":
                events.append("qmp-ready")
                return self

            def __exit__(self, *_: object) -> None:
                events.append("qmp-close")

            def wait_for_event(self, name: str, timeout: float) -> dict[str, object]:
                events.append(f"wait-event:{name}")
                return {"event": name, "data": {"guest": True, "reason": "guest-reset"}}

        def serial(_path: Path, command: str, _timeout: float, **_kwargs: object) -> str:
            events.append(f"serial:{command}")
            return "reboot scheduled"

        with mock.patch("run_qemu.QmpClient", FakeQmp), mock.patch(
            "run_qemu.serial_command_stream", side_effect=serial
        ):
            observed = schedule_and_wait_for_guest_reboot(
                Path("qmp.sock"), Path("serial.sock"), scheduled_reboot_command("test")
            )
        self.assertEqual(observed, {"event": "RESET", "guest": True, "reason": "guest-reset"})
        self.assertLess(events.index("qmp-ready"), next(i for i, item in enumerate(events) if item.startswith("serial:")))
        self.assertLess(next(i for i, item in enumerate(events) if item.startswith("serial:")), events.index("wait-event:RESET"))
        self.assertEqual(events[-1], "qmp-close")

    def test_plasma_verification_captures_largest_installed_qemu_display(self) -> None:
        with TemporaryDirectory() as temporary:
            destination = Path(temporary) / "greeter.png"
            calls: list[list[str]] = []

            def run(command: list[str], **kwargs: object) -> subprocess.CompletedProcess[str]:
                calls.append(command)
                if command[0].endswith("xwininfo"):
                    output = '\n'.join([
                        '  0x100 "QEMU (mattos-installed-plasma-login-verification-1)" 640x384+0+0',
                        '  0x200 "QEMU (mattos-installed-plasma-login-verification-0)" 1280x800+0+0',
                    ])
                    return subprocess.CompletedProcess(command, 0, output, "")
                if command[0].endswith("import"):
                    destination.write_bytes(b"test-image")
                    return subprocess.CompletedProcess(command, 0, "", "")
                if command[0].endswith("identify"):
                    return subprocess.CompletedProcess(command, 0, "1280 800" if "-format" in command and "%w %h" in command else "0.18", "")
                self.fail(f"unexpected capture command: {command}")

            with mock.patch("run_qemu.shutil.which", side_effect=lambda name: f"/usr/bin/{name}"), mock.patch(
                "run_qemu.subprocess.run", side_effect=run
            ):
                self.assertEqual(
                    _capture_plasma_verification_window(Path(temporary), destination),
                    (1280, 800),
                )
            self.assertIn("0x200", calls[1])
            self.assertEqual(calls[1][calls[1].index("-window") + 1], "0x200")
            self.assertIn("%[fx:standard_deviation]", calls[3])

    def test_plasma_verification_rejects_black_guest_surface(self) -> None:
        with TemporaryDirectory() as temporary:
            destination = Path(temporary) / "black-greeter.png"

            def run(command: list[str], **kwargs: object) -> subprocess.CompletedProcess[str]:
                if command[0].endswith("xwininfo"):
                    return subprocess.CompletedProcess(command, 0,
                        '  0x200 "QEMU (mattos-installed-plasma-login-verification-0)" 1280x800+0+0', "")
                if command[0].endswith("import"):
                    destination.write_bytes(b"test-image")
                    return subprocess.CompletedProcess(command, 0, "", "")
                if command[0].endswith("identify"):
                    value = "1280 800" if "%w %h" in command else "0"
                    return subprocess.CompletedProcess(command, 0, value, "")
                self.fail(f"unexpected capture command: {command}")

            with mock.patch("run_qemu.shutil.which", side_effect=lambda name: f"/usr/bin/{name}"), mock.patch(
                "run_qemu.subprocess.run", side_effect=run
            ), self.assertRaisesRegex(RepoError, "did not visibly render"):
                _capture_plasma_verification_window(Path(temporary), destination, timeout=0)

    def test_plasma_capture_retries_transient_black_startup_frame(self) -> None:
        with TemporaryDirectory() as temporary:
            destination = Path(temporary) / "greeter.png"
            calls = 0

            def run(command: list[str], **kwargs: object) -> subprocess.CompletedProcess[str]:
                nonlocal calls
                if command[0].endswith("xwininfo"):
                    return subprocess.CompletedProcess(command, 0,
                        '  0x200 "QEMU (mattos-installed-plasma-login-verification-0)" 1280x800+0+0', "")
                if command[0].endswith("import"):
                    calls += 1
                    destination.write_bytes(b"frame")
                    return subprocess.CompletedProcess(command, 0, "", "")
                if command[0].endswith("identify"):
                    value = "1280 800" if "%w %h" in command else ("0" if calls == 1 else "0.2")
                    return subprocess.CompletedProcess(command, 0, value, "")
                self.fail(f"unexpected capture command: {command}")

            with mock.patch("run_qemu.shutil.which", side_effect=lambda name: f"/usr/bin/{name}"), mock.patch(
                "run_qemu.subprocess.run", side_effect=run
            ), mock.patch("run_qemu.time.sleep"):
                self.assertEqual(
                    _capture_plasma_verification_window(Path(temporary), destination, timeout=5),
                    (1280, 800),
                )
            self.assertEqual(calls, 2)

    def test_installed_plasma_probe_distinguishes_greeter_kwin_from_user_desktop(self) -> None:
        greeter = installed_plasma_greeter_probe()
        self.assertIn("plasmalogin.service", greeter)
        self.assertIn("loginctl --no-pager show-user plasmalogin -p Sessions", greeter)
        self.assertIn("pgrep -u plasmalogin -x kwin_wayland", greeter)
        self.assertIn("pgrep -u plasmalogin -f startplasma-login-wayland", greeter)
        self.assertIn("wayland-0", greeter)
        socket_probe = installed_plasma_wayland_socket_probe()
        self.assertIn("sudo -S -p '' test -S", socket_probe)
        self.assertIn("$(id -u plasmalogin)", socket_probe)
        self.assertTrue(greeter.endswith(socket_probe))
        process_probe = installed_plasma_greeter_process_probe()
        self.assertIn("startplasma-login-wayland", process_probe)
        self.assertNotIn("kwin_wayland", process_probe)
        self.assertNotIn("awk", greeter)
        self.assertNotIn("! pgrep", greeter)

        user_absent = installed_plasma_user_desktop_absent_probe("mattos")
        self.assertIn("! pgrep -u mattos -x kwin_wayland", user_absent)
        self.assertIn("! pgrep -u mattos -x plasmashell", user_absent)

    def test_installed_plasma_verification_keeps_graphical_auth_enabled(self) -> None:
        plan = run_qemu._test_install_plan("plasma")
        self.assertIn('automatic_login = false', plan)
        self.assertIn('test_autologin = true', plan)

    def test_plasma_window_selection_ignores_blank_secondary_sdl_head(self) -> None:
        tree = '\n'.join([
            '  0x3200012 "QEMU (mattos-installed-plasma-login-verification-1)" 640x384+0+0',
            '  0x320000a "QEMU (mattos-installed-plasma-login-verification-0)" 1280x800+0+0',
            '  0x1600004 "unrelated application" 1920x1080+0+0',
        ])
        self.assertEqual(_select_installed_qemu_window(tree), "0x320000a")
        self.assertIsNone(_select_installed_qemu_window('  0x1 "Other window" 800x600+0+0'))

    def test_greeter_wakeup_is_real_qmp_keyboard_input(self) -> None:
        events: list[str] = []

        class FakeQmp:
            def __init__(self, socket: Path, timeout: int) -> None:
                self.socket = socket

            def __enter__(self) -> "FakeQmp":
                events.append("connect")
                return self

            def __exit__(self, *_: object) -> None:
                events.append("close")

        with mock.patch("run_qemu.QmpClient", FakeQmp), mock.patch(
            "run_qemu.send_key", side_effect=lambda _qmp, key: events.append(key)
        ), mock.patch("run_qemu.time.sleep") as sleep:
            wake_installed_plasma_greeter(Path("guest-qmp.sock"))
        self.assertEqual(events, ["connect", "shift", "close"])
        sleep.assert_called_once_with(0.4)

    def test_greeter_password_submission_clears_rejected_entry_before_typing(self) -> None:
        events: list[tuple[str, str]] = []
        with mock.patch(
            "run_qemu.send_key", side_effect=lambda _qmp, key: events.append(("key", key))
        ), mock.patch(
            "run_qemu.type_text", side_effect=lambda _qmp, value: events.append(("text", value))
        ), mock.patch("run_qemu.time.sleep"):
            submit_plasma_greeter_password(object(), "mattos")
        self.assertEqual(events, [
            ("key", "ctrl-a"),
            ("key", "backspace"),
            ("text", "mattos"),
            ("key", "ret"),
        ])

    def test_plasma_logout_uses_session_shutdown_service_and_user_bus(self) -> None:
        command = installed_plasma_logout_command()
        self.assertIn('test -S "/run/user/$uid/bus"', command)
        self.assertIn('DBUS_SESSION_BUS_ADDRESS="unix:path=/run/user/$uid/bus"', command)
        self.assertIn("qdbus org.kde.Shutdown /Shutdown org.kde.Shutdown.logout", command)
        self.assertNotIn("loginctl terminate-session", command)

    def test_cli_install_fixture_does_not_enable_a_display_manager(self) -> None:
        plan = run_qemu._test_install_plan("cli")
        self.assertIn('installed_profile = "cli"', plan)
        self.assertIn('automatic_login = false', plan)
        self.assertIn('test_autologin = true', plan)

    def test_shutdown_is_scheduled_after_serial_result_marker(self) -> None:
        self.assertEqual(
            scheduled_poweroff_command(),
            "sudo systemd-run --on-active=1s --unit=mattos-test-poweroff systemctl poweroff",
        )
        password_command = scheduled_poweroff_command("test password")
        self.assertIn("printf '%s\\n' 'test password'", password_command)
        self.assertIn("sudo -S systemd-run --on-active=1s", password_command)
        self.assertNotIn("systemctl --no-block poweroff", password_command)

    def test_forced_shutdown_is_failure_even_when_qemu_exits_zero(self) -> None:
        proc = mock.Mock()
        proc.poll.return_value = None
        proc.wait.side_effect = [subprocess.TimeoutExpired("qemu", 1), 0]
        self.assertEqual(run_qemu._terminate_task_vm(proc, None, "test"), 1)
        proc.terminate.assert_called_once()

    def test_clean_shutdown_preserves_success(self) -> None:
        proc = mock.Mock()
        proc.poll.return_value = None
        proc.wait.return_value = 0
        self.assertEqual(run_qemu._terminate_task_vm(proc, None, "test"), 0)
        proc.terminate.assert_not_called()

    def test_failed_install_retains_disk_and_removes_completion(self) -> None:
        with TemporaryDirectory() as temporary:
            root = Path(temporary)
            disk = root / "out/qemu/installed-test.qcow2"
            disk.parent.mkdir(parents=True)
            disk.write_bytes(b"diagnostic evidence")
            marker = install_completion_marker(root)
            marker.write_text("stale marker")
            args = mock.Mock(install=True, dry_run=False)
            with mock.patch("run_qemu.prepare_install_disk", return_value=disk), mock.patch(
                "run_qemu._launch_one", side_effect=RepoError("verification failure")
            ):
                with self.assertRaisesRegex(RepoError, "verification failure"):
                    launch_qemu(root, root / "image.iso", args)
            self.assertEqual(disk.read_bytes(), b"diagnostic evidence")
            self.assertFalse(marker.exists())

    def test_generated_grub_menu_is_inspected_with_fixture_authorization(self) -> None:
        command = run_qemu.installed_grub_menu_probe()
        self.assertIn("sudo -S grep -q", command)
        self.assertIn("sudo -n grep -q", command)
        self.assertIn("menuentry 'MattOS GNU/Linux'", command)
        self.assertIn("Advanced options for MattOS", command)
        self.assertIn(run_qemu.TEST_INSTALL_PASSWORD, command)
        self.assertNotIn("chmod", command)
        self.assertNotIn("NOPASSWD", command)

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

    def test_no_network_explicitly_disables_qemu_default_networking(self) -> None:
        self.assertEqual(network_arguments(True), ["-nic", "none"])

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
            self.assertIn("-qmp", command)
            self.assertIn("virtio-vga-gl,blob=true,hostmem=256M", command)

    def test_run_installed_keeps_normal_virgl_while_exposing_scoped_shutdown_control(self) -> None:
        with TemporaryDirectory() as temporary:
            root = Path(temporary)
            args = type(
                "Args", (), {
                    "install": False, "run_installed": True, "test_control": False,
                    "qmp_socket": None, "headless": False, "dry_run": False,
                },
            )()
            qmp, serial = run_qemu.test_control_paths(root, args)
            self.assertEqual(qmp, root / "out/qemu/test-control/qmp.sock")
            self.assertEqual(serial, root / "out/qemu/test-control/serial.sock")
            run_qemu.cleanup_test_control_paths((qmp, serial))

    def test_installed_graphics_verification_forces_the_production_virgl_path(self) -> None:
        with TemporaryDirectory() as temporary:
            root = Path(temporary)
            disk = root / "out/qemu/installed-test.qcow2"
            disk.parent.mkdir(parents=True)
            disk.write_bytes(b"qcow2")
            args = type(
                "Args", (), {
                    "install": True, "run_installed": False, "install_profile": "plasma",
                    "test_control": False, "qmp_socket": None, "serial_console": False,
                    "headless": False, "dry_run": False, "qemu_arg": [], "memory": 1024,
                    "cpus": 1, "no_network": True,
                }
            )()
            with mock.patch("run_qemu.install_task_log", return_value=root / "boot.log"), mock.patch(
                "run_qemu.write_install_completion", return_value=root / "marker.json"
            ), mock.patch("run_qemu._launch_one", return_value=0) as launched, mock.patch(
                "run_qemu.wait_for_socket"
            ), mock.patch("run_qemu.serial_command_stream", side_effect=["ok"] * 40), mock.patch(
                "run_qemu.validate_completed_install", return_value={}
            ):
                run_qemu._verify_installed_disk_boot(root, disk, args)
            verification_args = launched.call_args.args[2]
            self.assertTrue(verification_args.test_control)
            self.assertTrue(verification_args.require_virgl)
            command = launched.call_args.args[1]
            self.assertIsNone(command)

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

    def test_installed_acceptance_keeps_virgl_even_with_qmp_control(self) -> None:
        with TemporaryDirectory() as temporary:
            root = Path(temporary)
            args = type(
                "Args", (), {
                    "dry_run": False, "no_kvm": True, "memory": 1024, "cpus": 1,
                    "headless": False, "test_control": True, "qmp_socket": None,
                    "require_virgl": True, "run_installed": True, "install": False,
                    "plasma_login_verification": True,
                    "serial_console": False, "no_network": True, "qemu_arg": [],
                    "live_media": "optical", "no_install_disk": False,
                    "install_disk": Path("out/qemu/installed-test.qcow2"),
                }
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
                run_qemu._launch_one(
                    root,
                    None,
                    args,
                    boot_iso=False,
                    install_disk=root / "out/qemu/installed-test.qcow2",
                )
            command = launched.call_args.args[0]
            self.assertIn("virtio-vga-gl,blob=true,hostmem=256M", command)
            self.assertIn("sdl,gl=on", command)
            self.assertNotIn("gtk,gl=off", command)
            self.assertEqual(launched.call_args.kwargs["env"]["SDL_VIDEODRIVER"], "x11")


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
