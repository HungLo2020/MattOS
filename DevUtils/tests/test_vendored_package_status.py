from __future__ import annotations

import importlib.util
import json
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock


ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "DevUtils"))
MODULE_PATH = ROOT / "DevUtils/VendoredPackageStatus.py"
SPEC = importlib.util.spec_from_file_location("vendored_package_status", MODULE_PATH)
assert SPEC and SPEC.loader
status = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = status
SPEC.loader.exec_module(status)


class VendoredPackageStatusTests(unittest.TestCase):
    def test_inventory_groups_exact_package_facts_by_source_component(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "inventory.toml"
            path.write_text(
                "[[package]]\nname = 'libwayland-client0'\nversion = '1.26.0-1mattos1'\n"
                "architecture = 'amd64'\nsource_component = 'wayland'\nsha256 = 'abc'\n"
            )
            grouped, error = status.load_inventory(path)
        self.assertIsNone(error)
        self.assertEqual(grouped["wayland"][0].name, "libwayland-client0")
        self.assertEqual(grouped["wayland"][0].version, "1.26.0-1mattos1")

    def test_inventory_fails_closed_when_required_fact_is_missing(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "inventory.toml"
            path.write_text("[[package]]\nname = 'broken'\n")
            grouped, error = status.load_inventory(path)
        self.assertEqual(grouped, {})
        self.assertIn("malformed", error or "")

    def test_imported_state_requires_declared_pin_and_identity_to_match(self):
        component = status.UpdateChecker.Component("wayland", "https://example.invalid/wayland.git", "1.0", "a" * 40, "src/wayland")
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            path = root / "upstream/state/wayland.toml"
            path.parent.mkdir(parents=True)
            path.write_text(
                "component = 'wayland'\nrepo = 'https://example.invalid/wayland.git'\n"
                "destination_path = 'src/wayland'\nimported_commit = '" + "a" * 40 + "'\n"
            )
            self.assertEqual(status.imported_state(root, component).status, "verified")
            path.write_text("component = 'wayland'\nimported_commit = '" + "b" * 40 + "'\n")
            self.assertEqual(status.imported_state(root, component).status, "provenance-mismatch")

    def test_hosted_inventory_uses_one_explicit_read_only_manager_list_call(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            publisher = root / status.PUBLISHER_RELATIVE
            publisher.parent.mkdir(parents=True)
            publisher.touch()
            completed = mock.Mock(returncode=0, stdout="wayland\t1.26.0-1mattos1\tamd64\n")
            with mock.patch.object(status.subprocess, "run", return_value=completed) as run:
                inventory = status.hosted_inventory(root, "mattos")
        self.assertEqual(inventory, {"wayland": ["1.26.0-1mattos1"]})
        self.assertEqual(run.call_args.args[0][2:], ["--non-interactive", "--repo", "mattos", "list"])

    def test_component_report_joins_selected_source_local_target_and_hosted_version(self):
        component = status.UpdateChecker.Component("wayland", "repo", "1.26.0", "a" * 40, "src/wayland")
        upstream = status.UpdateChecker.UpdateResult(component, "up-to-date", 0, "a" * 40, "a" * 40)
        imported = status.ImportedState("wayland", "a" * 40, "verified", None)
        package = status.PackageRecord("libwayland-client0", "1.26.0-1mattos1", "amd64", "wayland", "f" * 64)
        row = status.component_row(component, upstream, imported, [package], {"libwayland-client0": ["1.26.0-1mattos1"]})
        self.assertEqual(row["packages"][0]["publication_status"], "published")
        self.assertEqual(row["packages"][0]["newest_published"], "1.26.0-1mattos1")
        plain = "\n".join(status.render_row(row, verbose=True, colors=status.Colors(False)))
        self.assertIn("[UPSTREAM CURRENT]", plain)
        self.assertIn("[PROVENANCE VERIFIED]", plain)
        self.assertIn("[PUBLISHED]", plain)
        colored = "\n".join(status.render_row(row, verbose=False, colors=status.Colors(True)))
        self.assertIn("\033[", colored)

    def test_main_reports_all_authorities_without_network_when_upstream_and_hosted_are_mocked(self):
        component = status.UpdateChecker.Component("wayland", "repo", "1.0", "a" * 40, "src/wayland")
        result = status.UpdateChecker.UpdateResult(component, "up-to-date", 0, "a" * 40, "a" * 40)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "upstream/state").mkdir(parents=True)
            (root / "upstream/state/wayland.toml").write_text(
                "component = 'wayland'\nrepo = 'repo'\ndestination_path = 'src/wayland'\n"
                "imported_commit = '" + "a" * 40 + "'\n"
            )
            (root / "out/packages").mkdir(parents=True)
            (root / "out/packages/inventory.toml").write_text(
                "[[package]]\nname = 'libwayland-client0'\nversion = '1.0-1mattos1'\n"
                "architecture = 'amd64'\nsource_component = 'wayland'\nsha256 = 'abc'\n"
            )
            with mock.patch.object(status, "ROOT", root), \
                 mock.patch.object(status.UpdateChecker, "load_components", return_value=[component]), \
                 mock.patch.object(status, "inspect_upstream", return_value={"wayland": result}), \
                 mock.patch.object(status, "hosted_inventory", return_value={"libwayland-client0": ["1.0-1mattos1"]}):
                with mock.patch("sys.stdout") as stdout:
                    self.assertEqual(status.main(["--json"]), 0)
        report = json.loads("".join(call.args[0] for call in stdout.write.call_args_list))
        self.assertEqual(report["components"][0]["packages"][0]["publication_status"], "published")


if __name__ == "__main__":
    unittest.main()
