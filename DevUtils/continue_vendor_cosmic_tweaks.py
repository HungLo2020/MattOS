#!/usr/bin/env python3
from __future__ import annotations

import subprocess
from pathlib import Path

BRANCH = "agent/vendor-cosmic-tweaks"
ROOT = Path(__file__).resolve().parents[1]
REPAIR = ROOT / "DevUtils/repair_runtime_font_provenance.py"
RUNNER = ROOT / "DevUtils/run_vendor_cosmic_tweaks.py"
RESUME = ROOT / "DevUtils/resume_vendor_cosmic_tweaks.py"
SELF_REL = "DevUtils/continue_vendor_cosmic_tweaks.py"
REPAIR_REL = "DevUtils/repair_runtime_font_provenance.py"
RESUME_REL = "DevUtils/resume_vendor_cosmic_tweaks.py"


def output(*args: str) -> str:
    return subprocess.check_output(args, cwd=ROOT, text=True).strip()


def patch_resume_bootstrap_contract() -> None:
    text = RESUME.read_text(encoding="utf-8")

    # The runtime-font repair edits this temporary resume helper itself. Permit
    # that bootstrap-only dirty path without broadening the real integration
    # file allowlist.
    allowed_anchor = '        "DevUtils/test_source_ownership_overrides.py",\n'
    allowed_entries = (
        '        "DevUtils/resume_vendor_cosmic_tweaks.py",\n'
    )
    if '        "DevUtils/resume_vendor_cosmic_tweaks.py",\n' not in text:
        if text.count(allowed_anchor) != 1:
            raise SystemExit("resume helper allowlist anchor is not unique")
        text = text.replace(allowed_anchor, allowed_anchor + allowed_entries, 1)

    # All temporary bootstrap helpers must disappear from the real integration
    # commit. Add this continuation helper to the deletion set.
    helper_anchor = '    ROOT / "DevUtils/repair_runtime_font_provenance.py",\n'
    if '    ROOT / "DevUtils/continue_vendor_cosmic_tweaks.py",\n' not in text:
        if text.count(helper_anchor) != 1:
            raise SystemExit("resume helper repair-helper anchor is not unique")
        text = text.replace(
            helper_anchor,
            helper_anchor + '    ROOT / "DevUtils/continue_vendor_cosmic_tweaks.py",\n',
            1,
        )

    # Deleting a tracked helper is not enough; explicitly stage both new helper
    # deletions alongside the three older bootstrap files.
    stage_anchor = '        "DevUtils/resume_vendor_cosmic_tweaks.py",\n'
    stage_entries = (
        '        "DevUtils/repair_runtime_font_provenance.py",\n'
        '        "DevUtils/continue_vendor_cosmic_tweaks.py",\n'
    )
    # The resume path occurs in HELPERS and in the final git-add argv. Target
    # the final occurrence only.
    if '        "DevUtils/repair_runtime_font_provenance.py",\n        "DevUtils/continue_vendor_cosmic_tweaks.py",\n' not in text:
        index = text.rfind(stage_anchor)
        if index < 0:
            raise SystemExit("resume helper final helper-staging anchor is missing")
        end = index + len(stage_anchor)
        text = text[:end] + stage_entries + text[end:]

    RESUME.write_text(text, encoding="utf-8")


def main() -> None:
    if output("git", "branch", "--show-current") != BRANCH:
        raise SystemExit(f"expected branch {BRANCH!r}")
    for required in (REPAIR, RUNNER, RESUME):
        if not required.is_file():
            raise SystemExit(f"missing bootstrap helper {required.relative_to(ROOT)}")

    subprocess.run(["python3", str(REPAIR.relative_to(ROOT))], cwd=ROOT, check=True)
    patch_resume_bootstrap_contract()

    # The existing runner retains the proven cargo-fmt cleanup and semantic
    # stage-graph regression repair, then invokes the now-updated resume helper.
    subprocess.run(["python3", str(RUNNER.relative_to(ROOT))], cwd=ROOT, check=True)


if __name__ == "__main__":
    main()
