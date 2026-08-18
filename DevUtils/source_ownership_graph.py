from __future__ import annotations

import hashlib
import json
import os
import pathlib
import re
import shutil
import subprocess
import tomllib
from typing import Any

BARE_KEY = re.compile(r'^[A-Za-z0-9_-]+$')
GIT_KEYS = {'git', 'rev', 'branch', 'tag'}
DEP_TABLE_NAMES = {'dependencies', 'dev-dependencies', 'build-dependencies'}


class OwnershipError(RuntimeError):
    pass


def norm_repo(url: str) -> str:
    value = url.strip().rstrip('/')
    if value.endswith('.git'):
        value = value[:-4]
    return value.lower()


def qkey(key: str) -> str:
    if BARE_KEY.fullmatch(key):
        return key
    return json.dumps(key)


def toml_value(value: Any) -> str:
    if isinstance(value, bool):
        return 'true' if value else 'false'
    if isinstance(value, str):
        return json.dumps(value)
    if isinstance(value, int):
        return str(value)
    if isinstance(value, float):
        return repr(value)
    if isinstance(value, list):
        return '[' + ', '.join(toml_value(v) if not isinstance(v, dict) else inline_table(v) for v in value) + ']'
    if isinstance(value, dict):
        return inline_table(value)
    raise OwnershipError(f'unsupported TOML value type: {type(value).__name__}')


def inline_table(table: dict[str, Any]) -> str:
    return '{ ' + ', '.join(f'{qkey(str(k))} = {toml_value(v)}' for k, v in table.items()) + ' }'


def dump_toml(data: dict[str, Any]) -> str:
    lines: list[str] = []

    def emit_table(table: dict[str, Any], path: list[str], header: bool) -> None:
        if header:
            if lines and lines[-1] != '':
                lines.append('')
            lines.append('[' + '.'.join(qkey(p) for p in path) + ']')
        children: list[tuple[str, dict[str, Any]]] = []
        arrays: list[tuple[str, list[dict[str, Any]]]] = []
        for key, value in table.items():
            if isinstance(value, dict):
                children.append((str(key), value))
            elif isinstance(value, list) and value and all(isinstance(v, dict) for v in value):
                arrays.append((str(key), value))
            else:
                lines.append(f'{qkey(str(key))} = {toml_value(value)}')
        for key, child in children:
            emit_table(child, [*path, key], True)
        for key, items in arrays:
            for item in items:
                if lines and lines[-1] != '':
                    lines.append('')
                lines.append('[[' + '.'.join(qkey(p) for p in [*path, key]) + ']]')
                emit_table(item, [*path, key], False)

    emit_table(data, [], False)
    return '\n'.join(lines).rstrip() + '\n'


def load_toml(path: pathlib.Path) -> dict[str, Any]:
    with path.open('rb') as stream:
        return tomllib.load(stream)


def repo_component_closure(index: dict[str, Any], git: str) -> list[str]:
    """Return owned components represented by a Git repository and replacements.

    Some upstream repositories expose packages through gitlinks/submodules. MattOS
    keeps those gitlinks as separately pinned first-class components, so a Git
    dependency on the parent repository may legitimately name a package owned by
    one of the declared replacement components (notably libcosmic -> cosmic-iced).
    """
    pending = list(index.get('repos', {}).get(norm_repo(git), []))
    seen: set[str] = set()
    while pending:
        component = pending.pop(0)
        if component in seen:
            continue
        seen.add(component)
        for replacement in index.get('gitlink_replacements', {}).get(component, []):
            child = replacement.get('component')
            if isinstance(child, str) and child not in seen:
                pending.append(child)
    return sorted(seen)


def choose_owned_target(index: dict[str, Any], package: str, git: str | None) -> dict[str, str] | None:
    root_target = index.get('root_packages', {}).get(package)
    if root_target:
        return dict(root_target)
    if git is None:
        return None
    candidates: list[dict[str, str]] = []
    for component in repo_component_closure(index, git):
        rel = index['components'][component].get('packages', {}).get(package)
        if rel is not None:
            candidates.append({'component': component, 'package_path': rel})
    unique = {(c['component'], c['package_path']) for c in candidates}
    if len(unique) > 1:
        rendered = ', '.join(f'{component}:{path or "."}' for component, path in sorted(unique))
        raise OwnershipError(
            f'owned Git dependency {package!r} from {git!r} is ambiguous across canonical sources: {rendered}'
        )
    if len(unique) == 1:
        component, package_path = next(iter(unique))
        return {'component': component, 'package_path': package_path}
    return None


def uses_workspace_inheritance(value: Any) -> bool:
    if isinstance(value, dict):
        return value.get('workspace') is True or any(uses_workspace_inheritance(v) for v in value.values())
    if isinstance(value, list):
        return any(uses_workspace_inheritance(v) for v in value)
    return False


def rewrite_spec(spec: Any, target_path: pathlib.Path) -> tuple[Any, bool]:
    if isinstance(spec, str):
        return {'version': spec, 'path': str(target_path)}, True
    if not isinstance(spec, dict) or spec.get('workspace') is True:
        return spec, False
    updated = dict(spec)
    changed = False
    for field in GIT_KEYS:
        if field in updated:
            updated.pop(field, None)
            changed = True
    new_path = str(target_path)
    if updated.get('path') != new_path:
        updated['path'] = new_path
        changed = True
    return updated, changed


def target_for_existing_mirror_path(
    package: str,
    resolved: pathlib.Path,
    index: dict[str, Any],
    mirrors: dict[str, pathlib.Path],
) -> dict[str, str] | None:
    matches: list[tuple[str, str]] = []
    for component, base in mirrors.items():
        package_rel = index['components'][component].get('packages', {}).get(package)
        if package_rel is None:
            continue
        if (base / package_rel).resolve() == resolved:
            matches.append((component, package_rel))
    unique = set(matches)
    if len(unique) > 1:
        rendered = ', '.join(f'{component}:{path or "."}' for component, path in sorted(unique))
        raise OwnershipError(f'path dependency {package!r} is ambiguous across owned mirrors: {rendered}')
    if len(unique) == 1:
        component, package_path = next(iter(unique))
        return {'component': component, 'package_path': package_path}
    return None


def rewrite_dependency_table(table: Any, index: dict[str, Any], mirrors: dict[str, pathlib.Path], current_manifest: pathlib.Path, current_component: str) -> tuple[bool, set[str]]:
    if not isinstance(table, dict):
        return False, set()
    changed = False
    needed: set[str] = set()
    for key, raw in list(table.items()):
        spec = {'version': raw} if isinstance(raw, str) else raw
        if not isinstance(spec, dict) or spec.get('workspace') is True:
            continue
        package = spec.get('package', key)
        if not isinstance(package, str):
            continue
        git = spec.get('git') if isinstance(spec.get('git'), str) else None
        target = choose_owned_target(index, package, git)
        if target is None and git is None and 'path' not in spec:
            target = choose_owned_target(index, package, None)
        if target is None and isinstance(spec.get('path'), str):
            raw_path = pathlib.Path(spec['path'])
            resolved = (current_manifest.parent / raw_path).resolve()
            current_root = mirrors[current_component].resolve()
            try:
                rel = resolved.relative_to(current_root).as_posix()
            except ValueError:
                rel = None
            if rel is not None:
                for replacement in index.get('gitlink_replacements', {}).get(current_component, []):
                    prefix = replacement['path'].rstrip('/')
                    if rel == prefix or rel.startswith(prefix + '/'):
                        replacement_component = replacement['component']
                        package_rel = index['components'][replacement_component].get('packages', {}).get(package)
                        if package_rel is not None:
                            target = {'component': replacement_component, 'package_path': package_rel}
                            break
            if target is None:
                target = target_for_existing_mirror_path(package, resolved, index, mirrors)
            if target is None:
                root_target = index.get('root_packages', {}).get(package)
                target = dict(root_target) if root_target else None
        if target is None:
            continue
        component = target['component']
        package_path = target['package_path']
        target_path = (mirrors[component] / package_path).resolve()
        replacement, did_change = rewrite_spec(raw, target_path)
        if did_change:
            table[key] = replacement
            changed = True
        needed.add(component)
    return changed, needed


def rewrite_manifest(path: pathlib.Path, index: dict[str, Any], mirrors: dict[str, pathlib.Path], component: str) -> set[str]:
    data = load_toml(path)
    changed = False
    needed: set[str] = set()
    for name in DEP_TABLE_NAMES:
        c, n = rewrite_dependency_table(data.get(name), index, mirrors, path, component)
        changed |= c
        needed |= n
    workspace = data.get('workspace')
    if isinstance(workspace, dict):
        c, n = rewrite_dependency_table(workspace.get('dependencies'), index, mirrors, path, component)
        changed |= c
        needed |= n
    target = data.get('target')
    if isinstance(target, dict):
        for cfg in target.values():
            if isinstance(cfg, dict):
                for name in DEP_TABLE_NAMES:
                    c, n = rewrite_dependency_table(cfg.get(name), index, mirrors, path, component)
                    changed |= c
                    needed |= n
    package = data.get('package')
    rel_manifest_dir = path.parent.relative_to(mirrors[component]).as_posix()
    if isinstance(package, dict) and rel_manifest_dir not in ('', '.') and uses_workspace_inheritance(data):
        if 'workspace' not in package:
            root_manifest = mirrors[component] / 'Cargo.toml'
            root_data = load_toml(root_manifest) if root_manifest.is_file() else {}
            if isinstance(root_data.get('workspace'), dict):
                package['workspace'] = pathlib.Path(os.path.relpath(mirrors[component], path.parent)).as_posix()
                changed = True
    if changed:
        path.write_text(dump_toml(data), encoding='utf-8')
    return needed


def copy_tracked_component(root: pathlib.Path, source_rel: str, destination: pathlib.Path) -> None:
    if destination.exists():
        shutil.rmtree(destination)
    destination.mkdir(parents=True, exist_ok=True)
    result = subprocess.run(['git', 'ls-files', '-z', '--', source_rel], cwd=root, stdout=subprocess.PIPE, check=True)
    prefix = pathlib.PurePosixPath(source_rel)
    for raw in result.stdout.split(b'\0'):
        if not raw:
            continue
        rel_repo = pathlib.PurePosixPath(raw.decode())
        rel = rel_repo.relative_to(prefix)
        src = root / pathlib.Path(rel_repo.as_posix())
        dst = destination / pathlib.Path(rel.as_posix())
        dst.parent.mkdir(parents=True, exist_ok=True)
        if src.is_symlink():
            dst.symlink_to(os.readlink(src))
        else:
            shutil.copy2(src, dst, follow_symlinks=False)


def apply_component_patches(root: pathlib.Path, metadata: dict[str, Any], destination: pathlib.Path) -> None:
    manifest_rel = metadata.get('patch_manifest')
    if not manifest_rel:
        return
    manifest_path = root / manifest_rel
    manifest_bytes = manifest_path.read_bytes()
    expected_manifest = metadata.get('patch_manifest_sha256')
    if expected_manifest and hashlib.sha256(manifest_bytes).hexdigest() != expected_manifest:
        raise OwnershipError(f'patch manifest checksum mismatch: {manifest_rel}')
    manifest = tomllib.loads(manifest_bytes.decode())
    if manifest.get('component') != metadata.get('name') or manifest.get('application') != 'output-mirror-only':
        raise OwnershipError(f'invalid output-mirror patch manifest: {manifest_rel}')
    if manifest.get('upstream_commit') != metadata.get('revision'):
        raise OwnershipError(f'patch manifest revision mismatch: {manifest_rel}')
    for patch in manifest.get('patch', []):
        patch_path = root / patch['path']
        payload = patch_path.read_bytes()
        if hashlib.sha256(payload).hexdigest() != patch['sha256']:
            raise OwnershipError(f'patch checksum mismatch: {patch["path"]}')
        completed = subprocess.run(['patch', '-p1', '--batch', '--forward', '-i', str(patch_path)], cwd=destination, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        if completed.returncode != 0:
            raise OwnershipError(f'failed to apply {patch["path"]}: {completed.stderr.strip() or completed.stdout.strip()}')


def mirror_fingerprint(root: pathlib.Path, index: dict[str, Any], component: str) -> str:
    meta = index['components'][component]
    digest = hashlib.sha256()
    digest.update(json.dumps({'version': index.get('version'), 'component': meta}, sort_keys=True).encode())
    for helper in ['DevUtils/source_ownership_graph.py', 'DevUtils/generate_source_overrides.py']:
        p = root / helper
        if p.is_file():
            digest.update(p.read_bytes())
    return digest.hexdigest()


def prepare_graph(root: pathlib.Path, index: dict[str, Any], consumer_component: str, consumer_mirror: pathlib.Path) -> dict[str, pathlib.Path]:
    mirror_root = root / 'out' / 'source-ownership' / 'sources'
    mirrors = {name: mirror_root / name for name in index.get('components', {})}
    mirrors[consumer_component] = consumer_mirror.resolve()
    visiting: set[str] = set()
    prepared: set[str] = {consumer_component}

    def ensure_component(component: str) -> None:
        if component == consumer_component or component in prepared:
            return
        if component in visiting:
            return
        visiting.add(component)
        meta = index['components'][component]
        dest = mirrors[component]
        marker = dest / '.mattos-source-ownership.json'
        fingerprint = mirror_fingerprint(root, index, component)
        valid = False
        if marker.is_file():
            try:
                valid = json.loads(marker.read_text()).get('fingerprint') == fingerprint
            except Exception:
                valid = False
        if not valid:
            copy_tracked_component(root, meta['source_path'], dest)
            apply_component_patches(root, meta, dest)
        needed: set[str] = set()
        for manifest in sorted(dest.rglob('Cargo.toml')):
            needed |= rewrite_manifest(manifest, index, mirrors, component)
        marker.write_text(json.dumps({'fingerprint': fingerprint}, sort_keys=True) + '\n')
        prepared.add(component)
        visiting.remove(component)
        for dep in sorted(needed):
            ensure_component(dep)

    needed: set[str] = set()
    for manifest in sorted(consumer_mirror.rglob('Cargo.toml')):
        needed |= rewrite_manifest(manifest, index, mirrors, consumer_component)
    for dep in sorted(needed):
        ensure_component(dep)
    for component in sorted(prepared):
        base = consumer_mirror if component == consumer_component else mirrors[component]
        for manifest in sorted(base.rglob('Cargo.toml')):
            rewrite_manifest(manifest, index, mirrors, component)
    return mirrors


def external_git_package_is_owned(index: dict[str, Any], package: str, source: str) -> bool:
    if not source.startswith('git+'):
        return False
    raw = source[4:].split('#', 1)[0].split('?', 1)[0]
    for component in repo_component_closure(index, raw):
        if package in index['components'][component].get('packages', {}):
            return True
    return False


def verify_metadata(metadata_json: str, root: pathlib.Path, index: dict[str, Any], mirrors: dict[str, pathlib.Path]) -> list[str]:
    data = json.loads(metadata_json)
    failures: list[str] = []
    root_packages = index.get('root_packages', {})
    for pkg in data.get('packages', []):
        name = pkg.get('name')
        source = pkg.get('source')
        manifest = pathlib.Path(pkg.get('manifest_path', '')).resolve() if pkg.get('manifest_path') else None
        if isinstance(name, str) and name in root_packages:
            expected = root_packages[name]
            expected_manifest = mirrors[expected['component']] / expected['package_path'] / 'Cargo.toml'
            if manifest != expected_manifest.resolve():
                failures.append(f'owned root package {name} resolved from {source or manifest}, expected {expected_manifest}')
        if isinstance(name, str) and isinstance(source, str) and external_git_package_is_owned(index, name, source):
            failures.append(f'owned git package {name} remained external: {source}')
    return sorted(set(failures))
