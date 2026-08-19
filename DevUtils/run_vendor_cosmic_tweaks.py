#!/usr/bin/env python3
from __future__ import annotations

import subprocess
from pathlib import Path

BRANCH = "agent/vendor-cosmic-tweaks"
ROOT = Path(__file__).resolve().parents[1]
RESUME = ROOT / "DevUtils/resume_vendor_cosmic_tweaks.py"


def output(*args: str) -> str:
    return subprocess.check_output(args, cwd=ROOT, text=True).strip()


def ensure_once(path: Path, old: str, new: str, label: str) -> None:
    text = path.read_text(encoding="utf-8")
    old_count = text.count(old)
    new_count = text.count(new)
    if old_count == 0 and new_count == 1:
        return
    if old_count == 1 and new_count == 0:
        path.write_text(text.replace(old, new, 1), encoding="utf-8")
        return
    raise SystemExit(
        f"{path.relative_to(ROOT)}: unexpected {label} state: "
        f"pending={old_count}, applied={new_count}"
    )


def patch_stage_graph_expectations() -> None:
    path = ROOT / "src/tools/mattos-build/src/stage_graph.rs"

    # These are two different semantic test locations, not duplicate text.
    # Keep them separate so this helper cannot accidentally modify a production
    # dependency list merely because it shares the same three component names.
    ensure_once(
        path,
        '                "cosmic-workspaces",\n                "cosmic-files",\n                "cosmic-term",\n                "cosmic-utilities",\n                "cosmic-portal",\n                "greetd",\n                "cosmic-desktop",',
        '                "cosmic-workspaces",\n                "cosmic-files",\n                "cosmic-term",\n                "cosmic-tweaks",\n                "cosmic-utilities",\n                "cosmic-portal",\n                "greetd",\n                "cosmic-desktop",',
        "LLVM exact downstream closure",
    )
    ensure_once(
        path,
        '            "cosmic-workspaces",\n            "cosmic-files",\n            "cosmic-term",\n            "cosmic-utilities",\n            "cosmic-portal",\n            "cosmic-assets",\n            "greetd",',
        '            "cosmic-workspaces",\n            "cosmic-files",\n            "cosmic-term",\n            "cosmic-tweaks",\n            "cosmic-utilities",\n            "cosmic-portal",\n            "cosmic-assets",\n            "greetd",',
        "per-COSMIC-leaf isolation coverage",
    )

    # Broad upstream changes that already reached the full COSMIC leaf family
    # now invalidate exactly one additional stage: cosmic-tweaks. Narrow Linux,
    # package, repository, rootfs and initramfs scenarios are intentionally
    # unchanged.
    for old, new, label in [
        (
            '("glibc source", &["glibc"], 101, &["linux"]),',
            '("glibc source", &["glibc"], 102, &["linux"]),',
            "glibc cascade count",
        ),
        (
            '                102,\n                &[],\n            ),',
            '                103,\n                &[],\n            ),',
            "Linux UAPI cascade count",
        ),
        (
            '                99,\n                &["linux", "glibc", "linux-headers"],\n            ),',
            '                100,\n                &["linux", "glibc", "linux-headers"],\n            ),',
            "GCC cascade count",
        ),
        (
            '("zlib shared library", &["zlib"], 49, &["brush", "linux"]),',
            '("zlib shared library", &["zlib"], 50, &["brush", "linux"]),',
            "zlib cascade count",
        ),
    ]:
        ensure_once(path, old, new, label)


def patch_main_expectations() -> None:
    path = ROOT / "src/tools/mattos-build/src/main.rs"
    ensure_once(
        path,
        "                    | BuildStage::CosmicFiles\n                    | BuildStage::CosmicTerm\n                    | BuildStage::CosmicUtilities\n                    | BuildStage::CosmicPortal",
        "                    | BuildStage::CosmicFiles\n                    | BuildStage::CosmicTerm\n                    | BuildStage::CosmicTweaks\n                    | BuildStage::CosmicUtilities\n                    | BuildStage::CosmicPortal",
        "high-memory scheduler regression expectation",
    )


def main() -> None:
    if output("git", "branch", "--show-current") != BRANCH:
        raise SystemExit(f"expected branch {BRANCH!r}")
    if not RESUME.is_file():
        raise SystemExit(f"missing recovery helper {RESUME}")

    patch_stage_graph_expectations()
    patch_main_expectations()

    # Continue through the existing idempotent integration helper. On success
    # that helper deletes this bootstrap script, the applicator, and itself from
    # the real integration commit, so no recovery machinery survives in the PR.
    subprocess.run(
        ["python3", str(RESUME.relative_to(ROOT))],
        cwd=ROOT,
        check=True,
    )


if __name__ == "__main__":
    main()
