#!/usr/bin/env python3
"""Generate repo-wide Cargo source overrides from MattOS-owned source trees.

First-class source roots come from upstream/sources.toml. A Cargo package is a
project-wide canonical owner only when it is the root package of one of those
first-class components. Nested packages remain private to their owning upstream
component unless another manifest explicitly depends on that component's Git
repository and requests that nested package by name.

This distinction is important for large imported projects such as Rust, which
contain shims, tests, fixtures and intentionally duplicated package names that
must not become global MattOS dependency owners. It also gives the desired
COSMIC behavior: the first-class cosmic-iced component owns `iced`, while
libcosmic's embedded iced copy may not override it.

The generated .cargo/config.toml contains paths only. Cargo resolves `[patch]`
paths relative to the workspace/root invocation context, so generated paths are
repository-root-relative (for example `src/desktop/cosmic/libcosmic`). Package
versions continue to come from authoritative vendored Cargo.toml files, so
source updates propagate without duplicating version numbers here.
"""

from __future__ import annotations

import argparse
import os
import pathlib
import subprocess
import sys
import tomllib
from collections import defaultdict

ROOT = pathlib.Path(__file__).resolve().parents[1]
SOURCES = ROOT / "upstream" / "sources.toml"
OUTPUT = ROOT / ".cargo" / "config.toml"
DEPENDENCY_TABLES = {"dependencies", "dev-dependencies", "build-dependencies"}


def norm_repo(url: str) -> str:
    value = url.strip().rstrip("/")
    if value.endswith(".git"):
        value = value[:-4]
    return value.lower()


def load_components() -> list[dict[str, str]]:
    with SOURCES.open("rb") as fh:
        data = tomllib.load(fh)
    components = []
    for raw in data.get("component", []):
        path = raw.get("path")
        repo = raw.get("repo")
        name = raw.get("name")
        if not (path and repo and name):
            continue
        absolute = ROOT / path
        if absolute.exists():
            components.append({"name": name, "repo": repo, "path": path})
    return components


def tracked_manifests() -> list[pathlib.Path]:
    proc = subprocess.run(
        ["git", "ls-files", "-z", "--", ":(glob)src/**/Cargo.toml"],
        cwd=ROOT,
        check=True,
        stdout=subprocess.PIPE,
    )
    return [ROOT / pathlib.Path(p.decode()) for p in proc.stdout.split(b"\0") if p]


def read_manifest(manifest: pathlib.Path):
    try:
        with manifest.open("rb") as fh:
            return tomllib.load(fh)
    except (OSError, tomllib.TOMLDecodeError):
        return None


def package_identity(manifest: pathlib.Path):
    data = read_manifest(manifest)
    if data is None:
        return None
    package = data.get("package")
    if not isinstance(package, dict):
        return None
    name = package.get("name")
    if not isinstance(name, str):
        return None
    return name, manifest.parent, data


def dependency_specs(table, out):
    if not isinstance(table, dict):
        return
    for key, value in table.items():
        if isinstance(value, str):
            out.append((key, {"version": value}))
        elif isinstance(value, dict):
            out.append((key, value))


def collect_dependencies(data):
    out = []
    for name in DEPENDENCY_TABLES:
        dependency_specs(data.get(name), out)
    workspace = data.get("workspace")
    if isinstance(workspace, dict):
        dependency_specs(workspace.get("dependencies"), out)
    target = data.get("target")
    if isinstance(target, dict):
        for cfg in target.values():
            if isinstance(cfg, dict):
                for name in DEPENDENCY_TABLES:
                    dependency_specs(cfg.get(name), out)
    return out


def rel_from_config(path: pathlib.Path) -> str:
    # Cargo interprets patch paths from the workspace/root invocation context,
    # not from the `.cargo` directory containing config.toml. Keep generated
    # paths repository-root-relative so `src/...` remains inside MattOS.
    return pathlib.Path(os.path.relpath(path, ROOT)).as_posix()


def quote(value: str) -> str:
    return '"' + value.replace('\\', '\\\\').replace('"', '\\"') + '"'


def generate() -> str:
    components = load_components()
    manifests = tracked_manifests()
    components_by_repo: dict[str, list[dict[str, str]]] = defaultdict(list)
    for component in components:
        components_by_repo[norm_repo(component["repo"])].append(component)

    component_roots = {c["name"]: (ROOT / c["path"]).resolve() for c in components}
    owned_by_component: dict[str, dict[str, pathlib.Path]] = {}

    # Only component-root packages become globally canonical. Nested crates are
    # implementation details of their imported project unless referenced by a
    # Git dependency on that exact project's repository.
    root_owned_packages: dict[str, pathlib.Path] = {}
    root_owner_conflicts: dict[str, list[pathlib.Path]] = defaultdict(list)

    for component in components:
        packages: dict[str, pathlib.Path] = {}
        duplicate_names: set[str] = set()
        root = component_roots[component["name"]]
        for manifest in manifests:
            try:
                manifest.resolve().relative_to(root)
            except ValueError:
                continue
            identity = package_identity(manifest)
            if identity is None:
                continue
            name, package_path, _ = identity
            if name in duplicate_names:
                continue
            old = packages.get(name)
            if old is not None and old != package_path:
                packages.pop(name, None)
                duplicate_names.add(name)
                continue
            packages[name] = package_path
        owned_by_component[component["name"]] = packages

        root_identity = package_identity(root / "Cargo.toml")
        if root_identity is not None:
            root_name, root_path, _ = root_identity
            previous = root_owned_packages.get(root_name)
            if previous is None:
                root_owned_packages[root_name] = root_path
            elif previous.resolve() != root_path.resolve():
                root_owner_conflicts[root_name].extend([previous, root_path])
                root_owned_packages.pop(root_name, None)

    git_patches: dict[str, dict[str, pathlib.Path]] = defaultdict(dict)
    registry_patches: dict[str, pathlib.Path] = {}
    unresolved: list[str] = []

    for manifest in manifests:
        data = read_manifest(manifest)
        if data is None:
            continue
        for key, spec in collect_dependencies(data):
            package = spec.get("package", key)
            if not isinstance(package, str):
                continue

            git = spec.get("git")
            if isinstance(git, str):
                matching_components = components_by_repo.get(norm_repo(git), [])
                if not matching_components:
                    continue

                # A first-class component whose root package has this name wins
                # even if another imported component embeds a private copy.
                candidate = root_owned_packages.get(package)
                if candidate is None:
                    local_candidates = {
                        owned_by_component.get(component["name"], {}).get(package)
                        for component in matching_components
                    }
                    local_candidates.discard(None)
                    if len(local_candidates) == 1:
                        candidate = next(iter(local_candidates))

                if candidate is not None:
                    git_patches[git][package] = candidate
                # If the requested package is not present in MattOS's copy of
                # that repository, there is no local source to substitute. Keep
                # the upstream dependency rather than inventing an owner. This
                # is intentionally different from resolving an existing owned
                # package to the wrong copy.
                continue

            path_value = spec.get("path")
            if isinstance(path_value, str):
                candidate = root_owned_packages.get(package)
                if candidate is None:
                    continue
                resolved = (manifest.parent / path_value).resolve()
                if resolved != candidate.resolve():
                    display_resolved = (
                        resolved.relative_to(ROOT) if resolved.is_relative_to(ROOT) else resolved
                    )
                    unresolved.append(
                        f"{manifest.relative_to(ROOT)}: path dependency {package!r} resolves to "
                        f"{display_resolved}, but first-class MattOS ownership requires "
                        f"{candidate.relative_to(ROOT)}"
                    )
                continue

            if "workspace" in spec:
                continue
            candidate = root_owned_packages.get(package)
            if candidate is not None:
                registry_patches[package] = candidate

    if root_owner_conflicts:
        for package, paths in sorted(root_owner_conflicts.items()):
            unique = sorted({str(path.relative_to(ROOT)) for path in paths})
            unresolved.append(
                f"first-class package {package!r} has multiple component roots: {', '.join(unique)}"
            )

    if unresolved:
        formatted = "\n  ".join(sorted(set(unresolved)))
        raise SystemExit(
            "source ownership generation failed; MattOS-owned dependencies must resolve to one canonical source:\n  "
            + formatted
        )

    lines = [
        "# GENERATED by DevUtils/generate_source_overrides.py; DO NOT EDIT.",
        "# MattOS source-ownership invariant: first-class source components",
        "# listed in upstream/sources.toml own their root Cargo package.",
        "# Git dependencies on owned component repositories are rebound to",
        "# packages actually present in those local source trees.",
        "# Paths below are relative to the MattOS repository root.",
        "# Package versions are read from the local source manifests.",
        "",
    ]
    for source in sorted(git_patches, key=norm_repo):
        lines.append(f"[patch.{quote(source)}]")
        for package, path in sorted(git_patches[source].items()):
            lines.append(f"{package} = {{ path = {quote(rel_from_config(path))} }}")
        lines.append("")

    if registry_patches:
        lines.append("[patch.crates-io]")
        for package, path in sorted(registry_patches.items()):
            lines.append(f"{package} = {{ path = {quote(rel_from_config(path))} }}")
        lines.append("")

    return "\n".join(lines)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true", help="fail if .cargo/config.toml is stale")
    args = parser.parse_args()
    generated = generate()
    if args.check:
        current = OUTPUT.read_text() if OUTPUT.exists() else ""
        if current != generated:
            print(
                "MattOS Cargo source overrides are stale; run DevUtils/generate_source_overrides.py",
                file=sys.stderr,
            )
            return 1
        return 0
    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    OUTPUT.write_text(generated)
    print(f"wrote {OUTPUT.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
