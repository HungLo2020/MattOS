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

    def fixture_descriptor(self, path: Path, package: str = "htop"):
        return runner.RecipeDescriptor(path, package, "mattos", "1.0")

    def test_check_invocation_uses_snapshot_and_never_requests_build(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "out/tmp").mkdir(parents=True)
            recipe = root / "htop.py"
            recipe.write_text("# fixture\n")

            inventory = root / "mattos.json"
            inventory.write_text("{}")

            def completed(args, **_kwargs):
                result = Path(args[args.index("--result-json") + 1])
                result.write_text(json.dumps({
                    "status": "checked", "package": "htop", "upstream_version": "1.1",
                    "selected_version": "1.0", "repository_version": "1.0", "release_state": "upstream-newer",
                }))
                return SimpleNamespace(returncode=0, stdout="recipe output\n")

            with mock.patch.object(runner, "ROOT", root), mock.patch.object(subprocess, "run", side_effect=completed) as invoke:
                outcome = runner.invoke(self.fixture_descriptor(recipe), mode="check", dry_run=False, inventory=inventory)
            self.assertEqual((outcome.recipe, outcome.status), ("htop", "UPSTREAM NEWER"))
            self.assertIn("check", invoke.call_args.args[0])
            self.assertIn("--repository-inventory", invoke.call_args.args[0])
            self.assertNotIn("--dry-run", invoke.call_args.args[0])

    def test_update_invocation_uses_selected_release_and_dry_run(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "out/tmp").mkdir(parents=True)
            recipe = root / "htop.py"
            recipe.write_text("# fixture\n")
            inventory = root / "mattos.json"
            inventory.write_text("{}")

            def completed(args, **_kwargs):
                result = Path(args[args.index("--result-json") + 1])
                result.write_text(json.dumps({
                    "status": "dry-run", "package": "htop", "upstream_version": "1.0",
                    "selected_version": "1.0", "repository_version": None,
                }))
                return SimpleNamespace(returncode=0, stdout="recipe output\n")

            with mock.patch.object(runner, "ROOT", root), mock.patch.object(subprocess, "run", side_effect=completed) as invoke:
                outcome = runner.invoke(self.fixture_descriptor(recipe), mode="update", dry_run=True, inventory=inventory)
            self.assertEqual((outcome.recipe, outcome.status), ("htop", "DRY RUN"))
            self.assertIn("update", invoke.call_args.args[0])
            self.assertIn("--dry-run", invoke.call_args.args[0])

    def test_failure_is_isolated_and_has_a_concise_diagnostic(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "out/tmp").mkdir(parents=True)
            recipe = root / "broken.py"
            recipe.write_text("# fixture\n")
            inventory = root / "mattos.json"
            inventory.write_text("{}")
            result = SimpleNamespace(returncode=7, stdout="line one\nactual failure\n")
            with mock.patch.object(runner, "ROOT", root), mock.patch.object(subprocess, "run", return_value=result):
                outcome = runner.invoke(self.fixture_descriptor(recipe, "broken"), mode="check", dry_run=False, inventory=inventory)
            self.assertEqual(outcome.status, "FAILED")
            self.assertIn("exit 7", outcome.detail)
            self.assertEqual(outcome.detail, "exit 7: actual failure")

    def test_single_repository_inventory_is_shared_by_all_recipe_processes(self):
        recipes = [
            runner.RecipeDescriptor(Path("one.py"), "one", "mattos", "1.0"),
            runner.RecipeDescriptor(Path("two.py"), "two", "mattos", "1.0"),
        ]
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "out/tmp").mkdir(parents=True)
            with mock.patch.object(runner, "ROOT", root), \
                 mock.patch.object(runner, "descriptor", side_effect=recipes), \
                 mock.patch.object(runner, "discover_recipes", return_value=[Path("one.py"), Path("two.py")]), \
                 mock.patch.object(runner, "repository_inventory", return_value={"one": ["1.0"], "two": ["1.0"]}) as inventory, \
                 mock.patch.object(runner, "invoke", return_value=runner.Outcome("one", "UP TO DATE", "1.0", "1.0", "1.0", "ok", "")):
                self.assertEqual(runner.main(["--check", "--color", "never"]), 0)
            inventory.assert_called_once_with(root, "mattos")


if __name__ == "__main__":
    unittest.main()
