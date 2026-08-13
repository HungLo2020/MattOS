#!/usr/bin/env python3
import argparse
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


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Build and run MattOS in QEMU")
    parser.add_argument("--no-build", action="store_true", help="skip build/image steps")
    parser.add_argument("--clean", action="store_true", help="clean build artifacts before rebuilding")
    parser.add_argument("--memory", type=int, default=1024, help="VM memory in MiB (default: 1024)")
    parser.add_argument("--cpus", type=int, default=1, help="virtual CPU count (default: 1)")
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


def choose_graphical_display(repo_root: Path) -> str:
    # Prefer GTK; fall back to SDL if GTK is not built in the local QEMU binary.
    try:
        output = run_command_capture(["qemu-system-x86_64", "-display", "help"], cwd=repo_root)
    except RepoError:
        return "gtk"

    displays = {line.strip() for line in output.splitlines() if line.strip()}
    if "gtk" in displays:
        return "gtk"
    if "sdl" in displays:
        return "sdl"
    return "default"


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

    qemu_cmd: List[str] = [
        "qemu-system-x86_64",
        "-m",
        str(args.memory),
        "-smp",
        str(args.cpus),
        "-cdrom",
        str(iso_path),
        "-boot",
        "d",
        "-vga",
        "std",
    ]
    install_disk = prepare_install_disk(repo_root, args)
    if install_disk is not None:
        qemu_cmd.extend(["-drive", f"file={install_disk},if=virtio,format=qcow2"])
    qemu_cmd.extend(network_arguments(args.no_network))

    if args.serial_console:
        qemu_cmd.extend(["-nographic", "-serial", "stdio", "-monitor", "none", "-no-reboot", "-no-shutdown"])
    else:
        display = choose_graphical_display(repo_root)
        if display == "default":
            qemu_cmd.extend(["-display", "default"])
        else:
            qemu_cmd.extend(["-display", display])
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
