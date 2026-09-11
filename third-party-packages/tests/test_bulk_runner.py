from __future__ import annotations

import importlib.util
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest import mock


ROOT = Path(__file__).resolve().parents[2]
MODULE_PATH = ROOT / "DevUtils/BuildAndUploadThirdPartyPackages.py"
SPEC = importlib.util.spec_from_file_location("third_party_bulk_runner", MODULE_PATH)
assert SPEC and SPEC.loader
runner = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = runner
SPEC.loader.exec_module(runner)


class BulkRunnerTests(unittest.TestCase):
    def test_discovers_only_immediate_recipe_modules_in_order(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for name in ("zeta.py", "alpha.py", "__private.py", "__init__.py"):
                (root / name).write_text("# fixture\n")
            (root / "nested").mkdir()
            (root / "nested" / "ignored.py").write_text("# fixture\n")
            self.assertEqual([path.name for path in runner.discover_recipes(root)], ["alpha.py", "zeta.py"])

    def test_update_invocation_uses_result_manifest_and_dry_run(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "out/tmp").mkdir(parents=True)
            recipe = root / "htop.py"
            recipe.write_text("# fixture\n")

            def completed(args, **_kwargs):
                result = Path(args[args.index("--result-json") + 1])
                result.write_text(json.dumps({"status": "dry-run", "package": "htop", "version": "1.0"}))
                return SimpleNamespace(returncode=0, stdout="recipe output\n")

            with mock.patch.object(runner, "ROOT", root), mock.patch.object(subprocess, "run", side_effect=completed) as invoke:
                outcome = runner.invoke(recipe, dry_run=True)
            self.assertEqual((outcome.recipe, outcome.status, outcome.detail), ("htop", "DRY RUN", "1.0"))
            self.assertIn("update", invoke.call_args.args[0])
            self.assertIn("--dry-run", invoke.call_args.args[0])

    def test_failure_is_isolated_and_has_a_concise_diagnostic(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "out/tmp").mkdir(parents=True)
            recipe = root / "broken.py"
            recipe.write_text("# fixture\n")
            result = SimpleNamespace(returncode=7, stdout="line one\nactual failure\n")
            with mock.patch.object(runner, "ROOT", root), mock.patch.object(subprocess, "run", return_value=result):
                outcome = runner.invoke(recipe, dry_run=False)
            self.assertEqual(outcome.status, "FAILED")
            self.assertIn("exit 7", outcome.detail)
            self.assertEqual(outcome.detail, "exit 7: actual failure")


if __name__ == "__main__":
    unittest.main()
