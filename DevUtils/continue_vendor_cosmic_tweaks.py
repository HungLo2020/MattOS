#!/usr/bin/env python3
from __future__ import annotations

import re
import subprocess
from pathlib import Path

BRANCH = "agent/vendor-cosmic-tweaks"
ROOT = Path(__file__).resolve().parents[1]
AUDIT = ROOT / "DevUtils/test_vendored_source_provenance.py"
REPAIR = ROOT / "DevUtils/repair_runtime_font_provenance.py"
RUNNER = ROOT / "DevUtils/run_vendor_cosmic_tweaks.py"
RESUME = ROOT / "DevUtils/resume_vendor_cosmic_tweaks.py"
SELF_REL = "DevUtils/continue_vendor_cosmic_tweaks.py"
REPAIR_REL = "DevUtils/repair_runtime_font_provenance.py"
RESUME_REL = "DevUtils/resume_vendor_cosmic_tweaks.py"


def output(*args: str) -> str:
    return subprocess.check_output(args, cwd=ROOT, text=True).strip()


def remove_brittle_component_count() -> None:
    """Keep uniqueness as the invariant; never hard-code inventory size."""
    text = AUDIT.read_text(encoding="utf-8")
    replacement = '''    if len(components) != len(component_list):\n        failures.append(\n            f"sources.toml declares {len(component_list)} component entries but only "\n            f"{len(components)} unique component names"\n        )\n'''
    if replacement in text:
        return

    pattern = re.compile(
        r'''    expected_component_count = \d+\n'''
        r'''    if len\(components\) != expected_component_count or len\(component_list\) != expected_component_count:\n'''
        r'''        failures\.append\(\n'''
        r'''            f"sources\.toml declares \{len\(component_list\)\} components, "\n'''
        r'''            f"expected \{expected_component_count\} unique components"\n'''
        r'''        \)\n'''
    )
    text, count = pattern.subn(replacement, text, count=1)
    if count != 1:
        raise SystemExit(
            "provenance audit does not contain the expected stale component-count invariant"
        )
    AUDIT.write_text(text, encoding="utf-8")


def remove_resume_count_rewrite() -> None:
    """Stop the temporary Tweaks helper from reintroducing a fixed count."""
    text = RESUME.read_text(encoding="utf-8")
    old = '''def finish_provenance_audit() -> None:\n    path = ROOT / "DevUtils/test_vendored_source_provenance.py"\n    ensure_once(\n        path,\n        "    expected_component_count = 63",\n        "    expected_component_count = 64",\n        "vendored component count",\n    )\n    ensure_once(\n        path,\n        '    print(f"components verified: {verified}/47")',\n        '    print(f"components verified: {verified}/{len(component_list)}")',\n        "dynamic provenance audit denominator",\n    )\n'''
    new = '''def finish_provenance_audit() -> None:\n    path = ROOT / "DevUtils/test_vendored_source_provenance.py"\n    text = path.read_text(encoding="utf-8")\n    old = '    print(f"components verified: {verified}/47")'\n    new = '    print(f"components verified: {verified}/{len(component_list)}")'\n    if old in text:\n        if text.count(old) != 1:\n            raise SystemExit("provenance audit denominator marker is not unique")\n        path.write_text(text.replace(old, new, 1), encoding="utf-8")\n    elif new not in text:\n        raise SystemExit("provenance audit dynamic denominator is missing")\n'''
    if new in text:
        return
    if old not in text:
        raise SystemExit("resume helper provenance function is not in the expected state")
    RESUME.write_text(text.replace(old, new, 1), encoding="utf-8")


def patch_resume_bootstrap_contract() -> None:
    text = RESUME.read_text(encoding="utf-8")

    allowed_anchor = '        "DevUtils/test_source_ownership_overrides.py",\n'
    allowed_entries = '        "DevUtils/resume_vendor_cosmic_tweaks.py",\n'
    if allowed_entries not in text:
        if text.count(allowed_anchor) != 1:
            raise SystemExit("resume helper allowlist anchor is not unique")
        text = text.replace(allowed_anchor, allowed_anchor + allowed_entries, 1)

    helper_anchor = '    ROOT / "DevUtils/repair_runtime_font_provenance.py",\n'
    continuation_helper = '    ROOT / "DevUtils/continue_vendor_cosmic_tweaks.py",\n'
    if continuation_helper not in text:
        if text.count(helper_anchor) != 1:
            raise SystemExit("resume helper repair-helper anchor is not unique")
        text = text.replace(helper_anchor, helper_anchor + continuation_helper, 1)

    stage_anchor = '        "DevUtils/resume_vendor_cosmic_tweaks.py",\n'
    stage_entries = (
        '        "DevUtils/repair_runtime_font_provenance.py",\n'
        '        "DevUtils/continue_vendor_cosmic_tweaks.py",\n'
    )
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
    for required in (AUDIT, REPAIR, RUNNER, RESUME):
        if not required.is_file():
            raise SystemExit(f"missing bootstrap input {required.relative_to(ROOT)}")

    remove_brittle_component_count()
    remove_resume_count_rewrite()

    subprocess.run(["python3", str(REPAIR.relative_to(ROOT))], cwd=ROOT, check=True)
    patch_resume_bootstrap_contract()
    subprocess.run(["python3", str(RUNNER.relative_to(ROOT))], cwd=ROOT, check=True)


if __name__ == "__main__":
    main()
