#!/usr/bin/env python3
"""Run Cargo after enforcing MattOS-owned dependency paths in build mirrors."""
from __future__ import annotations

import hashlib
import json
import os
import pathlib
import subprocess
import sys
import tomllib
from datetime import datetime, timezone


def repo_root() -> pathlib.Path:
    value = os.environ.get('MATTOS_REPO_ROOT')
    if value:
        return pathlib.Path(value).resolve()
    return pathlib.Path(__file__).resolve().parents[1]


def load_graph_module(root: pathlib.Path):
    sys.path.insert(0, str(root / 'DevUtils'))
    import source_ownership_graph  # type: ignore
    return source_ownership_graph


def component_for_cwd(root: pathlib.Path, cwd: pathlib.Path, index: dict) -> str | None:
    resolved = cwd.resolve()
    parts = resolved.parts
    marker = ('out', 'build', 'cosmic-desktop', 'sources')
    for i in range(len(parts) - len(marker)):
        if tuple(parts[i:i+len(marker)]) == marker and i + len(marker) < len(parts):
            candidate = parts[i + len(marker)]
            if candidate in index.get('components', {}):
                return candidate
    for i in range(len(parts) - 2):
        if tuple(parts[i:i+2]) == ('out', 'build'):
            candidate = parts[i+2]
            if candidate in index.get('components', {}):
                return candidate
    for name, metadata in index.get('components', {}).items():
        source = (root / metadata['source_path']).resolve()
        try:
            resolved.relative_to(source)
        except ValueError:
            continue
        return name
    return None


def effective_manifest(cwd: pathlib.Path, original: list[str]) -> pathlib.Path | None:
    i = 0
    while i < len(original):
        arg = original[i]
        if arg == '--manifest-path' and i + 1 < len(original):
            path = pathlib.Path(original[i+1])
            return (path if path.is_absolute() else cwd / path).resolve()
        if arg.startswith('--manifest-path='):
            path = pathlib.Path(arg.split('=', 1)[1])
            return (path if path.is_absolute() else cwd / path).resolve()
        i += 1
    candidate = cwd / 'Cargo.toml'
    return candidate.resolve() if candidate.is_file() else None


def metadata_resolution_args(original: list[str]) -> list[str]:
    selected: list[str] = []
    value_flags = {'--manifest-path', '--features', '-F'}
    switches = {
        '--all-features',
        '--no-default-features',
        '--locked',
        '--offline',
        '--frozen',
    }
    i = 0
    while i < len(original):
        arg = original[i]
        if arg in value_flags and i + 1 < len(original):
            selected.extend([arg, original[i+1]])
            i += 2
            continue
        if any(arg.startswith(prefix + '=') for prefix in ('--manifest-path', '--features')):
            selected.append(arg)
        elif arg in switches:
            selected.append(arg)
        i += 1
    return selected


def digest_file(path: pathlib.Path | None) -> str:
    if path is None or not path.is_file():
        return 'missing'
    h = hashlib.sha256()
    h.update(path.read_bytes())
    return h.hexdigest()


def append_trace(trace: pathlib.Path, message: str) -> None:
    trace.parent.mkdir(parents=True, exist_ok=True)
    with trace.open('a', encoding='utf-8') as stream:
        stream.write(message.rstrip('\n') + '\n')


def run_capture(command: list[str], cwd: pathlib.Path, trace: pathlib.Path, label: str, capture_stdout: bool = False) -> subprocess.CompletedProcess[str]:
    append_trace(trace, f'{label}_argv=' + json.dumps(command))
    completed = subprocess.run(
        command,
        cwd=str(cwd),
        check=False,
        text=True,
        stdout=subprocess.PIPE if capture_stdout else None,
        stderr=subprocess.PIPE,
    )
    append_trace(trace, f'{label}_status={completed.returncode}')
    if completed.stderr:
        append_trace(trace, f'{label}_stderr_begin')
        append_trace(trace, completed.stderr)
        append_trace(trace, f'{label}_stderr_end')
    return completed


def validated_consumer_patch_manifest(root: pathlib.Path, metadata: dict) -> tuple[str, list[pathlib.Path]] | None:
    manifest_rel = metadata.get('patch_manifest')
    if not manifest_rel:
        return None
    manifest_path = root / manifest_rel
    payload = manifest_path.read_bytes()
    expected_manifest = metadata.get('patch_manifest_sha256')
    if expected_manifest and hashlib.sha256(payload).hexdigest() != expected_manifest:
        raise RuntimeError(f'consumer patch manifest checksum mismatch: {manifest_rel}')
    manifest = tomllib.loads(payload.decode('utf-8'))
    if manifest.get('component') != metadata.get('name'):
        raise RuntimeError(f'consumer patch manifest component mismatch: {manifest_rel}')
    if manifest.get('upstream_commit') != metadata.get('revision'):
        raise RuntimeError(f'consumer patch manifest revision mismatch: {manifest_rel}')
    if manifest.get('application') != 'output-mirror-only':
        raise RuntimeError(f'consumer patch manifest is not output-mirror-only: {manifest_rel}')

    patch_paths: list[pathlib.Path] = []
    for item in manifest.get('patch', []):
        patch_path = root / item['path']
        patch_payload = patch_path.read_bytes()
        if hashlib.sha256(patch_payload).hexdigest() != item.get('sha256'):
            raise RuntimeError(f'consumer patch checksum mismatch: {item["path"]}')
        patch_paths.append(patch_path)
    return manifest_rel, patch_paths


def consumer_patch_status(root: pathlib.Path, metadata: dict, consumer_mirror: pathlib.Path) -> str:
    """Return none, pending, or applied for a consumer's output patch chain.

    The Cargo dispatcher can be invoked repeatedly against one stage mirror.
    Detect an already-applied chain with ``git apply --reverse --check`` rather
    than trying to apply it twice. Mixed or non-applicable chains fail closed.
    """
    validated = validated_consumer_patch_manifest(root, metadata)
    if validated is None:
        return 'none'
    _, patch_paths = validated
    try:
        mirror_rel = consumer_mirror.resolve().relative_to(root.resolve())
    except ValueError as exc:
        raise RuntimeError(
            f'consumer patch destination is outside the MattOS repository: {consumer_mirror}'
        ) from exc
    directory_arg = f'--directory={mirror_rel.as_posix()}'

    statuses: list[str] = []
    for patch_path in patch_paths:
        forward = subprocess.run(
            ['git', 'apply', '--whitespace=error-all', directory_arg, '--check', str(patch_path)],
            cwd=root,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            check=False,
        )
        if forward.returncode == 0:
            statuses.append('pending')
            continue
        reverse = subprocess.run(
            ['git', 'apply', '--reverse', '--whitespace=error-all', directory_arg, '--check', str(patch_path)],
            cwd=root,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            check=False,
        )
        if reverse.returncode == 0:
            statuses.append('applied')
            continue
        detail = forward.stderr.strip() or reverse.stderr.strip() or 'patch does not match mirror'
        raise RuntimeError(
            f'consumer patch {patch_path.relative_to(root)} is neither pending nor applied: {detail}'
        )
    if not statuses:
        return 'none'
    unique = set(statuses)
    if len(unique) != 1:
        raise RuntimeError(f'consumer patch chain is partially applied: {statuses}')
    return statuses[0]


def apply_consumer_patches(
    root: pathlib.Path,
    metadata: dict,
    consumer_mirror: pathlib.Path,
    graph,
) -> str:
    """Apply the current component's output-only patches exactly once."""
    status = consumer_patch_status(root, metadata, consumer_mirror)
    if status == 'pending':
        graph.apply_component_patches(root, metadata, consumer_mirror)
        return 'applied'
    return status


def main() -> int:
    root = repo_root()
    real_cargo = os.environ.get('MATTOS_REAL_CARGO')
    if not real_cargo:
        raise SystemExit('MATTOS_REAL_CARGO is not set')
    index_path = root / 'out' / 'source-ownership' / 'cargo' / 'index.json'
    if not index_path.is_file():
        os.execv(real_cargo, [real_cargo, *sys.argv[1:]])
        return 127
    index = json.loads(index_path.read_text(encoding='utf-8'))
    cwd = pathlib.Path.cwd().resolve()
    component = component_for_cwd(root, cwd, index)
    if component is None:
        os.execv(real_cargo, [real_cargo, *sys.argv[1:]])
        return 127

    manifest = effective_manifest(cwd, sys.argv[1:])
    out_build = (root / 'out' / 'build').resolve()
    if manifest is None:
        os.execv(real_cargo, [real_cargo, *sys.argv[1:]])
        return 127
    try:
        manifest.relative_to(out_build)
        is_build_mirror = True
    except ValueError:
        is_build_mirror = False

    # Authoritative imported source is never rewritten. Cargo ownership
    # transformations are output-mirror-only, matching MattOS provenance rules.
    if not is_build_mirror:
        os.execv(real_cargo, [real_cargo, *sys.argv[1:]])
        return 127

    trace = root / 'out' / 'source-ownership' / 'logs' / f'{component}.log'
    trace.parent.mkdir(parents=True, exist_ok=True)
    trace.write_text(
        f'timestamp={datetime.now(timezone.utc).isoformat()}\n'
        f'component={component}\n'
        f'cwd={cwd}\n'
        f'real_cargo={real_cargo}\n'
        f'manifest={manifest}\n',
        encoding='utf-8',
    )

    graph = load_graph_module(root)
    consumer_mirror = manifest.parent
    try:
        patch_status = apply_consumer_patches(
            root,
            index['components'][component],
            consumer_mirror,
            graph,
        )
        append_trace(trace, f'consumer_patches={patch_status}')
        mirrors = graph.prepare_graph(root, index, component, consumer_mirror)
    except Exception as exc:
        append_trace(trace, f'prepare_error={exc}')
        raise SystemExit(f'MattOS source ownership preparation failed for {component}: {exc}') from exc
    append_trace(trace, 'prepare=success')
    append_trace(
        trace,
        'mirrors=' + json.dumps({k: str(v) for k, v in sorted(mirrors.items()) if v.exists()}, sort_keys=True),
    )

    lockfile = manifest.parent / 'Cargo.lock'
    append_trace(trace, f'lock_sha256_before={digest_file(lockfile)}')
    metadata_command = [real_cargo, 'metadata', '--format-version', '1', *metadata_resolution_args(sys.argv[1:])]
    metadata = run_capture(metadata_command, cwd, trace, 'metadata', capture_stdout=True)
    if metadata.returncode != 0:
        if metadata.stderr:
            sys.stderr.write(metadata.stderr)
        return metadata.returncode
    append_trace(trace, f'lock_sha256_after_metadata={digest_file(lockfile)}')
    failures = graph.verify_metadata(metadata.stdout or '{}', root, index, mirrors)
    append_trace(trace, 'ownership_failures=' + json.dumps(failures))
    if failures:
        message = 'MattOS source ownership invariant failed:\n  ' + '\n  '.join(failures)
        sys.stderr.write(message + '\n')
        return 101

    final_args = [real_cargo, *sys.argv[1:]]
    append_trace(trace, 'final_argv=' + json.dumps(final_args))
    final = subprocess.run(final_args, cwd=str(cwd), check=False, text=True, stderr=subprocess.PIPE)
    append_trace(trace, f'final_status={final.returncode}')
    if final.stderr:
        sys.stderr.write(final.stderr)
        append_trace(trace, 'final_stderr_begin')
        append_trace(trace, final.stderr)
        append_trace(trace, 'final_stderr_end')
    return final.returncode


if __name__ == '__main__':
    raise SystemExit(main())
