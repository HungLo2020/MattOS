#!/usr/bin/env python3
"""Interactively inspect this host, select a package profile, and apply it."""

from __future__ import annotations

import os
import subprocess
import sys
from pathlib import Path


REPOSITORY_ROOT = Path(__file__).resolve().parents[1]
SOURCE_DIRECTORY = REPOSITORY_ROOT / "src"


def use_project_interpreter() -> None:
    """Re-execute with the bootstrapped virtual environment when available."""

    venv_python = REPOSITORY_ROOT / (".venv/Scripts/python.exe" if os.name == "nt" else ".venv/bin/python")
    if venv_python.is_file() and Path(sys.executable).resolve() != venv_python.resolve():
        os.execv(str(venv_python), (str(venv_python), *sys.argv))


use_project_interpreter()

if str(SOURCE_DIRECTORY) not in sys.path:
    sys.path.insert(0, str(SOURCE_DIRECTORY))

from host import detect_host
from mattpackages import configure_repository, eligibility_problem
from packages.cli import build_command_plan, load_resources, main as package_cli_main, print_plan
from packages.executor import execute_operations
from packages.planner import PackageResolutionError
from packages.providers import ProviderPlanningError
from preflight import run as run_preflight
from storage_smb import configure_interactively
from system import PackageManager, detect_active_desktop_environment, detect_installed_desktop_environments, detect_linux_distro, detect_package_manager, detect_package_platform


def prompt_yes_no(question: str) -> bool:
    """Return True only for an explicit affirmative response."""

    try:
        return input(f"{question} [y/N]: ").strip().lower() in {"y", "yes"}
    except EOFError:
        return False


def print_system_summary() -> tuple[str, PackageManager | None]:
    """Show host facts that determine profile and provider selection."""

    host = detect_host()
    distro = detect_linux_distro() if host.system == "linux" else None
    package_manager = detect_package_manager(distro) if distro is not None else None
    active_desktop = detect_active_desktop_environment()
    installed_desktops = detect_installed_desktop_environments() if host.system == "linux" else ()

    print("LinuxScripts Setup")
    print("=" * 18)
    print(f"Operating system: {host.system}")
    print(f"Architecture: {host.architecture} ({host.machine})")
    if distro is not None:
        version = f" {distro.version_id}" if distro.version_id else ""
        print(f"Distribution: {distro.name}{version}")
    print(f"Package platform: {detect_package_platform(host, distro)}")
    print(f"Package manager: {package_manager.value if package_manager else 'not detected'}")
    print(f"Active desktop: {active_desktop or 'not detected'}")
    print(f"Installed desktops: {', '.join(installed_desktops) if installed_desktops else 'not detected'}")
    print()
    return detect_package_platform(host, distro), package_manager


def choose_profile(profiles) -> str | None:
    """Display a numbered profile menu and return the selected profile name."""

    choices = tuple(sorted(profiles.values(), key=lambda profile: profile.name))
    print("Available profiles:")
    for index, profile in enumerate(choices, start=1):
        print(f"  {index}. {profile.name} - {profile.description}")

    while True:
        try:
            entered = input("Select a profile by number, or press Enter to cancel: ").strip()
        except EOFError:
            return None
        if not entered:
            return None
        if entered.isdigit() and 1 <= int(entered) <= len(choices):
            return choices[int(entered) - 1].name
        print("Enter one of the listed profile numbers.")


def offer_storage_mount(platform_name: str, package_manager: PackageManager | None) -> None:
    """Offer the Linux/APT-only persistent SMB mount after package installation."""

    if platform_name != "linux" or package_manager is not PackageManager.APT:
        return
    if not prompt_yes_no("Configure the persistent Tailscale SMB storage mount?"):
        return
    configure_interactively()


def offer_mattpackages(platform_name: str, package_manager: PackageManager | None) -> None:
    """Offer public repository enrollment before selecting any packages."""

    if platform_name not in {"linux", "mattos"} or package_manager is not PackageManager.APT:
        return
    problem = eligibility_problem()
    if problem:
        print(f"Skipping MattPackages: {problem}")
        return
    if prompt_yes_no("Enable the MattPackages APT repository for this machine?"):
        configure_repository()


def offer_server_manager(platform_name: str) -> None:
    """Offer server administration as an independent optional Setup capability."""

    if platform_name != "linux" or not prompt_yes_no("Open the Server Manager?"):
        return
    subprocess.run((sys.executable, str(REPOSITORY_ROOT / "Tools" / "ServerManager.py")), check=True)


def run_package_flow() -> bool:
    """Offer one package profile without ending later interactive setup steps."""

    if not prompt_yes_no("Choose and apply a package profile?"):
        print("Skipping package profile setup.")
        return True

    _, profiles = load_resources(REPOSITORY_ROOT)
    profile_name = choose_profile(profiles)
    if profile_name is None:
        print("Skipping package profile setup.")
        return True

    try:
        result = build_command_plan(REPOSITORY_ROOT, (profile_name,))
    except (PackageResolutionError, ProviderPlanningError, RuntimeError, ValueError) as error:
        print(f"Package setup failed: {error}", file=sys.stderr)
        return False

    print()
    print_plan(*result)
    if not prompt_yes_no("Apply this plan now?"):
        print("Package plan was not applied.")
        return True

    try:
        execute_operations(result[-1], REPOSITORY_ROOT)
    except (OSError, RuntimeError, subprocess.CalledProcessError) as error:
        print(f"Package setup failed: {error}", file=sys.stderr)
        return False
    return True


def main() -> int:
    """Run the interactive setup flow or forward a package subcommand."""

    if len(sys.argv) > 1:
        return package_cli_main(sys.argv[1:])

    platform_name, package_manager = print_system_summary()
    if platform_name in {"linux", "mattos"}:
        try:
            run_preflight(REPOSITORY_ROOT)
        except (OSError, RuntimeError, subprocess.CalledProcessError) as error:
            print(f"Preflight failed: {error}", file=sys.stderr)
            return 1
    try:
        offer_mattpackages(platform_name, package_manager)
    except (OSError, RuntimeError, subprocess.CalledProcessError) as error:
        print(f"MattPackages setup failed: {error}", file=sys.stderr)
        return 1
    succeeded = run_package_flow()
    if not succeeded:
        return 1
    try:
        offer_storage_mount(platform_name, package_manager)
        offer_server_manager(platform_name)
    except (OSError, RuntimeError, subprocess.CalledProcessError) as error:
        print(f"Storage mount setup failed: {error}", file=sys.stderr)
        succeeded = False
    return 0 if succeeded else 1


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except KeyboardInterrupt:
        print("\nSetup cancelled.")
        raise SystemExit(130) from None
