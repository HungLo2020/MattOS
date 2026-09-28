#!/usr/bin/env python3
"""Check that MattOS stages rebuild byte-for-byte from unchanged inputs.

Every target stage's cache key includes the output digests of the toolchain
stages (``cross-toolchain``, ``glibc``, ``gcc-runtime``), so one of them
publishing different bytes from the same inputs recompiles the whole system.
For each selected stage this script brings it up to date, records its
per-file output inventory, rebuilds only that stage, and compares the two.
A nondeterministic file is listed with how it changed.

The second build is the dependency-correct result and is kept: if a stage is
not reproducible, its downstream stages rebuild on the next ``build``.

A verified (input digest, output digest) pair is recorded under
``out/state/reproducibility/``; with ``--if-changed`` a stage whose current
inputs and output were already verified is skipped, so the check is cheap to
run routinely and only rebuilds after a toolchain input changes.

Usage:
    python3 DevUtils/check_stage_reproducibility.py              # toolchain stages
    python3 DevUtils/check_stage_reproducibility.py --if-changed
    python3 DevUtils/check_stage_reproducibility.py gcc-runtime zlib
"""

from __future__ import annotations

import argparse
import difflib
import json
import subprocess
import sys
import time
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
MATTOS_BUILD = ["cargo", "run", "-q", "-p", "mattos-build", "--"]
TOOLCHAIN_STAGES = ("cross-toolchain", "glibc", "gcc-runtime")
STATE_DIRECTORY = Path("out/state/reproducibility")
MAX_TEXT_DIFF_BYTES = 64 << 10
MAX_SNAPSHOT_BYTES = 64 << 20
MAX_DIFF_LINES = 40


def manifest_path(root: Path, stage: str) -> Path:
    return root / "out/state/stages" / f"{stage}.json"


def read_manifest(root: Path, stage: str) -> dict:
    return json.loads(manifest_path(root, stage).read_text(encoding="utf-8"))


def inventory(manifest: dict) -> dict[str, dict]:
    """The manifest's published outputs, keyed by path."""
    return {entry["path"]: entry for entry in manifest["expected_outputs"]}


def compare_inventories(first: dict[str, dict], second: dict[str, dict]) -> list[str]:
    """Human-readable differences between two output inventories."""
    differences = []
    for path in sorted(first.keys() - second.keys()):
        differences.append(f"only in the first build: {path}")
    for path in sorted(second.keys() - first.keys()):
        differences.append(f"only in the second build: {path}")
    for path in sorted(first.keys() & second.keys()):
        old, new = first[path], second[path]
        changed = [field for field in ("kind", "mode", "size", "content") if old.get(field) != new.get(field)]
        if changed:
            detail = ", ".join(f"{field} {old.get(field)!r} -> {new.get(field)!r}" for field in changed if field != "content")
            differences.append(f"changed: {path} ({', '.join(changed)}{'; ' + detail if detail else ''})")
    return differences


def text_diff(before: bytes, after: bytes, path: str) -> list[str]:
    """A bounded unified diff when both versions are small UTF-8 text."""
    if max(len(before), len(after)) > MAX_TEXT_DIFF_BYTES:
        return []
    try:
        old, new = before.decode("utf-8"), after.decode("utf-8")
    except UnicodeDecodeError:
        return []
    lines = list(difflib.unified_diff(old.splitlines(), new.splitlines(), f"first/{path}", f"second/{path}", lineterm=""))
    return lines[:MAX_DIFF_LINES] + (["  ..."] if len(lines) > MAX_DIFF_LINES else [])


def state_path(root: Path, stage: str) -> Path:
    return root / STATE_DIRECTORY / f"{stage}.json"


def already_verified(root: Path, stage: str, manifest: dict) -> bool:
    """Whether these exact inputs and this exact output were verified before."""
    try:
        record = json.loads(state_path(root, stage).read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return False
    return (
        record.get("input_digest") == manifest["inputs"]["full_digest"]
        and record.get("output_digest") == manifest["output_content_digest"]
    )


def record_verified(root: Path, stage: str, manifest: dict) -> None:
    path = state_path(root, stage)
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix(".tmp")
    temporary.write_text(
        json.dumps(
            {
                "stage": stage,
                "input_digest": manifest["inputs"]["full_digest"],
                "output_digest": manifest["output_content_digest"],
                "verified_at_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
            },
            indent=2,
        )
        + "\n",
        encoding="utf-8",
    )
    temporary.replace(path)


def run(arguments: list[str], label: str) -> None:
    command = MATTOS_BUILD + arguments
    print(f"[reproducibility] {label}: {' '.join(command)}", flush=True)
    completed = subprocess.run(command, cwd=REPO_ROOT, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=False)
    if completed.returncode != 0:
        sys.stdout.write(completed.stdout[-8000:])
        raise SystemExit(f"[reproducibility] {label} failed with exit {completed.returncode}")


def snapshot_text_files(root: Path, entries: dict[str, dict]) -> dict[str, bytes]:
    """Small regular files (within a total budget), kept so a changed one can
    be shown as a diff after the rebuild replaces it."""
    snapshot, total = {}, 0
    for path, entry in sorted(entries.items()):
        size = entry.get("size", 0)
        if entry.get("kind") != "file" or size > MAX_TEXT_DIFF_BYTES or total + size > MAX_SNAPSHOT_BYTES:
            continue
        try:
            snapshot[path] = (root / path).read_bytes()
            total += size
        except OSError:
            pass
    return snapshot


def check_stage(root: Path, stage: str, if_changed: bool) -> bool:
    run(["build", stage], f"{stage}: bring up to date")
    first = read_manifest(root, stage)
    if if_changed and already_verified(root, stage, first):
        print(f"[reproducibility] {stage}: already verified for these inputs; skipped")
        return True
    first_inventory = inventory(first)
    before = snapshot_text_files(root, first_inventory)
    run(["cache", "invalidate", stage], f"{stage}: invalidate")
    run(["build", stage], f"{stage}: rebuild")
    second = read_manifest(root, stage)
    if second["output_content_digest"] == first["output_content_digest"]:
        record_verified(root, stage, second)
        print(f"[reproducibility] {stage}: PASS (output {second['output_content_digest'][:16]} reproduced)")
        return True
    differences = compare_inventories(first_inventory, inventory(second))
    print(f"[reproducibility] {stage}: FAIL: {len(differences)} difference(s) between two builds of the same inputs")
    for line in differences:
        print(f"  {line}")
        if line.startswith("changed: "):
            path = line[len("changed: ") :].split(" (", 1)[0]
            if path in before and (root / path).is_file():
                for diff_line in text_diff(before[path], (root / path).read_bytes(), path):
                    print(f"      {diff_line}")
    print(f"[reproducibility] {stage}: the second build is kept; its downstream stages rebuild on the next build")
    return False


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("stages", nargs="*", default=list(TOOLCHAIN_STAGES), help="stages to check (default: the toolchain stages)")
    parser.add_argument("--if-changed", action="store_true", help="skip stages already verified for their current inputs and output")
    args = parser.parse_args()
    failed = [stage for stage in args.stages if not check_stage(REPO_ROOT, stage, args.if_changed)]
    if failed:
        print(f"[reproducibility] FAIL: not reproducible: {', '.join(failed)}")
        return 1
    print(f"[reproducibility] PASS: {', '.join(args.stages)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
