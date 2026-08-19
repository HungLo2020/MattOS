#!/usr/bin/env python3
"""Recover the PR #46 transitive-owned-source applicator after its fixture failed.

The first applicator correctly left its production/doc edits uncommitted when the
new regression failed.  That regression accidentally used the same directory as
both the original Git source and the Cargo [patch] replacement, which Cargo
rightly rejects.  This recovery script accepts only that exact dirty state,
changes the fixture to use a distinct MattOS-style output mirror, reruns the
suite, removes itself, and commits/pushes only after validation passes.
"""
from __future__ import annotations

import subprocess
from pathlib import Path

BRANCH = "agent/repair-cosmic-files-provenance"
ROOT = Path(__file__).resolve().parents[1]
DISPATCHER = ROOT / "DevUtils/cargo_source_owned.py"
TESTS = ROOT / "DevUtils/test_source_ownership_overrides.py"
DOC = ROOT / "docs/SOURCE_OWNERSHIP.md"
SELF = Path(__file__).resolve()


def run(*args: str) -> None:
    subprocess.run(args, cwd=ROOT, check=True)


def output(*args: str) -> str:
    return subprocess.check_output(args, cwd=ROOT, text=True).strip()


def require_expected_recovery_state() -> None:
    branch = output("git", "branch", "--show-current")
    if branch != BRANCH:
        raise SystemExit(f"expected branch {BRANCH!r}, got {branch!r}")

    dirty_paths = {
        line[3:]
        for line in output("git", "status", "--porcelain", "--untracked-files=no").splitlines()
        if len(line) >= 4
    }
    expected = {
        str(DISPATCHER.relative_to(ROOT)),
        str(TESTS.relative_to(ROOT)),
        str(DOC.relative_to(ROOT)),
    }
    if dirty_paths != expected:
        raise SystemExit(
            "refusing recovery because tracked dirtiness is not the exact failed-applicator state:\n"
            f"expected={sorted(expected)}\nactual={sorted(dirty_paths)}"
        )

    dispatcher = DISPATCHER.read_text(encoding="utf-8")
    tests = TESTS.read_text(encoding="utf-8")
    docs = DOC.read_text(encoding="utf-8")
    required = [
        ("dispatcher", "def inject_locked_transitive_owned_patches(", dispatcher),
        ("dispatcher call", "transitive_owned_patches=", dispatcher),
        ("regression", "test_lock_derived_patch_closes_external_transitive_owned_git_edge", tests),
        ("documentation", "transitive callers outside MattOS-owned mirrors", docs),
    ]
    missing = [label for label, marker, text in required if marker not in text]
    if missing:
        raise SystemExit(f"failed-applicator recovery markers are missing: {missing}")


def fix_regression_fixture() -> None:
    text = TESTS.read_text(encoding="utf-8")

    commit_block = '''            subprocess.run(\n                [\n                    "git",\n                    "-c",\n                    "user.name=MattOS Test",\n                    "-c",\n                    "user.email=mattos-test@example.invalid",\n                    "commit",\n                    "-qm",\n                    "owned fixture",\n                ],\n                cwd=owned,\n                check=True,\n            )\n\n            external = fixture / "external"\n'''
    commit_replacement = '''            subprocess.run(\n                [\n                    "git",\n                    "-c",\n                    "user.name=MattOS Test",\n                    "-c",\n                    "user.email=mattos-test@example.invalid",\n                    "commit",\n                    "-qm",\n                    "owned fixture",\n                ],\n                cwd=owned,\n                check=True,\n            )\n\n            # Model MattOS accurately: the original dependency is a Git source,\n            # while the ownership replacement is a distinct derived mirror.\n            # Cargo forbids a [patch] that points back to the exact same source.\n            owned_mirror = fixture / "owned-mirror"\n            shutil.copytree(owned, owned_mirror, ignore=shutil.ignore_patterns(".git"))\n\n            external = fixture / "external"\n'''
    if text.count(commit_block) != 1:
        raise SystemExit(f"expected one owned-fixture commit block, found {text.count(commit_block)}")
    text = text.replace(commit_block, commit_replacement, 1)

    old_call = '''                {"owned": owned},\n                graph,\n            )\n            self.assertEqual(len(applied), 1)\n            patched = tomllib.loads(manifest.read_text())\n            self.assertEqual(\n                patched["patch"][owned.resolve().as_uri()]["owned-fixture"]["path"],\n                str(owned.resolve()),\n            )\n'''
    new_call = '''                {"owned": owned_mirror},\n                graph,\n            )\n            self.assertEqual(len(applied), 1)\n            patched = tomllib.loads(manifest.read_text())\n            self.assertEqual(\n                patched["patch"][owned.resolve().as_uri()]["owned-fixture"]["path"],\n                str(owned_mirror.resolve()),\n            )\n'''
    if text.count(old_call) != 1:
        raise SystemExit(f"expected one same-source fixture assertion block, found {text.count(old_call)}")
    text = text.replace(old_call, new_call, 1)

    TESTS.write_text(text, encoding="utf-8")


def main() -> None:
    require_expected_recovery_state()
    fix_regression_fixture()

    run("git", "diff", "--check")
    run("python3", "-m", "unittest", "-v", "DevUtils.test_source_ownership_overrides")

    SELF.unlink()
    run(
        "git",
        "add",
        "-A",
        "--",
        str(DISPATCHER.relative_to(ROOT)),
        str(TESTS.relative_to(ROOT)),
        str(DOC.relative_to(ROOT)),
        str(SELF.relative_to(ROOT)),
    )
    run("git", "commit", "-m", "Close transitive owned Cargo sources")
    run("git", "push", "origin", f"HEAD:{BRANCH}")
    print("PR #46 transitive owned-source closure recovered, tested, committed, and pushed.")


if __name__ == "__main__":
    main()
