"""Contracts for pinned desktop schemas and MattOS-owned integration policy."""
from pathlib import Path
import hashlib
import importlib.util
import re
import subprocess
import tempfile
import tomllib
import unittest

ROOT = Path(__file__).resolve().parents[2]


class DesktopPolicyTests(unittest.TestCase):
    def test_store_patch_supports_normal_forward_and_reverse_verification(self):
        catalog = tomllib.loads((ROOT / "upstream/sources.toml").read_text())
        component = next(c for c in catalog["component"] if c["name"] == "cosmic-store")
        manifest = ROOT / component["patch_manifest"]
        self.assertEqual(hashlib.sha256(manifest.read_bytes()).hexdigest(), component["patch_manifest_sha256"])
        patch = tomllib.loads(manifest.read_text())["patch"][0]
        patch_path = ROOT / patch["path"]
        self.assertEqual(hashlib.sha256(patch_path.read_bytes()).hexdigest(), patch["sha256"])
        with tempfile.TemporaryDirectory() as temporary:
            target = Path(temporary) / "src/backend/flatpak.rs"
            target.parent.mkdir(parents=True)
            original = (ROOT / "src/desktop/cosmic/cosmic-store/src/backend/flatpak.rs").read_bytes()
            target.write_bytes(original)
            def apply(*options):
                subprocess.run(["git", "apply", "--whitespace=error-all", *options, str(patch_path)],
                               cwd=temporary, check=True, capture_output=True)
            apply("--check")
            apply()
            self.assertIn("update_appstream_sync(&remote_name,", target.read_text())
            apply("--reverse", "--check")
            apply("--reverse")
            self.assertEqual(target.read_bytes(), original)

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

    def test_theme_defaults_cover_current_models_and_boolean_types(self):
        source = (ROOT / "src/desktop/cosmic/libcosmic/cosmic-theme/src/model/theme.rs").read_text()
        for model, suffix in [("Theme", ""), ("ThemeBuilder", ".Builder")]:
            body = source.split(f"pub struct {model} {{", 1)[1].split("\n}", 1)[0]
            fields = re.findall(r"pub(?:\(crate\))? (\w+): ([^,\n]+)", body)
            self.assertGreater(len(fields), 20)
            for theme in ["Dark", "Light"]:
                name = f"com.system76.CosmicTheme.{theme}{suffix}"
                base = ROOT / "src/desktop/cosmic/cosmic-settings/resources/default_schema" / name
                overlay = ROOT / "resources/COSMIC/defaults" / name / "v2"
                for key, ty in fields:
                    path = overlay / key if (overlay / key).exists() else base / "v2" / key
                    if key == "list_button" and not path.exists():
                        # Explicit v1 -> v2 staging adaptation.
                        path = base / "v1" / key
                    self.assertTrue(path.is_file(), f"{model}: missing {path}")
                    if ty == "bool":
                        self.assertIn(path.read_text().strip(), ("true", "false"), str(path))

    def test_all_panel_layouts_cover_current_schema(self):
        source = (ROOT / "src/desktop/cosmic/cosmic-panel/cosmic-panel-config/src/panel_config.rs").read_text()
        body = source.split("pub struct CosmicPanelConfig {", 1)[1].split("\n}", 1)[0]
        fields = re.findall(r"pub (\w+): ([^,\n]+)", body)
        self.assertGreater(len(fields), 20)
        for directory in (ROOT / "resources/COSMIC").rglob("v1"):
            if directory.parent.name not in ("com.system76.CosmicPanel.Panel", "com.system76.CosmicPanel.Dock"):
                continue
            for key, ty in fields:
                path = directory / key
                self.assertTrue(path.is_file(), f"missing {path}")
                if ty == "bool":
                    self.assertIn(path.read_text().strip(), ("true", "false"), str(path))

    def test_initial_setup_patch_is_declared_and_does_not_change_dbus_policy(self):
        catalog = tomllib.loads((ROOT / "upstream/sources.toml").read_text())
        component = next(c for c in catalog["component"] if c["name"] == "cosmic-initial-setup")
        manifest = ROOT / component["patch_manifest"]
        self.assertEqual(hashlib.sha256(manifest.read_bytes()).hexdigest(), component["patch_manifest_sha256"])
        data = tomllib.loads(manifest.read_text())
        for patch in data["patch"]:
            content = (ROOT / patch["path"]).read_bytes()
            self.assertEqual(hashlib.sha256(content).hexdigest(), patch["sha256"])
            self.assertIn(b"-        _ = settings.load_connections(&[]).await;", content)
        self.assertEqual(data["application"], "output-mirror-only")

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
