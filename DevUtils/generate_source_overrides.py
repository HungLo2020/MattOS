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
import hashlib
import json
import pathlib
import subprocess
import tomllib
from collections import defaultdict

ROOT = pathlib.Path(__file__).resolve().parents[1]
SOURCES = ROOT / 'upstream' / 'sources.toml'
STATE_DIR = ROOT / 'upstream' / 'state'
GITLINK_POLICY = ROOT / 'upstream' / 'policies' / 'gitlinks.toml'
OUTPUT_ROOT = ROOT / 'out' / 'source-ownership' / 'cargo'
INDEX = OUTPUT_ROOT / 'index.json'
CONTRACT_ROOT = OUTPUT_ROOT / 'contracts'
OWNERSHIP_CONTRACT_SCHEMA_VERSION = 1
LEGACY_ROOT_CONFIG = ROOT / '.cargo' / 'config.toml'


def norm_repo(url: str) -> str:
    value = url.strip().rstrip('/')
    if value.endswith('.git'):
        value = value[:-4]
    return value.lower()


def fail(message: str) -> None:
    raise SystemExit(f'source ownership generation failed: {message}')


def validate_patch_provenance(component: dict) -> None:
    name = component['name']
    revision = component['revision']
    manifest_rel = component.get('patch_manifest')
    expected_manifest_sha = component.get('patch_manifest_sha256')
    if not manifest_rel and not expected_manifest_sha:
        return
    if not isinstance(manifest_rel, str) or not manifest_rel:
        fail(f'{name}: patch_manifest_sha256 exists without patch_manifest')
    if not isinstance(expected_manifest_sha, str) or not expected_manifest_sha:
        fail(f'{name}: patch_manifest exists without patch_manifest_sha256')

    manifest_path = ROOT / manifest_rel
    if not manifest_path.is_file():
        fail(f'{name}: patch manifest is missing: {manifest_rel}')
    payload = manifest_path.read_bytes()
    actual_manifest_sha = hashlib.sha256(payload).hexdigest()
    if actual_manifest_sha != expected_manifest_sha:
        fail(
            f'{name}: patch manifest checksum mismatch for {manifest_rel}: '
            f'sources.toml={expected_manifest_sha}, actual={actual_manifest_sha}'
        )

    state_path = STATE_DIR / f'{name}.toml'
    if state_path.is_file():
        with state_path.open('rb') as stream:
            state = tomllib.load(stream)
        if state.get('patch_manifest') != manifest_rel:
            fail(
                f'{name}: provenance state patch_manifest {state.get("patch_manifest")!r} '
                f'does not match sources.toml {manifest_rel!r}'
            )
        if state.get('patch_manifest_sha256') != expected_manifest_sha:
            fail(
                f'{name}: provenance state patch_manifest_sha256 '
                f'{state.get("patch_manifest_sha256")!r} does not match sources.toml '
                f'{expected_manifest_sha!r}'
            )

    try:
        manifest = tomllib.loads(payload.decode('utf-8'))
    except (UnicodeDecodeError, tomllib.TOMLDecodeError) as exc:
        fail(f'{name}: invalid patch manifest {manifest_rel}: {exc}')
    if manifest.get('component') != name:
        fail(f'{name}: patch manifest component is {manifest.get("component")!r}')
    if manifest.get('upstream_commit') != revision:
        fail(
            f'{name}: patch manifest upstream_commit {manifest.get("upstream_commit")!r} '
            f'does not match pinned revision {revision!r}'
        )
    if manifest.get('application') != 'output-mirror-only':
        fail(
            f'{name}: patch manifest application must be output-mirror-only, '
            f'got {manifest.get("application")!r}'
        )

    seen_paths: set[str] = set()
    for item in manifest.get('patch', []):
        if not isinstance(item, dict):
            fail(f'{name}: malformed patch entry in {manifest_rel}')
        patch_rel = item.get('path')
        expected_patch_sha = item.get('sha256')
        if not isinstance(patch_rel, str) or not patch_rel:
            fail(f'{name}: patch entry has no path in {manifest_rel}')
        if patch_rel in seen_paths:
            fail(f'{name}: duplicate patch path in {manifest_rel}: {patch_rel}')
        seen_paths.add(patch_rel)
        if not isinstance(expected_patch_sha, str) or not expected_patch_sha:
            fail(f'{name}: patch entry has no sha256: {patch_rel}')
        patch_path = ROOT / patch_rel
        if not patch_path.is_file():
            fail(f'{name}: patch payload is missing: {patch_rel}')
        actual_patch_sha = hashlib.sha256(patch_path.read_bytes()).hexdigest()
        if actual_patch_sha != expected_patch_sha:
            fail(
                f'{name}: patch payload checksum mismatch for {patch_rel}: '
                f'manifest={expected_patch_sha}, actual={actual_patch_sha}'
            )


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
        component = {
            'name': name,
            'repo': repo,
            'revision': revision,
            'source_path': path,
            'patch_manifest': raw.get('patch_manifest'),
            'patch_manifest_sha256': raw.get('patch_manifest_sha256'),
        }
        validate_patch_provenance(component)
        components.append(component)
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


def write_ownership_contracts(index: dict) -> None:
    """Publish deterministic per-component ownership contracts for caches."""
    import sys
    sys.path.insert(0, str(ROOT / 'DevUtils'))
    import source_ownership_graph as graph  # type: ignore

    CONTRACT_ROOT.mkdir(parents=True, exist_ok=True)
    expected: set[str] = set()
    for component in sorted(index.get('components', {})):
        rewrite_contract = graph.ownership_rewrite_contract(ROOT, index, component)
        patch_records = []
        state_records = []
        for name, metadata in sorted(rewrite_contract.get('components', {}).items()):
            manifest_rel = metadata.get('patch_manifest')
            if isinstance(manifest_rel, str) and manifest_rel:
                manifest_path = ROOT / manifest_rel
                manifest_payload = manifest_path.read_bytes()
                manifest = tomllib.loads(manifest_payload.decode())
                patch_records.append({
                    'component': name,
                    'manifest': manifest_rel,
                    'manifest_sha256': hashlib.sha256(manifest_payload).hexdigest(),
                    'patches': [
                        {
                            'path': item['path'],
                            'sha256': hashlib.sha256((ROOT / item['path']).read_bytes()).hexdigest(),
                        }
                        for item in manifest.get('patch', [])
                    ],
                })
            state_path = ROOT / 'upstream/state' / f'{name}.toml'
            if state_path.is_file():
                state_records.append({
                    'component': name,
                    'sha256': hashlib.sha256(state_path.read_bytes()).hexdigest(),
                })
        contract = {
            'schema_version': OWNERSHIP_CONTRACT_SCHEMA_VERSION,
            'component': component,
            'rewrite_contract': rewrite_contract,
            'patches': patch_records,
            'state': state_records,
        }
        body = json.dumps(contract, indent=2, sort_keys=True) + '\n'
        digest = hashlib.sha256(body.encode()).hexdigest()
        destination = CONTRACT_ROOT / f'{component}.json'
        destination.write_text(
            json.dumps({**contract, 'digest': digest}, indent=2, sort_keys=True) + '\n',
            encoding='utf-8',
        )
        expected.add(destination.name)
    for stale in CONTRACT_ROOT.glob('*.json'):
        if stale.name not in expected:
            stale.unlink()


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    generated = json.dumps(generate_index(), indent=2, sort_keys=True) + '\n'
    current = INDEX.read_text(encoding='utf-8') if INDEX.exists() else ''
    if args.check:
        if current != generated:
            raise SystemExit('MattOS Cargo source ownership catalog is stale')
        if not CONTRACT_ROOT.is_dir():
            raise SystemExit('MattOS Cargo ownership contracts are missing')
        import sys
        sys.path.insert(0, str(ROOT / 'DevUtils'))
        import source_ownership_graph as graph  # type: ignore
        expected_names = {f'{name}.json' for name in json.loads(generated).get('components', {})}
        actual_names = {path.name for path in CONTRACT_ROOT.glob('*.json')}
        if actual_names != expected_names:
            raise SystemExit('MattOS Cargo ownership contracts are stale')
        for name in sorted(json.loads(generated).get('components', {})):
            path = CONTRACT_ROOT / f'{name}.json'
            payload = json.loads(path.read_text(encoding='utf-8'))
            digest = payload.pop('digest', None)
            body = json.dumps(payload, indent=2, sort_keys=True) + '\n'
            if digest != hashlib.sha256(body.encode()).hexdigest():
                raise SystemExit(f'MattOS Cargo ownership contract checksum is stale: {name}')
            if payload.get('rewrite_contract') != graph.ownership_rewrite_contract(
                ROOT, json.loads(generated), name
            ):
                raise SystemExit(f'MattOS Cargo ownership contract is stale: {name}')
        return 0
    OUTPUT_ROOT.mkdir(parents=True, exist_ok=True)
    INDEX.write_text(generated, encoding='utf-8')
    write_ownership_contracts(json.loads(generated))
    if LEGACY_ROOT_CONFIG.exists():
        LEGACY_ROOT_CONFIG.unlink()
    for path in OUTPUT_ROOT.glob('*/config.toml'):
        path.unlink()
    print(f'wrote MattOS Cargo source ownership catalog to {INDEX.relative_to(ROOT)}')
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
