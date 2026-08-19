#!/usr/bin/env python3
from __future__ import annotations

import importlib.util
import json
import subprocess
import sys
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


def verify_partial_state() -> None:
    branch = output("git", "branch", "--show-current")
    if branch != BRANCH:
        raise SystemExit(f"expected branch {BRANCH!r}, got {branch!r}")

    allowed = (
        "upstream/sources.toml",
        "upstream/state/cosmic-tweaks.toml",
        "src/desktop/cosmic/cosmic-tweaks/",
        "src/tools/mattos-build/src/stage_graph.rs",
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
    text = path.read_text(encoding="utf-8")

    # The failed first pass completed these three edits before discovering that
    # the aggregate dependency sequence intentionally appears twice.
    required_done = [
        "    CosmicTerm,\n    CosmicTweaks,\n    CosmicUtilities,",
        '        BuildStage::CosmicTweaks => "cosmic-tweaks",',
        "        | BuildStage::CosmicTerm\n        | BuildStage::CosmicTweaks\n        | BuildStage::CosmicUtilities",
    ]
    for marker in required_done:
        if marker not in text:
            raise SystemExit(
                "stage_graph.rs is not in the expected resumable partial state; missing marker:\n"
                + marker
            )

    old = '            "cosmic-files",\n            "cosmic-term",\n            "cosmic-utilities",'
    new = '            "cosmic-files",\n            "cosmic-term",\n            "cosmic-tweaks",\n            "cosmic-utilities",'
    old_count = text.count(old)
    new_count = text.count(new)
    if old_count:
        # Both occurrences are deliberate graph descriptions and must agree.
        text = text.replace(old, new)
    elif new_count < 2:
        raise SystemExit(
            f"unexpected aggregate COSMIC dependency state: old={old_count}, new={new_count}"
        )

    old = "        BuildStage::CosmicFiles,\n        BuildStage::CosmicTerm,\n        BuildStage::CosmicUtilities,"
    new = "        BuildStage::CosmicFiles,\n        BuildStage::CosmicTerm,\n        BuildStage::CosmicTweaks,\n        BuildStage::CosmicUtilities,"
    if old in text:
        if text.count(old) != 1:
            raise SystemExit("unexpected all-build-stages COSMIC sequence multiplicity")
        text = text.replace(old, new, 1)
    elif new not in text:
        raise SystemExit("all-build-stages COSMIC Tweaks insertion is neither pending nor applied")

    path.write_text(text, encoding="utf-8")


def stage_import_for_catalog() -> None:
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
    # script remains executable in memory after unlinking on Unix.
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
    app.patch_stage_inputs()
    app.patch_main()
    app.patch_packaging()
    app.patch_provenance_audit()
    app.patch_ownership_tests()
    validate_and_publish(app)


if __name__ == "__main__":
    main()
