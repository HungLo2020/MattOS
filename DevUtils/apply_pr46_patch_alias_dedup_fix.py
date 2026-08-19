#!/usr/bin/env python3
from __future__ import annotations

import subprocess
from pathlib import Path

BRANCH = "agent/repair-cosmic-files-provenance"
ROOT = Path(__file__).resolve().parents[1]
DISPATCHER = ROOT / "DevUtils/cargo_source_owned.py"
TESTS = ROOT / "DevUtils/test_source_ownership_overrides.py"
SELF = Path(__file__).resolve()


def run(*args: str) -> None:
    subprocess.run(args, cwd=ROOT, check=True)


def output(*args: str) -> str:
    return subprocess.check_output(args, cwd=ROOT, text=True).strip()


def replace_once(text: str, old: str, new: str, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"expected exactly one {label}, found {count}")
    return text.replace(old, new, 1)


def require_clean_branch() -> None:
    branch = output("git", "branch", "--show-current")
    if branch != BRANCH:
        raise SystemExit(f"expected branch {BRANCH!r}, got {branch!r}")
    status = subprocess.check_output(
        ["git", "status", "--porcelain=v1", "--untracked-files=no"],
        cwd=ROOT,
        text=True,
    )
    if status:
        raise SystemExit("refusing to modify a dirty tracked checkout:\n" + status)


def patch_dispatcher() -> None:
    text = DISPATCHER.read_text(encoding="utf-8")
    old = '''        replacement = {'path': str(target_path)}\n        if table.get(package) != replacement:\n            table[package] = replacement\n            changed = True\n        applied.append(f'{source_key}:{package}->{target_path}')\n'''
    new = '''        # Cargo patch keys may be aliases. The effective package identity is\n        # ``spec.package`` when present, otherwise the table key. Replacing by\n        # the literal package name would create a second entry for manifests\n        # such as ``cctk = { package = \"cosmic-client-toolkit\", ... }``.\n        matching_package_keys: list[str] = []\n        for entry_key, entry_spec in table.items():\n            effective_package = entry_key\n            if isinstance(entry_spec, dict):\n                declared_package = entry_spec.get('package')\n                if isinstance(declared_package, str):\n                    effective_package = declared_package\n            if effective_package == package:\n                matching_package_keys.append(entry_key)\n\n        if len(matching_package_keys) > 1:\n            raise RuntimeError(\n                f'multiple Cargo patch entries resolve to owned package {package!r} '\n                f'for source {source_key}: {matching_package_keys}'\n            )\n\n        entry_key = matching_package_keys[0] if matching_package_keys else package\n        replacement = {'path': str(target_path)}\n        if entry_key != package:\n            replacement['package'] = package\n        if table.get(entry_key) != replacement:\n            table[entry_key] = replacement\n            changed = True\n        applied.append(f'{source_key}:{entry_key}({package})->{target_path}')\n'''
    DISPATCHER.write_text(replace_once(text, old, new, "patch injection block"), encoding="utf-8")


def patch_tests() -> None:
    text = TESTS.read_text(encoding="utf-8")
    marker = '''    def test_rewrite_does_not_conflate_same_name_git_package(self) -> None:\n'''
    test = '''    def test_lock_derived_patch_reuses_existing_package_alias(self) -> None:\n        output_root = ROOT / "out" / "tmp"\n        output_root.mkdir(parents=True, exist_ok=True)\n        with tempfile.TemporaryDirectory(prefix="source-patch-alias-", dir=output_root) as raw:\n            fixture = pathlib.Path(raw)\n            mirror = fixture / "owned-mirror"\n            mirror.mkdir()\n            (mirror / "Cargo.toml").write_text(\n                "[package]\\nname='owned-fixture'\\nversion='0.1.0'\\nedition='2024'\\n"\n            )\n\n            repo = "https://github.com/example/owned"\n            manifest = fixture / "Cargo.toml"\n            manifest.write_text(\n                "[package]\\nname='consumer'\\nversion='0.1.0'\\nedition='2024'\\n\\n"\n                f"[patch.\\\"{repo}\\\"]\\n"\n                f"alias = {{ git = '{repo}//', package = 'owned-fixture', rev = 'deadbeef' }}\\n"\n            )\n            lockfile = fixture / "Cargo.lock"\n            lockfile.write_text(\n                "version = 3\\n\\n"\n                "[[package]]\\n"\n                "name = 'owned-fixture'\\n"\n                "version = '0.1.0'\\n"\n                f"source = 'git+{repo}#0123456789abcdef'\\n"\n            )\n            index = {\n                "components": {\n                    "owned": {\n                        "name": "owned",\n                        "repo": repo,\n                        "packages": {"owned-fixture": ""},\n                    }\n                },\n                "repos": {graph.norm_repo(repo): ["owned"]},\n                "gitlink_replacements": {},\n            }\n\n            applied = dispatcher.inject_locked_transitive_owned_patches(\n                manifest, lockfile, index, {"owned": mirror}, graph\n            )\n            self.assertEqual(len(applied), 1)\n            patched = tomllib.loads(manifest.read_text())\n            table = patched["patch"][repo]\n            self.assertEqual(set(table), {"alias"})\n            self.assertEqual(\n                table["alias"],\n                {"path": str(mirror.resolve()), "package": "owned-fixture"},\n            )\n\n''' + marker
    TESTS.write_text(replace_once(text, marker, test, "alias regression insertion"), encoding="utf-8")


def main() -> None:
    require_clean_branch()
    patch_dispatcher()
    patch_tests()
    run("git", "diff", "--check")
    run("python3", "-m", "unittest", "-v", "DevUtils.test_source_ownership_overrides")

    SELF.unlink()
    run(
        "git", "add", "-A", "--",
        str(DISPATCHER.relative_to(ROOT)),
        str(TESTS.relative_to(ROOT)),
        str(SELF.relative_to(ROOT)),
    )
    run("git", "commit", "-m", "Deduplicate aliased Cargo ownership patches")
    run("git", "push", "origin", f"HEAD:{BRANCH}")
    print("PR #46 alias-aware Cargo patch closure applied, tested, committed, and pushed.")


if __name__ == "__main__":
    main()
