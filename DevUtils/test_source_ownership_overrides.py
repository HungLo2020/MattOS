#!/usr/bin/env python3
from __future__ import annotations

import json
import pathlib
import shutil
import subprocess
import tempfile
import tomllib
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]
GENERATOR = ROOT / "DevUtils" / "generate_source_overrides.py"
INDEX = ROOT / "out" / "source-ownership" / "cargo" / "index.json"

import sys
sys.path.insert(0, str(ROOT / "DevUtils"))
import cargo_source_owned as dispatcher  # noqa: E402
import source_ownership_graph as graph  # noqa: E402


class SourceOwnershipGraphTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        subprocess.run(["python3", str(GENERATOR)], cwd=ROOT, check=True)
        cls.index = json.loads(INDEX.read_text())

    def test_no_repo_root_patch_config(self) -> None:
        self.assertFalse((ROOT / ".cargo" / "config.toml").exists())

    def test_git_resolution_is_source_qualified(self) -> None:
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
            mirrors = {name: tmp_path / name for name in self.index.get("components", {})}
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
            mirrors = {name: tmp_path / "canonical" / name for name in self.index.get("components", {})}
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

    def test_consumer_override_does_not_mutate_canonical_mirrors(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = pathlib.Path(tmp)
            canonical = {name: tmp_path / "canonical" / name for name in self.index.get("components", {})}
            canonical_comp = canonical["cosmic-comp"]
            private_comp = tmp_path / "build" / "cosmic-comp"
            consumer = graph.consumer_mirrors(canonical, "cosmic-comp", private_comp)
            self.assertEqual(consumer["cosmic-comp"], private_comp.resolve())
            self.assertEqual(canonical["cosmic-comp"], canonical_comp)
            self.assertNotEqual(consumer["cosmic-comp"], canonical["cosmic-comp"])

    def test_cosmic_comp_output_patch_manifest_applies_with_git_semantics(self) -> None:
        metadata = self.index["components"]["cosmic-comp"]
        source = ROOT / metadata["source_path"] / "src" / "lib.rs"
        output_root = ROOT / "out" / "tmp"
        output_root.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(prefix="cosmic-comp-owned-patch-", dir=output_root) as raw:
            mirror = pathlib.Path(raw)
            mirrored = mirror / "src" / "lib.rs"
            mirrored.parent.mkdir(parents=True)
            shutil.copy2(source, mirrored)
            pristine = source.read_bytes()
            graph.apply_component_patches(ROOT, metadata, mirror)
            text = mirrored.read_text()
            self.assertIn('.filter(|arg| arg != "--no-xwayland")', text)
            self.assertIn('ListeningSocketSource::with_name(&name)', text)
            self.assertEqual(source.read_bytes(), pristine)

    def test_cosmic_files_provenance_matches_registered_patch(self) -> None:
        metadata = self.index["components"]["cosmic-files"]
        state = tomllib.loads((ROOT / "upstream/state/cosmic-files.toml").read_text())
        sources = tomllib.loads((ROOT / "upstream/sources.toml").read_text())
        source_entry = next(item for item in sources["component"] if item["name"] == "cosmic-files")
        self.assertEqual(
            metadata["revision"],
            "24e34eaa0f0acf4e24ea1338ad4bbde3a138e1f3",
        )
        self.assertEqual(state["imported_commit"], metadata["revision"])
        self.assertEqual(source_entry["revision"], metadata["revision"])
        self.assertEqual(
            metadata["patch_manifest"],
            "upstream/patches/cosmic-files/manifest.toml",
        )
        self.assertEqual(state["patch_manifest"], metadata["patch_manifest"])
        self.assertEqual(source_entry["patch_manifest"], metadata["patch_manifest"])

    def test_cosmic_files_consumer_patch_is_idempotent_and_matches_owned_libcosmic_api(self) -> None:
        metadata = self.index["components"]["cosmic-files"]
        source = ROOT / metadata["source_path"] / "src" / "tab.rs"
        output_root = ROOT / "out" / "tmp"
        output_root.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(prefix="cosmic-files-owned-patch-", dir=output_root) as raw:
            mirror = pathlib.Path(raw)
            mirrored = mirror / "src" / "tab.rs"
            mirrored.parent.mkdir(parents=True)
            shutil.copy2(source, mirrored)
            pristine = source.read_bytes()
            self.assertEqual(
                dispatcher.apply_consumer_patches(ROOT, metadata, mirror, graph),
                "applied",
            )
            self.assertEqual(
                dispatcher.apply_consumer_patches(ROOT, metadata, mirror, graph),
                "applied",
            )
            text = mirrored.read_text()
            self.assertIn("widget::text_editor::text_editor(content)", text)
            self.assertIn("widget::text_editor::text_editor(text)", text)
            self.assertIn(".style(text_editor_class)", text)
            self.assertNotIn("widget::text_editor(content)", text)
            self.assertNotIn("widget::text_editor(text)", text)
            self.assertEqual(source.read_bytes(), pristine)

    def test_cosmic_build_mirror_applies_registered_patch_before_cargo_isolation(self) -> None:
        source = (ROOT / "src/tools/mattos-build/src/main.rs").read_text()
        body = source.split("fn build_cosmic_just_component(", 1)[1].split(
            "fn build_cosmic_desktop_component(", 1
        )[0]
        sync = body.index("sync_build_source(")
        patch = body.index("apply_component_patches(repo_root, component, &mirror)?;")
        isolate = body.index("isolate_cargo_build_mirror(&mirror)?;")
        self.assertLess(sync, patch)
        self.assertLess(patch, isolate)

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
        self.assertEqual(graph.verify_metadata(json.dumps(metadata), ROOT, self.index, mirrors), [])

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
        manifest = tomllib.loads((ROOT / "src/desktop/cosmic/libcosmic/cosmic-config/Cargo.toml").read_text())
        dep = manifest["dependencies"]["cosmic-settings-daemon"]
        self.assertEqual(dep["git"], "https://github.com/pop-os/dbus-settings-bindings")
        self.assertNotIn("path", dep)


if __name__ == "__main__":
    unittest.main()
