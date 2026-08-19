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


def ensure_block_entry(
    path: Path,
    start_marker: str,
    end_marker: str,
    anchor: str,
    entry: str,
    label: str,
) -> None:
    """Ensure one entry inside one semantically delimited block.

    This intentionally ignores identical text elsewhere in the file. Recovery
    helpers must not infer semantic multiplicity from shared list fragments.
    """
    text = path.read_text(encoding="utf-8")
    start = text.find(start_marker)
    if start < 0:
        raise SystemExit(f"{path.relative_to(ROOT)}: missing {label} start marker")
    end = text.find(end_marker, start + len(start_marker))
    if end < 0:
        raise SystemExit(f"{path.relative_to(ROOT)}: missing {label} end marker")

    block = text[start:end]
    if entry in block:
        return
    if block.count(anchor) != 1:
        raise SystemExit(
            f"{path.relative_to(ROOT)}: unexpected {label} anchor multiplicity: "
            f"{block.count(anchor)}"
        )

    block = block.replace(anchor, anchor + entry, 1)
    path.write_text(text[:start] + block + text[end:], encoding="utf-8")


def patch_stage_graph_expectations() -> None:
    path = ROOT / "src/tools/mattos-build/src/stage_graph.rs"

    # Address the two tests by semantic block, not by global text counts. The
    # previous recovery pass successfully edited the LLVM block before its next
    # global-count check tripped over the now-identical fragments.
    ensure_block_entry(
        path,
        '            downstream_invalidation(&["llvm"]),',
        "        for component in [",
        '                "cosmic-term",\n',
        '                "cosmic-tweaks",\n',
        "LLVM exact downstream closure",
    )
    ensure_block_entry(
        path,
        "        for component in [",
        "        ] {",
        '            "cosmic-term",\n',
        '            "cosmic-tweaks",\n',
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
            '                "Linux x86_64 UAPI source",\n                &["linux", "glibc", "linux-headers"],\n                102,\n                &[],',
            '                "Linux x86_64 UAPI source",\n                &["linux", "glibc", "linux-headers"],\n                103,\n                &[],',
            "Linux UAPI cascade count",
        ),
        (
            '                "GCC source",\n                &["gcc-runtime", "gcc-compiler"],\n                99,\n                &["linux", "glibc", "linux-headers"],',
            '                "GCC source",\n                &["gcc-runtime", "gcc-compiler"],\n                100,\n                &["linux", "glibc", "linux-headers"],',
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
