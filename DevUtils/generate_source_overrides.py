#!/usr/bin/env python3
"""Generate repo-wide Cargo source overrides from MattOS-owned source trees.

First-class source roots come from upstream/sources.toml. For every tracked
Cargo manifest below src/, external git dependencies that point at an owned
upstream repository are rebound to the canonical MattOS package with the same
Cargo package name. When the same package exists in more than one imported tree,
the package closest to a first-class component root wins; ties are ambiguous and
fail closed. This makes a standalone component such as src/desktop/cosmic/iced
authoritative over a duplicate copy embedded beneath libcosmic.

Registry dependencies are rebound only when the component's *root* Cargo package
has that name; this deliberately avoids mistaking toolchain/vendor internals
(for example a vendored `serde` or `libc`) for first-class MattOS ownership.

The generated .cargo/config.toml contains paths only. Package versions continue
to come from the authoritative vendored Cargo.toml files themselves, so source
updates automatically propagate without duplicating version numbers here.
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
    return pathlib.Path(os.path.relpath(path, ROOT / ".cargo")).as_posix()


def quote(value: str) -> str:
    return '"' + value.replace('\\', '\\\\').replace('"', '\\"') + '"'


def generate() -> str:
    components = load_components()
    manifests = tracked_manifests()
    by_repo = {norm_repo(c["repo"]): c for c in components}
    component_roots = {c["name"]: (ROOT / c["path"]).resolve() for c in components}
    owned_by_component: dict[str, dict[str, pathlib.Path]] = {}
    global_candidates: dict[str, list[tuple[int, str, pathlib.Path]]] = defaultdict(list)
    root_owned_packages: dict[str, pathlib.Path] = {}
    root_owner_conflicts: set[str] = set()

    for component in components:
        packages: dict[str, pathlib.Path] = {}
        duplicate_names: set[str] = set()
        root = component_roots[component["name"]]
        for manifest in manifests:
            try:
                relative_manifest = manifest.resolve().relative_to(root)
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
            depth = max(0, len(relative_manifest.parent.parts))
            global_candidates[name].append((depth, component["name"], package_path))
        owned_by_component[component["name"]] = packages

        root_identity = package_identity(root / "Cargo.toml")
        if root_identity is not None:
            root_name, root_path, _ = root_identity
            if root_name in root_owner_conflicts:
                continue
            old = root_owned_packages.get(root_name)
            if old is not None and old != root_path:
                root_owned_packages.pop(root_name, None)
                root_owner_conflicts.add(root_name)
            else:
                root_owned_packages[root_name] = root_path

    canonical_owner: dict[str, pathlib.Path] = {}
    ambiguous_owners: dict[str, list[tuple[int, str, pathlib.Path]]] = {}
    for package, candidates in global_candidates.items():
        # Collapse duplicate observations of the exact same package path before
        # comparing first-class ownership depth.
        unique = {}
        for depth, component_name, path in candidates:
            key = path.resolve()
            prior = unique.get(key)
            if prior is None or depth < prior[0]:
                unique[key] = (depth, component_name, path)
        ordered = sorted(unique.values(), key=lambda item: (item[0], item[1], str(item[2])))
        if not ordered:
            continue
        best_depth = ordered[0][0]
        best = [item for item in ordered if item[0] == best_depth]
        if len(best) == 1:
            canonical_owner[package] = best[0][2]
        else:
            ambiguous_owners[package] = best

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

            if package in ambiguous_owners:
                candidates = ", ".join(
                    f"{component}:{path.relative_to(ROOT)}"
                    for _, component, path in ambiguous_owners[package]
                )
                unresolved.append(
                    f"{manifest.relative_to(ROOT)}: package {package!r} has ambiguous first-class owners: {candidates}"
                )
                continue

            git = spec.get("git")
            if isinstance(git, str):
                component = by_repo.get(norm_repo(git))
                if component is None:
                    continue
                candidate = canonical_owner.get(package)
                if candidate is None:
                    candidate = owned_by_component.get(component["name"], {}).get(package)
                if candidate is None:
                    unresolved.append(
                        f"{manifest.relative_to(ROOT)}: {package} points at owned repo {git} "
                        f"but no unique MattOS package {package!r} is available"
                    )
                    continue
                git_patches[git][package] = candidate
                continue

            path_value = spec.get("path")
            if isinstance(path_value, str):
                candidate = canonical_owner.get(package)
                if candidate is None:
                    continue
                resolved = (manifest.parent / path_value).resolve()
                if resolved != candidate.resolve():
                    # Only police path dependencies when the requested package
                    # name itself is source-owned. This catches embedded copies
                    # such as libcosmic/iced without interfering with private
                    # helper crates that have no first-class owner.
                    unresolved.append(
                        f"{manifest.relative_to(ROOT)}: path dependency {package!r} resolves to "
                        f"{resolved.relative_to(ROOT) if resolved.is_relative_to(ROOT) else resolved}, "
                        f"but MattOS owns it at {candidate.relative_to(ROOT)}"
                    )
                continue

            if "workspace" in spec:
                continue
            candidate = root_owned_packages.get(package)
            if candidate is not None:
                registry_patches[package] = candidate

    if unresolved:
        formatted = "\n  ".join(sorted(set(unresolved)))
        raise SystemExit(
            "source ownership generation failed; MattOS-owned dependencies must resolve to one canonical source:\n  "
            + formatted
        )

    lines = [
        "# GENERATED by DevUtils/generate_source_overrides.py; DO NOT EDIT.",
        "# MattOS source-ownership invariant: when a dependency resolves to a",
        "# first-class source tree listed in upstream/sources.toml, Cargo must",
        "# consume that tree instead of downloading another copy.",
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
