#!/usr/bin/env python3
from __future__ import annotations

import json
import os
import pathlib
import shutil
import subprocess
import tempfile
import time
import tomllib
import unittest
from unittest import mock

ROOT = pathlib.Path(__file__).resolve().parents[2]
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

    def test_relocated_dispatcher_finds_checkout_from_consumer_cwd(self) -> None:
        with tempfile.TemporaryDirectory(prefix="source-dispatch-root-") as raw:
            root = pathlib.Path(raw) / "checkout"
            (root / "DevUtils").mkdir(parents=True)
            (root / "DevUtils" / "cargo_source_owned.py").write_text("# marker\n")
            consumer = root / "out" / "build" / "fixture" / "source"
            consumer.mkdir(parents=True)
            previous = pathlib.Path.cwd()
            try:
                os.chdir(consumer)
                with mock.patch.dict(os.environ, {"MATTOS_REPO_ROOT": ""}, clear=False):
                    self.assertEqual(dispatcher.repo_root(), root.resolve())
            finally:
                os.chdir(previous)

    def test_no_repo_root_patch_config(self) -> None:
        self.assertFalse((ROOT / ".cargo" / "config.toml").exists())

    def test_ownership_fingerprint_contract_excludes_unrelated_components(self) -> None:
        contract = graph.ownership_rewrite_contract(ROOT, self.index, "cosmic-files")
        self.assertIn("cosmic-files", contract["components"])
        self.assertNotIn("duktape", contract["components"])
        self.assertNotIn("polkit", contract["components"])

    def test_generated_consumer_contract_is_component_scoped(self) -> None:
        contract_path = ROOT / "out/source-ownership/cargo/contracts/cosmic-files.json"
        self.assertTrue(contract_path.is_file())
        contract = json.loads(contract_path.read_text())
        self.assertEqual(contract["schema_version"], 1)
        self.assertEqual(contract["component"], "cosmic-files")
        self.assertNotIn("duktape", contract["rewrite_contract"]["components"])

    def test_unrelated_source_record_does_not_change_mirror_fingerprint(self) -> None:
        with tempfile.TemporaryDirectory() as raw:
            root = pathlib.Path(raw)
            source = root / "src/owned"
            source.mkdir(parents=True)
            (source / "Cargo.toml").write_text(
                "[package]\nname='owned'\nversion='0.1.0'\nedition='2024'\n"
            )
            (source / "src").mkdir()
            (source / "src/lib.rs").write_text("pub fn value() -> u8 { 1 }\n")
            (root / "upstream/state").mkdir(parents=True)
            sources = root / "upstream/sources.toml"
            sources.write_text(
                '[[component]]\nname="owned"\nrepo="https://example.invalid/owned"\n'
                'revision="1"\npath="src/owned"\nsync="copy"\n'
            )
            subprocess.run(["git", "init", "-q"], cwd=root, check=True)
            subprocess.run(["git", "add", "."], cwd=root, check=True)
            index = {
                "components": {
                    "owned": {
                        "name": "owned",
                        "repo": "https://example.invalid/owned",
                        "revision": "1",
                        "source_path": "src/owned",
                        "packages": {"owned": ""},
                    }
                },
                "root_packages": {"owned": {"component": "owned", "package_path": ""}},
                "gitlink_replacements": {},
            }
            first = graph.mirror_fingerprint(root, index, "owned")
            with sources.open("a") as stream:
                stream.write(
                    '\n[[component]]\nname="unrelated"\nrepo="https://example.invalid/unrelated"\n'
                    'revision="1"\npath="src/unrelated"\nsync="copy"\n'
                )
            self.assertEqual(first, graph.mirror_fingerprint(root, index, "owned"))

    def test_canonical_mirror_prunes_derived_cargo_and_python_outputs(self) -> None:
        with tempfile.TemporaryDirectory() as raw:
            mirror = pathlib.Path(raw) / "mirror"
            (mirror / "target/debug/deps").mkdir(parents=True)
            (mirror / "target/debug/deps/example.rmeta").write_text("derived")
            (mirror / "tools/__pycache__").mkdir(parents=True)
            (mirror / "tools/__pycache__/helper.pyc").write_bytes(b"derived")
            (mirror / "src/lib.rs").parent.mkdir(parents=True)
            (mirror / "src/lib.rs").write_text("pub fn value() -> u8 { 1 }\n")

            removed = graph.prune_derived_source_mirror_artifacts(mirror)

            self.assertIn(mirror / "target", removed)
            self.assertIn(mirror / "tools/__pycache__", removed)
            self.assertFalse((mirror / "target").exists())
            self.assertFalse((mirror / "tools/__pycache__").exists())
            self.assertTrue((mirror / "src/lib.rs").is_file())

    def test_equivalent_owned_dependencies_keep_one_canonical_source_identity(self) -> None:
        canonical = {"libcosmic": pathlib.Path("/out/source-ownership/sources/libcosmic")}
        first = graph.consumer_mirrors(
            canonical, "cosmic-files", pathlib.Path("/out/build/files/sources")
        )
        second = graph.consumer_mirrors(
            canonical, "cosmic-settings", pathlib.Path("/out/build/settings/sources")
        )
        self.assertEqual(first["libcosmic"], second["libcosmic"])
        self.assertNotEqual(first["cosmic-files"], second["cosmic-settings"])

    def test_nested_cargo_invocation_uses_enclosing_component_mirror(self) -> None:
        index = {
            "components": {
                "dbus-broker": {"source_path": "src/system/dbus/dbus-broker"},
                "cosmic-comp": {"source_path": "src/desktop/cosmic/cosmic-comp"},
                "cosmic-files": {"source_path": "src/desktop/cosmic/cosmic-files"},
                "cosmic-initial-setup": {
                    "source_path": "src/desktop/cosmic/cosmic-initial-setup"
                },
            }
        }
        self.assertEqual(
            dispatcher.component_mirror(
                pathlib.Path("/workspace"), "dbus-broker", index
            ),
            pathlib.Path("/workspace/out/build/dbus-broker/source"),
        )
        self.assertEqual(
            dispatcher.component_mirror(
                pathlib.Path("/workspace"), "cosmic-files", index
            ),
            pathlib.Path(
                "/workspace/out/build/cosmic-desktop/sources/cosmic-files"
            ),
        )
        self.assertEqual(
            dispatcher.component_mirror(
                pathlib.Path("/workspace"), "cosmic-comp", index
            ),
            pathlib.Path("/workspace/out/build/cosmic-comp/source"),
        )
        self.assertEqual(
            dispatcher.component_mirror(
                pathlib.Path("/workspace"), "cosmic-initial-setup", index
            ),
            pathlib.Path(
                "/workspace/out/build/cosmic-desktop/sources/cosmic-initial-setup"
            ),
        )

    def test_dead_source_ownership_transactions_are_reclaimed_but_live_are_preserved(self) -> None:
        with tempfile.TemporaryDirectory() as raw:
            root = pathlib.Path(raw)
            mirror_root = root / "out/source-ownership/sources"
            mirror_root.mkdir(parents=True)
            dead = mirror_root / ".cosmic-files.building-999999999"
            dead.mkdir()
            live = mirror_root / f".cosmic-files.previous-{os.getpid()}"
            live.mkdir()
            removed = graph.cleanup_abandoned_component_transactions(root, "cosmic-files")
            self.assertEqual(removed, [dead])
            self.assertFalse(dead.exists())
            self.assertTrue(live.exists())

    def test_consumer_lock_serializes_same_mirror(self) -> None:
        with tempfile.TemporaryDirectory() as raw:
            root = pathlib.Path(raw)
            mirror = root / "out/build/cosmic-desktop/sources/cosmic-files"
            ready = root / "ready"
            script = f"""
import pathlib, sys, time
sys.path.insert(0, {str(ROOT / 'DevUtils')!r})
import source_ownership_graph as g
root = pathlib.Path({str(root)!r})
mirror = pathlib.Path({str(mirror)!r})
with g.consumer_mirror_lock(root, mirror):
    pathlib.Path({str(ready)!r}).write_text('ready')
    time.sleep(0.7)
"""
            child = subprocess.Popen([sys.executable, "-c", script])
            try:
                for _ in range(50):
                    if ready.exists():
                        break
                    time.sleep(0.02)
                self.assertTrue(ready.exists())
                started = time.monotonic()
                with graph.consumer_mirror_lock(root, mirror):
                    pass
                self.assertGreaterEqual(time.monotonic() - started, 0.5)
            finally:
                child.wait(timeout=5)

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

    def test_cosmic_tweaks_is_first_class_source_owned(self) -> None:
        component = self.index["components"].get("cosmic-tweaks")
        self.assertIsNotNone(component)
        assert component is not None
        self.assertEqual(component["repo"], "https://github.com/cosmic-utils/tweaks.git")
        self.assertEqual(component["revision"], "069c31b7b1beffddf744b28f8f056ace972830bc")
        self.assertEqual(component["packages"].get("cosmic-ext-tweaks"), "")

        for package, repo, expected in [
            ("libcosmic", "https://github.com/pop-os/libcosmic.git", "libcosmic"),
            ("cosmic-panel-config", "https://github.com/pop-os/cosmic-panel", "cosmic-panel"),
            (
                "cosmic-settings-config",
                "https://github.com/pop-os/cosmic-settings-daemon",
                "cosmic-settings-daemon",
            ),
        ]:
            target = graph.choose_owned_git_target(self.index, package, repo)
            self.assertIsNotNone(target)
            assert target is not None
            self.assertEqual(target["component"], expected)

    def test_registry_resolution_can_use_first_class_root(self) -> None:
        root_packages = self.index.get("root_packages", {})
        if "libcosmic" in root_packages:
            self.assertEqual(
                graph.choose_owned_registry_target(self.index, "libcosmic"),
                root_packages["libcosmic"],
            )

    def test_metadata_probe_preserves_caller_resolution_policy(self) -> None:
        original = [
            "build",
            "--release",
            "--locked",
            "--offline",
            "--features",
            "wayland,systemd",
        ]
        self.assertEqual(
            dispatcher.metadata_resolution_args(original),
            ["--locked", "--offline", "--features", "wayland,systemd"],
        )
        self.assertEqual(
            dispatcher.lock_reconciliation_args(original),
            ["--offline", "--features", "wayland,systemd"],
        )
        self.assertEqual(
            dispatcher.fetch_reconciliation_args(original),
            ["--offline"],
        )
        frozen = ["check", "--frozen", "--all-features", "--manifest-path=Cargo.toml"]
        self.assertEqual(
            dispatcher.metadata_resolution_args(frozen),
            ["--frozen", "--all-features", "--manifest-path=Cargo.toml"],
        )
        self.assertEqual(
            dispatcher.lock_reconciliation_args(frozen),
            ["--offline", "--all-features", "--manifest-path=Cargo.toml"],
        )
        self.assertEqual(
            dispatcher.fetch_reconciliation_args(frozen),
            ["--offline", "--manifest-path=Cargo.toml"],
        )
        self.assertTrue(dispatcher.requires_lock_reconciliation(original))
        self.assertTrue(dispatcher.requires_lock_reconciliation(frozen))
        self.assertFalse(dispatcher.requires_lock_reconciliation(["build", "--release"]))

    def test_locked_output_lock_reconciles_after_git_to_path_rewrite(self) -> None:
        output_root = ROOT / "out" / "tmp"
        output_root.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(prefix="source-lock-reconcile-", dir=output_root) as raw:
            fixture = pathlib.Path(raw)
            owned = fixture / "owned"
            (owned / "src").mkdir(parents=True)
            (owned / "Cargo.toml").write_text(
                "[package]\nname='owned-fixture'\nversion='0.1.0'\nedition='2024'\n"
            )
            (owned / "src/lib.rs").write_text("pub fn value() -> u8 { 1 }\n")
            subprocess.run(["git", "init", "-q"], cwd=owned, check=True)
            subprocess.run(["git", "add", "."], cwd=owned, check=True)
            subprocess.run(
                [
                    "git",
                    "-c",
                    "user.name=MattOS Test",
                    "-c",
                    "user.email=mattos-test@example.invalid",
                    "commit",
                    "-qm",
                    "fixture",
                ],
                cwd=owned,
                check=True,
            )

            consumer = fixture / "consumer"
            (consumer / "src").mkdir(parents=True)
            (consumer / "src/main.rs").write_text("fn main() {}\n")
            manifest = consumer / "Cargo.toml"
            manifest.write_text(
                "[package]\n"
                "name='consumer-fixture'\n"
                "version='0.1.0'\n"
                "edition='2024'\n\n"
                "[dependencies]\n"
                f"owned-fixture = {{ git = '{owned.resolve().as_uri()}' }}\n\n"
                "[workspace]\n"
            )
            subprocess.run(
                ["cargo", "generate-lockfile"],
                cwd=consumer,
                check=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )
            lockfile = consumer / "Cargo.lock"
            original_lock = lockfile.read_bytes()
            strict_original = subprocess.run(
                ["cargo", "metadata", "--format-version", "1", "--locked"],
                cwd=consumer,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                check=False,
            )
            self.assertEqual(strict_original.returncode, 0, strict_original.stderr)

            manifest.write_text(
                "[package]\n"
                "name='consumer-fixture'\n"
                "version='0.1.0'\n"
                "edition='2024'\n\n"
                "[dependencies]\n"
                "owned-fixture = { path = '../owned' }\n\n"
                "[workspace]\n"
            )
            stale = subprocess.run(
                ["cargo", "metadata", "--format-version", "1", "--locked"],
                cwd=consumer,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                check=False,
            )
            self.assertNotEqual(stale.returncode, 0)
            self.assertIn("lock file", stale.stderr.lower())

            trace = fixture / "lock-reconcile.log"
            reconciled = dispatcher.reconcile_output_lock(
                "cargo",
                consumer,
                ["build", "--locked"],
                trace,
            )
            self.assertIsNotNone(reconciled)
            assert reconciled is not None
            self.assertEqual(reconciled.returncode, 0, reconciled.stderr)
            self.assertNotEqual(lockfile.read_bytes(), original_lock)

            strict_rewritten = subprocess.run(
                ["cargo", "metadata", "--format-version", "1", "--locked"],
                cwd=consumer,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                check=False,
            )
            self.assertEqual(strict_rewritten.returncode, 0, strict_rewritten.stderr)
            trace_text = trace.read_text()
            self.assertIn('lock_reconcile_1_argv=', trace_text)
            self.assertIn('"fetch"', trace_text)
            self.assertNotIn('"update"', trace_text)

    def test_lock_derived_patch_closes_external_transitive_owned_git_edge(self) -> None:
        output_root = ROOT / "out" / "tmp"
        output_root.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(prefix="source-transitive-patch-", dir=output_root) as raw:
            fixture = pathlib.Path(raw)

            owned = fixture / "owned"
            (owned / "src").mkdir(parents=True)
            (owned / "Cargo.toml").write_text(
                "[package]\nname='owned-fixture'\nversion='0.1.0'\nedition='2024'\n"
            )
            (owned / "src/lib.rs").write_text("pub fn value() -> u8 { 1 }\n")
            subprocess.run(["git", "init", "-q"], cwd=owned, check=True)
            subprocess.run(["git", "add", "."], cwd=owned, check=True)
            subprocess.run(
                [
                    "git",
                    "-c",
                    "user.name=MattOS Test",
                    "-c",
                    "user.email=mattos-test@example.invalid",
                    "commit",
                    "-qm",
                    "owned fixture",
                ],
                cwd=owned,
                check=True,
            )

            # Model MattOS accurately: the original dependency is a Git source,
            # while the ownership replacement is a distinct derived mirror.
            # Cargo forbids a [patch] that points back to the exact same source.
            owned_mirror = fixture / "owned-mirror"
            shutil.copytree(owned, owned_mirror, ignore=shutil.ignore_patterns(".git"))

            external = fixture / "external"
            (external / "src").mkdir(parents=True)
            (external / "Cargo.toml").write_text(
                "[package]\n"
                "name='external-fixture'\n"
                "version='0.1.0'\n"
                "edition='2024'\n\n"
                "[dependencies]\n"
                f"owned-fixture = {{ git = '{owned.resolve().as_uri()}' }}\n"
            )
            (external / "src/lib.rs").write_text("pub fn external() -> u8 { 2 }\n")
            subprocess.run(["git", "init", "-q"], cwd=external, check=True)
            subprocess.run(["git", "add", "."], cwd=external, check=True)
            subprocess.run(
                [
                    "git",
                    "-c",
                    "user.name=MattOS Test",
                    "-c",
                    "user.email=mattos-test@example.invalid",
                    "commit",
                    "-qm",
                    "external fixture",
                ],
                cwd=external,
                check=True,
            )

            consumer = fixture / "consumer"
            (consumer / "src").mkdir(parents=True)
            (consumer / "src/main.rs").write_text("fn main() {}\n")
            manifest = consumer / "Cargo.toml"
            manifest.write_text(
                "[package]\n"
                "name='consumer-fixture'\n"
                "version='0.1.0'\n"
                "edition='2024'\n\n"
                "[dependencies]\n"
                "owned-fixture = { path = '../owned' }\n"
                f"external-fixture = {{ git = '{external.resolve().as_uri()}' }}\n\n"
                "[workspace]\n"
            )
            subprocess.run(
                ["cargo", "generate-lockfile"],
                cwd=consumer,
                check=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )
            lockfile = consumer / "Cargo.lock"
            before = subprocess.run(
                ["cargo", "metadata", "--format-version", "1", "--locked"],
                cwd=consumer,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                check=False,
            )
            self.assertEqual(before.returncode, 0, before.stderr)
            before_metadata = json.loads(before.stdout)
            self.assertTrue(
                any(
                    pkg.get("name") == "owned-fixture"
                    and isinstance(pkg.get("source"), str)
                    and dispatcher.cargo_git_source_repo(pkg["source"]) == owned.resolve().as_uri()
                    for pkg in before_metadata["packages"]
                )
            )

            index = {
                "components": {
                    "owned": {
                        "name": "owned",
                        "repo": owned.resolve().as_uri(),
                        "packages": {"owned-fixture": ""},
                    }
                },
                "repos": {graph.norm_repo(owned.resolve().as_uri()): ["owned"]},
                "gitlink_replacements": {},
            }
            applied = dispatcher.inject_locked_transitive_owned_patches(
                manifest,
                lockfile,
                index,
                {"owned": owned_mirror},
                graph,
            )
            self.assertEqual(len(applied), 1)
            patched = tomllib.loads(manifest.read_text())
            self.assertEqual(
                patched["patch"][owned.resolve().as_uri()]["owned-fixture"]["path"],
                str(owned_mirror.resolve()),
            )

            stale = subprocess.run(
                ["cargo", "metadata", "--format-version", "1", "--locked"],
                cwd=consumer,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                check=False,
            )
            self.assertNotEqual(stale.returncode, 0)

            trace = fixture / "transitive-lock-reconcile.log"
            reconciled = dispatcher.reconcile_output_lock(
                "cargo",
                consumer,
                ["build", "--locked"],
                trace,
            )
            self.assertIsNotNone(reconciled)
            assert reconciled is not None
            self.assertEqual(reconciled.returncode, 0, reconciled.stderr)

            strict = subprocess.run(
                ["cargo", "metadata", "--format-version", "1", "--locked"],
                cwd=consumer,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                check=False,
            )
            self.assertEqual(strict.returncode, 0, strict.stderr)
            after_metadata = json.loads(strict.stdout)
            offenders = [
                pkg
                for pkg in after_metadata["packages"]
                if pkg.get("name") == "owned-fixture"
                and isinstance(pkg.get("source"), str)
                and dispatcher.cargo_git_source_repo(pkg["source"]) == owned.resolve().as_uri()
            ]
            self.assertEqual(offenders, [])

    def test_lock_derived_patch_reuses_existing_package_alias(self) -> None:
        output_root = ROOT / "out" / "tmp"
        output_root.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(prefix="source-patch-alias-", dir=output_root) as raw:
            fixture = pathlib.Path(raw)
            mirror = fixture / "owned-mirror"
            mirror.mkdir()
            (mirror / "Cargo.toml").write_text(
                "[package]\nname='owned-fixture'\nversion='0.1.0'\nedition='2024'\n"
            )

            repo = "https://github.com/example/owned"
            manifest = fixture / "Cargo.toml"
            manifest.write_text(
                "[package]\nname='consumer'\nversion='0.1.0'\nedition='2024'\n\n"
                f"[patch.\"{repo}\"]\n"
                f"alias = {{ git = '{repo}//', package = 'owned-fixture', rev = 'deadbeef' }}\n"
            )
            lockfile = fixture / "Cargo.lock"
            lockfile.write_text(
                "version = 3\n\n"
                "[[package]]\n"
                "name = 'owned-fixture'\n"
                "version = '0.1.0'\n"
                f"source = 'git+{repo}#0123456789abcdef'\n"
            )
            index = {
                "components": {
                    "owned": {
                        "name": "owned",
                        "repo": repo,
                        "packages": {"owned-fixture": ""},
                    }
                },
                "repos": {graph.norm_repo(repo): ["owned"]},
                "gitlink_replacements": {},
            }

            applied = dispatcher.inject_locked_transitive_owned_patches(
                manifest, lockfile, index, {"owned": mirror}, graph
            )
            self.assertEqual(len(applied), 1)
            patched = tomllib.loads(manifest.read_text())
            table = patched["patch"][repo]
            self.assertEqual(set(table), {"alias"})
            self.assertEqual(
                table["alias"],
                {"path": str(mirror.resolve()), "package": "owned-fixture"},
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

    def test_production_canonical_mirror_rebuilds_stale_patched_source(self) -> None:
        """Exercise prepare_graph's shared-mirror path, not only patch helpers."""
        canonical = ROOT / "out/source-ownership/sources/cosmic-files"
        if not canonical.is_dir():
            self.skipTest("canonical ownership mirror has not been materialized")
        output_root = ROOT / "out" / "tmp"
        output_root.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(prefix="canonical-mirror-regression-", dir=output_root) as raw:
            backup = pathlib.Path(raw) / "backup"
            shutil.copytree(canonical, backup, symlinks=True)
            try:
                tab = canonical / "src/tab.rs"
                text = tab.read_text()
                tab.write_text(text.replace("widget::text_editor::text_editor", "widget::text_editor"))
                marker = canonical / ".mattos-source-ownership.json"
                marker.write_text(marker.read_text())
                with tempfile.TemporaryDirectory(prefix="canonical-consumer-", dir=output_root) as consumer_raw:
                    consumer = pathlib.Path(consumer_raw)
                    shutil.copy2(ROOT / "src/desktop/cosmic/cosmic-edit/Cargo.toml", consumer / "Cargo.toml")
                    graph.prepare_graph(ROOT, self.index, "cosmic-edit", consumer)
                    rebuilt = tab.read_text()
                    self.assertIn("widget::text_editor::text_editor(content)", rebuilt)
                    self.assertIn("widget::text_editor::text_editor(text)", rebuilt)
                    graph.prepare_graph(ROOT, self.index, "cosmic-edit", consumer)
                    self.assertIn("widget::text_editor::text_editor(content)", tab.read_text())
            finally:
                shutil.rmtree(canonical)
                shutil.copytree(backup, canonical, symlinks=True)

    def test_cosmic_build_mirror_applies_registered_patch_before_cargo_isolation(self) -> None:
        source = (ROOT / "src/tools/mattos-build/src/stages/desktop.rs").read_text()
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
