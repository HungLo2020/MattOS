#!/usr/bin/env python3
"""Generate repo-wide Cargo source overrides from MattOS-owned source trees.

First-class source roots come from upstream/sources.toml.  For every Cargo
manifest below src/, external git dependencies that point at an owned upstream
repository are rebound to the package with the same Cargo package name inside
that owned source root.  Registry dependencies are rebound only when their
package name has one unique first-class owner.  The generated .cargo/config.toml
contains paths only; package versions continue to come from the authoritative
vendored Cargo.toml files themselves.
"""

from __future__ import annotations

import argparse
import os
import pathlib
import sys
import tomllib
from collections import defaultdict

ROOT = pathlib.Path(__file__).resolve().parents[1]
SOURCES = ROOT / "upstream" / "sources.toml"
OUTPUT = ROOT / ".cargo" / "config.toml"
SKIP_DIRS = {".git", "target", "vendor", "third_party", "node_modules", "out"}
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
        if not absolute.exists():
            continue
        components.append({"name": name, "repo": repo, "path": path})
    return components


def walk_manifests(root: pathlib.Path):
    for current, dirs, files in os.walk(root):
        dirs[:] = [d for d in dirs if d not in SKIP_DIRS and not d.startswith(".")]
        if "Cargo.toml" in files:
            yield pathlib.Path(current) / "Cargo.toml"


def package_identity(manifest: pathlib.Path):
    try:
        with manifest.open("rb") as fh:
            data = tomllib.load(fh)
    except (OSError, tomllib.TOMLDecodeError):
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
    by_repo = {norm_repo(c["repo"]): c for c in components}
    owned_by_component: dict[str, dict[str, pathlib.Path]] = {}
    owners: dict[str, list[tuple[str, pathlib.Path]]] = defaultdict(list)

    for component in components:
        packages: dict[str, pathlib.Path] = {}
        root = ROOT / component["path"]
        for manifest in walk_manifests(root):
            identity = package_identity(manifest)
            if identity is None:
                continue
            name, package_path, _ = identity
            old = packages.get(name)
            if old is not None and old != package_path:
                # Internal duplicate names are not safe to auto-own.
                packages.pop(name, None)
                continue
            packages[name] = package_path
        owned_by_component[component["name"]] = packages
        for name, path in packages.items():
            owners[name].append((component["name"], path))

    git_patches: dict[str, dict[str, pathlib.Path]] = defaultdict(dict)
    registry_patches: dict[str, pathlib.Path] = {}
    ambiguities: list[str] = []

    for manifest in walk_manifests(ROOT / "src"):
        identity = package_identity(manifest)
        if identity is None:
            try:
                with manifest.open("rb") as fh:
                    data = tomllib.load(fh)
            except (OSError, tomllib.TOMLDecodeError):
                continue
        else:
            _, _, data = identity

        for key, spec in collect_dependencies(data):
            package = spec.get("package", key)
            if not isinstance(package, str):
                continue
            git = spec.get("git")
            if isinstance(git, str):
                component = by_repo.get(norm_repo(git))
                if component is None:
                    continue
                candidate = owned_by_component.get(component["name"], {}).get(package)
                if candidate is None:
                    ambiguities.append(
                        f"{manifest.relative_to(ROOT)}: {package} points at owned repo {git} "
                        f"but no unique package {package!r} exists under {component['path']}"
                    )
                    continue
                git_patches[git][package] = candidate
                continue

            if "path" in spec or "workspace" in spec:
                continue
            candidates = owners.get(package, [])
            if len(candidates) == 1:
                registry_patches[package] = candidates[0][1]
            elif len(candidates) > 1:
                # Ambiguous registry names must remain external until ownership
                # is made explicit instead of silently choosing a source tree.
                continue

    lines = [
        "# GENERATED by DevUtils/generate_source_overrides.py; DO NOT EDIT.",
        "# MattOS source-ownership invariant: when a dependency resolves to a",
        "# first-class source tree listed in upstream/sources.toml, Cargo must",
        "# consume that tree instead of downloading another copy.",
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

    if ambiguities:
        lines.append("# Unresolved owned-repository references (generation is fail-closed):")
        for item in sorted(set(ambiguities)):
            lines.append("# " + item)
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
            print("MattOS Cargo source overrides are stale; run DevUtils/generate_source_overrides.py", file=sys.stderr)
            return 1
        return 0
    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    OUTPUT.write_text(generated)
    print(f"wrote {OUTPUT.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
