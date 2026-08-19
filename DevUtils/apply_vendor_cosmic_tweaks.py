#!/usr/bin/env python3
from __future__ import annotations

import json
import os
import subprocess
import sys
from pathlib import Path

BRANCH = "agent/vendor-cosmic-tweaks"
ROOT = Path(__file__).resolve().parents[1]
SELF = Path(__file__).resolve()
TWEAKS_COMMIT = "069c31b7b1beffddf744b28f8f056ace972830bc"
TWEAKS_REPO = "https://github.com/cosmic-utils/tweaks.git"
TWEAKS_SOURCE = ROOT / "src/desktop/cosmic/cosmic-tweaks"


def run(*args: str, env: dict[str, str] | None = None) -> None:
    print("+", " ".join(args), flush=True)
    subprocess.run(args, cwd=ROOT, env=env, check=True)


def output(*args: str) -> str:
    return subprocess.check_output(args, cwd=ROOT, text=True).strip()


def replace_once(path: Path, old: str, new: str, label: str) -> None:
    text = path.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path.relative_to(ROOT)}: expected exactly one {label}, found {count}")
    path.write_text(text.replace(old, new, 1), encoding="utf-8")


def replace_all(path: Path, old: str, new: str, label: str, minimum: int = 1) -> None:
    text = path.read_text(encoding="utf-8")
    count = text.count(old)
    if count < minimum:
        raise SystemExit(f"{path.relative_to(ROOT)}: expected at least {minimum} {label}, found {count}")
    path.write_text(text.replace(old, new), encoding="utf-8")


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


def register_source() -> None:
    sources = ROOT / "upstream/sources.toml"
    text = sources.read_text(encoding="utf-8")
    if 'name = "cosmic-tweaks"' in text:
        raise SystemExit("cosmic-tweaks is already registered in upstream/sources.toml")
    block = f'''\n\n[[component]]\nname = "cosmic-tweaks"\nrepo = "{TWEAKS_REPO}"\nbranch = "main"\nrevision = "{TWEAKS_COMMIT}"\npath = "src/desktop/cosmic/cosmic-tweaks"\nsync = "copy"\n'''
    sources.write_text(text.rstrip() + block, encoding="utf-8")


def import_source() -> None:
    env = os.environ.copy()
    # Keep the importer from partially staging this transaction. The script
    # stages the complete integration only after every static check passes.
    env["MATTOS_IMPORT_NO_INDEX"] = "1"
    run(
        "cargo",
        "run",
        "-p",
        "mattos-build",
        "--",
        "upstream",
        "import",
        "cosmic-tweaks",
        env=env,
    )
    if not (TWEAKS_SOURCE / "Cargo.toml").is_file():
        raise SystemExit("COSMIC Tweaks import did not materialize Cargo.toml")
    state = ROOT / "upstream/state/cosmic-tweaks.toml"
    if not state.is_file():
        raise SystemExit("COSMIC Tweaks import did not create provenance state")
    body = state.read_text(encoding="utf-8")
    for required in [
        f'imported_commit = "{TWEAKS_COMMIT}"',
        f'repo = "{TWEAKS_REPO}"',
        'destination_path = "src/desktop/cosmic/cosmic-tweaks"',
    ]:
        if required not in body:
            raise SystemExit(f"COSMIC Tweaks provenance state is missing {required!r}")


def patch_stage_graph() -> None:
    path = ROOT / "src/tools/mattos-build/src/stage_graph.rs"
    replace_once(
        path,
        "    CosmicFiles,\n    CosmicTerm,\n    CosmicUtilities,",
        "    CosmicFiles,\n    CosmicTerm,\n    CosmicTweaks,\n    CosmicUtilities,",
        "BuildStage enum insertion",
    )
    replace_once(
        path,
        '        BuildStage::CosmicTerm => "cosmic-term",\n        BuildStage::CosmicUtilities => "cosmic-utilities",',
        '        BuildStage::CosmicTerm => "cosmic-term",\n        BuildStage::CosmicTweaks => "cosmic-tweaks",\n        BuildStage::CosmicUtilities => "cosmic-utilities",',
        "stage-id insertion",
    )
    replace_once(
        path,
        "        | BuildStage::CosmicFiles\n        | BuildStage::CosmicTerm\n        | BuildStage::CosmicUtilities",
        "        | BuildStage::CosmicFiles\n        | BuildStage::CosmicTerm\n        | BuildStage::CosmicTweaks\n        | BuildStage::CosmicUtilities",
        "COSMIC dependency-class insertion",
    )
    replace_once(
        path,
        '            "cosmic-files",\n            "cosmic-term",\n            "cosmic-utilities",',
        '            "cosmic-files",\n            "cosmic-term",\n            "cosmic-tweaks",\n            "cosmic-utilities",',
        "COSMIC desktop dependency insertion",
    )
    replace_once(
        path,
        "        BuildStage::CosmicFiles,\n        BuildStage::CosmicTerm,\n        BuildStage::CosmicUtilities,",
        "        BuildStage::CosmicFiles,\n        BuildStage::CosmicTerm,\n        BuildStage::CosmicTweaks,\n        BuildStage::CosmicUtilities,",
        "all-build-stages insertion",
    )


def patch_stage_inputs() -> None:
    path = ROOT / "src/tools/mattos-build/src/stage_inputs.rs"
    replace_once(
        path,
        '        BuildStage::CosmicTerm => &["src/desktop/cosmic/cosmic-term"],\n        BuildStage::CosmicUtilities => &[',
        '        BuildStage::CosmicTerm => &["src/desktop/cosmic/cosmic-term"],\n        BuildStage::CosmicTweaks => &["src/desktop/cosmic/cosmic-tweaks"],\n        BuildStage::CosmicUtilities => &[',
        "COSMIC Tweaks source input",
    )
    replace_once(
        path,
        "        | BuildStage::CosmicFiles\n        | BuildStage::CosmicTerm\n        | BuildStage::CosmicUtilities",
        "        | BuildStage::CosmicFiles\n        | BuildStage::CosmicTerm\n        | BuildStage::CosmicTweaks\n        | BuildStage::CosmicUtilities",
        "COSMIC Tweaks tool family",
    )
    replace_once(
        path,
        "            BuildStage::CosmicLauncher,\n            BuildStage::CosmicSettings,\n        ] {",
        "            BuildStage::CosmicLauncher,\n            BuildStage::CosmicSettings,\n            BuildStage::CosmicTweaks,\n        ] {",
        "COSMIC leaf-input regression coverage",
    )


def patch_main() -> None:
    path = ROOT / "src/tools/mattos-build/src/main.rs"
    # These two adjacent arms occur in both execution dispatch and the COSMIC
    # high-memory resource class. Tweaks belongs in both.
    replace_all(
        path,
        "        | BuildStage::CosmicTerm\n        | BuildStage::CosmicUtilities",
        "        | BuildStage::CosmicTerm\n        | BuildStage::CosmicTweaks\n        | BuildStage::CosmicUtilities",
        "COSMIC stage family insertion",
        minimum=2,
    )
    replace_once(
        path,
        '        BuildStage::CosmicTerm => {\n            vec!["out/build/cosmic-term/install/usr/bin/cosmic-term".into()]\n        }\n        BuildStage::CosmicUtilities =>',
        '        BuildStage::CosmicTerm => {\n            vec!["out/build/cosmic-term/install/usr/bin/cosmic-term".into()]\n        }\n        BuildStage::CosmicTweaks => {\n            vec!["out/build/cosmic-tweaks/install/usr/bin/cosmic-ext-tweaks".into()]\n        }\n        BuildStage::CosmicUtilities =>',
        "COSMIC Tweaks expected output",
    )
    replace_once(
        path,
        '        BuildStage::CosmicFiles => Some("cosmic-files"),\n        BuildStage::CosmicTerm => Some("cosmic-term"),\n        _ => None,',
        '        BuildStage::CosmicFiles => Some("cosmic-files"),\n        BuildStage::CosmicTerm => Some("cosmic-term"),\n        BuildStage::CosmicTweaks => Some("cosmic-tweaks"),\n        _ => None,',
        "generic Just component mapping",
    )
    replace_once(
        path,
        '        "cosmic-workspaces",\n        "cosmic-files",\n        "cosmic-term",\n        "cosmic-utilities",',
        '        "cosmic-workspaces",\n        "cosmic-files",\n        "cosmic-term",\n        "cosmic-tweaks",\n        "cosmic-utilities",',
        "aggregate COSMIC install list",
    )
    replace_all(
        path,
        '        "usr/bin/cosmic-files",\n        "usr/bin/cosmic-term",\n        "usr/bin/greetd",',
        '        "usr/bin/cosmic-files",\n        "usr/bin/cosmic-term",\n        "usr/bin/cosmic-ext-tweaks",\n        "usr/bin/greetd",',
        "aggregate/rootfs COSMIC Tweaks validation",
    )
    replace_once(
        path,
        '            ("cosmic-files", 120.000),\n            ("cosmic-term", 90.000),\n            ("cosmic-utilities", 120.000),',
        '            ("cosmic-files", 120.000),\n            ("cosmic-term", 90.000),\n            ("cosmic-tweaks", 90.000),\n            ("cosmic-utilities", 120.000),',
        "scheduler timing estimate",
    )


def patch_packaging() -> None:
    path = ROOT / "src/tools/mattos-build/src/packaging.rs"
    replace_once(
        path,
        '        "usr/bin/cosmic-launcher",\n        "usr/bin/cosmic-term",\n        "usr/bin/greetd",',
        '        "usr/bin/cosmic-launcher",\n        "usr/bin/cosmic-term",\n        "usr/bin/cosmic-ext-tweaks",\n        "usr/bin/greetd",',
        "cosmic-desktop package payload validation",
    )
    replace_once(
        path,
        '        // Revision 3 keeps the greeter daemon display-manager-scoped instead\n        // of enabling it in every multi-user/CLI boot. Revision 2 supplied the\n        // freedesktop hicolor fallback index.\n        "cosmic-desktop" => 3,',
        '        // Revision 4 requires COSMIC Tweaks in the aggregate desktop payload.\n        // Revision 3 keeps the greeter daemon display-manager-scoped instead\n        // of enabling it in every multi-user/CLI boot. Revision 2 supplied the\n        // freedesktop hicolor fallback index.\n        "cosmic-desktop" => 4,',
        "cosmic-desktop package recipe revision",
    )


def patch_provenance_audit() -> None:
    path = ROOT / "DevUtils/test_vendored_source_provenance.py"
    replace_once(
        path,
        "    expected_component_count = 63",
        "    expected_component_count = 64",
        "vendored component count",
    )
    replace_once(
        path,
        '    print(f"components verified: {verified}/47")',
        '    print(f"components verified: {verified}/{len(component_list)}")',
        "dynamic provenance audit denominator",
    )


def patch_ownership_tests() -> None:
    path = ROOT / "DevUtils/test_source_ownership_overrides.py"
    marker = "    def test_registry_resolution_can_use_first_class_root(self) -> None:\n"
    test = '''    def test_cosmic_tweaks_is_first_class_source_owned(self) -> None:\n        component = self.index["components"].get("cosmic-tweaks")\n        self.assertIsNotNone(component)\n        assert component is not None\n        self.assertEqual(component["repo"], "https://github.com/cosmic-utils/tweaks.git")\n        self.assertEqual(component["revision"], "069c31b7b1beffddf744b28f8f056ace972830bc")\n        self.assertEqual(component["packages"].get("cosmic-ext-tweaks"), "")\n\n        for package, repo, expected in [\n            ("libcosmic", "https://github.com/pop-os/libcosmic.git", "libcosmic"),\n            ("cosmic-panel-config", "https://github.com/pop-os/cosmic-panel", "cosmic-panel"),\n            (\n                "cosmic-settings-config",\n                "https://github.com/pop-os/cosmic-settings-daemon",\n                "cosmic-settings-daemon",\n            ),\n        ]:\n            target = graph.choose_owned_git_target(self.index, package, repo)\n            self.assertIsNotNone(target)\n            assert target is not None\n            self.assertEqual(target["component"], expected)\n\n''' + marker
    replace_once(path, marker, test, "COSMIC Tweaks ownership regression")


def static_validation() -> None:
    run("cargo", "fmt", "-p", "mattos-build")
    run("git", "diff", "--check")
    run("python3", "DevUtils/generate_source_overrides.py")
    index = json.loads((ROOT / "out/source-ownership/cargo/index.json").read_text())
    component = index.get("components", {}).get("cosmic-tweaks")
    if component is None or component.get("packages", {}).get("cosmic-ext-tweaks") != "":
        raise SystemExit("generated ownership catalog does not own cosmic-ext-tweaks from cosmic-tweaks")
    run("python3", "-m", "unittest", "-v", "DevUtils.test_source_ownership_overrides")
    run("cargo", "test", "-p", "mattos-build", "--bin", "mattos-build")


def commit_and_push() -> None:
    SELF.unlink()
    run(
        "git",
        "add",
        "-A",
        "--",
        "upstream/sources.toml",
        "upstream/state/cosmic-tweaks.toml",
        "src/desktop/cosmic/cosmic-tweaks",
        "src/tools/mattos-build/src/main.rs",
        "src/tools/mattos-build/src/stage_graph.rs",
        "src/tools/mattos-build/src/stage_inputs.rs",
        "src/tools/mattos-build/src/packaging.rs",
        "DevUtils/test_vendored_source_provenance.py",
        "DevUtils/test_source_ownership_overrides.py",
        str(SELF.relative_to(ROOT)),
    )
    run("git", "diff", "--cached", "--check")
    run("git", "commit", "-m", "Vendor COSMIC Tweaks")
    run("git", "push", "origin", f"HEAD:{BRANCH}")


def targeted_validation() -> None:
    sys.path.insert(0, str(ROOT / "DevUtils"))
    from common import mattos_build_environment  # type: ignore

    env = mattos_build_environment(ROOT)
    run(
        "cargo",
        "run",
        "-p",
        "mattos-build",
        "--",
        "build",
        "cosmic-tweaks",
        env=env,
    )
    binary = ROOT / "out/build/cosmic-tweaks/install/usr/bin/cosmic-ext-tweaks"
    desktop = (
        ROOT
        / "out/build/cosmic-tweaks/install/usr/share/applications/dev.edfloreshz.CosmicTweaks.desktop"
    )
    icon = (
        ROOT
        / "out/build/cosmic-tweaks/install/usr/share/icons/hicolor/scalable/apps/dev.edfloreshz.CosmicTweaks.svg"
    )
    for required in [binary, desktop, icon]:
        if not required.is_file():
            raise SystemExit(f"COSMIC Tweaks targeted build is missing {required.relative_to(ROOT)}")

    # Aggregate and package validation prove the app is actually shipped, not
    # merely buildable as an orphaned stage.
    run(
        "cargo",
        "run",
        "-p",
        "mattos-build",
        "--",
        "build",
        "cosmic-desktop",
        env=env,
    )
    run(
        "cargo",
        "run",
        "-p",
        "mattos-build",
        "--",
        "package",
        "build",
        "cosmic-desktop",
        env=env,
    )


def main() -> None:
    require_clean_branch()
    register_source()
    import_source()
    patch_stage_graph()
    patch_stage_inputs()
    patch_main()
    patch_packaging()
    patch_provenance_audit()
    patch_ownership_tests()
    static_validation()
    commit_and_push()
    print("COSMIC Tweaks source/integration committed and pushed; running targeted build validation.")
    targeted_validation()
    print("COSMIC Tweaks is vendored, source-owned, built, aggregated, and package-validated on PR branch.")


if __name__ == "__main__":
    main()
