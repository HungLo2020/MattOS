#!/usr/bin/env python3
from __future__ import annotations

import json
import pathlib
import subprocess
import tomllib
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]
GENERATOR = ROOT / "DevUtils" / "generate_source_overrides.py"
OUTPUT = ROOT / "out" / "source-ownership" / "cargo"
INDEX = OUTPUT / "index.json"


def normalize_repo(value: str) -> str:
    value = value.rstrip("/")
    if value.endswith(".git"):
        value = value[:-4]
    return value.lower()


def uses_workspace(value) -> bool:
    if isinstance(value, dict):
        return value.get("workspace") is True or any(uses_workspace(v) for v in value.values())
    if isinstance(value, list):
        return any(uses_workspace(v) for v in value)
    return False


class SourceOwnershipOverridesTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        subprocess.run(["python3", str(GENERATOR)], cwd=ROOT, check=True)
        cls.index = json.loads(INDEX.read_text())

    def component_config(self, component: str) -> dict:
        metadata = self.index["components"][component]
        config = metadata.get("config")
        if config is None:
            return {}
        with (ROOT / config).open("rb") as stream:
            return tomllib.load(stream)

    def patches_for(self, component: str, repository: str) -> dict:
        config = self.component_config(component)
        expected = normalize_repo(repository)
        merged = {}
        for source, packages in config.get("patch", {}).items():
            if source != "crates-io" and normalize_repo(source) == expected:
                merged.update(packages)
        return merged

    def assert_patch_path(self, component: str, repository: str, package: str, expected: str) -> None:
        patches = self.patches_for(component, repository)
        self.assertIn(package, patches, f"missing {package} override for {component}")
        self.assertEqual(pathlib.Path(patches[package]["path"]).resolve(), (ROOT / expected).resolve())

    def test_no_repo_root_patch_config(self) -> None:
        self.assertFalse((ROOT / ".cargo" / "config.toml").exists())

    def test_unrelated_workspaces_receive_no_cosmic_patches(self) -> None:
        self.assertIsNone(self.index["components"]["brush"].get("config"))
        self.assertIsNone(self.index["components"]["coreutils"].get("config"))

    def test_cosmic_applibrary_uses_owned_dependencies(self) -> None:
        self.assert_patch_path(
            "cosmic-applibrary",
            "https://github.com/pop-os/libcosmic",
            "libcosmic",
            "src/desktop/cosmic/libcosmic",
        )
        self.assert_patch_path(
            "cosmic-applibrary",
            "https://github.com/pop-os/cosmic-applets",
            "cosmic-app-list-config",
            "src/desktop/cosmic/cosmic-applets/cosmic-app-list/cosmic-app-list-config",
        )

    def test_cosmic_panel_uses_owned_libcosmic_and_protocols(self) -> None:
        self.assert_patch_path(
            "cosmic-panel", "https://github.com/pop-os/libcosmic", "libcosmic", "src/desktop/cosmic/libcosmic"
        )
        self.assert_patch_path(
            "cosmic-panel",
            "https://github.com/pop-os/cosmic-protocols",
            "cosmic-protocols",
            "src/desktop/cosmic/cosmic-protocols",
        )

    def test_libcosmic_has_no_private_iced_path_edges(self) -> None:
        manifest = tomllib.loads((ROOT / "src/desktop/cosmic/libcosmic/Cargo.toml").read_text())
        for name in [
            "iced", "iced_runtime", "iced_renderer", "iced_core", "iced_widget",
            "iced_futures", "iced_accessibility", "iced_tiny_skia", "iced_winit", "iced_wgpu",
        ]:
            self.assertTrue(manifest["dependencies"][name]["path"].startswith("../iced"))
        self.assertEqual(manifest["build-dependencies"]["build_helpers"]["path"], "../iced/build_helpers")

    def test_every_emitted_nested_patch_preserves_workspace(self) -> None:
        failures = []
        for component, metadata in self.index["components"].items():
            config_path = metadata.get("config")
            if not config_path:
                continue
            config = tomllib.loads((ROOT / config_path).read_text())
            for packages in config.get("patch", {}).values():
                for package, spec in packages.items():
                    crate = pathlib.Path(spec["path"]).resolve()
                    manifest_path = crate / "Cargo.toml"
                    manifest = tomllib.loads(manifest_path.read_text())
                    if not uses_workspace(manifest) or "workspace" in manifest:
                        continue
                    package_table = manifest.get("package", {})
                    if not isinstance(package_table.get("workspace"), str):
                        failures.append(f"{component}:{package}:{manifest_path.relative_to(ROOT)}")
        self.assertEqual(failures, [])


if __name__ == "__main__":
    unittest.main()
