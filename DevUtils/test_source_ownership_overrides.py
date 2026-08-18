#!/usr/bin/env python3
from __future__ import annotations

import json
import pathlib
import subprocess
import tempfile
import tomllib
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]
GENERATOR = ROOT / "DevUtils" / "generate_source_overrides.py"
INDEX = ROOT / "out" / "source-ownership" / "cargo" / "index.json"

import sys
sys.path.insert(0, str(ROOT / "DevUtils"))
import source_ownership_graph as graph  # noqa: E402


class SourceOwnershipGraphTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        subprocess.run(["python3", str(GENERATOR)], cwd=ROOT, check=True)
        cls.index = json.loads(INDEX.read_text())

    def test_no_repo_root_patch_config(self) -> None:
        self.assertFalse((ROOT / ".cargo" / "config.toml").exists())

    def test_git_resolution_is_source_qualified(self) -> None:
        # libcosmic's cosmic-config crate intentionally depends on a package named
        # cosmic-settings-daemon from dbus-settings-bindings. MattOS also owns a
        # different first-class project whose root package has that same name.
        # A package-name-only resolver creates the cosmic-comp dependency cycle
        # observed during the 2026-08-18 build.
        target = graph.choose_owned_git_target(
            self.index,
            "cosmic-settings-daemon",
            "https://github.com/pop-os/dbus-settings-bindings",
        )
        self.assertIsNone(target)

        target = graph.choose_owned_git_target(
            self.index,
            "cosmic-comp-config",
            "https://github.com/pop-os/cosmic-comp",
        )
        self.assertEqual(target, {"component": "cosmic-comp", "package_path": "cosmic-comp-config"})

    def test_registry_resolution_can_use_first_class_root(self) -> None:
        root_packages = self.index.get("root_packages", {})
        if "libcosmic" in root_packages:
            self.assertEqual(
                graph.choose_owned_registry_target(self.index, "libcosmic"),
                root_packages["libcosmic"],
            )

    def test_rewrite_does_not_conflate_same_name_git_package(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = pathlib.Path(tmp)
            current = tmp_path / "libcosmic"
            current.mkdir()
            manifest = current / "Cargo.toml"
            manifest.write_text("[package]\nname='fixture'\nversion='1.0.0'\n")
            mirrors = {
                name: tmp_path / name for name in self.index.get("components", {})
            }
            mirrors["libcosmic"] = current
            table = {
                "cosmic-settings-daemon": {
                    "git": "https://github.com/pop-os/dbus-settings-bindings",
                    "optional": True,
                }
            }
            changed, needed = graph.rewrite_dependency_table(
                table,
                self.index,
                mirrors,
                manifest,
                "libcosmic",
            )
            self.assertFalse(changed)
            self.assertEqual(needed, set())
            self.assertEqual(
                table["cosmic-settings-daemon"]["git"],
                "https://github.com/pop-os/dbus-settings-bindings",
            )
            self.assertNotIn("path", table["cosmic-settings-daemon"])

    def test_rewrite_owned_git_edge_to_canonical_path(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = pathlib.Path(tmp)
            current = tmp_path / "settings-daemon"
            current.mkdir()
            manifest = current / "Cargo.toml"
            manifest.write_text("[package]\nname='fixture'\nversion='1.0.0'\n")
            mirrors = {
                name: tmp_path / name for name in self.index.get("components", {})
            }
            mirrors["cosmic-settings-daemon"] = current
            table = {
                "cosmic-comp-config": {
                    "git": "https://github.com/pop-os/cosmic-comp",
                }
            }
            changed, needed = graph.rewrite_dependency_table(
                table,
                self.index,
                mirrors,
                manifest,
                "cosmic-settings-daemon",
            )
            self.assertTrue(changed)
            self.assertEqual(needed, {"cosmic-comp"})
            self.assertNotIn("git", table["cosmic-comp-config"])
            self.assertEqual(
                pathlib.Path(table["cosmic-comp-config"]["path"]),
                (mirrors["cosmic-comp"] / "cosmic-comp-config").resolve(),
            )

    def test_metadata_verifier_does_not_claim_unrelated_git_collision(self) -> None:
        mirrors = {
            name: ROOT / "out" / "source-ownership" / "sources" / name
            for name in self.index.get("components", {})
        }
        metadata = {
            "packages": [
                {
                    "name": "cosmic-settings-daemon",
                    "source": "git+https://github.com/pop-os/dbus-settings-bindings#deadbeef",
                    "manifest_path": "/tmp/dbus-settings-bindings/Cargo.toml",
                }
            ]
        }
        self.assertEqual(
            graph.verify_metadata(json.dumps(metadata), ROOT, self.index, mirrors),
            [],
        )

    def test_metadata_verifier_rejects_owned_git_source(self) -> None:
        mirrors = {
            name: ROOT / "out" / "source-ownership" / "sources" / name
            for name in self.index.get("components", {})
        }
        metadata = {
            "packages": [
                {
                    "name": "libcosmic",
                    "source": "git+https://github.com/pop-os/libcosmic#deadbeef",
                    "manifest_path": "/tmp/libcosmic/Cargo.toml",
                }
            ]
        }
        failures = graph.verify_metadata(json.dumps(metadata), ROOT, self.index, mirrors)
        self.assertEqual(len(failures), 1)
        self.assertIn("owned git package libcosmic remained external", failures[0])

    def test_authoritative_cosmic_manifests_remain_pristine(self) -> None:
        # Structural ownership lives in output mirrors. The imported source tree
        # must retain upstream dependency declarations for provenance checking.
        manifest = tomllib.loads((ROOT / "src/desktop/cosmic/libcosmic/cosmic-config/Cargo.toml").read_text())
        dep = manifest["dependencies"]["cosmic-settings-daemon"]
        self.assertEqual(dep["git"], "https://github.com/pop-os/dbus-settings-bindings")
        self.assertNotIn("path", dep)


if __name__ == "__main__":
    unittest.main()
