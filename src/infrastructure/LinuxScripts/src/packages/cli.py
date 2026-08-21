"""Shared command-line helpers for package planning and installation."""

from __future__ import annotations

import argparse
import shlex
import subprocess
import sys
from pathlib import Path

from host import detect_host
from packages.catalog import load_catalog, load_profiles
from packages.executor import execute_operations, validate_script_dependencies
from packages.models import NodejsOperation, ScriptOperation, ShellInstallerOperation
from packages.planner import PackageResolutionError, resolve_profiles
from packages.providers import ProviderPlanningError, plan_execution_steps, preferred_provider
from paths import find_repository_root
from system import PackageManager, detect_package_manager, detect_package_platform


def load_resources(repository_root: Path):
    """Load the package catalog and profiles from the repository resources."""

    catalog = load_catalog(repository_root / "resources" / "packages")
    profiles = load_profiles(repository_root / "resources" / "profiles")
    return catalog, profiles


def build_command_plan(
    repository_root: Path,
    profile_names: tuple[str, ...],
    *,
    platform_name: str | None = None,
    package_manager: PackageManager | None = None,
):
    """Resolve requested profiles and their provider operations for one host."""

    catalog, profiles = load_resources(repository_root)
    host = detect_host()
    platform_name = platform_name or detect_package_platform(host)
    if package_manager is None and platform_name in {"linux", "mattos"} and host.system == "linux":
        package_manager = detect_package_manager()

    provider_preferences = (preferred_provider(package_manager),) if package_manager else ()
    package_plan = resolve_profiles(profile_names, catalog, profiles, platform_name, provider_preferences)
    operations = plan_execution_steps(
        package_plan.packages,
        package_plan.profile_scripts,
        package_manager,
        package_plan.delete_packages,
    )
    validate_script_dependencies(operations, repository_root)
    return host, platform_name, package_manager, package_plan, operations


def print_plan(host, platform_name, package_manager, package_plan, operations) -> None:
    """Render a human-readable plan before any provider execution."""

    print(f"Host: {host.system}/{host.architecture}")
    print(f"Plan platform: {platform_name}")
    print(f"Native package manager: {package_manager.value if package_manager else 'not applicable'}")
    print(f"Profiles: {', '.join(package_plan.profiles)}")
    print("Resolved packages:")
    for package in package_plan.packages:
        print(f"  {package.name}: {package.target.provider}/{package.target.identifier}")
    if package_plan.skipped:
        print("Skipped packages:")
        for name, reason in package_plan.skipped.items():
            print(f"  {name}: {reason}")
    print("Provider operations:")
    for operation in operations:
        if isinstance(operation, ScriptOperation):
            print(f"  script: {operation.script}")
            print(f"    {operation.description}")
            continue
        if isinstance(operation, NodejsOperation):
            print(f"  nodejs: {', '.join(operation.packages)}")
            print("    Ensure Node.js and npm are available")
            continue
        if isinstance(operation, ShellInstallerOperation):
            print(f"  shell_installer: {', '.join(operation.packages)}")
            for package, url in zip(operation.packages, operation.urls, strict=True):
                print(f"    Run shell installer for '{package}': {url}")
            continue
        print(f"  {operation.provider}: {', '.join(operation.packages)}")
        for command in operation.commands:
            privilege = "sudo " if command.elevated else ""
            print(f"    {privilege}{shlex.join(command.argv)}")


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    """Parse the non-interactive package-management interface arguments."""

    parser = argparse.ArgumentParser(description=__doc__)
    subcommands = parser.add_subparsers(dest="command", required=True)
    subcommands.add_parser("profiles", help="List available package profiles.")
    for command_name, help_text in (("plan", "Display a package plan without changing the system."), ("apply", "Apply a package plan.")):
        command = subcommands.add_parser(command_name, help=help_text)
        command.add_argument("profiles", nargs="+", help="One or more profile names to resolve.")
        if command_name == "plan":
            command.add_argument("--platform", choices=("linux", "mattos", "windows", "macos"), help="Override the detected platform for planning.")
            command.add_argument(
                "--package-manager",
                choices=tuple(manager.value for manager in PackageManager),
                help="Override the detected Linux package manager for planning.",
            )
        else:
            command.add_argument("--yes", action="store_true", help="Required acknowledgement before executing the plan.")
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    """Run the non-interactive package-management interface."""

    args = parse_args(argv)
    repository_root = find_repository_root(Path(__file__).parent)
    if args.command == "profiles":
        _, profiles = load_resources(repository_root)
        for profile in profiles.values():
            print(f"{profile.name}: {profile.description}")
        return 0

    package_manager = PackageManager(args.package_manager) if getattr(args, "package_manager", None) else None
    try:
        result = build_command_plan(
            repository_root,
            tuple(args.profiles),
            platform_name=getattr(args, "platform", None),
            package_manager=package_manager,
        )
    except (PackageResolutionError, ProviderPlanningError, RuntimeError, ValueError) as error:
        print(f"Error: {error}", file=sys.stderr)
        return 1

    print_plan(*result)
    if args.command == "plan":
        return 0
    if not args.yes:
        print("Error: apply requires --yes after reviewing the plan.", file=sys.stderr)
        return 2

    try:
        execute_operations(result[-1], repository_root)
    except (OSError, RuntimeError, subprocess.CalledProcessError) as error:
        print(f"Error: package apply failed: {error}", file=sys.stderr)
        return error.returncode or 1 if isinstance(error, subprocess.CalledProcessError) else 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())