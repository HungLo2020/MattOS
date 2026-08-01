#!/usr/bin/env python3
import argparse
import shutil
import signal
import subprocess
import sys
from pathlib import Path
from typing import List

from common import RepoError, ensure_tools, find_repo_root, run_command, run_command_capture


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
    return iso_path


def build_if_needed(repo_root: Path, args: argparse.Namespace) -> None:
    if args.no_build:
        return

    # Fail fast on missing or broken toolchain prerequisites before expensive builds.
    try:
        run_command(
            ["cargo", "run", "-p", "mattos-build", "--", "doctor"],
            cwd=repo_root,
            dry_run=args.dry_run,
        )
    except RepoError as exc:
        raise RepoError(
            "mattos-build doctor reported missing or broken prerequisites. "
            "Run: python3 DevUtils/setup.py"
        ) from exc

    if args.clean:
        run_command(
            ["cargo", "run", "-p", "mattos-build", "--", "clean", "artifacts"],
            cwd=repo_root,
            dry_run=args.dry_run,
        )

    # Reuse existing orchestrator behavior for incremental compilation and image assembly.
    run_command(
        ["cargo", "run", "-p", "mattos-build", "--", "build", "all"],
        cwd=repo_root,
        dry_run=args.dry_run,
    )
    run_command(
        ["cargo", "run", "-p", "mattos-build", "--", "image"],
        cwd=repo_root,
        dry_run=args.dry_run,
    )


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
    ]
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

    proc = subprocess.Popen(qemu_cmd, cwd=str(repo_root))
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
