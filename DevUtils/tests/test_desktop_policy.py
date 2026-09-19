"""Contracts for pinned desktop schemas and MattOS-owned integration policy."""
from pathlib import Path
import importlib.util
import tomllib
import unittest

ROOT = Path(__file__).resolve().parents[2]


class DesktopPolicyTests(unittest.TestCase):
    def test_provenance_exceptions_are_exact_and_child_imports_remain_validated(self):
        path = ROOT / "DevUtils/audits/test_vendored_source_provenance.py"
        spec = importlib.util.spec_from_file_location("polish_provenance", path)
        audit = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(audit)
        component = {"name": "appstream", "revision": "c046aa7e28e1950d44874bc5bdfc2268a338255f"}
        fixture = "tests/samples/compose/usr/share/metainfo/org.example.nonexistent-badlink.metainfo.xml"
        self.assertTrue(audit.declared_broken_test_link(component, fixture, "/nonexistent"))
        for name, revision, path, target in [
            ("other", component["revision"], fixture, "/nonexistent"),
            ("appstream", "0" * 40, fixture, "/nonexistent"),
            ("appstream", component["revision"], "tests/other", "/nonexistent"),
            ("appstream", component["revision"], fixture, "/etc/passwd"),
        ]:
            self.assertFalse(audit.declared_broken_test_link({"name": name, "revision": revision}, path, target))
        _, policies = audit.load_gitlink_policies()
        components = {c["name"]: c for c in tomllib.loads((ROOT / "upstream/sources.toml").read_text())["component"]}
        self.assertEqual(audit.verify_gitlink_replacements(policies, components), [])
        components.pop("libglnx")
        self.assertTrue(any("libglnx" in error for error in audit.verify_gitlink_replacements(policies, components)))

    def test_distro_resources_are_not_in_upstream_trees(self):
        for component in ("apt", "flatpak"):
            directory = ROOT / "src/system/packages" / component / "resources"
            self.assertFalse(directory.exists() and any(directory.iterdir()))
        state = tomllib.loads((ROOT / "upstream/state/gnupg.toml").read_text())
        self.assertEqual(state["patch_manifest"], "none")

    def test_bootstrap_is_bounded_and_does_not_upgrade_packages(self):
        root = ROOT / "src/system/packages/config/apt/units"
        service = (root / "mattos-apt-bootstrap.service").read_text()
        timer = (root / "mattos-apt-bootstrap.timer").read_text()
        self.assertIn("APT::Update::Error-Mode=any", service)
        self.assertIn("TimeoutStartSec=120", service)
        self.assertIn("ConditionPathExists=!/var/lib/mattos/apt/initialized", service)
        self.assertIn("ExecStartPost=", service)
        self.assertIn("OnBootSec=15s", timer)
        self.assertNotIn("RandomizedDelaySec", timer)
        self.assertNotIn("Restart=", service)
        self.assertNotIn(" upgrade", service)


if __name__ == "__main__":
    unittest.main()
