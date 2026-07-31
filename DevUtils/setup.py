#!/usr/bin/env python3
import argparse
import subprocess
import sys
from pathlib import Path
from typing import Dict, List, Set

from common import RepoError, command_exists, find_repo_root, read_os_release, run_command


REQUIRED_TOOLS = [
    "git",
    "cargo",
    "rustc",
    "make",
    "gcc",
    "ld",
    "objcopy",
    "perl",
    "python3",
    "bc",
    "cpio",
    "gzip",
    "mformat",
    "mcopy",
    "grub-mkrescue",
    "xorriso",
    "pkg-config",
    "qemu-system-x86_64",
    "bash",
    "bison",
    "flex",
]

# These are pulled in by the existing kernel workflow and WSL bootstrap logic.
EXTRA_KERNEL_PACKAGES = [
    "libssl-dev",
    "libelf-dev",
]

DEBIAN_TOOL_PACKAGES: Dict[str, List[str]] = {
    "git": ["git"],
    "cargo": ["cargo"],
    "rustc": ["rustc"],
    "make": ["build-essential"],
    "gcc": ["build-essential"],
    "ld": ["binutils"],
    "objcopy": ["binutils"],
    "perl": ["perl"],
    "python3": ["python3"],
    "bc": ["bc"],
    "cpio": ["cpio"],
    "gzip": ["gzip"],
    "mformat": ["mtools"],
    "mcopy": ["mtools"],
    "grub-mkrescue": ["grub-pc-bin", "grub-common"],
    "xorriso": ["xorriso"],
    "pkg-config": ["pkg-config"],
    "qemu-system-x86_64": ["qemu-system-x86"],
    "bash": ["bash"],
    "bison": ["bison"],
    "flex": ["flex"],
}


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Set up Linux host dependencies for MattOS")
    parser.add_argument("--check", action="store_true", help="report readiness without making changes")
    parser.add_argument("--dry-run", action="store_true", help="print setup commands without executing")
    parser.add_argument("--yes", action="store_true", help="install without confirmation prompt")
    return parser.parse_args()


def is_supported_debian_family(os_release: Dict[str, str]) -> bool:
    distro_id = os_release.get("ID", "").strip().lower()
    id_like = os_release.get("ID_LIKE", "").strip().lower().split()
    return distro_id in {"debian", "ubuntu"} or "debian" in id_like or "ubuntu" in id_like


def detect_missing_tools() -> List[str]:
    return [tool for tool in REQUIRED_TOOLS if not command_exists(tool)]


def is_dpkg_installed(package_name: str, dry_run: bool) -> bool:
    cmd = ["dpkg-query", "-W", "-f=${Status}", "--", package_name]
    print("+", " ".join(cmd))
    if dry_run:
        return False

    proc = subprocess.run(
        cmd,
        check=False,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    return proc.returncode == 0 and "install ok installed" in proc.stdout


def compute_missing_packages(missing_tools: List[str], dry_run: bool) -> List[str]:
    packages: List[str] = []
    seen: Set[str] = set()
    installed_cache: Dict[str, bool] = {}

    def package_installed(name: str) -> bool:
        if name not in installed_cache:
            installed_cache[name] = is_dpkg_installed(name, dry_run)
        return installed_cache[name]

    for tool in missing_tools:
        for package_name in DEBIAN_TOOL_PACKAGES.get(tool, []):
            if package_installed(package_name):
                continue
            if package_name not in seen:
                seen.add(package_name)
                packages.append(package_name)

    for package_name in EXTRA_KERNEL_PACKAGES:
        if not package_installed(package_name) and package_name not in seen:
            seen.add(package_name)
            packages.append(package_name)

    return packages


def print_host_summary(os_release: Dict[str, str]) -> None:
    distro_id = os_release.get("ID", "unknown")
    distro_like = os_release.get("ID_LIKE", "")
    pretty = os_release.get("PRETTY_NAME", distro_id)
    print(f"Detected Linux distribution: {pretty}")
    if distro_like:
        print(f"ID={distro_id} ID_LIKE={distro_like}")


def maybe_confirm_install(packages: List[str], assume_yes: bool, dry_run: bool) -> bool:
    if assume_yes:
        return True
    if not packages:
        return True

    if dry_run:
        print("Dry-run mode: skipping interactive confirmation prompt.")
        return True

    print("The following packages will be installed:")
    print("  " + " ".join(packages))
    reply = input("Proceed with installation? [y/N]: ").strip().lower()
    return reply in {"y", "yes"}


def run_doctor(repo_root: Path, dry_run: bool) -> int:
    return run_command(
        ["cargo", "run", "-p", "mattos-build", "--", "doctor"],
        cwd=repo_root,
        dry_run=dry_run,
        check=False,
    )


def main() -> int:
    args = parse_args()

    if sys.platform != "linux":
        raise RepoError("DevUtils/setup.py currently supports native Linux only")

    script_path = Path(__file__).resolve()
    repo_root = find_repo_root(script_path.parent)

    os_release = read_os_release()
    print_host_summary(os_release)

    if not is_supported_debian_family(os_release):
        distro_id = os_release.get("ID", "unknown")
        raise RepoError(
            "unsupported Linux distribution: "
            f"{distro_id}. Currently supported: Debian, Ubuntu, Ubuntu-derived (ID_LIKE=debian/ubuntu)."
        )

    if not command_exists("apt-get"):
        raise RepoError("apt-get is required on Debian/Ubuntu systems")

    missing_tools = detect_missing_tools()
    print(f"Missing tools: {', '.join(missing_tools) if missing_tools else '<none>'}")

    missing_packages = compute_missing_packages(missing_tools, args.dry_run)
    print(f"Missing packages: {', '.join(missing_packages) if missing_packages else '<none>'}")

    install_performed = False
    if not args.check and missing_packages:
        if not maybe_confirm_install(missing_packages, args.yes, args.dry_run):
            print("Installation cancelled by user.")
            return 1

        if args.dry_run:
            run_command(["sudo", "apt-get", "update"], cwd=repo_root, dry_run=True)
            run_command(
                ["sudo", "apt-get", "install", "-y", *missing_packages],
                cwd=repo_root,
                dry_run=True,
            )
        else:
            install_performed = True
            try:
                run_command(["sudo", "apt-get", "update"], cwd=repo_root, dry_run=False)
                run_command(
                    ["sudo", "apt-get", "install", "-y", *missing_packages],
                    cwd=repo_root,
                    dry_run=False,
                )
            except RepoError as exc:
                raise RepoError(
                    "package installation failed. Review the apt output above and rerun setup after fixing the issue."
                ) from exc

    if args.check and missing_packages:
        print("Check mode: dependencies are missing; no changes were made.")

    doctor_rc = run_doctor(repo_root, args.dry_run)
    if doctor_rc != 0:
        print("Setup is incomplete: mattos-build doctor still reports required issues.")
        return doctor_rc

    if missing_packages and args.dry_run:
        print("Dry-run completed: dependencies are not yet installed.")
        return 1

    if missing_packages and not install_performed:
        return 1

    print("Setup complete: MattOS prerequisites are ready.")
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except RepoError as exc:
        print(f"error: {exc}", file=sys.stderr)
        sys.exit(1)
