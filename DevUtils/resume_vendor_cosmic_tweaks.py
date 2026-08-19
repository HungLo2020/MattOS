#!/usr/bin/env python3
from __future__ import annotations

import importlib.util
import json
import subprocess
from pathlib import Path

BRANCH = "agent/vendor-cosmic-tweaks"
ROOT = Path.cwd().resolve()
TWEAKS_COMMIT = "069c31b7b1beffddf744b28f8f056ace972830bc"
TWEAKS_REPO = "https://github.com/cosmic-utils/tweaks.git"
TWEAKS_SOURCE = ROOT / "src/desktop/cosmic/cosmic-tweaks"
HELPERS = [
    ROOT / "DevUtils/apply_vendor_cosmic_tweaks.py",
    ROOT / "DevUtils/run_vendor_cosmic_tweaks.py",
    ROOT / "DevUtils/resume_vendor_cosmic_tweaks.py",
]


def output(*args: str) -> str:
    return subprocess.check_output(args, cwd=ROOT, text=True).strip()


def run(*args: str, env: dict[str, str] | None = None) -> None:
    print("+", " ".join(args), flush=True)
    subprocess.run(args, cwd=ROOT, env=env, check=True)


def load_applicator():
    path = ROOT / "DevUtils/apply_vendor_cosmic_tweaks.py"
    spec = importlib.util.spec_from_file_location("apply_vendor_cosmic_tweaks", path)
    if spec is None or spec.loader is None:
        raise SystemExit(f"unable to load {path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def ensure_once(path: Path, old: str, new: str, label: str) -> None:
    text = path.read_text(encoding="utf-8")
    old_count = text.count(old)
    new_count = text.count(new)
    if new_count == 1 and old_count == 0:
        return
    if old_count == 1 and new_count == 0:
        path.write_text(text.replace(old, new, 1), encoding="utf-8")
        return
    raise SystemExit(
        f"{path.relative_to(ROOT)}: unexpected {label} state: "
        f"pending={old_count}, applied={new_count}"
    )


def ensure_all(path: Path, old: str, new: str, label: str, expected: int) -> None:
    text = path.read_text(encoding="utf-8")
    old_count = text.count(old)
    new_count = text.count(new)
    if old_count == 0 and new_count == expected:
        return
    if old_count + new_count == expected:
        path.write_text(text.replace(old, new), encoding="utf-8")
        return
    raise SystemExit(
        f"{path.relative_to(ROOT)}: unexpected {label} multiplicity: "
        f"pending={old_count}, applied={new_count}, expected={expected}"
    )


def verify_partial_state() -> None:
    branch = output("git", "branch", "--show-current")
    if branch != BRANCH:
        raise SystemExit(f"expected branch {BRANCH!r}, got {branch!r}")

    # The first attempt imported source and partially changed stage_graph.rs.
    # The second attempt completed stage_graph.rs and stage_inputs.rs, then
    # stopped before touching main.rs. Permit exactly those integration paths
    # plus any later idempotent integration paths if this helper is rerun.
    allowed = (
        "upstream/sources.toml",
        "upstream/state/cosmic-tweaks.toml",
        "src/desktop/cosmic/cosmic-tweaks/",
        "src/tools/mattos-build/src/stage_graph.rs",
        "src/tools/mattos-build/src/stage_inputs.rs",
        "src/tools/mattos-build/src/main.rs",
        "src/tools/mattos-build/src/packaging.rs",
        "DevUtils/test_vendored_source_provenance.py",
        "DevUtils/test_source_ownership_overrides.py",
    )
    status = subprocess.check_output(
        ["git", "status", "--porcelain=v1", "--untracked-files=all"],
        cwd=ROOT,
        text=True,
    )
    unexpected: list[str] = []
    for line in status.splitlines():
        if len(line) < 4:
            continue
        path = line[3:]
        if " -> " in path:
            path = path.split(" -> ", 1)[1]
        if not any(path == prefix or path.startswith(prefix) for prefix in allowed):
            unexpected.append(line)
    if unexpected:
        raise SystemExit(
            "refusing to resume with unrelated local changes:\n" + "\n".join(unexpected)
        )

    sources = (ROOT / "upstream/sources.toml").read_text(encoding="utf-8")
    for required in [
        'name = "cosmic-tweaks"',
        f'repo = "{TWEAKS_REPO}"',
        f'revision = "{TWEAKS_COMMIT}"',
        'path = "src/desktop/cosmic/cosmic-tweaks"',
    ]:
        if required not in sources:
            raise SystemExit(f"partial source registration is missing {required!r}")

    state_path = ROOT / "upstream/state/cosmic-tweaks.toml"
    if not state_path.is_file() or not (TWEAKS_SOURCE / "Cargo.toml").is_file():
        raise SystemExit("successful COSMIC Tweaks import is not present; refusing to guess")
    state = state_path.read_text(encoding="utf-8")
    for required in [
        f'imported_commit = "{TWEAKS_COMMIT}"',
        f'repo = "{TWEAKS_REPO}"',
        'destination_path = "src/desktop/cosmic/cosmic-tweaks"',
    ]:
        if required not in state:
            raise SystemExit(f"COSMIC Tweaks provenance state is missing {required!r}")


def finish_stage_graph() -> None:
    path = ROOT / "src/tools/mattos-build/src/stage_graph.rs"
    ensure_once(
        path,
        "    CosmicFiles,\n    CosmicTerm,\n    CosmicUtilities,",
        "    CosmicFiles,\n    CosmicTerm,\n    CosmicTweaks,\n    CosmicUtilities,",
        "BuildStage enum insertion",
    )
    ensure_once(
        path,
        '        BuildStage::CosmicTerm => "cosmic-term",\n        BuildStage::CosmicUtilities => "cosmic-utilities",',
        '        BuildStage::CosmicTerm => "cosmic-term",\n        BuildStage::CosmicTweaks => "cosmic-tweaks",\n        BuildStage::CosmicUtilities => "cosmic-utilities",',
        "stage-id insertion",
    )
    ensure_once(
        path,
        "        | BuildStage::CosmicFiles\n        | BuildStage::CosmicTerm\n        | BuildStage::CosmicUtilities",
        "        | BuildStage::CosmicFiles\n        | BuildStage::CosmicTerm\n        | BuildStage::CosmicTweaks\n        | BuildStage::CosmicUtilities",
        "COSMIC dependency-class insertion",
    )
    ensure_all(
        path,
        '            "cosmic-files",\n            "cosmic-term",\n            "cosmic-utilities",',
        '            "cosmic-files",\n            "cosmic-term",\n            "cosmic-tweaks",\n            "cosmic-utilities",',
        "aggregate COSMIC dependency insertion",
        2,
    )
    ensure_once(
        path,
        "        BuildStage::CosmicFiles,\n        BuildStage::CosmicTerm,\n        BuildStage::CosmicUtilities,",
        "        BuildStage::CosmicFiles,\n        BuildStage::CosmicTerm,\n        BuildStage::CosmicTweaks,\n        BuildStage::CosmicUtilities,",
        "all-build-stages insertion",
    )


def finish_stage_inputs() -> None:
    path = ROOT / "src/tools/mattos-build/src/stage_inputs.rs"
    ensure_once(
        path,
        '        BuildStage::CosmicTerm => &["src/desktop/cosmic/cosmic-term"],\n        BuildStage::CosmicUtilities => &[',
        '        BuildStage::CosmicTerm => &["src/desktop/cosmic/cosmic-term"],\n        BuildStage::CosmicTweaks => &["src/desktop/cosmic/cosmic-tweaks"],\n        BuildStage::CosmicUtilities => &[',
        "COSMIC Tweaks source input",
    )
    ensure_once(
        path,
        "        | BuildStage::CosmicFiles\n        | BuildStage::CosmicTerm\n        | BuildStage::CosmicUtilities",
        "        | BuildStage::CosmicFiles\n        | BuildStage::CosmicTerm\n        | BuildStage::CosmicTweaks\n        | BuildStage::CosmicUtilities",
        "COSMIC Tweaks tool family",
    )
    ensure_once(
        path,
        "            BuildStage::CosmicLauncher,\n            BuildStage::CosmicSettings,\n        ] {",
        "            BuildStage::CosmicLauncher,\n            BuildStage::CosmicSettings,\n            BuildStage::CosmicTweaks,\n        ] {",
        "COSMIC leaf-input regression coverage",
    )


def finish_main() -> None:
    path = ROOT / "src/tools/mattos-build/src/main.rs"

    # Build dispatch and scheduler resource classification are separate match
    # expressions. Do not infer multiplicity from a shared text fragment.
    ensure_once(
        path,
        "        | BuildStage::CosmicFiles\n        | BuildStage::CosmicTerm\n        | BuildStage::CosmicUtilities\n        | BuildStage::CosmicPortal\n        | BuildStage::CosmicAssets\n        | BuildStage::Greetd => build_cosmic_desktop_component(repo_root, stage),",
        "        | BuildStage::CosmicFiles\n        | BuildStage::CosmicTerm\n        | BuildStage::CosmicTweaks\n        | BuildStage::CosmicUtilities\n        | BuildStage::CosmicPortal\n        | BuildStage::CosmicAssets\n        | BuildStage::Greetd => build_cosmic_desktop_component(repo_root, stage),",
        "COSMIC build-dispatch insertion",
    )
    ensure_once(
        path,
        "            | BuildStage::CosmicFiles\n            | BuildStage::CosmicTerm\n            | BuildStage::CosmicUtilities\n            | BuildStage::CosmicPortal\n            | BuildStage::Greetd",
        "            | BuildStage::CosmicFiles\n            | BuildStage::CosmicTerm\n            | BuildStage::CosmicTweaks\n            | BuildStage::CosmicUtilities\n            | BuildStage::CosmicPortal\n            | BuildStage::Greetd",
        "COSMIC high-memory resource insertion",
    )
    ensure_once(
        path,
        '        BuildStage::CosmicTerm => {\n            vec!["out/build/cosmic-term/install/usr/bin/cosmic-term".into()]\n        }\n        BuildStage::CosmicUtilities =>',
        '        BuildStage::CosmicTerm => {\n            vec!["out/build/cosmic-term/install/usr/bin/cosmic-term".into()]\n        }\n        BuildStage::CosmicTweaks => {\n            vec!["out/build/cosmic-tweaks/install/usr/bin/cosmic-ext-tweaks".into()]\n        }\n        BuildStage::CosmicUtilities =>',
        "COSMIC Tweaks expected output",
    )
    ensure_once(
        path,
        '        BuildStage::CosmicFiles => Some("cosmic-files"),\n        BuildStage::CosmicTerm => Some("cosmic-term"),\n        _ => None,',
        '        BuildStage::CosmicFiles => Some("cosmic-files"),\n        BuildStage::CosmicTerm => Some("cosmic-term"),\n        BuildStage::CosmicTweaks => Some("cosmic-tweaks"),\n        _ => None,',
        "generic Just component mapping",
    )
    ensure_once(
        path,
        '        "cosmic-workspaces",\n        "cosmic-files",\n        "cosmic-term",\n        "cosmic-utilities",',
        '        "cosmic-workspaces",\n        "cosmic-files",\n        "cosmic-term",\n        "cosmic-tweaks",\n        "cosmic-utilities",',
        "aggregate COSMIC install list",
    )
    ensure_all(
        path,
        '        "usr/bin/cosmic-files",\n        "usr/bin/cosmic-term",\n        "usr/bin/greetd",',
        '        "usr/bin/cosmic-files",\n        "usr/bin/cosmic-term",\n        "usr/bin/cosmic-ext-tweaks",\n        "usr/bin/greetd",',
        "aggregate/rootfs COSMIC Tweaks validation",
        2,
    )
    ensure_once(
        path,
        '            ("cosmic-files", 120.000),\n            ("cosmic-term", 90.000),\n            ("cosmic-utilities", 120.000),',
        '            ("cosmic-files", 120.000),\n            ("cosmic-term", 90.000),\n            ("cosmic-tweaks", 90.000),\n            ("cosmic-utilities", 120.000),',
        "scheduler timing estimate",
    )


def finish_packaging() -> None:
    path = ROOT / "src/tools/mattos-build/src/packaging.rs"
    ensure_once(
        path,
        '        "usr/bin/cosmic-launcher",\n        "usr/bin/cosmic-term",\n        "usr/bin/greetd",',
        '        "usr/bin/cosmic-launcher",\n        "usr/bin/cosmic-term",\n        "usr/bin/cosmic-ext-tweaks",\n        "usr/bin/greetd",',
        "cosmic-desktop package payload validation",
    )
    ensure_once(
        path,
        '        // Revision 3 keeps the greeter daemon display-manager-scoped instead\n        // of enabling it in every multi-user/CLI boot. Revision 2 supplied the\n        // freedesktop hicolor fallback index.\n        "cosmic-desktop" => 3,',
        '        // Revision 4 requires COSMIC Tweaks in the aggregate desktop payload.\n        // Revision 3 keeps the greeter daemon display-manager-scoped instead\n        // of enabling it in every multi-user/CLI boot. Revision 2 supplied the\n        // freedesktop hicolor fallback index.\n        "cosmic-desktop" => 4,',
        "cosmic-desktop package recipe revision",
    )


def finish_provenance_audit() -> None:
    path = ROOT / "DevUtils/test_vendored_source_provenance.py"
    ensure_once(
        path,
        "    expected_component_count = 63",
        "    expected_component_count = 64",
        "vendored component count",
    )
    ensure_once(
        path,
        '    print(f"components verified: {verified}/47")',
        '    print(f"components verified: {verified}/{len(component_list)}")',
        "dynamic provenance audit denominator",
    )


def finish_ownership_test() -> None:
    path = ROOT / "DevUtils/test_source_ownership_overrides.py"
    text = path.read_text(encoding="utf-8")
    method = "    def test_cosmic_tweaks_is_first_class_source_owned(self) -> None:\n"
    if method in text:
        return
    marker = "    def test_registry_resolution_can_use_first_class_root(self) -> None:\n"
    if text.count(marker) != 1:
        raise SystemExit("ownership test insertion marker is not unique")
    test = '''    def test_cosmic_tweaks_is_first_class_source_owned(self) -> None:\n        component = self.index["components"].get("cosmic-tweaks")\n        self.assertIsNotNone(component)\n        assert component is not None\n        self.assertEqual(component["repo"], "https://github.com/cosmic-utils/tweaks.git")\n        self.assertEqual(component["revision"], "069c31b7b1beffddf744b28f8f056ace972830bc")\n        self.assertEqual(component["packages"].get("cosmic-ext-tweaks"), "")\n\n        for package, repo, expected in [\n            ("libcosmic", "https://github.com/pop-os/libcosmic.git", "libcosmic"),\n            ("cosmic-panel-config", "https://github.com/pop-os/cosmic-panel", "cosmic-panel"),\n            (\n                "cosmic-settings-config",\n                "https://github.com/pop-os/cosmic-settings-daemon",\n                "cosmic-settings-daemon",\n            ),\n        ]:\n            target = graph.choose_owned_git_target(self.index, package, repo)\n            self.assertIsNotNone(target)\n            assert target is not None\n            self.assertEqual(target["component"], expected)\n\n'''
    path.write_text(text.replace(marker, test + marker, 1), encoding="utf-8")


def stage_import_for_catalog() -> None:
    # The ownership catalog enumerates Git-tracked Cargo.toml files. Stage the
    # imported transaction before generation; this is still not a commit.
    run(
        "git",
        "add",
        "-A",
        "--",
        "upstream/sources.toml",
        "upstream/state/cosmic-tweaks.toml",
        "src/desktop/cosmic/cosmic-tweaks",
    )


def validate_and_publish(app) -> None:
    stage_import_for_catalog()
    app.static_validation()
    run("python3", "DevUtils/test_vendored_source_provenance.py")

    index = json.loads((ROOT / "out/source-ownership/cargo/index.json").read_text())
    component = index.get("components", {}).get("cosmic-tweaks")
    if not component or component.get("revision") != TWEAKS_COMMIT:
        raise SystemExit("COSMIC Tweaks is absent or incorrectly pinned in ownership catalog")
    if component.get("packages", {}).get("cosmic-ext-tweaks") != "":
        raise SystemExit("cosmic-ext-tweaks is not owned by the COSMIC Tweaks source root")

    # Remove every bootstrap helper from the real integration commit. This
    # process keeps executing the already-loaded Python code after unlinking.
    for helper in HELPERS:
        if helper.exists():
            helper.unlink()

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
        "DevUtils/apply_vendor_cosmic_tweaks.py",
        "DevUtils/run_vendor_cosmic_tweaks.py",
        "DevUtils/resume_vendor_cosmic_tweaks.py",
    )
    run("git", "diff", "--cached", "--check")
    run("git", "commit", "-m", "Vendor COSMIC Tweaks")
    run("git", "push", "origin", f"HEAD:{BRANCH}")

    print("COSMIC Tweaks integration committed and pushed; running targeted validation.")
    app.targeted_validation()
    print("COSMIC Tweaks is vendored, source-owned, built, aggregated, and package-validated.")


def main() -> None:
    if not (ROOT / "Cargo.toml").is_file() or not (ROOT / "upstream/sources.toml").is_file():
        raise SystemExit("run this from the MattOS repository root")
    verify_partial_state()
    app = load_applicator()
    finish_stage_graph()
    finish_stage_inputs()
    finish_main()
    finish_packaging()
    finish_provenance_audit()
    finish_ownership_test()
    validate_and_publish(app)


if __name__ == "__main__":
    main()
