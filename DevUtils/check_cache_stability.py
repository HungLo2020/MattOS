#!/usr/bin/env python3
"""Check that the MattOS build cache is stable when nothing has changed.

Runs ``build all``, then the Rust unit tests, then ``build all`` again, and
fails if the second build misses any stage or package cache.  With no source
edits in between, every miss means a cache key depends on state that is not
a real build input -- for example a test rewriting the compiler wrappers, or
a key hashing incidental directory contents.  Each offending line keeps the
cache's own explanation of what changed.

Usage:
    python3 DevUtils/check_cache_stability.py            # full check
    python3 DevUtils/check_cache_stability.py --no-establish
        # skip the first build when one just completed
"""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
BUILD_ALL = ["cargo", "run", "-q", "-p", "mattos-build", "--", "build", "all"]
UNIT_TESTS = ["cargo", "test", "-q", "-p", "mattos-build", "-p", "mattos-installer"]
MISS_LINE = re.compile(r"^(?:package )?cache miss: .*$", re.MULTILINE)


def cache_misses(build_output: str) -> list[str]:
    """Every stage or package cache miss a build reported, with its reason."""
    return MISS_LINE.findall(build_output)


def run(command: list[str], label: str) -> str:
    print(f"[cache-stability] {label}: {' '.join(command)}", flush=True)
    completed = subprocess.run(
        command,
        cwd=REPO_ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    if completed.returncode != 0:
        sys.stdout.write(completed.stdout[-8000:])
        raise SystemExit(f"[cache-stability] {label} failed with exit {completed.returncode}")
    return completed.stdout


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument(
        "--no-establish",
        action="store_true",
        help="skip the first build (use right after a successful build all)",
    )
    args = parser.parse_args()
    if not args.no_establish:
        run(BUILD_ALL, "establishing build")
    run(UNIT_TESTS, "unit tests")
    misses = cache_misses(run(BUILD_ALL, "no-op build"))
    if misses:
        print(f"[cache-stability] FAIL: {len(misses)} cache miss(es) with no input changes:")
        for line in misses:
            print(f"  {line}")
        return 1
    print("[cache-stability] PASS: no-op build after the unit tests reused every cache")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
