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
import atexit
from datetime import datetime, timezone


def repo_root() -> pathlib.Path:
    value = os.environ.get('MATTOS_REPO_ROOT')
    if value:
        return pathlib.Path(value).resolve()
    # The production dispatcher is copied to out/source-ownership/bin.  Its
    # own path is not below the checkout root, but a Cargo consumer's working
    # directory is.  Keep direct/manual dispatcher use safe as well as the
    # normal MATTOS_REPO_ROOT environment supplied by mattos-build.
    cwd = pathlib.Path.cwd().resolve()
    for candidate in (cwd, *cwd.parents):
        if (candidate / 'DevUtils' / 'cargo_source_owned.py').is_file():
            return candidate
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


def component_mirror(root: pathlib.Path, component: str, index: dict) -> pathlib.Path:
    """Return the enclosing output mirror for a component's Cargo invocation.

    A native build may invoke Cargo from a nested subproject (for example
    dbus-broker's libc-rs Meson subproject).  The nested manifest is still
    owned by the enclosing component; treating its parent as the consumer
    mirror would apply the component patch and ownership rewrites relative to
    the subdirectory instead of the component root.
    """
    metadata = index.get('components', {}).get(component, {})
    source_path = metadata.get('source_path', '')
    # These two COSMIC components have dedicated native builders and their
    # Cargo root lives in out/build/<component>/source.  The other COSMIC
    # applications use the shared cosmic-desktop source mirror.
    if component in {'cosmic-comp', 'cosmic-edit'}:
        return root / 'out' / 'build' / component / 'source'
    if source_path.startswith('src/desktop/cosmic/'):
        return root / 'out' / 'build' / 'cosmic-desktop' / 'sources' / component
    return root / 'out' / 'build' / component / 'source'


def metadata_resolution_args(original: list[str]) -> list[str]:
    """Return selection plus the caller's strict Cargo resolution policy."""
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


def lock_reconciliation_args(original: list[str], allow_network: bool = False) -> list[str]:
    """Allow only the derived lock to change while retaining offline policy.

    Source ownership deliberately changes dependency source identity in the
    output mirror, so a copied upstream Cargo.lock can no longer satisfy
    ``--locked`` until Cargo reconciles those path substitutions.  Remove only
    the lock prohibition for this derived-output step.  An existing derived
    lock keeps ``--frozen`` offline; a missing lock is resolved once online in
    the output mirror before the caller's original frozen command runs.
    """
    reconciled: list[str] = []
    for arg in metadata_resolution_args(original):
        if arg == '--locked':
            continue
        if arg == '--frozen':
            if not allow_network and '--offline' not in reconciled:
                reconciled.append('--offline')
            continue
        if arg == '--offline':
            if '--offline' not in reconciled:
                reconciled.append(arg)
            continue
        reconciled.append(arg)
    return reconciled


def requires_lock_reconciliation(original: list[str]) -> bool:
    return any(arg in {'--locked', '--frozen'} for arg in original)


def fetch_reconciliation_args(original: list[str], allow_network: bool = False) -> list[str]:
    """Return the subset of lock policy accepted by ``cargo fetch``.

    Cargo metadata does not always canonicalize ``[[patch.unused]]`` after
    output-owned patch injection, while a later ``cargo build --locked`` does
    notice that stale serialization.  ``cargo fetch`` resolves and writes the
    lock without compiling; it deliberately receives neither feature flags nor
    a build subcommand, because those are not valid fetch options.
    """
    result: list[str] = []
    values = {'--manifest-path'}
    i = 0
    for arg in lock_reconciliation_args(original, allow_network):
        if arg in values:
            # The matching value is appended by the next loop iteration.
            result.append(arg)
            i = 1
            continue
        if i:
            result.append(arg)
            i = 0
            continue
        if arg.startswith('--manifest-path=') or arg == '--offline':
            result.append(arg)
    return result


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


def reconcile_output_lock(
    real_cargo: str,
    cwd: pathlib.Path,
    original: list[str],
    trace: pathlib.Path,
    allow_network: bool = False,
) -> subprocess.CompletedProcess[str] | None:
    """Reconcile a copied output lock after MattOS rewrites source identities."""
    if not requires_lock_reconciliation(original):
        return None
    command = [real_cargo, 'fetch', *fetch_reconciliation_args(original, allow_network)]
    # Cargo applies output-owned path patches in dependency layers.  A lock
    # digest is not by itself proof that Cargo regards the lock as settled:
    # Cargo can retain an identical lock file while its internal patch graph
    # has just become usable after a fetch.  Verify the *same strict
    # resolution contract* as the caller after every non-compiling fetch
    # instead of treating a stable digest as a cache-validity proxy.
    #
    # This executes only inside an output consumer mirror and only for a
    # caller that explicitly requested --locked/--frozen.  MattOS stage cache
    # hits return before the dispatcher, so a reusable stage cannot mutate
    # its mirror or shared target through this path.
    verification = [
        real_cargo,
        'metadata',
        '--format-version',
        '1',
        *metadata_resolution_args(original),
    ]
    for attempt in range(1, 5):
        completed = run_capture(command, cwd, trace, f'lock_reconcile_{attempt}', capture_stdout=True)
        if completed.returncode != 0:
            return completed
        verified = run_capture(
            verification,
            cwd,
            trace,
            f'lock_verify_{attempt}',
            capture_stdout=True,
        )
        if verified.returncode == 0:
            return completed
    return verified


def cargo_git_source_repo(source: str) -> str | None:
    """Return the repository identity from a Cargo ``git+`` source string."""
    if not source.startswith('git+'):
        return None
    return source[4:].split('#', 1)[0].split('?', 1)[0]


def locked_owned_git_targets(lockfile: pathlib.Path, index: dict, graph) -> list[dict[str, str]]:
    """Find owned Git packages still named by the copied upstream lock.

    Structural rewriting changes manifests that MattOS owns, but an ordinary
    external Git dependency can itself name one of MattOS's owned repositories.
    The copied lock is the deterministic inventory of those source identities
    before derived-lock reconciliation.
    """
    if not lockfile.is_file():
        return []
    data = tomllib.loads(lockfile.read_text(encoding='utf-8'))
    targets: dict[tuple[str, str], dict[str, str]] = {}
    for package in data.get('package', []):
        if not isinstance(package, dict):
            continue
        name = package.get('name')
        source = package.get('source')
        if not isinstance(name, str) or not isinstance(source, str):
            continue
        repo = cargo_git_source_repo(source)
        if repo is None:
            continue
        target = graph.choose_owned_git_target(index, name, repo)
        if target is None:
            continue
        item = {'repo': repo, 'package': name, **target}
        key = (graph.norm_repo(repo), name)
        previous = targets.get(key)
        if previous is not None and previous != item:
            raise RuntimeError(
                f'locked owned Git package {name!r} from {repo!r} has ambiguous targets: '
                f'{previous} vs {item}'
            )
        targets[key] = item
    return [targets[key] for key in sorted(targets)]


def inject_locked_transitive_owned_patches(
    manifest: pathlib.Path,
    lockfile: pathlib.Path,
    index: dict,
    mirrors: dict[str, pathlib.Path],
    graph,
) -> list[str]:
    """Close owned Git sources used by external transitive manifests.

    Only the derived consumer manifest is changed. Structural path rewriting
    remains the primary ownership mechanism; this minimal source-qualified
    ``[patch]`` table covers callers whose downloaded manifests MattOS cannot
    rewrite. The strict metadata verifier remains the final authority.
    """
    targets = locked_owned_git_targets(lockfile, index, graph)
    if not targets:
        return []

    data = tomllib.loads(manifest.read_text(encoding='utf-8'))
    patch_root = data.setdefault('patch', {})
    if not isinstance(patch_root, dict):
        raise RuntimeError(f'Cargo patch table is not a table: {manifest}')

    changed = False
    applied: list[str] = []
    for target in targets:
        component = target['component']
        package = target['package']
        package_path = target['package_path']
        mirror = mirrors.get(component)
        if mirror is None:
            raise RuntimeError(f'owned transitive target {component}:{package} has no mirror mapping')
        target_path = (mirror / package_path).resolve()
        if not (target_path / 'Cargo.toml').is_file():
            raise RuntimeError(
                f'owned transitive target mirror was not prepared for {package}: {target_path}'
            )

        repo = target['repo']
        normalized = graph.norm_repo(repo)
        matching_keys = [
            key
            for key in patch_root
            if isinstance(key, str) and graph.norm_repo(key) == normalized
        ]
        source_key = matching_keys[0] if matching_keys else repo
        if len(matching_keys) > 1:
            # Cargo treats URL spellings with and without ``.git`` as the
            # same source. Upstream manifests can contain both spellings,
            # especially after an output-only patch has been appended. Merge
            # equivalent tables instead of making an otherwise deterministic
            # ownership rewrite fail; conflicting package entries are still
            # rejected below.
            source_key = repo
            merged: dict = {}
            for key in matching_keys:
                table_value = patch_root[key]
                if not isinstance(table_value, dict):
                    raise RuntimeError(f'Cargo patch source is not a table: {key}')
                for entry_key, entry_value in table_value.items():
                    if entry_key in merged and merged[entry_key] != entry_value:
                        raise RuntimeError(
                            f'conflicting Cargo patch entries for normalized source {repo}: {entry_key}'
                        )
                    merged[entry_key] = entry_value
            for key in matching_keys:
                if key != source_key:
                    del patch_root[key]
            patch_root[source_key] = merged
        table = patch_root.setdefault(source_key, {})
        if not isinstance(table, dict):
            raise RuntimeError(f'Cargo patch source is not a table: {source_key}')

        # Cargo patch keys may be aliases. The effective package identity is
        # ``spec.package`` when present, otherwise the table key. Replacing by
        # the literal package name would create a second entry for manifests
        # such as ``cctk = { package = "cosmic-client-toolkit", ... }``.
        matching_package_keys: list[str] = []
        for entry_key, entry_spec in table.items():
            effective_package = entry_key
            if isinstance(entry_spec, dict):
                declared_package = entry_spec.get('package')
                if isinstance(declared_package, str):
                    effective_package = declared_package
            if effective_package == package:
                matching_package_keys.append(entry_key)

        if len(matching_package_keys) > 1:
            raise RuntimeError(
                f'multiple Cargo patch entries resolve to owned package {package!r} '
                f'for source {source_key}: {matching_package_keys}'
            )

        entry_key = matching_package_keys[0] if matching_package_keys else package
        replacement = {'path': str(target_path)}
        if entry_key != package:
            replacement['package'] = package
        if table.get(entry_key) != replacement:
            table[entry_key] = replacement
            changed = True
        applied.append(f'{source_key}:{entry_key}({package})->{target_path}')

    if changed:
        manifest.write_text(graph.dump_toml(data), encoding='utf-8')
    return sorted(applied)


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
    # Cargo can be launched from a nested manifest by a native build system.
    # Ownership, patching, and locking belong to the enclosing component
    # mirror, not to that nested subdirectory.
    consumer_mirror = component_mirror(root, component, index)
    # Hold the consumer transaction lock until this Cargo process exits. The
    # mirror is prepared, metadata-validated, and consumed by the final Cargo
    # command as one critical section; another build must not rewrite it in
    # between those phases.
    consumer_lock = graph.consumer_mirror_lock(root, consumer_mirror)
    consumer_lock.__enter__()
    atexit.register(consumer_lock.__exit__, None, None, None)
    try:
        patch_status = apply_consumer_patches(
            root,
            index['components'][component],
            consumer_mirror,
            graph,
        )
        append_trace(trace, f'consumer_patches={patch_status}')
        mirrors = graph.prepare_graph(root, index, component, consumer_mirror)
        # A native build can invoke Cargo for a nested subproject.  Lock
        # ownership follows the effective manifest, not necessarily the
        # enclosing component mirror (dbus-broker/libc-rs is one example).
        # Both paths are output-mirror paths, so this never mutates imported
        # authoritative source.
        lockfile = manifest.parent / 'Cargo.lock'
        transitive_patches = inject_locked_transitive_owned_patches(
            manifest,
            lockfile,
            index,
            mirrors,
            graph,
        )
        append_trace(trace, 'transitive_owned_patches=' + json.dumps(transitive_patches))
    except Exception as exc:
        append_trace(trace, f'prepare_error={exc}')
        raise SystemExit(f'MattOS source ownership preparation failed for {component}: {exc}') from exc
    append_trace(trace, 'prepare=success')
    append_trace(
        trace,
        'mirrors=' + json.dumps({k: str(v) for k, v in sorted(mirrors.items()) if v.exists()}, sort_keys=True),
    )

    append_trace(trace, f'lock_sha256_before={digest_file(lockfile)}')
    if requires_lock_reconciliation(sys.argv[1:]) and not lockfile.is_file():
        # Some upstream Cargo consumers intentionally omit Cargo.lock.  The
        # dispatcher still needs a deterministic output lock before it can
        # run a locked metadata/build command, so derive one in the output
        # mirror only.  This one-time derivation may resolve the graph online;
        # the caller's original frozen command remains unchanged.
        lock_command = [real_cargo, 'generate-lockfile']
        generated = run_capture(lock_command, cwd, trace, 'lock_generate')
        if generated.returncode != 0 or not lockfile.is_file():
            message = f'could not derive output lockfile: {lockfile}'
            append_trace(trace, f'lock_reconcile_error={message}')
            sys.stderr.write(message + '\n')
            return 101
        append_trace(trace, f'lock_sha256_after_generate={digest_file(lockfile)}')

    # The lock is derived from source-owned path substitutions.  Its package
    # index may not exist in Cargo's offline cache even when the lock file is
    # already present, so reconciliation must be allowed to resolve online.
    # This affects only the temporary output mirror; the caller's original
    # frozen/locked command is still executed unchanged below.
    reconciliation = reconcile_output_lock(
        real_cargo, cwd, sys.argv[1:], trace, allow_network=True
    )
    if reconciliation is not None:
        if reconciliation.returncode != 0:
            if reconciliation.stderr:
                sys.stderr.write(reconciliation.stderr)
            return reconciliation.returncode
        append_trace(trace, f'lock_sha256_after_reconcile={digest_file(lockfile)}')

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
