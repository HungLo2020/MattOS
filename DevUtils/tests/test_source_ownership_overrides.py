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













if __name__ == "__main__":
    unittest.main()
