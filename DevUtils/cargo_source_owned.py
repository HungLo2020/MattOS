#!/usr/bin/env python3
"""Dispatch Cargo with the MattOS source-ownership config for the current component."""
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
    value = os.environ.get("MATTOS_REPO_ROOT")
    if value:
        return pathlib.Path(value).resolve()
    here = pathlib.Path(__file__).resolve()
    return here.parents[1]


def component_for_cwd(root: pathlib.Path, cwd: pathlib.Path, index: dict) -> str | None:
    matches = []
    for name, metadata in index.get("components", {}).items():
        source = root / metadata["source_path"]
        try:
            relative = cwd.resolve().relative_to(source.resolve())
        except ValueError:
            continue
        matches.append((len(relative.parts), name))
    if matches:
        return min(matches)[1]

    parts = cwd.resolve().parts
    marker = ("out", "build", "cosmic-desktop", "sources")
    for i in range(len(parts) - len(marker)):
        if tuple(parts[i : i + len(marker)]) == marker and i + len(marker) < len(parts):
            candidate = parts[i + len(marker)]
            if candidate in index.get("components", {}):
                return candidate

    for i in range(len(parts) - 2):
        if parts[i : i + 2] == ("out", "build"):
            candidate = parts[i + 2]
            if candidate in index.get("components", {}):
                return candidate
    return None


def metadata_resolution_args(original: list[str]) -> list[str]:
    selected: list[str] = []
    value_flags = {"--manifest-path", "--features", "-F"}
    switch_flags = {"--all-features", "--no-default-features"}
    i = 0
    while i < len(original):
        arg = original[i]
        if arg in value_flags and i + 1 < len(original):
            selected.extend([arg, original[i + 1]])
            i += 2
            continue
        if any(arg.startswith(prefix + "=") for prefix in ("--manifest-path", "--features")):
            selected.append(arg)
            i += 1
            continue
        if arg in switch_flags:
            selected.append(arg)
        i += 1
    return selected


def effective_manifest(cwd: pathlib.Path, original: list[str]) -> pathlib.Path | None:
    i = 0
    while i < len(original):
        arg = original[i]
        if arg == "--manifest-path" and i + 1 < len(original):
            path = pathlib.Path(original[i + 1])
            return (path if path.is_absolute() else cwd / path).resolve()
        if arg.startswith("--manifest-path="):
            path = pathlib.Path(arg.split("=", 1)[1])
            return (path if path.is_absolute() else cwd / path).resolve()
        i += 1
    candidate = cwd / "Cargo.toml"
    return candidate.resolve() if candidate.is_file() else None


def digest_file(path: pathlib.Path | None) -> str:
    if path is None or not path.is_file():
        return "missing"
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def append_trace(trace: pathlib.Path, message: str) -> None:
    trace.parent.mkdir(parents=True, exist_ok=True)
    with trace.open("a", encoding="utf-8") as stream:
        stream.write(message.rstrip("\n") + "\n")


def external_owned_entries(lockfile: pathlib.Path, owned_packages: set[str]) -> list[tuple[str, str, str]]:
    """Return owned lock entries that still resolve through Git/registry sources."""
    try:
        with lockfile.open("rb") as stream:
            data = tomllib.load(stream)
    except (OSError, tomllib.TOMLDecodeError) as exc:
        raise SystemExit(f"cannot inspect derived Cargo.lock {lockfile}: {exc}") from exc
    out = []
    for package in data.get("package", []):
        if not isinstance(package, dict):
            continue
        name = package.get("name")
        version = package.get("version")
        source = package.get("source")
        if name in owned_packages and isinstance(version, str) and isinstance(source, str):
            out.append((name, version, source))
    return out


def run_reconcile_command(command: list[str], cwd: pathlib.Path, trace: pathlib.Path, label: str) -> None:
    append_trace(trace, f"{label}_argv=" + json.dumps(command))
    completed = subprocess.run(
        command,
        cwd=str(cwd),
        stdout=subprocess.DEVNULL,
        stderr=subprocess.PIPE,
        text=True,
        check=False,
    )
    append_trace(trace, f"{label}_status={completed.returncode}")
    if completed.stderr:
        append_trace(trace, f"{label}_stderr_begin")
        append_trace(trace, completed.stderr)
        append_trace(trace, f"{label}_stderr_end")
    if completed.returncode != 0:
        if completed.stderr:
            sys.stderr.write(completed.stderr)
        raise SystemExit(completed.returncode)


def reconcile_lockfile(
    real_cargo: str,
    config: pathlib.Path,
    original: list[str],
    trace: pathlib.Path,
    owned_packages: list[str],
) -> pathlib.Path | None:
    """Reconcile and verify the derived build-mirror lockfile under scoped ownership."""
    if "--locked" not in original:
        append_trace(trace, "reconcile=skipped reason=no---locked")
        return None

    cwd = pathlib.Path.cwd()
    manifest = effective_manifest(cwd, original)
    if manifest is None or not manifest.is_file():
        append_trace(trace, f"reconcile=skipped reason=manifest-missing manifest={manifest}")
        return None
    lockfile = manifest.parent / "Cargo.lock"
    if not lockfile.is_file():
        append_trace(trace, f"reconcile=skipped reason=lockfile-missing manifest={manifest}")
        return None

    before = digest_file(lockfile)
    metadata_command = [
        real_cargo,
        "--config",
        str(config),
        "metadata",
        "--format-version",
        "1",
        *metadata_resolution_args(original),
    ]
    append_trace(trace, f"manifest={manifest}")
    append_trace(trace, f"lockfile={lockfile}")
    append_trace(trace, f"owned_packages={json.dumps(owned_packages)}")
    append_trace(trace, f"lock_sha256_before={before}")
    run_reconcile_command(metadata_command, cwd, trace, "reconcile")

    owned = set(owned_packages)
    remaining = external_owned_entries(lockfile, owned)
    append_trace(trace, "external_owned_after_metadata=" + json.dumps(remaining))

    # Cargo metadata can retain an already-locked Git package when the local
    # patch has the same name/version. Force only those still-external owned
    # packages to be reconsidered; unrelated lock entries stay pinned.
    for name, version, source in remaining:
        package_spec = f"{name}@{version}"
        update_command = [real_cargo, "--config", str(config), "update", "-p", package_spec]
        run_reconcile_command(update_command, cwd, trace, f"force_update_{name}")

    # Re-run metadata after targeted updates so transitive path-owned edges are
    # materialized before the actual --locked build.
    if remaining:
        run_reconcile_command(metadata_command, cwd, trace, "post_update_metadata")

    unresolved = external_owned_entries(lockfile, owned)
    append_trace(trace, "external_owned_final=" + json.dumps(unresolved))
    after = digest_file(lockfile)
    append_trace(trace, f"lock_sha256_after={after}")
    append_trace(trace, f"lock_changed={str(before != after).lower()}")
    if unresolved:
        detail = ", ".join(f"{name}@{version} ({source})" for name, version, source in unresolved)
        message = f"MattOS source ownership invariant failed; owned Cargo packages remain external: {detail}"
        append_trace(trace, "ownership_error=" + message)
        raise SystemExit(message)
    return lockfile


def run_scoped_cargo(args: list[str], trace: pathlib.Path) -> int:
    append_trace(trace, "final_argv=" + json.dumps(args))
    completed = subprocess.run(
        args,
        cwd=os.getcwd(),
        stdout=None,
        stderr=subprocess.PIPE,
        text=True,
        check=False,
    )
    append_trace(trace, f"final_status={completed.returncode}")
    if completed.stderr:
        sys.stderr.write(completed.stderr)
        append_trace(trace, "final_stderr_begin")
        append_trace(trace, completed.stderr)
        append_trace(trace, "final_stderr_end")
    return completed.returncode


def main() -> int:
    root = repo_root()
    real_cargo = os.environ.get("MATTOS_REAL_CARGO")
    if not real_cargo:
        raise SystemExit("MATTOS_REAL_CARGO is not set")

    index_path = root / "out" / "source-ownership" / "cargo" / "index.json"
    args = [real_cargo]
    scoped_config: pathlib.Path | None = None
    component: str | None = None
    owned_packages: list[str] = []
    if index_path.is_file():
        index = json.loads(index_path.read_text(encoding="utf-8"))
        component = component_for_cwd(root, pathlib.Path.cwd(), index)
        if component is not None:
            metadata = index["components"][component]
            config = metadata.get("config")
            owned_packages = [item for item in metadata.get("owned_packages", []) if isinstance(item, str)]
            if config:
                scoped_config = (root / config).resolve()
                args += ["--config", str(scoped_config)]

    args += sys.argv[1:]
    if scoped_config is None or component is None:
        os.execv(real_cargo, args)
        return 127

    trace = root / "out" / "source-ownership" / "logs" / f"{component}.log"
    trace.parent.mkdir(parents=True, exist_ok=True)
    trace.write_text(
        f"timestamp={datetime.now(timezone.utc).isoformat()}\n"
        f"component={component}\n"
        f"cwd={pathlib.Path.cwd()}\n"
        f"config={scoped_config}\n"
        f"real_cargo={real_cargo}\n",
        encoding="utf-8",
    )
    reconcile_lockfile(real_cargo, scoped_config, sys.argv[1:], trace, owned_packages)
    return run_scoped_cargo(args, trace)


if __name__ == "__main__":
    raise SystemExit(main())
