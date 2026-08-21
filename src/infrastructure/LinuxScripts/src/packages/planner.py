"""Resolve profile and package dependency graphs into host-specific plans."""

from __future__ import annotations

from collections.abc import Mapping, Sequence

from packages.models import PackageDefinition, PackagePlan, PackageTarget, ProfileDefinition, ResolvedPackage


class PackageResolutionError(ValueError):
    """Raised when profiles or package dependencies cannot be resolved."""


_NATIVE_LINUX_PROVIDERS = {"apt", "dnf", "pacman", "zypper", "apk"}


def _platform_hierarchy(platform_name: str) -> tuple[str, ...]:
    return (platform_name,)


def _select_target(
    package: PackageDefinition,
    platform_name: str,
    preferred_providers: Sequence[str],
) -> PackageTarget | None:
    exact_targets = [
        target
        for compatible_platform in _platform_hierarchy(platform_name)
        for target in package.targets
        if target.platform == compatible_platform
    ]
    shared_targets = [target for target in package.targets if target.platform == "all"]
    candidates = exact_targets or shared_targets
    for provider in preferred_providers:
        matching_target = next((target for target in candidates if target.provider == provider), None)
        if matching_target is not None:
            return matching_target
    if preferred_providers and any(target.provider in _NATIVE_LINUX_PROVIDERS for target in candidates):
        return None
    return next(iter(candidates), None)


def _unavailable_target_reason(
    package: PackageDefinition,
    platform_name: str,
    preferred_providers: Sequence[str],
) -> str:
    compatible_targets = [
        target
        for compatible_platform in _platform_hierarchy(platform_name)
        for target in package.targets
        if target.platform == compatible_platform
    ]
    if not compatible_targets:
        compatible_targets = [target for target in package.targets if target.platform == "all"]
    if preferred_providers and compatible_targets:
        provider = preferred_providers[0]
        return f"Package '{package.name}' is not available for the {provider} package manager on {platform_name}."
    return f"No package target is defined for {platform_name}."


def _expand_profiles(
    requested_profiles: Sequence[str],
    profiles: Mapping[str, ProfileDefinition],
    platform_name: str,
) -> tuple[tuple[str, ...], tuple[tuple[str, bool], ...], tuple[str, ...], tuple[str, ...]]:
    expanded: list[str] = []
    requested_packages: dict[str, bool] = {}
    profile_scripts: dict[str, None] = {}
    delete_packages: dict[str, None] = {}
    visiting: set[str] = set()
    visited: set[str] = set()

    def visit(name: str) -> None:
        if name in visited:
            return
        if name in visiting:
            raise PackageResolutionError(f"Profile dependency cycle detected at '{name}'.")
        profile = profiles.get(name)
        if profile is None:
            raise PackageResolutionError(f"Unknown profile: '{name}'.")

        visiting.add(name)
        for included_profile in profile.includes:
            visit(included_profile)
        visiting.remove(name)
        visited.add(name)
        expanded.append(name)
        for script in profile.script_dependencies:
            profile_scripts[script] = None
        platform_packages = profile.platform_packages.get(platform_name, ())
        for package in (*profile.packages, *platform_packages):
            requested_packages[package.name] = requested_packages.get(package.name, False) or package.required
        for package_name in profile.platform_delete_packages.get(platform_name, ()):
            delete_packages[package_name] = None

    for profile_name in requested_profiles:
        visit(profile_name)
    return tuple(expanded), tuple(requested_packages.items()), tuple(profile_scripts), tuple(delete_packages)


def resolve_profiles(
    requested_profiles: Sequence[str],
    catalog: Mapping[str, PackageDefinition],
    profiles: Mapping[str, ProfileDefinition],
    platform_name: str,
    preferred_providers: Sequence[str] = (),
) -> PackagePlan:
    """Resolve profile and package dependencies in install order for one platform."""

    expanded_profiles, requested_packages, profile_scripts, delete_packages = _expand_profiles(
        requested_profiles,
        profiles,
        platform_name,
    )
    resolved: list[ResolvedPackage] = []
    skipped: dict[str, str] = {}
    visiting: set[str] = set()
    states: dict[str, bool] = {}

    def visit(name: str, required: bool) -> bool:
        known_state = states.get(name)
        if known_state is not None:
            if not known_state and required:
                raise PackageResolutionError(f"Required package '{name}' is unavailable on {platform_name}.")
            return known_state
        if name in visiting:
            raise PackageResolutionError(f"Package dependency cycle detected at '{name}'.")

        package = catalog.get(name)
        if package is None:
            raise PackageResolutionError(f"Unknown package: '{name}'.")

        target = _select_target(package, platform_name, preferred_providers)
        if target is None:
            reason = _unavailable_target_reason(package, platform_name, preferred_providers)
            skipped[name] = reason
            states[name] = False
            if required:
                raise PackageResolutionError(reason)
            return False

        visiting.add(name)
        dependencies_ready = True
        for dependency in (*package.dependencies, *target.dependencies):
            if not visit(dependency, required=True):
                dependencies_ready = False
                skipped[name] = f"Required dependency '{dependency}' is unavailable on {platform_name}."
                break
        visiting.remove(name)

        states[name] = dependencies_ready
        if dependencies_ready:
            resolved.append(ResolvedPackage(name, target, package.script_dependencies))
        return dependencies_ready

    for package_name, required in requested_packages:
        visit(package_name, required)

    installed_identifiers = {package.target.identifier for package in resolved}
    conflicting_packages = sorted(installed_identifiers.intersection(delete_packages))
    if conflicting_packages:
        names = ", ".join(conflicting_packages)
        raise PackageResolutionError(f"Profiles both install and delete package identifiers: {names}")

    return PackagePlan(expanded_profiles, tuple(resolved), skipped, profile_scripts, delete_packages)