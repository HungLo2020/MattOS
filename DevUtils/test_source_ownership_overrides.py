#!/usr/bin/env python3
from __future__ import annotations

import pathlib
import subprocess
import tomllib
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]
GENERATOR = ROOT / "DevUtils" / "generate_source_overrides.py"
CONFIG = ROOT / ".cargo" / "config.toml"


def normalize_repo(value: str) -> str:
    value = value.rstrip("/")
    if value.endswith(".git"):
        value = value[:-4]
    return value.lower()


class SourceOwnershipOverridesTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        subprocess.run(["python3", str(GENERATOR)], cwd=ROOT, check=True)
        with CONFIG.open("rb") as stream:
            cls.config = tomllib.load(stream)

    def patches_for(self, repository: str) -> dict:
        expected = normalize_repo(repository)
        merged = {}
        for source, packages in self.config.get("patch", {}).items():
            if source == "crates-io":
                continue
            if normalize_repo(source) == expected:
                merged.update(packages)
        return merged

    def assert_patch_path(self, repository: str, package: str, expected: str) -> None:
        patches = self.patches_for(repository)
        self.assertIn(package, patches, f"missing {package} override for {repository}")
        path = patches[package]["path"]
        resolved = (ROOT / ".cargo" / path).resolve()
        self.assertEqual(resolved, (ROOT / expected).resolve())

    def test_libcosmic_consumers_use_mattos_sources(self) -> None:
        repo = "https://github.com/pop-os/libcosmic"
        self.assert_patch_path(repo, "libcosmic", "src/desktop/cosmic/libcosmic")
        self.assert_patch_path(repo, "cosmic-config", "src/desktop/cosmic/libcosmic/cosmic-config")
        self.assert_patch_path(repo, "cosmic-theme", "src/desktop/cosmic/libcosmic/cosmic-theme")

    def test_iced_duplicates_collapse_to_first_class_component(self) -> None:
        repo = "https://github.com/pop-os/libcosmic"
        self.assert_patch_path(repo, "iced", "src/desktop/cosmic/iced")
        self.assert_patch_path(repo, "iced_core", "src/desktop/cosmic/iced/core")
        self.assert_patch_path(repo, "iced_tiny_skia", "src/desktop/cosmic/iced/tiny_skia")
        self.assert_patch_path(repo, "iced_wgpu", "src/desktop/cosmic/iced/wgpu")
        self.assert_patch_path(repo, "iced_winit", "src/desktop/cosmic/iced/winit")

    def test_cosmic_protocols_uses_first_class_component(self) -> None:
        repo = "https://github.com/pop-os/cosmic-protocols"
        self.assert_patch_path(repo, "cosmic-protocols", "src/desktop/cosmic/cosmic-protocols")
        self.assert_patch_path(
            repo,
            "cosmic-client-toolkit",
            "src/desktop/cosmic/cosmic-protocols/client-toolkit",
        )

    def test_libcosmic_has_no_private_iced_path_edges(self) -> None:
        manifest = tomllib.loads(
            (ROOT / "src/desktop/cosmic/libcosmic/Cargo.toml").read_text()
        )
        for name in [
            "iced",
            "iced_runtime",
            "iced_renderer",
            "iced_core",
            "iced_widget",
            "iced_futures",
            "iced_accessibility",
            "iced_tiny_skia",
            "iced_winit",
            "iced_wgpu",
        ]:
            path = manifest["dependencies"][name]["path"]
            self.assertTrue(path.startswith("../iced"), f"{name} still uses private path {path}")
        self.assertEqual(manifest["build-dependencies"]["build_helpers"]["path"], "../iced/build_helpers")


if __name__ == "__main__":
    unittest.main()
