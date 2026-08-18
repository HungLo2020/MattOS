#!/usr/bin/env python3
"""Generate the MattOS Cargo source-ownership catalog.

The catalog describes authoritative package ownership. It deliberately does not
use Cargo [patch] as the enforcement mechanism: [patch] participates in normal
resolver/lockfile selection and is therefore too weak for MattOS's invariant.
Build mirrors are rewritten to explicit path dependencies by
DevUtils/source_ownership_graph.py before Cargo resolves them.
"""
from __future__ import annotations

import argparse
import json
import pathlib
import subprocess
import tomllib
from collections import defaultdict

ROOT = pathlib.Path(__file__).resolve().parents[1]
SOURCES = ROOT / 'upstream' / 'sources.toml'
GITLINK_POLICY = ROOT / 'upstream' / 'policies' / 'gitlinks.toml'
OUTPUT_ROOT = ROOT / 'out' / 'source-ownership' / 'cargo'
INDEX = OUTPUT_ROOT / 'index.json'
LEGACY_ROOT_CONFIG = ROOT / '.cargo' / 'config.toml'


def norm_repo(url: str) -> str:
    value = url.strip().rstrip('/')
    if value.endswith('.git'):
        value = value[:-4]
    return value.lower()


def load_sources() -> list[dict]:
    with SOURCES.open('rb') as stream:
        data = tomllib.load(stream)
    components = []
    for raw in data.get('component', []):
        name, repo, path = raw.get('name'), raw.get('repo'), raw.get('path')
        revision = raw.get('revision')
        if not all(isinstance(v, str) and v for v in (name, repo, path, revision)):
            continue
        if not (ROOT / path).exists():
            continue
        components.append({
            'name': name,
            'repo': repo,
            'revision': revision,
            'source_path': path,
            'patch_manifest': raw.get('patch_manifest'),
            'patch_manifest_sha256': raw.get('patch_manifest_sha256'),
        })
    return components


def tracked_manifests() -> list[pathlib.Path]:
    result = subprocess.run(
        ['git', 'ls-files', '-z', '--', ':(glob)src/**/Cargo.toml'],
        cwd=ROOT,
        check=True,
        stdout=subprocess.PIPE,
    )
    return [ROOT / pathlib.Path(raw.decode()) for raw in result.stdout.split(b'\0') if raw]


def read_manifest(path: pathlib.Path):
    try:
        with path.open('rb') as stream:
            return tomllib.load(stream)
    except (OSError, tomllib.TOMLDecodeError):
        return None


def owner_for_path(path: pathlib.Path, components: list[dict]) -> dict | None:
    resolved = path.resolve()
    matches = []
    for component in components:
        root = (ROOT / component['source_path']).resolve()
        try:
            rel = resolved.relative_to(root)
        except ValueError:
            continue
        matches.append((len(rel.parts), component))
    return min(matches, key=lambda item: item[0])[1] if matches else None


def generate_index() -> dict:
    components = load_sources()
    manifests = tracked_manifests()
    packages_by_component: dict[str, dict[str, str]] = defaultdict(dict)
    duplicates: dict[str, set[str]] = defaultdict(set)

    for manifest in manifests:
        owner = owner_for_path(manifest.parent, components)
        if owner is None:
            continue
        data = read_manifest(manifest)
        package = data.get('package') if isinstance(data, dict) else None
        name = package.get('name') if isinstance(package, dict) else None
        if not isinstance(name, str):
            continue
        root = ROOT / owner['source_path']
        rel = manifest.parent.resolve().relative_to(root.resolve()).as_posix()
        rel = '' if rel == '.' else rel
        current = packages_by_component[owner['name']].get(name)
        if current is not None and current != rel:
            duplicates[owner['name']].add(name)
            packages_by_component[owner['name']].pop(name, None)
        elif name not in duplicates[owner['name']]:
            packages_by_component[owner['name']][name] = rel

    root_packages: dict[str, dict[str, str]] = {}
    conflicts: dict[str, list[str]] = defaultdict(list)
    for component in components:
        manifest = ROOT / component['source_path'] / 'Cargo.toml'
        data = read_manifest(manifest)
        package = data.get('package') if isinstance(data, dict) else None
        name = package.get('name') if isinstance(package, dict) else None
        if not isinstance(name, str):
            continue
        target = {'component': component['name'], 'package_path': ''}
        if name in root_packages and root_packages[name] != target:
            conflicts[name].extend([root_packages[name]['component'], component['name']])
            root_packages.pop(name, None)
        elif name not in conflicts:
            root_packages[name] = target

    if conflicts:
        detail = '\n  '.join(
            f"first-class package {name!r} has multiple roots: {', '.join(sorted(set(owners)))}"
            for name, owners in sorted(conflicts.items())
        )
        raise SystemExit('source ownership generation failed:\n  ' + detail)

    repo_map: dict[str, list[str]] = defaultdict(list)
    output_components = {}
    for component in components:
        repo_map[norm_repo(component['repo'])].append(component['name'])
        output_components[component['name']] = {
            **component,
            'packages': dict(sorted(packages_by_component.get(component['name'], {}).items())),
        }

    gitlink_replacements: dict[str, list[dict[str, str]]] = defaultdict(list)
    if GITLINK_POLICY.is_file():
        with GITLINK_POLICY.open('rb') as stream:
            policy = tomllib.load(stream)
        for item in policy.get('component', []):
            owner = item.get('name')
            if owner not in output_components:
                continue
            for link in item.get('gitlink', []):
                if link.get('action') != 'replacement':
                    continue
                replacement = link.get('replacement_component')
                path = link.get('path')
                if replacement in output_components and isinstance(path, str):
                    gitlink_replacements[owner].append({'path': path, 'component': replacement})

    return {
        'version': 3,
        'components': dict(sorted(output_components.items())),
        'repos': {repo: sorted(names) for repo, names in sorted(repo_map.items())},
        'root_packages': dict(sorted(root_packages.items())),
        'gitlink_replacements': {
            k: sorted(v, key=lambda item: item['path']) for k, v in sorted(gitlink_replacements.items())
        },
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    generated = json.dumps(generate_index(), indent=2, sort_keys=True) + '\n'
    current = INDEX.read_text(encoding='utf-8') if INDEX.exists() else ''
    if args.check:
        if current != generated:
            raise SystemExit('MattOS Cargo source ownership catalog is stale')
        return 0
    OUTPUT_ROOT.mkdir(parents=True, exist_ok=True)
    INDEX.write_text(generated, encoding='utf-8')
    if LEGACY_ROOT_CONFIG.exists():
        LEGACY_ROOT_CONFIG.unlink()
    for path in OUTPUT_ROOT.glob('*/config.toml'):
        path.unlink()
    print(f'wrote MattOS Cargo source ownership catalog to {INDEX.relative_to(ROOT)}')
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
