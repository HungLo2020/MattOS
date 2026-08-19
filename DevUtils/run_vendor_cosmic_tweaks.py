#!/usr/bin/env python3
from __future__ import annotations

import subprocess
import tempfile
from pathlib import Path

BRANCH = "agent/vendor-cosmic-tweaks"
ROOT = Path(__file__).resolve().parents[1]
RESUME = ROOT / "DevUtils/resume_vendor_cosmic_tweaks.py"
FORMATTER_COLLATERAL = (
    "src/tools/mattos-build/build.rs",
    "src/tools/mattos-build/src/scheduler.rs",
)


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


def tracked_status() -> dict[str, str]:
    raw = subprocess.check_output(
        ["git", "status", "--porcelain=v1", "--untracked-files=no"],
        cwd=ROOT,
        text=True,
    )
    result: dict[str, str] = {}
    for line in raw.splitlines():
        if len(line) < 4:
            continue
        path = line[3:]
        if " -> " in path:
            path = path.split(" -> ", 1)[1]
        result[path] = line[:2]
    return result


def cleanup_proven_formatter_collateral() -> None:
    """Remove only formatter output that can be reproduced from clean HEAD.

    Earlier recovery runs used `cargo fmt -p mattos-build`, which formats every
    Rust file in that package and dirtied two files unrelated to COSMIC Tweaks.
    Never whitelist or blindly restore them: reproduce formatting in a detached
    HEAD worktree and require byte-for-byte equality first.
    """
    status = tracked_status()
    dirty = [path for path in FORMATTER_COLLATERAL if path in status]
    if not dirty:
        return

    for path in dirty:
        if status[path] != " M":
            raise SystemExit(
                f"refusing to clean {path}: expected an unstaged formatter-only "
                f"modification, got status {status[path]!r}"
            )

    worktree_path: Path | None = None
    with tempfile.TemporaryDirectory(prefix="mattos-tweaks-fmt-proof-") as temp:
        worktree_path = Path(temp) / "head"
        subprocess.run(
            ["git", "worktree", "add", "--detach", str(worktree_path), "HEAD"],
            cwd=ROOT,
            check=True,
            stdout=subprocess.DEVNULL,
        )
        try:
            subprocess.run(
                ["cargo", "fmt", "-p", "mattos-build"],
                cwd=worktree_path,
                check=True,
            )
            for path in dirty:
                local_bytes = (ROOT / path).read_bytes()
                proof_bytes = (worktree_path / path).read_bytes()
                if local_bytes != proof_bytes:
                    raise SystemExit(
                        f"refusing to restore {path}: local bytes are not exactly "
                        "the formatter output reproduced from clean HEAD"
                    )
        finally:
            subprocess.run(
                ["git", "worktree", "remove", "--force", str(worktree_path)],
                cwd=ROOT,
                check=True,
                stdout=subprocess.DEVNULL,
            )

    subprocess.run(
        ["git", "restore", "--source=HEAD", "--worktree", "--", *dirty],
        cwd=ROOT,
        check=True,
    )
    print(
        "Removed proven cargo-fmt collateral: " + ", ".join(dirty),
        flush=True,
    )


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

    cleanup_proven_formatter_collateral()
    patch_stage_graph_expectations()
    patch_main_expectations()

    # Continue through the existing idempotent integration helper. On success
    # that helper deletes this bootstrap script, the applicator, and itself from
    # the real integration commit, so no recovery machinery survives in the PR.
    # Always remove formatter collateral created by its validation step, even if
    # a later test/build fails.
    try:
        subprocess.run(
            ["python3", str(RESUME.relative_to(ROOT))],
            cwd=ROOT,
            check=True,
        )
    finally:
        cleanup_proven_formatter_collateral()


if __name__ == "__main__":
    main()
