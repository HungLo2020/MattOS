#!/usr/bin/env python3
"""Regression tests for imported-source generated-residue detection."""

from __future__ import annotations

import importlib.util
from pathlib import Path
import subprocess
import tempfile
import unittest


MODULE_PATH = Path(__file__).with_name("test_imported_source_immutability.py")
SPEC = importlib.util.spec_from_file_location("imported_source_immutability", MODULE_PATH)
assert SPEC and SPEC.loader
IMMUTABILITY = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(IMMUTABILITY)


class ImportedSourceHygieneTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.repository = Path(self.temporary.name)
        subprocess.run(["git", "init", "-q"], cwd=self.repository, check=True)
        (self.repository / "upstream").mkdir()
        (self.repository / "vendor/component").mkdir(parents=True)
        (self.repository / "upstream/sources.toml").write_text(
            '[[component]]\nname = "component"\npath = "vendor/component"\n',
            encoding="utf-8",
        )
        (self.repository / ".gitignore").write_text("*.o\n", encoding="utf-8")

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def test_ignored_untracked_build_output_is_detected(self) -> None:
        generated = self.repository / "vendor/component/generated.o"
        generated.write_bytes(b"object")
        self.assertEqual(
            IMMUTABILITY.ignored_untracked_paths(self.repository),
            {"vendor/component/generated.o"},
        )

    def test_tracked_upstream_ignored_file_is_not_residue(self) -> None:
        tracked = self.repository / "vendor/component/upstream.o"
        tracked.write_bytes(b"upstream")
        subprocess.run(
            ["git", "add", "-f", ".gitignore", "upstream/sources.toml", str(tracked)],
            cwd=self.repository,
            check=True,
        )
        self.assertEqual(IMMUTABILITY.ignored_untracked_paths(self.repository), set())

    def test_only_new_ignored_paths_violate_the_baseline(self) -> None:
        before = {"vendor/component/existing.o"}
        after = {"vendor/component/existing.o", "vendor/component/new.o"}
        self.assertEqual(
            IMMUTABILITY.newly_ignored_paths(before, after),
            ["vendor/component/new.o"],
        )


if __name__ == "__main__":
    unittest.main()