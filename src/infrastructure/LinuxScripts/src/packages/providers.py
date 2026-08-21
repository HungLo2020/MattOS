"""Translate resolved logical packages into provider-specific command plans."""

from __future__ import annotations

from collections import OrderedDict
from collections.abc import Iterable

from packages.models import CommandSpec, NodejsOperation, PackageTarget, ProviderOperation, ResolvedPackage, ScriptOperation, ShellInstallerOperation
from system import PackageManager


class ProviderPlanningError(ValueError):
    """Raised when a resolved target cannot be handled by this host."""


_FLATPAK_REMOTES = {"flathub": "https://flathub.org/repo/flathub.flatpakrepo"}

_PACKAGE_MANAGER_PROVIDERS = {
    PackageManager.APT: "apt",
    PackageManager.DNF: "dnf",
    PackageManager.PACMAN: "pacman",
    PackageManager.ZYPPER: "zypper",
    PackageManager.APK: "apk",
}


def preferred_provider(package_manager: PackageManager | None) -> str | None:
    """Return the native package provider for a detected Linux manager."""

    return _PACKAGE_MANAGER_PROVIDERS.get(package_manager)


def _targets_by_provider(packages: Iterable[ResolvedPackage]) -> OrderedDict[str, list[ResolvedPackage]]:
    grouped: OrderedDict[str, list[ResolvedPackage]] = OrderedDict()
    for package in packages:
        grouped.setdefault(package.target.provider, []).append(package)
    return grouped


def _apt_operation(packages: list[ResolvedPackage], package_manager: PackageManager | None) -> ProviderOperation:
    if package_manager is not PackageManager.APT:
        raise ProviderPlanningError("APT targets require an apt-based Linux distribution.")
    identifiers = tuple(package.target.identifier for package in packages)
    return ProviderOperation(
        "apt",
        tuple(package.name for package in packages),
        (
            CommandSpec(("apt-get", "update"), "Refresh APT package metadata", elevated=True),
            CommandSpec(("apt-get", "install", "-y", *identifiers), "Install APT packages", elevated=True),
        ),
    )


def _apt_deb_operation(packages: list[ResolvedPackage], package_manager: PackageManager | None) -> ProviderOperation:
    if package_manager is not PackageManager.APT:
        raise ProviderPlanningError("Local Debian package targets require an apt-based Linux distribution.")
    identifiers = tuple(package.target.identifier for package in packages)
    return ProviderOperation(
        "apt_deb",
        tuple(package.name for package in packages),
        (
            CommandSpec(("apt-get", "update"), "Refresh APT package metadata", elevated=True),
            CommandSpec(("apt-get", "install", "-y", *identifiers), "Install local Debian packages", elevated=True),
        ),
    )


def _flatpak_operation(packages: list[ResolvedPackage]) -> ProviderOperation:
    commands: list[CommandSpec] = []
    by_remote: OrderedDict[str, list[str]] = OrderedDict()
    for package in packages:
        remote = package.target.options.get("remote", "flathub")
        by_remote.setdefault(remote, []).append(package.target.identifier)

    for remote, identifiers in by_remote.items():
        remote_url = _FLATPAK_REMOTES.get(remote)
        if remote_url is None:
            raise ProviderPlanningError(f"Unknown Flatpak remote '{remote}'.")
        commands.append(
            CommandSpec(
                ("flatpak", "remote-add", "--if-not-exists", remote, remote_url),
                f"Ensure Flatpak remote '{remote}' exists",
                elevated=True,
            )
        )
        commands.append(
            CommandSpec(
                ("flatpak", "install", "-y", remote, *identifiers),
                f"Install Flatpak applications from '{remote}'",
                elevated=True,
            )
        )
    return ProviderOperation("flatpak", tuple(package.name for package in packages), tuple(commands))


def _native_linux_operation(
    provider: str,
    packages: list[ResolvedPackage],
    package_manager: PackageManager | None,
) -> ProviderOperation:
    expected_manager = next(manager for manager, name in _PACKAGE_MANAGER_PROVIDERS.items() if name == provider)
    if package_manager is not expected_manager:
        raise ProviderPlanningError(f"{provider} targets require a matching {provider} Linux distribution.")
    identifiers = tuple(package.target.identifier for package in packages)
    command_sets = {
        "dnf": (
            CommandSpec(("dnf", "makecache"), "Refresh DNF package metadata", elevated=True),
            CommandSpec(("dnf", "install", "-y", *identifiers), "Install DNF packages", elevated=True),
        ),
        "pacman": (
            CommandSpec(("pacman", "-Sy", "--needed", "--noconfirm", *identifiers), "Install Pacman packages", elevated=True),
        ),
        "zypper": (
            CommandSpec(("zypper", "--non-interactive", "refresh"), "Refresh Zypper repositories", elevated=True),
            CommandSpec(("zypper", "--non-interactive", "install", *identifiers), "Install Zypper packages", elevated=True),
        ),
        "apk": (
            CommandSpec(("apk", "update"), "Refresh APK package metadata", elevated=True),
            CommandSpec(("apk", "add", *identifiers), "Install APK packages", elevated=True),
        ),
    }
    return ProviderOperation(provider, tuple(package.name for package in packages), command_sets[provider])


def _pipx_operation(packages: list[ResolvedPackage]) -> ProviderOperation:
    commands = tuple(
        CommandSpec(("pipx", "install", "--force", package.target.identifier), f"Install pipx application '{package.name}'")
        for package in packages
    )
    return ProviderOperation("pipx", tuple(package.name for package in packages), commands)


def _npm_operation(packages: list[ResolvedPackage]) -> ProviderOperation:
    commands = tuple(
        CommandSpec(
            ("npm", "install", "--global", package.target.identifier),
            f"Install npm package '{package.name}'",
        )
        for package in packages
    )
    return ProviderOperation("npm", tuple(package.name for package in packages), commands)


def _nodejs_operation(packages: list[ResolvedPackage], package_manager: PackageManager | None) -> NodejsOperation:
    if package_manager is not PackageManager.APT:
        raise ProviderPlanningError("Node.js capability targets require an apt-based Linux distribution.")
    return NodejsOperation(tuple(package.name for package in packages))


def _shell_installer_operation(packages: list[ResolvedPackage]) -> ShellInstallerOperation:
    return ShellInstallerOperation(
        tuple(package.name for package in packages),
        tuple(package.target.identifier for package in packages),
    )


def _snap_operation(packages: list[ResolvedPackage]) -> ProviderOperation:
    commands = tuple(
        CommandSpec(
            ("snap", "install", package.target.identifier),
            f"Install Snap package '{package.name}'",
            elevated=True,
        )
        for package in packages
    )
    return ProviderOperation("snap", tuple(package.name for package in packages), commands)


def _winget_operation(packages: list[ResolvedPackage]) -> ProviderOperation:
    commands = tuple(
        CommandSpec(
            (
                "winget",
                "install",
                "--id",
                package.target.identifier,
                "--exact",
                "--accept-package-agreements",
                "--accept-source-agreements",
            ),
            f"Install Winget package '{package.name}'",
        )
        for package in packages
    )
    return ProviderOperation("winget", tuple(package.name for package in packages), commands)


def _homebrew_operation(packages: list[ResolvedPackage]) -> ProviderOperation:
    formulae = [package.target.identifier for package in packages if package.target.options.get("kind") != "cask"]
    casks = [package.target.identifier for package in packages if package.target.options.get("kind") == "cask"]
    commands: list[CommandSpec] = []
    if formulae:
        commands.append(CommandSpec(("brew", "install", *formulae), "Install Homebrew formulae"))
    if casks:
        commands.append(CommandSpec(("brew", "install", "--cask", *casks), "Install Homebrew casks"))
    return ProviderOperation("homebrew", tuple(package.name for package in packages), tuple(commands))


def plan_provider_operations(
    packages: Iterable[ResolvedPackage],
    package_manager: PackageManager | None,
) -> tuple[ProviderOperation | NodejsOperation | ShellInstallerOperation, ...]:
    """Build batched install commands for all selected provider targets."""

    operations: list[ProviderOperation | NodejsOperation | ShellInstallerOperation] = []
    for provider, grouped_packages in _targets_by_provider(packages).items():
        if provider == "apt":
            operations.append(_apt_operation(grouped_packages, package_manager))
        elif provider == "apt_deb":
            operations.append(_apt_deb_operation(grouped_packages, package_manager))
        elif provider in {"dnf", "pacman", "zypper", "apk"}:
            operations.append(_native_linux_operation(provider, grouped_packages, package_manager))
        elif provider == "flatpak":
            operations.append(_flatpak_operation(grouped_packages))
        elif provider == "pipx":
            operations.append(_pipx_operation(grouped_packages))
        elif provider == "npm":
            operations.append(_npm_operation(grouped_packages))
        elif provider == "nodejs":
            operations.append(_nodejs_operation(grouped_packages, package_manager))
        elif provider == "shell_installer":
            operations.append(_shell_installer_operation(grouped_packages))
        elif provider == "snap":
            operations.append(_snap_operation(grouped_packages))
        elif provider == "winget":
            operations.append(_winget_operation(grouped_packages))
        elif provider == "homebrew":
            operations.append(_homebrew_operation(grouped_packages))
        else:
            raise ProviderPlanningError(f"Unsupported package provider: '{provider}'.")
    return tuple(operations)


def plan_removal_operations(
    delete_packages: Iterable[str],
    package_manager: PackageManager | None,
) -> tuple[ProviderOperation, ...]:
    """Build guarded removal commands for provider package identifiers."""

    identifiers = tuple(delete_packages)
    if not identifiers:
        return ()
    if package_manager is not PackageManager.APT:
        raise ProviderPlanningError("Profile package removals currently require an apt-based Linux distribution.")
    return (
        ProviderOperation(
            "apt",
            identifiers,
            (
                CommandSpec(
                    (
                        "bash",
                        "-c",
                        "for package; do "
                        "if apt-cache show \"$package\" >/dev/null 2>&1 "
                        "&& dpkg-query -W -f='${db:Status-Status}' \"$package\" 2>/dev/null | grep -qx installed; then "
                        "apt-get remove -y \"$package\"; "
                        "fi; "
                        "done",
                        "remove-profile-packages",
                        *identifiers,
                    ),
                    "Remove installed APT packages requested by profiles",
                    elevated=True,
                ),
            ),
        ),
    )


def plan_execution_steps(
    packages: Iterable[ResolvedPackage],
    profile_scripts: Iterable[str],
    package_manager: PackageManager | None,
    delete_packages: Iterable[str] = (),
) -> tuple[ProviderOperation | NodejsOperation | ShellInstallerOperation | ScriptOperation, ...]:
    """Build execution steps while honoring profile and package script boundaries."""

    steps: list[ProviderOperation | NodejsOperation | ShellInstallerOperation | ScriptOperation] = [
        ScriptOperation(script, f"Run profile dependency script '{script}'") for script in profile_scripts
    ]
    pending: list[ResolvedPackage] = []

    def flush_pending() -> None:
        nonlocal pending
        if pending:
            steps.extend(plan_provider_operations(pending, package_manager))
            pending = []

    for package in packages:
        definition_scripts = package.script_dependencies
        if not definition_scripts.before and not definition_scripts.after:
            pending.append(package)
            continue
        flush_pending()
        steps.extend(
            ScriptOperation(script, f"Run pre-install script for '{package.name}': {script}")
            for script in definition_scripts.before
        )
        steps.extend(plan_provider_operations((package,), package_manager))
        steps.extend(
            ScriptOperation(script, f"Run post-install script for '{package.name}': {script}")
            for script in definition_scripts.after
        )
    flush_pending()
    steps.extend(plan_removal_operations(delete_packages, package_manager))
    return tuple(steps)