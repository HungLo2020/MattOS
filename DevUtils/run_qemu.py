#!/usr/bin/env python3
import argparse
import base64
import json
import os
import shutil
import signal
import subprocess
import sys
import time
from datetime import UTC, datetime
from pathlib import Path
from collections.abc import Callable
from typing import List

from qemu_test_control import QmpClient, QmpError, serial_command, serial_command_stream, wait_for_socket

from common import (
    RepoError,
    ensure_tools,
    find_repo_root,
    mattos_build_environment,
    run_command,
    run_command_capture,
)

DEFAULT_INSTALL_DISK_RELATIVE = Path("out/qemu/mattos-dev.qcow2")
INSTALL_TEST_DISK_RELATIVE = Path("out/qemu/installed-test.qcow2")
INSTALL_TEST_COMPLETION_RELATIVE = Path("out/qemu/installed-test.complete.json")
INSTALL_TEST_LOG_RELATIVE = Path("out/logs/installed-test-install.log")
DEFAULT_INSTALL_DISK_SIZE = "16G"
DEFAULT_VM_MEMORY_MIB = 6144
DEFAULT_VM_CPUS = 4
# `virtio-vga-gl` is the one GPU that satisfies both development-launcher
# requirements: its VGA compatibility gives firmware/GRUB a scanout before
# Linux starts, and its VirtIO GL backend exposes the VirGL capset used later
# by Mesa and the native COSMIC compositor.
# QEMU's virtio-vga-gl defaults to a 1280×800 firmware scanout. Do not pass
# xres/yres explicitly: they are only firmware hints, not a Wayland policy,
# and leaving the device defaults intact lets KMS/cosmic-comp select the
# preferred DRM mode exposed by the virtual output.
VIRTIO_GPU_GL_DEVICE = "virtio-vga-gl,blob=true,hostmem=256M"
QEMU_TABLET_CONTROLLER = "qemu-xhci,id=mattos-xhci"
QEMU_TABLET_DEVICE = "usb-tablet,bus=mattos-xhci.0"
TEST_CONTROL_DIRECTORY_RELATIVE = Path("out/qemu/test-control")
TEST_CONTROL_QMP_SOCKET_NAME = "qmp.sock"
TEST_CONTROL_SERIAL_SOCKET_NAME = "serial.sock"
INSTALL_PROGRESS_IDLE_SECONDS = 10 * 60
INSTALL_BOOT_IDLE_SECONDS = 4 * 60
INSTALL_SHUTDOWN_SECONDS = 90
UEFI_FIRMWARE_CANDIDATES = (
    Path("/usr/share/ovmf/OVMF.fd"),
    Path("/usr/share/qemu/OVMF.fd"),
)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Build and run MattOS in QEMU")
    parser.add_argument("--no-build", action="store_true", help="skip build/image steps")
    parser.add_argument("--clean", action="store_true", help="clean build artifacts before rebuilding")
    parser.add_argument(
        "--memory",
        type=int,
        default=DEFAULT_VM_MEMORY_MIB,
        help=f"VM memory in MiB (default: {DEFAULT_VM_MEMORY_MIB})",
    )
    parser.add_argument(
        "--cpus",
        type=int,
        default=DEFAULT_VM_CPUS,
        help=f"virtual CPU count (default: {DEFAULT_VM_CPUS})",
    )
    parser.add_argument(
        "--no-kvm",
        action="store_true",
        help="force software emulation even when /dev/kvm is accessible",
    )
    parser.add_argument(
        "--no-network",
        action="store_true",
        help="disable the default unprivileged QEMU user-mode network",
    )
    parser.add_argument(
        "--serial-console",
        action="store_true",
        help="run terminal-oriented diagnostic mode (-nographic, serial stdio)",
    )
    parser.add_argument(
        "--headless",
        action="store_true",
        help="omit the graphical GPU and run with serial output (for CI/headless hosts)",
    )
    parser.add_argument(
        "--test-control",
        action="store_true",
        help=(
            "enable the local QMP control socket for graphical development testing; "
            "use DevUtils/qemu_test_control.py to capture screenshots or inject input"
        ),
    )
    parser.add_argument(
        "--qmp-socket",
        type=Path,
        metavar="PATH",
        help=(
            "override the local QMP socket used with --test-control "
            "(must be inside out/qemu/test-control)"
        ),
    )
    parser.add_argument(
        "--qemu-arg",
        action="append",
        default=[],
        help="additional raw QEMU argument (repeatable)",
    )
    parser.add_argument("--dry-run", action="store_true", help="print commands without executing")
    parser.add_argument(
        "--build-only",
        action="store_true",
        help="build the dependency-correct ISO once and exit without launching QEMU",
    )
    parser.add_argument(
        "--install",
        action="store_true",
        help="recreate the dedicated installed-test.qcow2 and install the Desktop profile",
    )
    parser.add_argument(
        "--run-installed",
        action="store_true",
        help="boot the existing dedicated installed-test.qcow2 without building or modifying it",
    )
    disk = parser.add_mutually_exclusive_group()
    disk.add_argument(
        "--no-install-disk",
        action="store_true",
        help="launch without the persistent development install disk",
    )
    disk.add_argument(
        "--install-disk",
        type=Path,
        metavar="PATH",
        help="use or create this persistent qcow2 install disk instead of the default",
    )
    args = parser.parse_args()
    if args.build_only and (args.install or args.run_installed):
        parser.error("--build-only cannot be combined with --install or --run-installed")
    if args.install and args.install_disk is not None:
        parser.error("--install always uses the dedicated installed-test.qcow2 disk")
    if args.run_installed and args.install_disk is not None:
        parser.error("--run-installed always uses the dedicated installed-test.qcow2 disk")
    if args.run_installed and args.no_install_disk:
        parser.error("--run-installed requires the dedicated installed-test.qcow2 disk")
    if args.install and args.no_install_disk:
        parser.error("--install requires the dedicated installed-test.qcow2 disk")
    if args.install and args.no_build:
        parser.error("--install always builds the canonical current ISO; omit --no-build")
    if args.run_installed and args.clean and not args.install:
        parser.error("--run-installed never builds; --clean is not applicable")
    return args


def network_arguments(disabled: bool) -> List[str]:
    if disabled:
        # Omitting a NIC configuration does not make QEMU offline: QEMU then
        # creates its default user-mode NIC.  This mode is used for installer
        # failure-path validation, so explicitly suppress every default NIC.
        return ["-nic", "none"]
    return ["-netdev", "user,id=net0", "-device", "virtio-net-pci,netdev=net0"]


def acceleration_selection(disabled: bool = False) -> tuple[List[str], str]:
    """Return QEMU acceleration arguments plus a user-visible status reason."""
    kvm = Path("/dev/kvm")
    if disabled:
        return [], "TCG software emulation (--no-kvm requested)"
    if not kvm.exists():
        return [], "TCG software emulation (/dev/kvm is missing)"
    if not os.access(kvm, os.R_OK | os.W_OK):
        return [], "TCG software emulation (/dev/kvm is not readable/writable by this user)"
    return ["-enable-kvm", "-cpu", "host"], "KVM hardware acceleration"


def acceleration_arguments(disabled: bool = False) -> List[str]:
    """Use native acceleration when this user can actually open /dev/kvm.

    Falling back to QEMU's default TCG keeps the launcher usable in containers
    and on hosts without KVM.  Avoiding an unconditional `-enable-kvm` also
    preserves a useful error-free diagnostic path on those systems.
    """
    return acceleration_selection(disabled)[0]


def report_launch_configuration(args: argparse.Namespace, acceleration_status: str) -> None:
    print(f"[qemu] resources: {args.cpus} vCPU(s), {args.memory} MiB RAM")
    if acceleration_status == "KVM hardware acceleration":
        print("[qemu] acceleration: KVM hardware acceleration (-enable-kvm -cpu host)")
    else:
        print(f"[qemu] WARNING: acceleration: {acceleration_status}")
        print("[qemu] WARNING: MattOS will be very slow under TCG; configure /dev/kvm for usable desktop performance.")


def uefi_firmware_arguments(
    candidates: tuple[Path, ...] = UEFI_FIRMWARE_CANDIDATES,
) -> List[str]:
    """Select combined OVMF firmware for the EFI-only installed boot path."""
    for firmware in candidates:
        if firmware.is_file():
            return ["-bios", str(firmware)]
    searched = ", ".join(str(path) for path in candidates)
    raise RepoError(
        "MattOS installs x86_64-EFI GRUB and requires OVMF in QEMU; "
        f"no combined OVMF image was found ({searched})"
    )


def choose_graphical_display(repo_root: Path) -> str:
    # VirGL scanout requires a GL-capable host display.  Plain GTK/SDL would
    # leave the guest's virtio GPU without the 3D capset that Mesa's source-
    # built virgl driver needs for the COSMIC KMS path.
    try:
        output = run_command_capture(["qemu-system-x86_64", "-display", "help"], cwd=repo_root)
    except RepoError:
        return "gtk,gl=on"

    displays = {line.strip() for line in output.splitlines() if line.strip()}
    if "gtk" in displays:
        return "gtk,gl=on"
    if "sdl" in displays:
        return "sdl,gl=on"
    return "default"


def graphical_gpu_device(repo_root: Path) -> str:
    """Return the VirtIO GPU that exposes the VirGL capset to the guest.

    `virtio-gpu-pci` is only a 2D VirtIO GPU.  The installer intentionally
    uses QEMU's GL variant so the ISO exercises the same DRM/GBM/EGL/dmabuf
    path that the native COSMIC compositor requires.  Fail closed instead of
    silently booting a graphical installer configuration that cannot render.
    """
    try:
        output = run_command_capture(["qemu-system-x86_64", "-device", "help"], cwd=repo_root)
    except RepoError as exc:
        raise RepoError("could not inspect QEMU VirtIO GPU support") from exc
    if "virtio-vga-gl" not in output:
        raise RepoError(
            "this QEMU lacks virtio-vga-gl; the native COSMIC installer "
            "requires a VGA-compatible QEMU VirtIO GPU with GL/VirGL support"
        )
    # VirGL alone provides an EGL context, but COSMIC's KMS renderer also
    # exports its scanout buffers as dmabufs.  Enable QEMU's VirtIO resource
    # blob backing and its bounded host-memory aperture so the guest advertises
    # resource_blob/host_visible instead of an unusable context-only device.
    return VIRTIO_GPU_GL_DEVICE


def ensure_iso_exists(repo_root: Path) -> Path:
    iso_path = repo_root / "out" / "images" / "mattos-x86_64.iso"
    if not iso_path.exists():
        raise RepoError(f"ISO not found at {iso_path}; build step did not produce expected artifact")
    if not shutil.which("xorriso"):
        raise RepoError("xorriso is required to validate the MattOS live-root ISO layout")
    inspection = subprocess.run(
        [
            "xorriso",
            "-indev",
            str(iso_path),
            "-ls",
            "/live/rootfs.squashfs",
        ],
        cwd=str(repo_root),
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    inspection_output = inspection.stdout + inspection.stderr
    if inspection.returncode != 0 or "/live/rootfs.squashfs" not in inspection_output:
        raise RepoError(
            f"ISO at {iso_path} does not contain the MattOS compressed live root; "
            "rebuild it without --no-build"
        )
    return iso_path


def image_build_commands(clean: bool) -> List[List[str]]:
    commands: List[List[str]] = []
    if clean:
        commands.append(["cargo", "run", "-p", "mattos-build", "--", "clean", "artifacts"])
    # `build all` formally ends with the ISO stage. Calling `image` afterward
    # would rebuild packages, the rootfs, initramfs, and ISO a second time.
    commands.append(["cargo", "run", "-p", "mattos-build", "--", "build", "all"])
    return commands


def test_control_paths(repo_root: Path, args: argparse.Namespace) -> tuple[Path, Path] | None:
    """Prepare local control sockets for tests and installed-disk lifecycle checks.

    Keeping the control endpoint under one repository-owned directory makes
    cleanup deterministic and prevents a command-line path typo from deleting
    an unrelated host socket. QEMU creates the socket itself once it starts.
    """
    lifecycle_control = getattr(args, "install", False) or getattr(args, "run_installed", False)
    if not (getattr(args, "test_control", False) or lifecycle_control):
        if getattr(args, "qmp_socket", None) is not None:
            raise RepoError("--qmp-socket requires --test-control, --install, or --run-installed")
        return None
    if args.headless:
        raise RepoError("--test-control requires a graphical QEMU run, not --headless")

    control_root = (repo_root / TEST_CONTROL_DIRECTORY_RELATIVE).resolve()
    requested = args.qmp_socket or (control_root / TEST_CONTROL_QMP_SOCKET_NAME)
    socket_path = requested if requested.is_absolute() else repo_root / requested
    socket_path = socket_path.resolve()
    try:
        socket_path.relative_to(control_root)
    except ValueError as exc:
        raise RepoError(f"test-control QMP socket must stay under {control_root}") from exc
    if socket_path.name == "." or socket_path == control_root:
        raise RepoError("test-control QMP socket must name a socket file")
    serial_path = control_root / TEST_CONTROL_SERIAL_SOCKET_NAME
    if socket_path == serial_path:
        raise RepoError("test-control QMP and serial sockets must use different paths")
    if not args.dry_run:
        socket_path.parent.mkdir(parents=True, exist_ok=True)
        for path in (socket_path, serial_path):
            if path.exists() or path.is_symlink():
                if path.is_dir():
                    raise RepoError(f"refusing to replace test-control directory: {path}")
                path.unlink()
    return socket_path, serial_path


def test_control_socket(repo_root: Path, args: argparse.Namespace) -> Path | None:
    """Compatibility wrapper for tests and callers needing only the QMP path."""
    paths = test_control_paths(repo_root, args)
    return paths[0] if paths is not None else None


def cleanup_test_control_paths(paths: tuple[Path, Path] | None) -> None:
    """Remove only the QMP/serial paths selected by ``test_control_paths``."""
    if paths is None:
        return
    for socket_path in paths:
        try:
            if socket_path.exists() or socket_path.is_symlink():
                if socket_path.is_socket() or socket_path.is_symlink() or socket_path.is_file():
                    socket_path.unlink()
        except OSError as exc:
            print(f"[qemu] warning: could not remove test-control socket {socket_path}: {exc}", file=sys.stderr)


def cleanup_test_control_socket(socket_path: Path | None) -> None:
    """Compatibility wrapper that removes one formerly QMP-only socket."""
    if socket_path is not None:
        cleanup_test_control_paths((socket_path, socket_path.with_name(".unused")))


def install_completion_marker(repo_root: Path) -> Path:
    return (repo_root / INSTALL_TEST_COMPLETION_RELATIVE).resolve()


def install_task_log(repo_root: Path) -> Path:
    return (repo_root / INSTALL_TEST_LOG_RELATIVE).resolve()


def invalidate_install_completion(repo_root: Path) -> None:
    """Remove only the dedicated success marker before a destructive install."""
    marker = install_completion_marker(repo_root)
    if marker.exists() or marker.is_symlink():
        if marker.is_dir():
            raise RepoError(f"refusing to remove install completion directory: {marker}")
        marker.unlink()


def write_install_completion(repo_root: Path, disk: Path, verification: dict[str, object]) -> Path:
    """Publish completion metadata only after an independent UEFI disk boot."""
    marker = install_completion_marker(repo_root)
    marker.parent.mkdir(parents=True, exist_ok=True)
    payload = {
        "schema": 1,
        "disk": str(disk.resolve()),
        "virtual_size": qemu_disk_virtual_size(disk),
        "verified_at": datetime.now(UTC).isoformat(),
        "verification": verification,
    }
    temporary = marker.with_name(f".{marker.name}.building-{os.getpid()}")
    temporary.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    temporary.replace(marker)
    return marker


def qemu_disk_virtual_size(disk: Path) -> int:
    if not shutil.which("qemu-img"):
        raise RepoError("qemu-img is required to validate the installed test disk")
    completed = subprocess.run(
        ["qemu-img", "info", "--output=json", str(disk)],
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if completed.returncode != 0:
        raise RepoError(f"qemu-img could not inspect installed test disk {disk}: {completed.stderr.strip()}")
    try:
        value = json.loads(completed.stdout)
        size = value["virtual-size"]
    except (KeyError, TypeError, ValueError, json.JSONDecodeError) as exc:
        raise RepoError(f"qemu-img returned invalid metadata for installed test disk {disk}") from exc
    if not isinstance(size, int) or size < 8 * 1024 * 1024 * 1024:
        raise RepoError(f"installed test disk has an invalid virtual size: {size!r}")
    return size


def validate_completed_install(repo_root: Path, disk: Path) -> dict[str, object]:
    """Refuse disk boots unless a prior real no-ISO boot published success."""
    marker = install_completion_marker(repo_root)
    if not disk.is_file() or not marker.is_file():
        raise RepoError(
            "installed test disk is missing or has not completed installation verification; "
            "run: python3 DevUtils/run_qemu.py --install"
        )
    try:
        payload = json.loads(marker.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        raise RepoError(
            "installed test disk completion metadata is invalid; "
            "run: python3 DevUtils/run_qemu.py --install"
        ) from exc
    if (
        payload.get("schema") != 1
        or payload.get("disk") != str(disk.resolve())
        or not isinstance(payload.get("verification"), dict)
    ):
        raise RepoError(
            "installed test disk completion metadata does not match this disk; "
            "run: python3 DevUtils/run_qemu.py --install"
        )
    if payload.get("virtual_size") != qemu_disk_virtual_size(disk):
        raise RepoError(
            "installed test disk changed after verification; "
            "run: python3 DevUtils/run_qemu.py --install"
        )
    completed = subprocess.run(
        ["qemu-img", "check", "--output=json", str(disk)],
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if completed.returncode != 0:
        raise RepoError(
            "installed test disk failed qemu-img integrity validation; "
            "run: python3 DevUtils/run_qemu.py --install"
        )
    return payload


def prepare_install_disk(repo_root: Path, args: argparse.Namespace) -> Path | None:
    """Return the selected disk, recreating only the explicit --install test disk."""
    if args.no_install_disk:
        return None

    if getattr(args, "install", False):
        if not getattr(args, "dry_run", False):
            invalidate_install_completion(repo_root)
        disk = repo_root / INSTALL_TEST_DISK_RELATIVE
        if not getattr(args, "dry_run", False) and (disk.exists() or disk.is_symlink()):
            if not disk.is_file():
                raise RepoError(f"refusing to replace non-file install test disk: {disk}")
            disk.unlink()
    elif getattr(args, "run_installed", False):
        disk = repo_root / INSTALL_TEST_DISK_RELATIVE
        disk = disk.resolve()
        validate_completed_install(repo_root, disk)
        return disk
    else:
        disk = args.install_disk or (repo_root / DEFAULT_INSTALL_DISK_RELATIVE)
    disk = disk if disk.is_absolute() else repo_root / disk
    disk = disk.resolve()
    disk.parent.mkdir(parents=True, exist_ok=True)
    if not disk.exists() and not getattr(args, "dry_run", False):
        if not shutil.which("qemu-img"):
            raise RepoError("qemu-img is required to create the development install disk")
        print(f"+ qemu-img create -f qcow2 {disk} {DEFAULT_INSTALL_DISK_SIZE}")
        completed = subprocess.run(
            ["qemu-img", "create", "-f", "qcow2", str(disk), DEFAULT_INSTALL_DISK_SIZE],
            cwd=str(repo_root),
            check=False,
            text=True,
        )
        if completed.returncode != 0:
            raise RepoError(f"failed to create QEMU install disk: {disk}")
    if not getattr(args, "dry_run", False) and not disk.is_file():
        raise RepoError(f"QEMU install disk is not a regular file: {disk}")
    return disk


def build_if_needed(repo_root: Path, args: argparse.Namespace) -> None:
    if args.no_build or getattr(args, "run_installed", False) and not getattr(args, "install", False):
        return

    build_environment = mattos_build_environment(repo_root)

    # Fail fast on missing or broken toolchain prerequisites before expensive builds.
    try:
        run_command(
            ["cargo", "run", "-p", "mattos-build", "--", "doctor"],
            cwd=repo_root,
            dry_run=args.dry_run,
            env=build_environment,
        )
    except RepoError as exc:
        raise RepoError(
            "mattos-build doctor reported missing or broken prerequisites. "
            "Run: python3 DevUtils/setup.py"
        ) from exc

    for command in image_build_commands(args.clean):
        run_command(command, cwd=repo_root, dry_run=args.dry_run, env=build_environment)


def _terminate_task_vm(proc: subprocess.Popen[str], control_paths: tuple[Path, Path] | None, reason: str) -> int:
    """Stop only the QEMU process this launcher started, with bounded cleanup."""
    if proc.poll() is not None:
        return proc.returncode or 0
    print(f"[qemu] {reason}; requesting guest shutdown")
    if control_paths is not None:
        try:
            with QmpClient(control_paths[0].resolve(), 10.0) as qmp:
                qmp.execute("system_powerdown")
        except (OSError, QmpError) as exc:
            print(f"[qemu] warning: QMP shutdown request failed: {exc}", file=sys.stderr)
    try:
        return proc.wait(timeout=INSTALL_SHUTDOWN_SECONDS)
    except subprocess.TimeoutExpired:
        print("[qemu] guest did not shut down cleanly; terminating task-owned QEMU", file=sys.stderr)
        proc.terminate()
        try:
            return proc.wait(timeout=15)
        except subprocess.TimeoutExpired:
            proc.kill()
            return proc.wait(timeout=10)


def _launch_one(
    repo_root: Path,
    iso_path: Path | None,
    args: argparse.Namespace,
    *,
    boot_iso: bool,
    install_disk: Path | None = None,
    lifecycle: Callable[[subprocess.Popen[str], tuple[Path, Path]], None] | None = None,
) -> int:
    logs_dir = repo_root / "out" / "logs"
    if not args.dry_run:
        logs_dir.mkdir(parents=True, exist_ok=True)

    acceleration_args, acceleration_status = acceleration_selection(getattr(args, "no_kvm", False))
    report_launch_configuration(args, acceleration_status)

    qemu_cmd: List[str] = [
        "qemu-system-x86_64",
        *acceleration_args,
        *uefi_firmware_arguments(),
        "-m",
        str(args.memory),
        "-smp",
        str(args.cpus),
    ]
    if boot_iso:
        if iso_path is None:
            raise RepoError("installation-media boot requires an ISO")
        qemu_cmd.extend([
            "-drive",
            f"file={iso_path},if=none,id=mattos-cd,media=cdrom,readonly=on",
            "-device",
            "virtio-scsi-pci,id=mattos-scsi",
            "-device",
            "scsi-cd,drive=mattos-cd,bus=mattos-scsi.0,bootindex=1",
            "-boot",
            "d",
        ])
    else:
        qemu_cmd.extend(["-boot", "order=c"])
    control_paths = test_control_paths(repo_root, args)
    if control_paths is not None:
        control_socket, control_serial = control_paths
        qemu_cmd.extend(["-qmp", f"unix:{control_socket},server=on,wait=off"])
        qemu_cmd.extend(
            [
                "-chardev",
                f"socket,id=mattos-test-serial,path={control_serial},server=on,wait=off,signal=off",
                "-serial",
                "chardev:mattos-test-serial",
            ]
        )
        print(
            f"[qemu] test control: QMP socket {control_socket} "
            f"and serial socket {control_serial} (DevUtils/qemu_test_control.py)"
        )
    if not args.headless:
        # The native COSMIC installer is a DRM/KMS Wayland client session.
        # Normal graphical runs deliberately use virtio-vga-gl and VirGL.
        # QEMU cannot expose a screendump surface for that GL display, so the
        # explicitly opt-in test-control mode uses a conventional VGA surface
        # that QMP can capture. This does not change normal graphical runs.
        # The normal installed-user workflow also gets a private QMP/serial
        # control channel so the launcher can be validated and shut down
        # cleanly, but it must retain the real VirGL GPU.  Only the explicit
        # screenshot/test mode (and the non-user install lifecycle) switches
        # to the QMP-capturable conventional VGA surface.
        use_capturable_gpu = getattr(args, "test_control", False) or getattr(args, "install", False)
        gpu_device = "virtio-vga" if use_capturable_gpu else graphical_gpu_device(repo_root)
        qemu_cmd.extend(["-device", gpu_device])
        # A graphical desktop guest needs an absolute pointer. The default
        # PS/2 mouse is relative-only and did not produce usable pointer
        # motion in the COSMIC KMS session. One USB tablet is sufficient.
        qemu_cmd.extend(["-device", QEMU_TABLET_CONTROLLER, "-device", QEMU_TABLET_DEVICE])
    if install_disk is None:
        install_disk = prepare_install_disk(repo_root, args)
    if install_disk is not None:
        qemu_cmd.extend(["-drive", f"file={install_disk},if=virtio,format=qcow2"])
    qemu_cmd.extend(network_arguments(args.no_network))

    if args.serial_console:
        # Keep the graphical backend alive for the GL-only virtio-vga device
        # while routing the guest's serial console to the invoking terminal.
        # `-nographic` forcibly disables that backend and therefore cannot be
        # combined with the normal COSMIC GPU configuration.
        display = choose_graphical_display(repo_root)
        if display == "default":
            qemu_cmd.extend(["-display", "default"])
        else:
            qemu_cmd.extend(["-display", display])
        graphics_label = "virtio-vga with QMP-capturable surface" if use_capturable_gpu else f"{VIRTIO_GPU_GL_DEVICE} (VirGL requested)"
        print(f"[qemu] graphics: {graphics_label} with display={display}")
        if control_paths is None:
            qemu_cmd.extend(["-serial", "stdio"])
        qemu_cmd.extend(["-monitor", "none", "-no-reboot"])
        if lifecycle is None:
            qemu_cmd.append("-no-shutdown")
    elif args.headless:
        qemu_cmd.extend(["-nographic", "-serial", "stdio", "-monitor", "none", "-no-reboot"])
        if lifecycle is None:
            qemu_cmd.append("-no-shutdown")
    else:
        use_capturable_gpu = getattr(args, "test_control", False) or getattr(args, "install", False)
        display = "gtk,gl=off" if use_capturable_gpu else choose_graphical_display(repo_root)
        if display == "default":
            qemu_cmd.extend(["-display", "default"])
        else:
            qemu_cmd.extend(["-display", display])
        graphics_label = "virtio-vga with QMP-capturable surface" if use_capturable_gpu else f"{VIRTIO_GPU_GL_DEVICE} (VirGL requested)"
        print(f"[qemu] graphics: {graphics_label} with display={display}")
        if control_paths is None:
            qemu_cmd.extend(["-serial", f"file:{logs_dir / 'qemu-serial.log'}"])
        if lifecycle is None:
            qemu_cmd.append("-no-shutdown")

    for extra in args.qemu_arg:
        qemu_cmd.append(extra)

    print("+", " ".join(qemu_cmd))
    if args.dry_run:
        return 0

    proc: subprocess.Popen[str] | None = None
    try:
        proc = subprocess.Popen(qemu_cmd, cwd=str(repo_root), env=mattos_build_environment(repo_root))
        if lifecycle is not None:
            if control_paths is None:
                raise RepoError("noninteractive QEMU lifecycle requires test-control sockets")
            lifecycle(proc, control_paths)
            return _terminate_task_vm(proc, control_paths, "noninteractive lifecycle completed")
        return proc.wait()
    except KeyboardInterrupt:
        print("\nInterrupted, terminating QEMU...")
        if proc is None:
            raise
        proc.send_signal(signal.SIGINT)
        try:
            return proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            return _terminate_task_vm(proc, control_paths, "interrupted")
    except Exception:
        if proc is not None and proc.poll() is None:
            _terminate_task_vm(proc, control_paths, "lifecycle failed")
        raise
    finally:
        cleanup_test_control_paths(control_paths)


TEST_INSTALL_PASSWORD_HASH = "$6$mattos$gNJjrFx.MYr9CTiHVOBlhO.TEvrlR9qInoPDSU2Gdy8X8M7knkYWQnK9XOJH2alPkUhn2eswpESNckixqbhpD/"


def _test_install_plan() -> str:
    return "\n".join([
        'version = 6', 'target_disk = "/dev/vda"',
        'storage = { mode = "guided_whole_disk", filesystem = "btrfs", efi = { policy = "create" } }',
        'installed_profile = "desktop"', 'optional_packages = ["firefox"]', 'hostname = "mattos-test"',
        'full_name = "MattOS Test User"', 'username = "mattos"',
        f'password_hash = "{TEST_INSTALL_PASSWORD_HASH}"', 'administrator = true',
        'automatic_login = true', 'root_credential = { mode = "same_as_user" }',
        'locale = "en_US.UTF-8"', 'keyboard_layout = "us"', 'keyboard_variant = ""',
        'timezone = "Etc/UTC"', 'test_autologin = true', '',
    ])


def _run_automatic_test_install(repo_root: Path, disk: Path, control_paths: tuple[Path, Path]) -> None:
    """Run the real installer with streamed, persistent, progress-aware logs."""
    wait_for_socket(control_paths[0], 120)
    log_path = install_task_log(repo_root)
    log_path.parent.mkdir(parents=True, exist_ok=True)
    log_path.write_text("MattOS automated installation log\n", encoding="utf-8")
    encoded = base64.b64encode(_test_install_plan().encode()).decode()
    command = (
        f"printf '%s' {encoded} | base64 -d > /tmp/mattos-test-plan.toml && "
        "sudo mattos-install install /tmp/mattos-test-plan.toml --yes-really-erase"
    )
    last_disk_state: tuple[int, int] | None = None

    def disk_progress() -> bool:
        nonlocal last_disk_state
        try:
            stat = disk.stat()
            state = (stat.st_size, stat.st_mtime_ns)
        except FileNotFoundError:
            return False
        changed = state != last_disk_state
        last_disk_state = state
        return changed

    def stream(chunk: str) -> None:
        print(chunk, end="", flush=True)
        with log_path.open("a", encoding="utf-8") as log:
            log.write(chunk)
            log.flush()

    disk_progress()
    try:
        serial_command_stream(
            control_paths[1].resolve(),
            command,
            INSTALL_PROGRESS_IDLE_SECONDS,
            on_output=stream,
            progress_probe=disk_progress,
        )
    except Exception as exc:
        raise RepoError(
            f"automated MattOS installation failed; full guest log: {log_path}; error: {exc}"
        ) from exc


def _verify_installed_disk_boot(
    repo_root: Path,
    disk: Path,
    args: argparse.Namespace,
) -> dict[str, object]:
    """Boot the target with no ISO and prove its real UEFI/GRUB path works."""
    verification_args = argparse.Namespace(**vars(args))
    verification_args.install = False
    verification_args.run_installed = False
    verification_args.test_control = True
    verification_args.serial_console = False
    verification_args.headless = False
    output_path = install_task_log(repo_root).with_name("installed-test-boot.log")
    output_path.write_text("MattOS installed-disk boot verification log\n", encoding="utf-8")
    # The QEMU serial backend is a UART, not a bulk shell-command transport.
    # A previous verifier injected every assertion as one multi-kilobyte
    # terminal line; its echo could be truncated before the shell received a
    # newline, leaving the VM healthy but the launcher waiting forever.  Keep
    # each assertion independently framed and short.  This does not weaken
    # verification: every check still has to exit zero, but it makes the
    # transport contract explicit and lets the log identify the failed probe.
    #
    # The ESP is mounted after the installed initramfs has already handed
    # control to the mounted root.  The authoritative proof of its fallback
    # loader/config is therefore the observed UEFI -> GRUB boot, while the
    # installed root-side GRUB configuration is directly inspectable here.
    checks = (
        ("not-live", "test ! -e /run/mattos-live"),
        ("profile-file", "test -f /etc/mattos-installed-profile"),
        ("profile-value", "test \"$(cat /etc/mattos-installed-profile)\" = desktop"),
        ("hostname", "test \"$(cat /etc/hostname)\" = mattos-test"),
        ("user", "id mattos"),
        ("uefi", "test -d /sys/firmware/efi"),
        ("efi-partition", "test -b /dev/vda1"),
        ("root-partition", "test -b /dev/vda2"),
        ("kernel", "test -f /boot/vmlinuz"),
        ("initramfs", "test -f /boot/installed-initramfs.cpio.xz"),
        ("grub-config", "test -f /boot/grub/grub.cfg"),
        ("grub-menu", "grep -q \"menuentry 'MattOS'\" /boot/grub/grub.cfg"),
        ("root-mount", "findmnt -no SOURCE /"),
        ("efi-mount", "findmnt -no SOURCE /boot/efi"),
        ("gpt", "lsblk -no PTTYPE /dev/vda"),
        ("graphical-target", "systemctl is-active graphical.target"),
        ("mattos-repository", "grep -q '^Enabled: yes' /etc/apt/sources.list.d/mattos-hosted.sources"),
    )
    result: dict[str, object] = {}

    def lifecycle(_proc: subprocess.Popen[str], control_paths: tuple[Path, Path]) -> None:
        wait_for_socket(control_paths[0], 120)

        def stream(chunk: str) -> None:
            print(chunk, end="", flush=True)
            with output_path.open("a", encoding="utf-8") as log:
                log.write(chunk)

        completed_checks: list[str] = []
        for name, command in checks:
            output = serial_command_stream(
                control_paths[1].resolve(),
                command,
                INSTALL_BOOT_IDLE_SECONDS,
                on_output=stream,
            )
            stream(f"[installed-check] PASS {name}\n")
            completed_checks.append(output)
        result.update({"uefi_grub_boot": True, "serial_checks": "".join(completed_checks)})

    exit_code = _launch_one(
        repo_root,
        None,
        verification_args,
        boot_iso=False,
        install_disk=disk,
        lifecycle=lifecycle,
    )
    if exit_code != 0:
        raise RepoError(f"installed-disk verification VM exited with status {exit_code}")
    return result


def launch_qemu(repo_root: Path, iso_path: Path | None, args: argparse.Namespace) -> int:
    """Launch QEMU, with --install publishing a marker only after real boot proof."""
    if not getattr(args, "install", False):
        return _launch_one(repo_root, iso_path, args, boot_iso=not getattr(args, "run_installed", False))
    disk = prepare_install_disk(repo_root, args)
    assert disk is not None
    if args.dry_run:
        # A dry run must never create completion state or claim that a disk
        # booted: it only renders the installation-media QEMU invocation.
        return _launch_one(repo_root, iso_path, args, boot_iso=True, install_disk=disk)
    try:
        result = _launch_one(
            repo_root,
            iso_path,
            args,
            boot_iso=True,
            install_disk=disk,
            lifecycle=lambda _proc, paths: _run_automatic_test_install(repo_root, disk, paths),
        )
        if result != 0:
            raise RepoError(f"installer VM exited with status {result}")
        verification = _verify_installed_disk_boot(repo_root, disk, args)
        marker = write_install_completion(repo_root, disk, verification)
        print(f"[qemu] installed disk validated: {disk} (completion marker: {marker})")
    except Exception:
        invalidate_install_completion(repo_root)
        if disk.exists() and disk.is_file():
            disk.unlink()
        raise
    if not args.run_installed:
        return 0
    installed_args = argparse.Namespace(**vars(args))
    installed_args.install = False
    installed_args.run_installed = True
    installed_args.test_control = False
    return _launch_one(repo_root, None, installed_args, boot_iso=False, install_disk=disk)


def main() -> int:
    args = parse_args()

    script_path = Path(__file__).resolve()
    repo_root = find_repo_root(script_path.parent)

    required = ["cargo", "qemu-system-x86_64"]
    ensure_tools(required)

    if not shutil.which("python3"):
        raise RepoError("python3 not available")

    build_if_needed(repo_root, args)

    if args.build_only:
        if not args.dry_run:
            ensure_iso_exists(repo_root)
        return 0

    if args.dry_run:
        iso_path = repo_root / "out" / "images" / "mattos-x86_64.iso"
    elif args.run_installed and not getattr(args, "install", False):
        iso_path = None
    else:
        iso_path = ensure_iso_exists(repo_root)

    return launch_qemu(repo_root, iso_path, args)


if __name__ == "__main__":
    try:
        sys.exit(main())
    except RepoError as exc:
        print(f"error: {exc}", file=sys.stderr)
        sys.exit(1)
