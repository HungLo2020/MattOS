#!/usr/bin/env python3
"""One-shot PR #46 applicator for transitive owned Git-source closure.

Structural source-ownership rewriting can only edit manifests MattOS controls.
An ordinary external Git dependency may itself depend on a Git repository that
MattOS owns, leaving a duplicate external source in Cargo's graph. This script
adds the lock-derived, source-qualified output-mirror [patch] closure, its
regression, and documentation. It validates before committing and removes
itself afterward.
"""
from __future__ import annotations

import subprocess
from pathlib import Path

BRANCH = "agent/repair-cosmic-files-provenance"
ROOT = Path(__file__).resolve().parents[1]
DISPATCHER = ROOT / "DevUtils/cargo_source_owned.py"
TESTS = ROOT / "DevUtils/test_source_ownership_overrides.py"
DOC = ROOT / "docs/SOURCE_OWNERSHIP.md"
SELF = Path(__file__).resolve()


def run(*args: str) -> None:
    subprocess.run(args, cwd=ROOT, check=True)


def output(*args: str) -> str:
    return subprocess.check_output(args, cwd=ROOT, text=True).strip()


def require_clean_branch() -> None:
    branch = output("git", "branch", "--show-current")
    if branch != BRANCH:
        raise SystemExit(f"expected branch {BRANCH!r}, got {branch!r}")
    dirty = output("git", "status", "--porcelain", "--untracked-files=no")
    if dirty:
        raise SystemExit("refusing to modify a dirty tracked checkout:\n" + dirty)


def replace_once(text: str, old: str, new: str, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"expected exactly one {label} block, found {count}")
    return text.replace(old, new, 1)


def patch_dispatcher() -> None:
    text = DISPATCHER.read_text(encoding="utf-8")

    marker = '''    return run_capture(command, cwd, trace, 'lock_reconcile', capture_stdout=True)\n\n\ndef validated_consumer_patch_manifest'''
    replacement = '''    return run_capture(command, cwd, trace, 'lock_reconcile', capture_stdout=True)\n\n\ndef cargo_git_source_repo(source: str) -> str | None:\n    \"\"\"Return the repository identity from a Cargo ``git+`` source string.\"\"\"\n    if not source.startswith('git+'):\n        return None\n    return source[4:].split('#', 1)[0].split('?', 1)[0]\n\n\ndef locked_owned_git_targets(lockfile: pathlib.Path, index: dict, graph) -> list[dict[str, str]]:\n    \"\"\"Find owned Git packages still named by the copied upstream lock.\n\n    Structural rewriting changes manifests that MattOS owns, but an ordinary\n    external Git dependency can itself name one of MattOS's owned repositories.\n    The copied lock is the deterministic inventory of those source identities\n    before derived-lock reconciliation.\n    \"\"\"\n    if not lockfile.is_file():\n        return []\n    data = tomllib.loads(lockfile.read_text(encoding='utf-8'))\n    targets: dict[tuple[str, str], dict[str, str]] = {}\n    for package in data.get('package', []):\n        if not isinstance(package, dict):\n            continue\n        name = package.get('name')\n        source = package.get('source')\n        if not isinstance(name, str) or not isinstance(source, str):\n            continue\n        repo = cargo_git_source_repo(source)\n        if repo is None:\n            continue\n        target = graph.choose_owned_git_target(index, name, repo)\n        if target is None:\n            continue\n        item = {'repo': repo, 'package': name, **target}\n        key = (graph.norm_repo(repo), name)\n        previous = targets.get(key)\n        if previous is not None and previous != item:\n            raise RuntimeError(\n                f'locked owned Git package {name!r} from {repo!r} has ambiguous targets: '\n                f'{previous} vs {item}'\n            )\n        targets[key] = item\n    return [targets[key] for key in sorted(targets)]\n\n\ndef inject_locked_transitive_owned_patches(\n    manifest: pathlib.Path,\n    lockfile: pathlib.Path,\n    index: dict,\n    mirrors: dict[str, pathlib.Path],\n    graph,\n) -> list[str]:\n    \"\"\"Close owned Git sources used by external transitive manifests.\n\n    Only the derived consumer manifest is changed. Structural path rewriting\n    remains the primary ownership mechanism; this minimal source-qualified\n    ``[patch]`` table covers callers whose downloaded manifests MattOS cannot\n    rewrite. The strict metadata verifier remains the final authority.\n    \"\"\"\n    targets = locked_owned_git_targets(lockfile, index, graph)\n    if not targets:\n        return []\n\n    data = tomllib.loads(manifest.read_text(encoding='utf-8'))\n    patch_root = data.setdefault('patch', {})\n    if not isinstance(patch_root, dict):\n        raise RuntimeError(f'Cargo patch table is not a table: {manifest}')\n\n    changed = False\n    applied: list[str] = []\n    for target in targets:\n        component = target['component']\n        package = target['package']\n        package_path = target['package_path']\n        mirror = mirrors.get(component)\n        if mirror is None:\n            raise RuntimeError(f'owned transitive target {component}:{package} has no mirror mapping')\n        target_path = (mirror / package_path).resolve()\n        if not (target_path / 'Cargo.toml').is_file():\n            raise RuntimeError(\n                f'owned transitive target mirror was not prepared for {package}: {target_path}'\n            )\n\n        repo = target['repo']\n        normalized = graph.norm_repo(repo)\n        matching_keys = [\n            key\n            for key in patch_root\n            if isinstance(key, str) and graph.norm_repo(key) == normalized\n        ]\n        if len(matching_keys) > 1:\n            raise RuntimeError(f'multiple Cargo patch tables normalize to owned source {repo}: {matching_keys}')\n        source_key = matching_keys[0] if matching_keys else repo\n        table = patch_root.setdefault(source_key, {})\n        if not isinstance(table, dict):\n            raise RuntimeError(f'Cargo patch source is not a table: {source_key}')\n\n        replacement = {'path': str(target_path)}\n        if table.get(package) != replacement:\n            table[package] = replacement\n            changed = True\n        applied.append(f'{source_key}:{package}->{target_path}')\n\n    if changed:\n        manifest.write_text(graph.dump_toml(data), encoding='utf-8')\n    return sorted(applied)\n\n\ndef validated_consumer_patch_manifest'''
    text = replace_once(text, marker, replacement, "dispatcher helper insertion")

    old = '''        mirrors = graph.prepare_graph(root, index, component, consumer_mirror)\n    except Exception as exc:\n        append_trace(trace, f'prepare_error={exc}')\n        raise SystemExit(f'MattOS source ownership preparation failed for {component}: {exc}') from exc\n    append_trace(trace, 'prepare=success')\n    append_trace(\n        trace,\n        'mirrors=' + json.dumps({k: str(v) for k, v in sorted(mirrors.items()) if v.exists()}, sort_keys=True),\n    )\n\n    lockfile = manifest.parent / 'Cargo.lock'\n    append_trace(trace, f'lock_sha256_before={digest_file(lockfile)}')\n'''
    new = '''        mirrors = graph.prepare_graph(root, index, component, consumer_mirror)\n        lockfile = manifest.parent / 'Cargo.lock'\n        transitive_patches = inject_locked_transitive_owned_patches(\n            manifest,\n            lockfile,\n            index,\n            mirrors,\n            graph,\n        )\n        append_trace(trace, 'transitive_owned_patches=' + json.dumps(transitive_patches))\n    except Exception as exc:\n        append_trace(trace, f'prepare_error={exc}')\n        raise SystemExit(f'MattOS source ownership preparation failed for {component}: {exc}') from exc\n    append_trace(trace, 'prepare=success')\n    append_trace(\n        trace,\n        'mirrors=' + json.dumps({k: str(v) for k, v in sorted(mirrors.items()) if v.exists()}, sort_keys=True),\n    )\n\n    append_trace(trace, f'lock_sha256_before={digest_file(lockfile)}')\n'''
    text = replace_once(text, old, new, "dispatcher prepare/lock block")
    DISPATCHER.write_text(text, encoding="utf-8")


def patch_tests() -> None:
    text = TESTS.read_text(encoding="utf-8")
    marker = '''    def test_rewrite_does_not_conflate_same_name_git_package(self) -> None:\n'''
    test = '''    def test_lock_derived_patch_closes_external_transitive_owned_git_edge(self) -> None:\n        output_root = ROOT / "out" / "tmp"\n        output_root.mkdir(parents=True, exist_ok=True)\n        with tempfile.TemporaryDirectory(prefix="source-transitive-patch-", dir=output_root) as raw:\n            fixture = pathlib.Path(raw)\n\n            owned = fixture / "owned"\n            (owned / "src").mkdir(parents=True)\n            (owned / "Cargo.toml").write_text(\n                "[package]\\nname='owned-fixture'\\nversion='0.1.0'\\nedition='2024'\\n"\n            )\n            (owned / "src/lib.rs").write_text("pub fn value() -> u8 { 1 }\\n")\n            subprocess.run(["git", "init", "-q"], cwd=owned, check=True)\n            subprocess.run(["git", "add", "."], cwd=owned, check=True)\n            subprocess.run(\n                [\n                    "git",\n                    "-c",\n                    "user.name=MattOS Test",\n                    "-c",\n                    "user.email=mattos-test@example.invalid",\n                    "commit",\n                    "-qm",\n                    "owned fixture",\n                ],\n                cwd=owned,\n                check=True,\n            )\n\n            external = fixture / "external"\n            (external / "src").mkdir(parents=True)\n            (external / "Cargo.toml").write_text(\n                "[package]\\n"\n                "name='external-fixture'\\n"\n                "version='0.1.0'\\n"\n                "edition='2024'\\n\\n"\n                "[dependencies]\\n"\n                f"owned-fixture = {{ git = '{owned.resolve().as_uri()}' }}\\n"\n            )\n            (external / "src/lib.rs").write_text("pub fn external() -> u8 { 2 }\\n")\n            subprocess.run(["git", "init", "-q"], cwd=external, check=True)\n            subprocess.run(["git", "add", "."], cwd=external, check=True)\n            subprocess.run(\n                [\n                    "git",\n                    "-c",\n                    "user.name=MattOS Test",\n                    "-c",\n                    "user.email=mattos-test@example.invalid",\n                    "commit",\n                    "-qm",\n                    "external fixture",\n                ],\n                cwd=external,\n                check=True,\n            )\n\n            consumer = fixture / "consumer"\n            (consumer / "src").mkdir(parents=True)\n            (consumer / "src/main.rs").write_text("fn main() {}\\n")\n            manifest = consumer / "Cargo.toml"\n            manifest.write_text(\n                "[package]\\n"\n                "name='consumer-fixture'\\n"\n                "version='0.1.0'\\n"\n                "edition='2024'\\n\\n"\n                "[dependencies]\\n"\n                "owned-fixture = { path = '../owned' }\\n"\n                f"external-fixture = {{ git = '{external.resolve().as_uri()}' }}\\n\\n"\n                "[workspace]\\n"\n            )\n            subprocess.run(\n                ["cargo", "generate-lockfile"],\n                cwd=consumer,\n                check=True,\n                stdout=subprocess.PIPE,\n                stderr=subprocess.PIPE,\n                text=True,\n            )\n            lockfile = consumer / "Cargo.lock"\n            before = subprocess.run(\n                ["cargo", "metadata", "--format-version", "1", "--locked"],\n                cwd=consumer,\n                stdout=subprocess.PIPE,\n                stderr=subprocess.PIPE,\n                text=True,\n                check=False,\n            )\n            self.assertEqual(before.returncode, 0, before.stderr)\n            before_metadata = json.loads(before.stdout)\n            self.assertTrue(\n                any(\n                    pkg.get("name") == "owned-fixture"\n                    and isinstance(pkg.get("source"), str)\n                    and dispatcher.cargo_git_source_repo(pkg["source"]) == owned.resolve().as_uri()\n                    for pkg in before_metadata["packages"]\n                )\n            )\n\n            index = {\n                "components": {\n                    "owned": {\n                        "name": "owned",\n                        "repo": owned.resolve().as_uri(),\n                        "packages": {"owned-fixture": ""},\n                    }\n                },\n                "repos": {graph.norm_repo(owned.resolve().as_uri()): ["owned"]},\n                "gitlink_replacements": {},\n            }\n            applied = dispatcher.inject_locked_transitive_owned_patches(\n                manifest,\n                lockfile,\n                index,\n                {"owned": owned},\n                graph,\n            )\n            self.assertEqual(len(applied), 1)\n            patched = tomllib.loads(manifest.read_text())\n            self.assertEqual(\n                patched["patch"][owned.resolve().as_uri()]["owned-fixture"]["path"],\n                str(owned.resolve()),\n            )\n\n            stale = subprocess.run(\n                ["cargo", "metadata", "--format-version", "1", "--locked"],\n                cwd=consumer,\n                stdout=subprocess.PIPE,\n                stderr=subprocess.PIPE,\n                text=True,\n                check=False,\n            )\n            self.assertNotEqual(stale.returncode, 0)\n\n            trace = fixture / "transitive-lock-reconcile.log"\n            reconciled = dispatcher.reconcile_output_lock(\n                "cargo",\n                consumer,\n                ["build", "--locked"],\n                trace,\n            )\n            self.assertIsNotNone(reconciled)\n            assert reconciled is not None\n            self.assertEqual(reconciled.returncode, 0, reconciled.stderr)\n\n            strict = subprocess.run(\n                ["cargo", "metadata", "--format-version", "1", "--locked"],\n                cwd=consumer,\n                stdout=subprocess.PIPE,\n                stderr=subprocess.PIPE,\n                text=True,\n                check=False,\n            )\n            self.assertEqual(strict.returncode, 0, strict.stderr)\n            after_metadata = json.loads(strict.stdout)\n            offenders = [\n                pkg\n                for pkg in after_metadata["packages"]\n                if pkg.get("name") == "owned-fixture"\n                and isinstance(pkg.get("source"), str)\n                and dispatcher.cargo_git_source_repo(pkg["source"]) == owned.resolve().as_uri()\n            ]\n            self.assertEqual(offenders, [])\n\n''' + marker
    text = replace_once(text, marker, test, "transitive ownership regression insertion")
    TESTS.write_text(text, encoding="utf-8")


def patch_docs() -> None:
    text = DOC.read_text(encoding="utf-8")
    old = '''Cargo `[patch]` is intentionally not the ownership enforcement mechanism. `[patch]` participates in normal Cargo resolution and therefore cannot express MattOS's stronger rule that an owned dependency must use one exact local source and may not fall back to another source identity.\n\n`DevUtils/source_ownership_graph.py` instead rewrites dependency declarations in derived build mirrors before Cargo resolves them. The authoritative imported trees under `src/` remain pristine.\n'''
    new = '''Cargo `[patch]` is intentionally not the primary ownership enforcement mechanism. `[patch]` participates in normal Cargo resolution and by itself cannot express MattOS's stronger rule that an owned dependency must use one exact local source and may not fall back to another source identity.\n\n`DevUtils/source_ownership_graph.py` therefore rewrites dependency declarations in derived build mirrors before Cargo resolves them. The authoritative imported trees under `src/` remain pristine. A downloaded ordinary Git build dependency is different: MattOS does not own or rewrite that external manifest, so it can still contain a transitive edge back to a Git repository that MattOS owns. For those edges, `DevUtils/cargo_source_owned.py` reads the copied upstream lock before reconciliation and adds a minimal source-qualified `[patch]` table to the **derived consumer mirror only**, covering exactly the locked package names whose Git source is owned. Each entry points at an already-prepared MattOS mirror; a missing mirror fails closed. Structural path rewriting remains primary and the metadata verifier remains the hard guarantee that no owned Git source survived.\n'''
    text = replace_once(text, old, new, "source ownership patch-policy paragraph")

    old = '''For a caller that requests `--locked` or `--frozen`, the dispatcher first reconciles only the `out/build/...` lockfile after patching and ownership rewriting. It temporarily removes the lock prohibition for that derived-output step while retaining offline policy: `--offline` stays offline and `--frozen` is reconciled as offline. Cargo begins from the copied upstream lock and keeps already locked dependency versions whenever they still satisfy the rewritten graph. The authoritative lockfile under `src/` is never edited.\n'''
    new = '''For a caller that requests `--locked` or `--frozen`, the dispatcher first closes any locked external-transitive references to owned Git sources with the derived source-qualified patch table, then reconciles only the `out/build/...` lockfile after patching and ownership rewriting. It temporarily removes the lock prohibition for that derived-output step while retaining offline policy: `--offline` stays offline and `--frozen` is reconciled as offline. Cargo begins from the copied upstream lock and keeps already locked dependency versions whenever they still satisfy the rewritten graph. The authoritative lockfile under `src/` is never edited.\n'''
    text = replace_once(text, old, new, "derived lock paragraph")

    old = '''3. prepares the transitive MattOS-owned canonical source mirrors and rewrites dependency edges;\n4. when the caller is locked/frozen, reconciles the copied output-mirror `Cargo.lock` for the rewritten source identities while preserving offline policy;\n5. runs `cargo metadata` against the rewritten graph using the caller's original dependency-resolution policy flags (`--locked`, `--offline`, and/or `--frozen`, plus relevant feature/manifest selection);\n6. verifies that an owned Git package did not remain external and that canonical first-class path/registry packages resolve from their expected MattOS mirror; and\n7. only after verification runs the original Cargo command.\n'''
    new = '''3. prepares the transitive MattOS-owned canonical source mirrors and rewrites dependency edges;\n4. derives a minimal source-qualified Cargo patch closure from owned Git packages still named by the copied upstream lock, covering transitive callers whose external manifests MattOS cannot rewrite;\n5. when the caller is locked/frozen, reconciles the copied output-mirror `Cargo.lock` for the rewritten source identities while preserving offline policy;\n6. runs `cargo metadata` against the rewritten graph using the caller's original dependency-resolution policy flags (`--locked`, `--offline`, and/or `--frozen`, plus relevant feature/manifest selection);\n7. verifies that an owned Git package did not remain external and that canonical first-class path/registry packages resolve from their expected MattOS mirror; and\n8. only after verification runs the original Cargo command.\n'''
    text = replace_once(text, old, new, "fail-closed workflow list")

    old = '''Ownership-enabled Cargo invocations write detailed traces under `out/source-ownership/logs/<component>.log`. These logs include consumer patch state, derived-lock reconciliation, graph preparation, metadata verification and final Cargo diagnostics.\n'''
    new = '''Ownership-enabled Cargo invocations write detailed traces under `out/source-ownership/logs/<component>.log`. These logs include consumer patch state, lock-derived transitive ownership patches, derived-lock reconciliation, graph preparation, metadata verification and final Cargo diagnostics.\n'''
    text = replace_once(text, old, new, "diagnostics paragraph")

    old = '''The first command validates source/patch provenance and regenerates the derived ownership catalog. The second exercises source-qualified resolution, canonical/private mirror separation, Git-format output-patch application, idempotent consumer patching, build-mirror patch ordering, derived-lock reconciliation, Cargo metadata resolution-policy propagation, gitlink replacement behavior, metadata fail-closed checks, provenance agreement, and preservation of pristine imported manifests.\n'''
    new = '''The first command validates source/patch provenance and regenerates the derived ownership catalog. The second exercises source-qualified resolution, canonical/private mirror separation, Git-format output-patch application, idempotent consumer patching, build-mirror patch ordering, lock-derived transitive owned-source closure, derived-lock reconciliation, Cargo metadata resolution-policy propagation, gitlink replacement behavior, metadata fail-closed checks, provenance agreement, and preservation of pristine imported manifests.\n'''
    text = replace_once(text, old, new, "maintenance regression paragraph")
    DOC.write_text(text, encoding="utf-8")


def main() -> None:
    require_clean_branch()
    patch_dispatcher()
    patch_tests()
    patch_docs()
    run("git", "diff", "--check")
    run("python3", "-m", "unittest", "-v", "DevUtils.test_source_ownership_overrides")

    SELF.unlink()
    run(
        "git",
        "add",
        "-A",
        "--",
        str(DISPATCHER.relative_to(ROOT)),
        str(TESTS.relative_to(ROOT)),
        str(DOC.relative_to(ROOT)),
        str(SELF.relative_to(ROOT)),
    )
    run("git", "commit", "-m", "Close transitive owned Cargo sources")
    run("git", "push", "origin", f"HEAD:{BRANCH}")
    print("PR #46 transitive owned-source closure applied, tested, committed, and pushed.")


if __name__ == "__main__":
    main()
