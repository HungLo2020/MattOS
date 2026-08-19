#!/usr/bin/env python3
from __future__ import annotations

import importlib.util
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SELF = Path(__file__).resolve()
APPLICATOR = ROOT / "DevUtils/apply_vendor_cosmic_tweaks.py"


def load_applicator():
    spec = importlib.util.spec_from_file_location("apply_vendor_cosmic_tweaks", APPLICATOR)
    if spec is None or spec.loader is None:
        raise SystemExit(f"unable to load {APPLICATOR}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def main() -> None:
    app = load_applicator()
    app.require_clean_branch()
    app.register_source()
    app.import_source()
    app.patch_stage_graph()
    app.patch_stage_inputs()
    app.patch_main()
    app.patch_packaging()
    app.patch_provenance_audit()
    app.patch_ownership_tests()

    # The ownership catalog intentionally enumerates Git-tracked Cargo
    # manifests. Stage only the newly imported source transaction before the
    # checks so cosmic-ext-tweaks is visible to catalog generation. Nothing is
    # committed until the full static test suite succeeds.
    subprocess.run(
        [
            "git",
            "add",
            "-A",
            "--",
            "upstream/sources.toml",
            "upstream/state/cosmic-tweaks.toml",
            "src/desktop/cosmic/cosmic-tweaks",
        ],
        cwd=ROOT,
        check=True,
    )

    app.static_validation()

    # Both temporary applicators disappear in the real integration commit.
    SELF.unlink()
    subprocess.run(
        ["git", "add", "-A", "--", str(SELF.relative_to(ROOT))],
        cwd=ROOT,
        check=True,
    )
    app.commit_and_push()
    print("COSMIC Tweaks source/integration committed and pushed; running targeted build validation.")
    app.targeted_validation()
    print("COSMIC Tweaks is vendored, source-owned, built, aggregated, and package-validated on PR branch.")


if __name__ == "__main__":
    main()
