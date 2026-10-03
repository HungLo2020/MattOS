#!/usr/bin/env python3
import argparse
import base64
import functools
import hashlib
import http.server
import json
import threading
import tomllib
import os
import re
import shlex
import shutil
import signal
import subprocess
import sys
import time
from datetime import UTC, datetime
from pathlib import Path
from collections.abc import Callable
from typing import List

from qemu_test_control import (
    QmpClient,
    QmpError,
    click,
    serial_command,
    serial_command_stream,
    send_key,
    type_text,
    wait_for_socket,
)

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
UPGRADE_TEST_DISK_RELATIVE = Path("out/qemu/upgrade-test.qcow2")
UPGRADE_BASELINE_RELATIVE = Path("out/images/upgrade-baseline/mattos-x86_64.iso")
UPGRADE_REPORT_RELATIVE = Path("out/logs/upgrade-test.json")
UPGRADE_LOG_RELATIVE = Path("out/logs/upgrade-test.log")
UPGRADE_TEST_SOURCE = "/etc/apt/sources.list.d/zz-mattos-upgrade-test.sources"
# QEMU user networking reaches the host's loopback at this address.
QEMU_HOST_ADDRESS = "10.0.2.2"
UPGRADE_SUMMARY = re.compile(
    r"(?P<upgraded>\d+) upgraded, (?P<installed>\d+) newly installed, "
    r"(?P<removed>\d+) to remove and (?P<held>\d+) not upgraded"
)
DEFAULT_INSTALL_DISK_SIZE = "16G"
DEFAULT_VM_MEMORY_MIB = 6144
DEFAULT_VM_CPUS = 4
# `virtio-vga-gl` is the one GPU that satisfies both development-launcher
# requirements: its VGA compatibility gives firmware/GRUB a scanout before
# Linux starts, and its VirtIO GL backend exposes the VirGL capset used later
# by Mesa and the native Plasma compositor.
# QEMU's virtio-vga-gl defaults to a 1280×800 firmware scanout. Do not pass
# xres/yres explicitly: they are only firmware hints, not a Wayland policy,
# and leaving the device defaults intact lets KWin select the
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
    parser.add_argument("--live-media", choices=("optical", "usb", "nvme", "sata"),
                        default="optical", help="ISO transport (USB/NVMe/SATA use raw disk bytes)")
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
        help="recreate an installed-test disk and install the selected MattOS profile",
    )
    parser.add_argument(
        "--install-profile",
        choices=("cli", "plasma"),
        default="plasma",
        help="installed-system profile used by --install (default: plasma)",
    )
    parser.add_argument(
        "--upgrade-test",
        action="store_true",
        help=(
            "install the baseline ISO (--upgrade-from) on a separate disk, upgrade it with apt to the "
            "current build's repository, reboot and rerun the installed-system checks"
        ),
    )
    parser.add_argument(
        "--upgrade-from",
        type=Path,
        metavar="ISO",
        help=f"baseline ISO for --upgrade-test (default: {UPGRADE_BASELINE_RELATIVE})",
    )
    parser.add_argument(
        "--save-upgrade-baseline",
        action="store_true",
        help=f"keep the current ISO as the --upgrade-test baseline ({UPGRADE_BASELINE_RELATIVE})",
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
    if args.run_installed and args.no_install_disk:
        parser.error("--run-installed requires the dedicated installed-test.qcow2 disk")
    if args.install and args.no_install_disk:
        parser.error("--install requires the dedicated installed-test.qcow2 disk")
    if args.install and args.no_build:
        parser.error("--install always builds the canonical current ISO; omit --no-build")
    if args.run_installed and args.clean and not args.install:
        parser.error("--run-installed never builds; --clean is not applicable")
    if args.upgrade_test and (args.install or args.run_installed or args.build_only or args.no_install_disk):
        parser.error("--upgrade-test cannot be combined with --install, --run-installed, --build-only or --no-install-disk")
    if args.upgrade_from and not args.upgrade_test:
        parser.error("--upgrade-from requires --upgrade-test")
    if args.save_upgrade_baseline and (args.upgrade_test or args.install or args.run_installed):
        parser.error("--save-upgrade-baseline only saves the current ISO; run the test separately")
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
    # built virgl driver needs for the KWin KMS path.
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
    path that the native Plasma compositor requires.  Fail closed instead of
    silently booting a graphical installer configuration that cannot render.
    """
    try:
        output = run_command_capture(["qemu-system-x86_64", "-device", "help"], cwd=repo_root)
    except RepoError as exc:
        raise RepoError("could not inspect QEMU VirtIO GPU support") from exc
    if "virtio-vga-gl" not in output:
        raise RepoError(
            "this QEMU lacks virtio-vga-gl; the native Plasma session "
            "requires a VGA-compatible QEMU VirtIO GPU with GL/VirGL support"
        )
    # VirGL alone provides an EGL context, but KWin's KMS renderer also
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


def install_completion_marker(repo_root: Path, disk: Path | None = None) -> Path:
    if disk is None or disk.resolve() == (repo_root / INSTALL_TEST_DISK_RELATIVE).resolve():
        return (repo_root / INSTALL_TEST_COMPLETION_RELATIVE).resolve()
    return disk.resolve().with_suffix(disk.suffix + ".verified.json")


def install_task_log(repo_root: Path) -> Path:
    return (repo_root / INSTALL_TEST_LOG_RELATIVE).resolve()


def invalidate_install_completion(repo_root: Path, disk: Path | None = None) -> None:
    """Remove only the dedicated success marker before a destructive install."""
    marker = install_completion_marker(repo_root, disk)
    if marker.exists() or marker.is_symlink():
        if marker.is_dir():
            raise RepoError(f"refusing to remove install completion directory: {marker}")
        marker.unlink()


def write_install_completion(repo_root: Path, disk: Path, verification: dict[str, object]) -> Path:
    """Publish completion metadata only after an independent UEFI disk boot."""
    marker = install_completion_marker(repo_root, disk)
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
    marker = install_completion_marker(repo_root, disk)
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
        disk = args.install_disk or (repo_root / INSTALL_TEST_DISK_RELATIVE)
        disk = disk if disk.is_absolute() else repo_root / disk
        disk = disk.resolve()
        if not getattr(args, "dry_run", False):
            invalidate_install_completion(repo_root, disk)
        if not getattr(args, "dry_run", False) and (disk.exists() or disk.is_symlink()):
            if not disk.is_file():
                raise RepoError(f"refusing to replace non-file install test disk: {disk}")
            disk.unlink()
    elif getattr(args, "run_installed", False):
        disk = args.install_disk or (repo_root / INSTALL_TEST_DISK_RELATIVE)
        disk = disk if disk.is_absolute() else repo_root / disk
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
            proc.wait(timeout=15)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait(timeout=10)
        # QEMU may return zero after SIGTERM. That is not a guest shutdown
        # and must never publish a successful installation completion marker.
        return 1


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
    if getattr(args, "plasma_login_verification", False):
        # Gives the host-side screenshot path a unique GTK/XWayland window
        # title without changing the guest configuration.
        qemu_cmd.extend(["-name", "mattos-installed-plasma-login-verification"])
    if boot_iso:
        if iso_path is None:
            raise RepoError("installation-media boot requires an ISO")
        media = getattr(args, "live_media", "optical")
        if media != "optical":
            # ATA rejects read-only block backends; OVMF's NVMe boot also
            # issues commands requiring a writable backend. Disposable
            # snapshots model raw-written disks without modifying the ISO.
            protection = "snapshot=on" if media in ("sata", "nvme") else "readonly=on"
            qemu_cmd.extend(["-drive", f"file={iso_path},format=raw,if=none,id=mattos-media,{protection}"])
            if media == "usb":
                qemu_cmd.extend(["-device", "qemu-xhci,id=mattos-media-xhci",
                                 "-device", "usb-storage,drive=mattos-media,bus=mattos-media-xhci.0,bootindex=1"])
            elif media == "nvme":
                qemu_cmd.extend(["-device", "nvme,drive=mattos-media,serial=MATTOSLIVE,bootindex=1"])
            else:
                qemu_cmd.extend(["-device", "ich9-ahci,id=mattos-media-ahci",
                                 "-device", "ide-hd,drive=mattos-media,bus=mattos-media-ahci.0,bootindex=1"])
        else:
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
        # The native Plasma session is a DRM/KMS Wayland client session.
        # Normal graphical runs deliberately use virtio-vga-gl and VirGL.
        # QEMU cannot expose a screendump surface for that GL display, so the
        # explicitly opt-in test-control mode uses a conventional VGA surface
        # that QMP can capture. This does not change normal graphical runs.
        # The normal installed-user workflow also gets a private QMP/serial
        # control channel so the launcher can be validated and shut down
        # cleanly, but it must retain the real VirGL GPU.  Only the explicit
        # screenshot/test mode (and the non-user install lifecycle) switches
        # to the QMP-capturable conventional VGA surface.
        use_capturable_gpu = (
            (getattr(args, "test_control", False) or getattr(args, "install", False))
            and not getattr(args, "require_virgl", False)
        )
        gpu_device = "virtio-vga" if use_capturable_gpu else graphical_gpu_device(repo_root)
        qemu_cmd.extend(["-device", gpu_device])
        # A graphical desktop guest needs an absolute pointer. The default
        # PS/2 mouse is relative-only and did not produce usable pointer
        # motion in the Plasma KMS session. One USB tablet is sufficient.
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
        # combined with the normal Plasma GPU configuration.
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
        use_capturable_gpu = (
            (getattr(args, "test_control", False) or getattr(args, "install", False))
            and not getattr(args, "require_virgl", False)
        )
        if use_capturable_gpu:
            display = "gtk,gl=off"
        elif getattr(args, "plasma_login_verification", False):
            # GTK follows the host's native Wayland backend, whose surface is
            # not visible to the X11-only screenshot helper below. SDL's X11
            # window keeps the guest GL/VirGL path while making the named
            # verifier window capturable under both X11 and Wayland hosts.
            display = "sdl,gl=on"
        else:
            display = choose_graphical_display(repo_root)
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
        process_environment = mattos_build_environment(repo_root)
        if getattr(args, "plasma_login_verification", False):
            # The screenshot helper needs the SDL surface on X11/Xwayland;
            # constrain only QEMU's host-side UI backend. Guest GL is still
            # provided by the same VirGL device.
            process_environment["SDL_VIDEODRIVER"] = "x11"
        proc = subprocess.Popen(qemu_cmd, cwd=str(repo_root), env=process_environment)
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


# Credentials belong only to the disposable --install fixture, not normal
# installer policy. Authenticate its menu inspection without relaxing GRUB's
# upstream root-only grub.cfg permissions or installed sudo policy.
TEST_INSTALL_PASSWORD = "mattos"
TEST_INSTALL_PASSWORD_HASH = "$6$mattos$gNJjrFx.MYr9CTiHVOBlhO.TEvrlR9qInoPDSU2Gdy8X8M7knkYWQnK9XOJH2alPkUhn2eswpESNckixqbhpD/"


def installed_grub_menu_probe() -> str:
    return (
        f"printf '%s\\n' {shlex.quote(TEST_INSTALL_PASSWORD)} | sudo -S grep -q "
        "\"menuentry 'MattOS GNU/Linux'\" /boot/grub/grub.cfg && "
        "sudo -n grep -q 'Advanced options for MattOS' /boot/grub/grub.cfg"
    )


def installed_plasma_greeter_process_probe() -> str:
    """Wait for PLM's greeter helper before checking compositor readiness."""
    return (
        "systemctl is-active --quiet plasmalogin.service && "
        "loginctl --no-pager show-user plasmalogin -p Sessions --value | grep -q '[^[:space:]]' && "
        "pgrep -u plasmalogin -f startplasma-login-wayland >/dev/null"
    )


def installed_plasma_wayland_socket_probe() -> str:
    """Check the greeter socket as root because /run/user/<uid> is mode 0700."""
    return (
        f"printf '%s\\n' {shlex.quote(TEST_INSTALL_PASSWORD)} | "
        "sudo -S -p '' test -S /run/user/$(id -u plasmalogin)/wayland-0"
    )


def installed_plasma_greeter_probe() -> str:
    """Require PLM's greeter session, KWin compositor, and Wayland socket."""
    return (
        f"{installed_plasma_greeter_process_probe()} && "
        "pgrep -u plasmalogin -x kwin_wayland >/dev/null && "
        f"{installed_plasma_wayland_socket_probe()}"
    )


def wake_installed_plasma_greeter(qmp_socket: Path) -> None:
    """Wake PLM's intentionally idle-hidden login controls with real input."""
    with QmpClient(qmp_socket, 10) as qmp:
        send_key(qmp, "shift")
    # Let the upstream greeter's wake animation finish before taking evidence.
    time.sleep(0.4)


def submit_plasma_greeter_password(qmp: QmpClient, password: str) -> None:
    """Replace the focused greeter password, then submit it through QMP.

    Plasma Login Manager leaves the password entry focused after a rejected
    attempt. Typing a subsequent credential without clearing that field
    appends it to the rejected password, making a correct account password
    fail PAM authentication. Explicitly select and delete the old contents.
    """
    send_key(qmp, "ctrl-a")
    send_key(qmp, "backspace")
    time.sleep(0.2)
    type_text(qmp, password)
    send_key(qmp, "ret")


def installed_plasma_user_desktop_absent_probe(username: str = "mattos") -> str:
    """Ensure an unauthenticated/logged-out user has no Plasma session."""
    return (
        f"! pgrep -u {shlex.quote(username)} -x kwin_wayland >/dev/null && "
        f"! pgrep -u {shlex.quote(username)} -x plasmashell >/dev/null"
    )


def installed_toolchain_probe() -> str:
    """Toolchain commands present, and GCC builds a program that runs."""
    commands = " ".join(("gcc", "g++", "cpp", "ld", "make", "clang", "lld", "rustc", "cargo"))
    return (
        "( dpkg-query -W -f='${db:Status-Status}' mattos-toolchain | grep -qx installed && "
        f"for tool in {commands}; do command -v $tool >/dev/null || {{ echo missing $tool; exit 1; }}; done && "
        "printf 'int main(void) { return 42; }\\n' > /tmp/mattos-toolchain-check.c && "
        "gcc /tmp/mattos-toolchain-check.c -o /tmp/mattos-toolchain-check && "
        "{ /tmp/mattos-toolchain-check; test $? -eq 42; } )"
    )


def installed_runtime_versions_probe() -> str:
    """Running kernel, OpenSSH, OpenSSL and sudo-rs match their installed packages."""
    return (
        "r=$(uname -r) && dpkg-query -W -f='${Status}' \"linux-modules-$r\" | grep -q 'ok installed' && "
        "s=$(ssh -V 2>&1) && o=$(dpkg-query -W -f='${Version}' openssh-client) && "
        "l=$(dpkg-query -W -f='${Version}' libssl3t64) && u=$(dpkg-query -W -f='${Version}' mattos-sudo-rs) && "
        "o=${o#*:} && l=${l#*:} && u=${u#*:} && "
        "echo \"$s\" | grep -q \"OpenSSH_${o%%-*}\" && echo \"$s\" | grep -q \"OpenSSL ${l%%-*}\" && "
        "sudo --version 2>&1 | grep -q \"${u%%-*}\""
    )


INSTALLED_HARDENING_PROGRAM = r"""#include <stdio.h>
#include <string.h>
int main(int argc, char **argv) {
    char buffer[64];
    strcpy(buffer, argc > 1 ? argv[1] : "hardened");
    puts(buffer);
    return buffer[0] == 'h' ? 42 : 1;
}
"""

# Each marker in `readelf -aW` proves one default of the shipped compiler:
# PIE, build IDs, CET (IBT and shadow stack), full RELRO with immediate
# binding, the stack protector, and _FORTIFY_SOURCE (a checked strcpy at -O2).
INSTALLED_HARDENING_MARKERS = (
    "Position-Independent Executable",
    "Build ID",
    "IBT, SHSTK",
    "GNU_RELRO",
    "BIND_NOW",
    "__stack_chk_fail",
    "__strcpy_chk",
)


def installed_toolchain_hardening_probe() -> str:
    """The installed gcc builds hardened programs without any extra flags.

    The serial console drops input beyond roughly 1.3 KiB on one line, so the
    markers are checked in a loop over a single readelf dump.
    """
    encoded = base64.b64encode(INSTALLED_HARDENING_PROGRAM.encode()).decode()
    binary = "/tmp/mattos-hardening-check"
    markers = " ".join(f"'{marker}'" for marker in INSTALLED_HARDENING_MARKERS)
    return (
        f"( printf '%s' {encoded} | base64 -d > {binary}.c && "
        f"gcc -O2 {binary}.c -o {binary} && readelf -aW {binary} > {binary}.elf && "
        f"for marker in {markers}; do grep -q \"$marker\" {binary}.elf || "
        f"{{ echo \"missing hardening: $marker\"; exit 1; }}; done && "
        f"{{ {binary}; test $? -eq 42; }} )"
    )


def installed_plasma_logout_command() -> str:
    """Request logout through Plasma's session shutdown service and user bus."""
    return (
        'uid=$(id -u); '
        'test -S "/run/user/$uid/bus" && '
        'env XDG_RUNTIME_DIR="/run/user/$uid" '
        'DBUS_SESSION_BUS_ADDRESS="unix:path=/run/user/$uid/bus" '
        'qdbus org.kde.Shutdown /Shutdown org.kde.Shutdown.logout'
    )


def installed_plasma_applet_runtime_probes() -> tuple[tuple[str, str], ...]:
    """Check dynamically loaded Plasma applet ABIs, not just plasmashell PID.

    Kickoff and Digital Clock are QML/plugin-loaded, so a healthy plasmashell
    process can coexist with those applets silently failing to instantiate.
    Verify the MattOS ICU/PulseAudio providers are installed, their applet
    plugins have no unresolved ELF dependencies, and the current boot journal
    contains no loader failure for the affected applets.
    """
    return (
        (
            "plasma-applet-runtime-libraries",
            "dpkg-query -W libicu78 libpulse0 >/dev/null && "
            "! ldd /usr/lib/x86_64-linux-gnu/qml/org/kde/plasma/private/kicker/libkickerplugin.so | grep -q 'not found' && "
            "! ldd /usr/lib/x86_64-linux-gnu/qml/org/kde/plasma/private/digitalclock/libdigitalclockplugin.so | grep -q 'not found' && "
            "! ldd /usr/lib/x86_64-linux-gnu/qml/org/kde/plasma/private/volume/libplasma-volume-declarative.so | grep -q 'not found'",
        ),
        (
            "plasma-applets-loaded",
            "! printf '%s\\n' mattos | sudo -S -p '' journalctl -b _COMM=plasmashell --no-pager | "
            "grep -Eq 'error when loading applet \\\"org\\.kde\\.plasma\\.(kickoff|digitalclock|pager|volume)\\\"|Cannot load library .*not found'",
        ),
    )


# The KDE applications the Plasma profile ships: (package, executable).
INSTALLED_KDE_APPLICATIONS = (
    ("dolphin", "dolphin"),
    ("kate", "kate"),
    ("plasma-discover", "plasma-discover"),
    ("plasma-systemmonitor", "plasma-systemmonitor"),
    ("konsole", "konsole"),
    ("systemsettings", "systemsettings"),
    ("haruna", "haruna"),
    ("spectacle", "spectacle"),
    ("kcalc", "kcalc"),
    ("gwenview", "gwenview"),
    ("partitionmanager", "partitionmanager"),
    ("kwalletmanager", "kwalletmanager5"),
    ("ark", "ark"),
    ("elisa", "elisa"),
)


def installed_kde_application_probes() -> tuple[tuple[str, str], ...]:
    """Each shipped KDE application is installed, every library it links
    resolves, and it starts far enough to answer --version (offscreen, so the
    probe does not depend on the session it runs beside)."""
    return tuple(
        (
            f"app-{package}",
            f"dpkg-query -W {package} >/dev/null && ! ldd /usr/bin/{executable} | grep -q 'not found' && "
            f"QT_QPA_PLATFORM=offscreen timeout 60 /usr/bin/{executable} --version",
        )
        for package, executable in INSTALLED_KDE_APPLICATIONS
    )


def _test_install_plan(profile: str = "plasma") -> str:
    if profile not in {"cli", "plasma"}:
        raise RepoError(f"unsupported test install profile: {profile}")
    installer_profile = "cli" if profile == "cli" else "desktop"
    optional_packages = "[]" if profile == "cli" else '["firefox"]'
    return "\n".join([
        'version = 6', 'target_disk = "/dev/vda"',
        'storage = { mode = "guided_whole_disk", filesystem = "btrfs", efi = { policy = "create" } }',
        f'installed_profile = "{installer_profile}"', f'optional_packages = {optional_packages}', 'hostname = "mattos-test"',
        'full_name = "MattOS Test User"', 'username = "mattos"',
        f'password_hash = "{TEST_INSTALL_PASSWORD_HASH}"', 'administrator = true',
        # Keep the installed graphical greeter on screen: the serial test
        # login below is deliberately separate from Plasma Login Manager.
        'automatic_login = false', 'root_credential = { mode = "same_as_user" }',
        'locale = "en_US.UTF-8"', 'keyboard_layout = "us"', 'keyboard_variant = ""',
        'timezone = "Etc/UTC"', 'test_autologin = true', '',
    ])


def _capture_plasma_verification_window(
    repo_root: Path,
    destination: Path,
    *,
    timeout: float = 30,
    qmp_socket: Path | None = None,
) -> tuple[int, int]:
    """Wait for a rendered guest frame instead of mistaking startup black for failure."""
    deadline = time.monotonic() + timeout
    while True:
        try:
            return _capture_plasma_verification_window_once(repo_root, destination, timeout=timeout)
        except RepoError as exc:
            if "did not visibly render" not in str(exc) or time.monotonic() >= deadline:
                raise
            if qmp_socket is not None:
                wake_installed_plasma_greeter(qmp_socket)
            else:
                time.sleep(min(0.4, max(0, deadline - time.monotonic())))


def _capture_plasma_verification_window_once(
    repo_root: Path,
    destination: Path,
    *,
    timeout: float,
) -> tuple[int, int]:
    """Capture the rendered QEMU guest window, not a blank SDL secondary head."""
    required = {name: shutil.which(name) for name in ("xwininfo", "import", "identify")}
    missing = [name for name, path in required.items() if path is None]
    if missing:
        raise RepoError(
            "installed Plasma visual verification requires host capture tools "
            f"({', '.join(missing)}); the VirGL display cannot be captured with QMP screendump"
        )
    destination.parent.mkdir(parents=True, exist_ok=True)
    windows = subprocess.run(
        [required["xwininfo"], "-root", "-tree"],
        check=False,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=timeout,
    )
    if windows.returncode != 0:
        raise RepoError(
            f"could not enumerate host QEMU windows: {windows.stderr.strip()}"
        )
    # SDL creates one top-level window per virtual display head. The smaller
    # secondary head can be entirely black and may be the host's active window;
    # choose the largest visible QEMU window belonging to this verifier.
    window_id = _select_installed_qemu_window(windows.stdout)
    if window_id is None:
        raise RepoError(
            "could not find the installed Plasma verification guest window in xwininfo output"
        )
    captured = subprocess.run(
        [required["import"], "-window", window_id, "-silent", str(destination)],
        check=False,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=timeout,
    )
    if captured.returncode != 0 or not destination.is_file():
        raise RepoError(
            "could not capture the installed Plasma guest window "
            f"{window_id}: {captured.stderr.strip()}"
        )
    dimensions = subprocess.run(
        [required["identify"], "-format", "%w %h", str(destination)],
        check=False,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    try:
        width, height = (int(part) for part in dimensions.stdout.split())
    except (ValueError, TypeError):
        raise RepoError(f"captured Plasma verification screenshot has invalid dimensions: {destination}")
    if dimensions.returncode != 0 or width <= 0 or height <= 0:
        raise RepoError(f"captured QEMU screenshot is empty or invalid: {destination}")
    # A capturable QEMU guest window may still contain a completely black
    # surface if KWin failed before opening Wayland. import captures the client
    # area only, so require actual rendered pixel variation directly.
    rendered = subprocess.run(
        [
            required["identify"], "-format", "%[fx:standard_deviation]", str(destination),
        ],
        check=False,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    try:
        variation = float(rendered.stdout.strip())
    except ValueError:
        raise RepoError(
            f"could not measure Plasma greeter rendering in screenshot {destination}: "
            f"{rendered.stderr.strip()}"
        )
    if rendered.returncode != 0 or variation <= 0.01:
        raise RepoError(
            "installed Plasma Login Manager did not visibly render in the QEMU guest "
            f"surface (pixel variation {variation:.5f}); screenshot: {destination}"
        )
    return width, height


# Normalized RMSE between two captures.  A settled desktop varies by less
# than the spinner of the splash screen.  A panel tooltip measures about 0.024
# and the opened launcher about 0.096, so a popup must clear 0.05.
PLASMA_SETTLED_MAX_DIFFERENCE = 0.003
PLASMA_POPUP_MIN_DIFFERENCE = 0.05


def _screenshot_difference(first: Path, second: Path) -> float:
    """Normalized root-mean-square pixel difference of two screenshots."""
    compared = subprocess.run(
        ["compare", "-metric", "RMSE", str(first), str(second), "null:"],
        check=False,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    # compare exits 1 for "images differ"; only 2 is an error.
    match = re.search(r"\(([0-9.eE+-]+)\)", compared.stderr)
    if compared.returncode > 1 or match is None:
        raise RepoError(f"could not compare {first} and {second}: {compared.stderr.strip()}")
    return float(match.group(1))


def _capture_settled_plasma_desktop(
    repo_root: Path,
    destination: Path,
    *,
    qmp_socket: Path,
    timeout: float = 90,
) -> tuple[int, int]:
    """Capture the guest once two frames 1.5 s apart are effectively identical."""
    probe = destination.with_name(destination.stem + "-settle-probe.png")
    deadline = time.monotonic() + timeout
    try:
        size = _capture_plasma_verification_window(repo_root, destination, qmp_socket=qmp_socket)
        while True:
            time.sleep(1.5)
            _capture_plasma_verification_window(repo_root, probe, qmp_socket=qmp_socket)
            difference = _screenshot_difference(destination, probe)
            if difference <= PLASMA_SETTLED_MAX_DIFFERENCE:
                return size
            if time.monotonic() >= deadline:
                raise RepoError(
                    f"the Plasma desktop never settled (RMSE {difference:.4f} between frames); "
                    f"screenshot: {destination}"
                )
            probe.replace(destination)
    finally:
        probe.unlink(missing_ok=True)


def _select_installed_qemu_window(tree: str) -> str | None:
    """Select the largest SDL head for this verification VM, not a blank head."""
    pattern = re.compile(
        r'^\s*(0x[0-9a-fA-F]+) "QEMU \(mattos-installed-plasma-login-verification[^\"]*\)".*?\b(\d+)x(\d+)[+-]'
    )
    candidates: list[tuple[int, str]] = []
    for line in tree.splitlines():
        match = pattern.match(line)
        if match:
            window_id, width_text, height_text = match.groups()
            candidates.append((int(width_text) * int(height_text), window_id))
    return max(candidates)[1] if candidates else None


def _run_automatic_test_install(
    repo_root: Path,
    disk: Path,
    control_paths: tuple[Path, Path],
    profile: str = "plasma",
) -> None:
    """Run the real installer with streamed, persistent, progress-aware logs."""
    wait_for_socket(control_paths[0], 120)
    log_path = install_task_log(repo_root)
    log_path.parent.mkdir(parents=True, exist_ok=True)
    log_path.write_text("MattOS automated installation log\n", encoding="utf-8")
    encoded = base64.b64encode(_test_install_plan(profile).encode()).decode()
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
        serial_command_stream(
            control_paths[1].resolve(),
            scheduled_poweroff_command(),
            INSTALL_BOOT_IDLE_SECONDS,
            on_output=stream,
        )
    except Exception as exc:
        raise RepoError(
            f"automated MattOS installation failed; full guest log: {log_path}; error: {exc}"
        ) from exc


def scheduled_poweroff_command(password: str | None = None) -> str:
    """Schedule shutdown after the serial protocol has emitted its result marker."""
    command = "sudo systemd-run --on-active=1s --unit=mattos-test-poweroff systemctl poweroff"
    if password is None:
        return command
    return f"printf '%s\\n' {shlex.quote(password)} | {command.replace('sudo ', 'sudo -S ', 1)}"


def scheduled_reboot_command(password: str | None = None) -> str:
    """Schedule reboot after the serial command's completion marker."""
    command = "sudo systemd-run --on-active=3s --unit=mattos-test-reboot systemctl reboot"
    if password is None:
        return command
    return f"printf '%s\\n' {shlex.quote(password)} | {command.replace('sudo ', 'sudo -S ', 1)}"


def schedule_and_wait_for_guest_reboot(
    qmp_socket: Path,
    serial_socket: Path,
    command: str,
    *,
    timeout: float = 90,
    on_output: Callable[[str], None] | None = None,
) -> dict[str, object]:
    """Schedule a guest reboot and wait for QEMU's actual guest RESET event.

    Do not probe the serial shell between scheduling and RESET: during the
    delay, the old boot's serial login shell is still live and would accept a
    post-boot readiness command that gets killed moments later by reboot.
    """
    with QmpClient(qmp_socket, min(timeout, 10)) as qmp:
        serial_command_stream(
            serial_socket,
            command,
            INSTALL_BOOT_IDLE_SECONDS,
            on_output=on_output,
        )
        event = qmp.wait_for_event("RESET", timeout)
    data = event.get("data", {})
    return {
        "event": event.get("event"),
        "guest": data.get("guest") if isinstance(data, dict) else None,
        "reason": data.get("reason") if isinstance(data, dict) else None,
    }


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
    # The installed graphical target must be verified with the same GL/VirGL
    # capability required by the production Plasma session. The ordinary
    # test-control VGA surface has GL disabled and can make KWin report that
    # neither hardware acceleration nor software rendering is available.
    verification_args.require_virgl = True
    verification_args.plasma_login_verification = getattr(args, "install_profile", "plasma") == "plasma"
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
    profile = getattr(args, "install_profile", "plasma")
    expected_profile = "cli" if profile == "cli" else "plasma"
    profile_checks = (
        (
            "cli-package-boundary",
            "dpkg-query -W mattos-cli >/dev/null 2>&1 && ! dpkg-query -W qt6-base kwin plasma-workspace plasma-desktop greetd plasma-login-manager mattos-plasma-live dolphin konsole systemsettings >/dev/null 2>&1",
        ),
        ("cli-target", "systemctl is-active multi-user.target && ! systemctl is-active graphical.target"),
    ) if profile == "cli" else (
        ("plasma-package", "dpkg-query -W mattos-plasma plasma-login-manager kwin plasma-workspace plasma-desktop dolphin konsole systemsettings && ! dpkg-query -W greetd mattos-plasma-live >/dev/null 2>&1"),
        ("graphical-target", "systemctl is-active graphical.target"),
        (
            "compositor",
            "(for n in $(seq 1 90); do "
            "pgrep -u mattos -x kwin_wayland >/dev/null && pgrep -u mattos -x plasmashell >/dev/null && "
            "systemctl --user is-active --quiet plasma-workspace.target && "
            "test -S \"${XDG_RUNTIME_DIR:-/run/user/$(id -u)}/wayland-0\" && "
            "busctl --user --no-pager list | grep -q '^org.kde.plasmashell[[:space:]]' && exit 0; "
            "sleep 1; done; "
            "echo 'Plasma compositor/session readiness timed out'; "
            "systemctl --no-pager --full status plasmalogin.service; "
            "systemctl --user --no-pager --full status plasma-workspace.target plasma-plasmashell.service; "
            "printf '%s\\n' mattos | sudo -S journalctl -b --no-pager -n 200 -u plasmalogin.service; "
            "exit 1)",
        ),
        *installed_plasma_applet_runtime_probes(),
        *installed_kde_application_probes(),
    )
    checks = (
        ("not-live", "test ! -e /run/mattos-live"),
        ("profile-file", "test -f /etc/mattos-installed-profile"),
        ("profile-value", f"test \"$(cat /etc/mattos-installed-profile)\" = {expected_profile}"),
        ("hostname", "test \"$(cat /etc/hostname)\" = mattos-test"),
        ("user", "id mattos"),
        ("uefi", "test -d /sys/firmware/efi"),
        ("efi-partition", "test -b /dev/vda1"),
        ("root-partition", "test -b /dev/vda2"),
        ("kernel", "test -f /boot/vmlinuz"),
        ("initramfs", "test -f /boot/installed-initramfs.cpio.xz"),
        ("grub-config", "test -f /boot/grub/grub.cfg"),
        ("grub-menu", installed_grub_menu_probe()),
        ("root-mount", "findmnt -no SOURCE /"),
        ("efi-mount", "findmnt -no SOURCE /boot/efi"),
        ("gpt", "lsblk -no PTTYPE /dev/vda"),
        *profile_checks,
        ("local-repository", "grep -q '^Enabled: yes' /etc/apt/sources.list.d/00-mattos-local.sources && test -f /usr/share/mattos/repository/dists/trixie/main/binary-amd64/Packages"),
        ("plasma-upgrade-candidate", "apt-cache show mattos-plasma >/dev/null"),
        # Every installed system carries the native toolchain (mattos-toolchain),
        # and it must actually compile and run a program.
        ("toolchain", installed_toolchain_probe()),
        # The shipped compiler hardens what users build, like MattOS's own builds.
        ("toolchain-hardening", installed_toolchain_hardening_probe()),
        # What runs is what dpkg says is installed: the booted kernel has its
        # modules package, and OpenSSH, its linked OpenSSL and sudo-rs report
        # the installed package versions (no version is hard-coded here).
        ("runtime-matches-packages", installed_runtime_versions_probe()),
        # Modules carry signatures the kernel accepts: a loaded module has a
        # signature (MattOS kmod has no OpenSSL, so modinfo detects it but
        # cannot name the signer), and the kernel, which trusts only the
        # MattOS certificate, recorded no unsigned-module taint (bit 13).
        # modinfo is in /usr/sbin, which a user's PATH does not include.
        # Base tools and the native build tools (mattos-build-essential in
        # mattos-toolchain) launch on the installed system.
        (
            "native-build-tools",
            "test \"$(readlink /usr/bin/sh)\" = dash && sed --version | grep -q 'GNU sed' && "
            "test \"$(echo 2 3 | awk '{print $1 * $2}')\" = 6 && rsync --version >/dev/null && "
            "cmake --version | grep -q 'cmake version 4' && pkg-config --modversion ncursesw >/dev/null",
        ),
        # Autotools (with Libtool), Meson and Ninja build real projects, and
        # Perl describes the installed toolchain rather than the build tree.
        (
            "autotools-meson-ninja",
            "test \"$(perl -MConfig -e 'print $Config{cc}')\" = gcc && d=$(mktemp -d) && cd \"$d\" && "
            "printf 'AC_INIT([t],[1])\\nAM_INIT_AUTOMAKE([foreign])\\nLT_INIT\\nAC_PROG_CC\\n"
            "AC_CONFIG_FILES([Makefile])\\nAC_OUTPUT\\n' > configure.ac && "
            "printf 'lib_LTLIBRARIES = libt.la\\nlibt_la_SOURCES = t.c\\n' > Makefile.am && "
            "echo 'int t(void) { return 1; }' > t.c && autoreconf -fi >/dev/null 2>&1 && "
            "./configure -q && make -s >/dev/null 2>&1 && test -f .libs/libt.so && "
            "mkdir m && cd m && printf \"project('m', 'c')\\nexecutable('m', 'm.c')\\n\" > meson.build && "
            "echo 'int main(void) { return 0; }' > m.c && meson setup b >/dev/null && "
            "ninja -C b >/dev/null && ./b/m",
        ),
        ("no-unsigned-module-taint", "test $(( $(cat /proc/sys/kernel/tainted) & 8192 )) -eq 0"),
        (
            "kernel-modules-signed",
            "grep -q '^btrfs ' /proc/modules && test -n \"$(/usr/sbin/modinfo -F sig_hashalgo btrfs)\"",
        ),
        # The installed policy replaced the live local source rather than
        # listing the embedded repository twice.
        (
            "single-local-source",
            "test \"$(grep -l 'file:/usr/share/mattos/repository' /etc/apt/sources.list.d/*.sources | wc -l)\" -eq 1",
        ),
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
        if profile == "plasma":
            greeter_probe = installed_plasma_greeter_probe()
            user_desktop_absent = installed_plasma_user_desktop_absent_probe()
            wait_for_greeter = (
                f"for n in $(seq 1 90); do {greeter_probe} && "
                f"{user_desktop_absent} && exit 0; sleep 1; done; exit 1"
            )
            serial_command_stream(
                control_paths[1].resolve(), wait_for_greeter,
                INSTALL_BOOT_IDLE_SECONDS, on_output=stream,
            )
            greeter_image = repo_root / "out/logs/installed-plasma-login-greeter.png"
            wake_installed_plasma_greeter(control_paths[0])
            greeter_size = _capture_plasma_verification_window(
                repo_root, greeter_image, qmp_socket=control_paths[0]
            )
            stream("[installed-check] PASS visible-graphical-greeter-before-login\n")
            stream(f"[installed-check] screenshot greeter={greeter_image} size={greeter_size}\n")
            stream("[installed-check] PASS greeter-kwin-wayland-ready\n")

            # The newly installed user must not get a session from a rejected
            # credential. The serial test login is on ttyS0 only; PLM owns the
            # graphical seat and must remain at its greeter after this attempt.
            with QmpClient(control_paths[0], 10) as qmp:
                type_text(qmp, "not-the-install-password")
                send_key(qmp, "ret")
            time.sleep(3)
            serial_command_stream(
                control_paths[1].resolve(),
                f"{greeter_probe} && {user_desktop_absent}",
                INSTALL_BOOT_IDLE_SECONDS,
                on_output=stream,
            )
            stream("[installed-check] PASS rejected-password-stays-at-greeter\n")
            failed_image = repo_root / "out/logs/installed-plasma-login-rejected-password.png"
            wake_installed_plasma_greeter(control_paths[0])
            failed_size = _capture_plasma_verification_window(
                repo_root, failed_image, qmp_socket=control_paths[0]
            )
            stream(f"[installed-check] screenshot rejected-password={failed_image} size={failed_size}\n")

            with QmpClient(control_paths[0], 10) as qmp:
                send_key(qmp, "ctrl-a")
                submit_plasma_greeter_password(qmp, TEST_INSTALL_PASSWORD)
            stream("[installed-check] submitted valid graphical credentials\n")

        for name, command in checks:
            output = serial_command_stream(
                control_paths[1].resolve(),
                command,
                INSTALL_BOOT_IDLE_SECONDS,
                on_output=stream,
            )
            stream(f"[installed-check] PASS {name}\n")
            completed_checks.append(output)
        if profile == "plasma":
            # plasmashell and KWin run while the splash is still on screen;
            # wait for it to exit and the desktop to stop animating first.
            serial_command_stream(
                control_paths[1].resolve(),
                "for n in $(seq 1 120); do pgrep -u mattos -x ksplashqml >/dev/null || exit 0; "
                "sleep 1; done; ps -eo user,pid,comm,args; exit 1",
                INSTALL_BOOT_IDLE_SECONDS,
                on_output=stream,
            )
            desktop_image = repo_root / "out/logs/installed-plasma-desktop.png"
            desktop_size = _capture_settled_plasma_desktop(
                repo_root, desktop_image, qmp_socket=control_paths[0]
            )
            stream(f"[installed-check] screenshot desktop={desktop_image} size={desktop_size}\n")
            height = desktop_size[1]
            popups = (
                # The launcher sits at the panel's left edge.  Right-edge
                # targets (the clock) are not clicked: with the secondary
                # display head, absolute pointer coordinates span both heads,
                # so they land on the show-desktop button instead.
                ("launcher", 30, height - 22),
            )
            popup_images = []
            for name, x, y in popups:
                baseline = repo_root / f"out/logs/installed-plasma-{name}-baseline.png"
                _capture_settled_plasma_desktop(repo_root, baseline, qmp_socket=control_paths[0])
                with QmpClient(control_paths[0], 10) as qmp:
                    click(qmp, x, y, *desktop_size)
                time.sleep(2)
                image = repo_root / f"out/logs/installed-plasma-{name}.png"
                _capture_plasma_verification_window(repo_root, image, qmp_socket=control_paths[0])
                difference = _screenshot_difference(baseline, image)
                if difference < PLASMA_POPUP_MIN_DIFFERENCE:
                    raise RepoError(
                        f"clicking the Plasma {name} did not open its popup "
                        f"(RMSE {difference:.4f} against {baseline}); screenshot: {image}"
                    )
                stream(f"[installed-check] PASS {name}-popup-opened rmse={difference:.4f}\n")
                popup_images.append(str(image))
                with QmpClient(control_paths[0], 10) as qmp:
                    send_key(qmp, "esc")
            stream(
                "[installed-check] PASS launcher interaction; "
                f"screenshots={','.join(popup_images)}\n"
            )
            logout_graphical = (
                f"{installed_plasma_logout_command()} && "
                "for n in $(seq 1 90); do "
                f"{installed_plasma_greeter_probe()} && "
                f"{installed_plasma_user_desktop_absent_probe()} && exit 0; "
                "sleep 1; done; "
                "journalctl -b --no-pager -n 100 -u plasmalogin.service; "
                "loginctl list-sessions --no-legend; "
                "ps -eo user,pid,comm,args; exit 1"
            )
            serial_command_stream(
                control_paths[1].resolve(), logout_graphical,
                INSTALL_BOOT_IDLE_SECONDS, on_output=stream,
            )
            stream("[installed-check] PASS logout-returned-to-greeter\n")
            logout_image = repo_root / "out/logs/installed-plasma-logout-greeter.png"
            wake_installed_plasma_greeter(control_paths[0])
            logout_size = _capture_plasma_verification_window(
                repo_root, logout_image, qmp_socket=control_paths[0]
            )
            stream(f"[installed-check] screenshot logout-greeter={logout_image} size={logout_size}\n")
            with QmpClient(control_paths[0], 10) as qmp:
                submit_plasma_greeter_password(qmp, TEST_INSTALL_PASSWORD)
            serial_command_stream(
                control_paths[1].resolve(),
                "for n in $(seq 1 90); do pgrep -u mattos -x kwin_wayland >/dev/null && "
                "pgrep -u mattos -x plasmashell >/dev/null && systemctl --user is-active --quiet plasma-workspace.target && "
                "test -S /run/user/$(id -u)/wayland-0 && exit 0; sleep 1; done; exit 1",
                INSTALL_BOOT_IDLE_SECONDS, on_output=stream,
            )
            stream("[installed-check] PASS second-graphical-login\n")
            reboot_event = schedule_and_wait_for_guest_reboot(
                control_paths[0].resolve(),
                control_paths[1].resolve(),
                scheduled_reboot_command(TEST_INSTALL_PASSWORD),
                on_output=stream,
            )
            stream(f"[installed-check] observed guest reboot event: {reboot_event}\n")
            serial_command_stream(
                control_paths[1].resolve(),
                "for n in $(seq 1 120); do "
                f"{installed_plasma_greeter_probe()} && "
                f"{installed_plasma_user_desktop_absent_probe()} && exit 0; "
                "sleep 1; done; loginctl list-sessions --no-legend; "
                "ps -eo user,pid,comm,args; exit 1",
                INSTALL_BOOT_IDLE_SECONDS, on_output=stream,
            )
            stream("[installed-check] PASS reboot-returned-to-graphical-greeter\n")
            reboot_image = repo_root / "out/logs/installed-plasma-reboot-greeter.png"
            wake_installed_plasma_greeter(control_paths[0])
            reboot_size = _capture_plasma_verification_window(
                repo_root, reboot_image, qmp_socket=control_paths[0]
            )
            stream(f"[installed-check] screenshot reboot-greeter={reboot_image} size={reboot_size}\n")
            with QmpClient(control_paths[0], 10) as qmp:
                submit_plasma_greeter_password(qmp, TEST_INSTALL_PASSWORD)
            serial_command_stream(
                control_paths[1].resolve(),
                "for n in $(seq 1 90); do pgrep -u mattos -x kwin_wayland >/dev/null && "
                "pgrep -u mattos -x plasmashell >/dev/null && systemctl --user is-active --quiet plasma-workspace.target && exit 0; "
                "sleep 1; done; exit 1",
                INSTALL_BOOT_IDLE_SECONDS, on_output=stream,
            )
            stream("[installed-check] PASS login-after-reboot\n")
        result.update({"uefi_grub_boot": True, "serial_checks": "".join(completed_checks)})
        serial_command_stream(
            control_paths[1].resolve(),
            scheduled_poweroff_command(TEST_INSTALL_PASSWORD),
            INSTALL_BOOT_IDLE_SECONDS,
            on_output=stream,
        )

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
    result["clean_shutdown"] = True
    return result


def upgrade_baseline_metadata(iso: Path) -> Path:
    return iso.with_name(iso.name + ".json")


def save_upgrade_baseline(repo_root: Path, iso: Path) -> Path:
    """Keep a copy of the current ISO as the baseline the upgrade test installs."""
    destination = (repo_root / UPGRADE_BASELINE_RELATIVE).resolve()
    destination.parent.mkdir(parents=True, exist_ok=True)
    temporary = destination.with_name(f".{destination.name}.saving-{os.getpid()}")
    shutil.copyfile(iso, temporary)
    digest = hashlib.sha256()
    with temporary.open("rb") as handle:
        for block in iter(lambda: handle.read(1 << 20), b""):
            digest.update(block)
    commit = run_command_capture(["git", "rev-parse", "HEAD"], repo_root).strip()
    dirty = bool(run_command_capture(["git", "status", "--porcelain"], repo_root).strip())
    temporary.replace(destination)
    metadata = {
        "schema": 1,
        "sha256": digest.hexdigest(),
        "git_commit": commit,
        "git_dirty": dirty,
        "saved_at": datetime.now(UTC).isoformat(),
    }
    upgrade_baseline_metadata(destination).write_text(json.dumps(metadata, indent=2) + "\n", encoding="utf-8")
    return destination


def upgrade_test_source(port: int) -> str:
    """The temporary APT source naming the build's local repository, served by the host.

    It is unsigned like the embedded local repository, and its Release carries
    the same `MattOS Local` label, so the installed pins treat it as MattOS.
    """
    return (
        "Types: deb\n"
        f"URIs: http://{QEMU_HOST_ADDRESS}:{port}/\n"
        "Suites: trixie\n"
        "Components: main\n"
        "Architectures: amd64\n"
        "Trusted: yes\n"
    )


# Terminal control sequences in guest output: OSC (systemd's shell
# integration marks every prompt and command with OSC 3008 context frames,
# terminated by ST or BEL) and CSI (colors).
TERMINAL_CONTROLS = re.compile(r"\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)|\x1b\[[0-9;?]*[ -/]*[@-~]")


def strip_terminal_controls(text: str) -> str:
    return TERMINAL_CONTROLS.sub("", text)


def parse_upgrade_summary(output: str) -> dict[str, int]:
    """The counts of apt-get's last `N upgraded, ...` summary line."""
    matches = list(UPGRADE_SUMMARY.finditer(strip_terminal_controls(output)))
    if not matches:
        raise RepoError("apt-get printed no upgrade summary")
    return {key: int(value) for key, value in matches[-1].groupdict().items()}


def framed_output(output: str, label: str) -> str:
    """The text a guest command printed between `<label>-begin` and `<label>-end` lines."""
    output = strip_terminal_controls(output)
    match = re.search(rf"(?m)^{re.escape(label)}-begin\r?$(.*?)^{re.escape(label)}-end\r?$", output, re.S)
    if match is None:
        raise RepoError(f"guest output lacks its {label} frame")
    return match.group(1)


def framed_command(command: str, label: str) -> str:
    # The frame lines are assembled at run time, so the echoed command line
    # itself can never be mistaken for them.
    head, tail = label[:1], label[1:]
    return f"printf '%s%s\\n' {head} {tail}-begin; {command}; printf '%s%s\\n' {head} {tail}-end"


def stale_installed_packages(installed: str, inventory: dict[str, str]) -> list[str]:
    """Installed packages the build produces whose version is not the build's.

    `installed` is `dpkg-query -W -f '${Package} ${Version}\\n'` output.
    """
    stale = []
    for line in installed.splitlines():
        fields = line.split()
        if len(fields) != 2:
            continue
        name, version = fields
        expected = inventory.get(name)
        if expected is not None and version != expected:
            stale.append(f"{name} {version} (built {expected})")
    return sorted(stale)


def installed_check_results(log: str) -> dict[str, list[str]]:
    """The named installed-system checks a verification log reports."""
    results: dict[str, list[str]] = {"pass": [], "fail": []}
    # Matched on the raw text: a stray ESC can directly precede the
    # bracket, which control-sequence stripping would read as CSI.
    for match in re.finditer(r"\[installed-check\] (PASS|FAIL) ([A-Za-z0-9-]+)", log):
        results[match.group(1).lower()].append(match.group(2))
    return results


def build_inventory_versions(repo_root: Path) -> dict[str, str]:
    inventory = tomllib.loads((repo_root / "out/packages/inventory.toml").read_text(encoding="utf-8"))
    return {entry["name"]: entry["version"] for entry in inventory.get("package", [])}


class _QuietRepositoryHandler(http.server.SimpleHTTPRequestHandler):
    def log_message(self, format: str, *args: object) -> None:
        pass


def serve_repository(directory: Path) -> tuple[http.server.ThreadingHTTPServer, int]:
    """Serve the build's local repository on the host loopback for the guest."""
    if not (directory / "dists/trixie/Release").is_file():
        raise RepoError(f"no built local repository at {directory}; build MattOS first")
    handler = functools.partial(_QuietRepositoryHandler, directory=str(directory))
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    return server, server.server_address[1]


def _upgrade_installed_system(
    repo_root: Path,
    disk: Path,
    args: argparse.Namespace,
    port: int,
    report: dict[str, object],
) -> None:
    """Boot the baseline install, upgrade it from the served repository, and
    check the result before shutting down."""
    upgrade_args = argparse.Namespace(**vars(args))
    upgrade_args.install = False
    upgrade_args.run_installed = False
    upgrade_args.test_control = True
    upgrade_args.require_virgl = getattr(args, "install_profile", "plasma") == "plasma"
    upgrade_args.serial_console = False
    upgrade_args.headless = False
    log_path = (repo_root / UPGRADE_LOG_RELATIVE).resolve()
    sudo = f"printf '%s\\n' {shlex.quote(TEST_INSTALL_PASSWORD)} | sudo -S"
    inventory = build_inventory_versions(repo_root)
    source = base64.b64encode(upgrade_test_source(port).encode()).decode()

    def lifecycle(_proc: subprocess.Popen[str], control_paths: tuple[Path, Path]) -> None:
        wait_for_socket(control_paths[0], 120)
        serial = control_paths[1].resolve()

        def stream(chunk: str) -> None:
            print(chunk, end="", flush=True)
            with log_path.open("a", encoding="utf-8") as log:
                log.write(chunk)

        def run(command: str, idle: float = INSTALL_BOOT_IDLE_SECONDS) -> str:
            return serial_command_stream(serial, command, idle, on_output=stream)

        installed_query = "dpkg-query -W -f '${Package} ${Version}\\n'"
        before = framed_output(run(framed_command(installed_query, "installed")), "installed")
        report["packages_before"] = len(before.split("\n")) - 1
        # Upgrade from the build alone: the hosted repository may hold other versions.
        run(
            f"{sudo} sh -c 'echo {source} | base64 -d > {UPGRADE_TEST_SOURCE} && "
            "sed -i \"s/^Enabled: yes/Enabled: no/\" /etc/apt/sources.list.d/mattos-hosted.sources'"
        )
        stream("[upgrade-test] serving the build repository at "
               f"http://{QEMU_HOST_ADDRESS}:{port}/\n")
        run(f"{sudo} apt-get update")
        upgrade = run(
            f"{sudo} env DEBIAN_FRONTEND=noninteractive apt-get -y "
            "-o Dpkg::Options::=--force-confdef -o Dpkg::Options::=--force-confold full-upgrade",
            INSTALL_PROGRESS_IDLE_SECONDS,
        )
        summary = parse_upgrade_summary(upgrade)
        report["apt_full_upgrade"] = summary
        stream(f"[upgrade-test] apt full-upgrade: {summary}\n")
        if summary["held"]:
            raise RepoError(f"apt held back {summary['held']} package(s) during the upgrade")
        remaining = parse_upgrade_summary(run(f"{sudo} apt-get -s full-upgrade"))
        if any(remaining[key] for key in ("upgraded", "installed", "removed", "held")):
            raise RepoError(f"the upgrade left work undone: {remaining}")
        run("test -z \"$(dpkg --audit)\"")
        after = framed_output(run(framed_command(installed_query, "installed")), "installed")
        stale = stale_installed_packages(after, inventory)
        if stale:
            raise RepoError("installed packages are not at the built versions: " + "; ".join(stale))
        report["packages_after"] = len(after.split("\n")) - 1
        report["changed_packages"] = sorted(
            set(after.strip().splitlines()) - set(before.strip().splitlines())
        )
        stream(f"[upgrade-test] PASS every installed MattOS package is at its built version "
               f"({len(report['changed_packages'])} changed)\n")
        # Leave the ordinary installed APT policy for the post-upgrade checks.
        run(
            f"{sudo} sh -c 'rm -f {UPGRADE_TEST_SOURCE} && "
            "sed -i \"s/^Enabled: no/Enabled: yes/\" /etc/apt/sources.list.d/mattos-hosted.sources'"
        )
        run(scheduled_poweroff_command(TEST_INSTALL_PASSWORD))

    log_path.parent.mkdir(parents=True, exist_ok=True)
    log_path.write_text("MattOS upgrade test log\n", encoding="utf-8")
    exit_code = _launch_one(repo_root, None, upgrade_args, boot_iso=False, install_disk=disk, lifecycle=lifecycle)
    if exit_code != 0:
        raise RepoError(f"upgrade VM exited with status {exit_code}")


def run_upgrade_test(repo_root: Path, args: argparse.Namespace) -> int:
    """Install the baseline ISO, upgrade it to the current build, reboot and
    verify the upgraded system with the same checks as a fresh install."""
    baseline = (args.upgrade_from or repo_root / UPGRADE_BASELINE_RELATIVE)
    baseline = (baseline if baseline.is_absolute() else repo_root / baseline).resolve()
    if not baseline.is_file():
        raise RepoError(
            f"no upgrade baseline ISO at {baseline}; save one with "
            "python3 DevUtils/run_qemu.py --save-upgrade-baseline"
        )
    current = ensure_iso_exists(repo_root)
    report: dict[str, object] = {
        "schema": 1,
        "profile": args.install_profile,
        "baseline_iso": str(baseline),
        "current_iso": str(current),
        "started_at": datetime.now(UTC).isoformat(),
    }
    metadata = upgrade_baseline_metadata(baseline)
    if metadata.is_file():
        report["baseline"] = json.loads(metadata.read_text(encoding="utf-8"))
    install_args = argparse.Namespace(**vars(args))
    install_args.install = True
    install_args.install_disk = args.install_disk or UPGRADE_TEST_DISK_RELATIVE
    disk = prepare_install_disk(repo_root, install_args)
    assert disk is not None
    report_path = (repo_root / UPGRADE_REPORT_RELATIVE).resolve()
    report_path.parent.mkdir(parents=True, exist_ok=True)
    server, port = serve_repository((repo_root / "out/repository").resolve())
    try:
        print(f"[upgrade-test] installing the baseline {baseline} ({args.install_profile})", flush=True)
        result = _launch_one(
            repo_root,
            baseline,
            install_args,
            boot_iso=True,
            install_disk=disk,
            lifecycle=lambda _proc, paths: _run_automatic_test_install(repo_root, disk, paths, args.install_profile),
        )
        if result != 0:
            raise RepoError(f"baseline installer VM exited with status {result}")
        print("[upgrade-test] upgrading the installed baseline to the current build", flush=True)
        _upgrade_installed_system(repo_root, disk, install_args, port, report)
        print("[upgrade-test] rebooting the upgraded system for the installed-system checks", flush=True)
        verification = _verify_installed_disk_boot(repo_root, disk, install_args)
        boot_log = install_task_log(repo_root).with_name("installed-test-boot.log")
        report["verification"] = {
            "uefi_grub_boot": verification.get("uefi_grub_boot"),
            "clean_shutdown": verification.get("clean_shutdown"),
            "installed_checks": installed_check_results(boot_log.read_text(encoding="utf-8", errors="replace")),
        }
        report["result"] = "pass"
    except Exception as exc:
        report["result"] = "fail"
        report["error"] = str(exc)
        print(f"[upgrade-test] failed upgrade disk retained for diagnosis: {disk}", file=sys.stderr)
        raise
    finally:
        server.shutdown()
        server.server_close()
        report["finished_at"] = datetime.now(UTC).isoformat()
        report_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(f"[upgrade-test] PASS: {baseline.name} upgraded to the current build and verified; report: {report_path}")
    return 0


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
            lifecycle=lambda _proc, paths: _run_automatic_test_install(
                repo_root, disk, paths, args.install_profile
            ),
        )
        if result != 0:
            raise RepoError(f"installer VM exited with status {result}")
        verification = _verify_installed_disk_boot(repo_root, disk, args)
        marker = write_install_completion(repo_root, disk, verification)
        print(f"[qemu] installed disk validated: {disk} (completion marker: {marker})")
    except Exception:
        invalidate_install_completion(repo_root, disk)
        print(f"[qemu] failed installation disk retained for diagnosis: {disk}", file=sys.stderr)
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

    if args.save_upgrade_baseline:
        saved = save_upgrade_baseline(repo_root, ensure_iso_exists(repo_root))
        print(f"[upgrade-test] saved the current ISO as the upgrade baseline: {saved}")
        return 0
    if args.upgrade_test:
        return run_upgrade_test(repo_root, args)

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
