#!/usr/bin/env python3
"""Dispatch Cargo with the MattOS source-ownership config for the current component."""
from __future__ import annotations

import hashlib
import json
import os
import pathlib
import subprocess
import sys
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
    """Return Cargo metadata flags that can affect dependency/lock resolution."""
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
    """Resolve the manifest Cargo will use for this invocation."""
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


def reconcile_lockfile(
    real_cargo: str,
    config: pathlib.Path,
    original: list[str],
    trace: pathlib.Path,
) -> pathlib.Path | None:
    """Reconcile the derived build-mirror lockfile under scoped ownership."""
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
    command = [
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
    append_trace(trace, f"lock_sha256_before={before}")
    append_trace(trace, "reconcile_argv=" + json.dumps(command))
    completed = subprocess.run(
        command,
        cwd=str(cwd),
        stdout=subprocess.DEVNULL,
        stderr=subprocess.PIPE,
        text=True,
        check=False,
    )
    append_trace(trace, f"reconcile_status={completed.returncode}")
    if completed.stderr:
        append_trace(trace, "reconcile_stderr_begin")
        append_trace(trace, completed.stderr)
        append_trace(trace, "reconcile_stderr_end")
    after = digest_file(lockfile)
    append_trace(trace, f"lock_sha256_after={after}")
    append_trace(trace, f"lock_changed={str(before != after).lower()}")
    if completed.returncode != 0:
        if completed.stderr:
            sys.stderr.write(completed.stderr)
        raise SystemExit(completed.returncode)
    return lockfile


def run_scoped_cargo(args: list[str], trace: pathlib.Path) -> int:
    """Run scoped Cargo while preserving stderr in a deterministic failure log."""
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
    if index_path.is_file():
        index = json.loads(index_path.read_text(encoding="utf-8"))
        component = component_for_cwd(root, pathlib.Path.cwd(), index)
        if component is not None:
            config = index["components"][component].get("config")
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
    reconcile_lockfile(real_cargo, scoped_config, sys.argv[1:], trace)
    return run_scoped_cargo(args, trace)


if __name__ == "__main__":
    raise SystemExit(main())
