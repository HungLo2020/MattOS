#!/usr/bin/env python3
import argparse
import os
import shutil
import signal
import subprocess
import sys
from pathlib import Path
from typing import List

from common import (
    RepoError,
    ensure_tools,
    find_repo_root,
    mattos_build_environment,
    run_command,
    run_command_capture,
)

DEFAULT_INSTALL_DISK_RELATIVE = Path("out/qemu/mattos-dev.qcow2")
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
    return parser.parse_args()


def network_arguments(disabled: bool) -> List[str]:
    if disabled:
        return []
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


def prepare_install_disk(repo_root: Path, args: argparse.Namespace) -> Path | None:
    """Return a persistent qcow2 target disk, creating it only when absent."""
    if args.no_install_disk:
        return None

    disk = args.install_disk or (repo_root / DEFAULT_INSTALL_DISK_RELATIVE)
    disk = disk if disk.is_absolute() else repo_root / disk
    disk = disk.resolve()
    disk.parent.mkdir(parents=True, exist_ok=True)
    if not disk.exists():
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
    if not disk.is_file():
        raise RepoError(f"QEMU install disk is not a regular file: {disk}")
    return disk


def build_if_needed(repo_root: Path, args: argparse.Namespace) -> None:
    if args.no_build:
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


def launch_qemu(repo_root: Path, iso_path: Path, args: argparse.Namespace) -> int:
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
        "-drive",
        f"file={iso_path},if=none,id=mattos-cd,media=cdrom,readonly=on",
        "-device",
        "virtio-scsi-pci,id=mattos-scsi",
        "-device",
        "scsi-cd,drive=mattos-cd,bus=mattos-scsi.0,bootindex=1",
        "-boot",
        "d",
    ]
    if not args.headless:
        # The native COSMIC installer is a DRM/KMS Wayland client session.
        # `virtio-vga-gl` is deliberately the only display device: unlike
        # `virtio-gpu-gl-pci` it remains visible to firmware and GRUB, while
        # also publishing the VirGL capset required by Mesa's source-built
        # virgl Gallium renderer for GBM/EGL dmabuf scanout.
        qemu_cmd.extend(["-device", graphical_gpu_device(repo_root)])
        # A graphical desktop guest needs an absolute pointer. The default
        # PS/2 mouse is relative-only and did not produce usable pointer
        # motion in the COSMIC KMS session. One USB tablet is sufficient.
        qemu_cmd.extend(["-device", QEMU_TABLET_CONTROLLER, "-device", QEMU_TABLET_DEVICE])
    install_disk = prepare_install_disk(repo_root, args)
    if install_disk is not None:
        qemu_cmd.extend(["-drive", f"file={install_disk},if=virtio,format=qcow2"])
    qemu_cmd.extend(network_arguments(args.no_network))

    if args.serial_console or args.headless:
        qemu_cmd.extend(["-nographic", "-serial", "stdio", "-monitor", "none", "-no-reboot", "-no-shutdown"])
    else:
        display = choose_graphical_display(repo_root)
        if display == "default":
            qemu_cmd.extend(["-display", "default"])
        else:
            qemu_cmd.extend(["-display", display])
        print(f"[qemu] graphics: {VIRTIO_GPU_GL_DEVICE} with display={display} (VirGL requested)")
        qemu_cmd.extend(["-serial", f"file:{logs_dir / 'qemu-serial.log'}"])

    for extra in args.qemu_arg:
        qemu_cmd.append(extra)

    print("+", " ".join(qemu_cmd))
    if args.dry_run:
        return 0

    proc = subprocess.Popen(qemu_cmd, cwd=str(repo_root), env=mattos_build_environment(repo_root))
    try:
        return proc.wait()
    except KeyboardInterrupt:
        print("\nInterrupted, terminating QEMU...")
        proc.send_signal(signal.SIGINT)
        try:
            return proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            proc.terminate()
            return proc.wait(timeout=5)


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
    else:
        iso_path = ensure_iso_exists(repo_root)

    return launch_qemu(repo_root, iso_path, args)


if __name__ == "__main__":
    try:
        sys.exit(main())
    except RepoError as exc:
        print(f"error: {exc}", file=sys.stderr)
        sys.exit(1)
