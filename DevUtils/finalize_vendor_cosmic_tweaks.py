#!/usr/bin/env python3
from __future__ import annotations

import subprocess
from pathlib import Path

BRANCH = "agent/vendor-cosmic-tweaks"
ROOT = Path(__file__).resolve().parents[1]
AUDIT = ROOT / "DevUtils/test_vendored_source_provenance.py"
RUNNER = ROOT / "DevUtils/run_vendor_cosmic_tweaks.py"
RESUME = ROOT / "DevUtils/resume_vendor_cosmic_tweaks.py"

FONT_STATES = {
    "upstream/state/noto-sans-mono.toml": "1dcdbe5e9a998c89c6156634d12750eef561c4454b12c822eae7d5281a8495a5",
    "upstream/state/open-sans.toml": "31e3dd04d66e526a01d7ef4f01ef84c6838515cc367e332a8133cd8bb8445717",
}
SELECTED_ALGORITHM = "sha256-selected-git-ls-tree-no-gitlinks-v1"
TEMP_HELPERS = (
    "DevUtils/apply_vendor_cosmic_tweaks.py",
    "DevUtils/run_vendor_cosmic_tweaks.py",
    "DevUtils/resume_vendor_cosmic_tweaks.py",
    "DevUtils/repair_runtime_font_provenance.py",
    "DevUtils/continue_vendor_cosmic_tweaks.py",
    "DevUtils/finalize_vendor_cosmic_tweaks.py",
)


def output(*args: str) -> str:
    return subprocess.check_output(args, cwd=ROOT, text=True).strip()


def verify_checkpoint() -> None:
    if output("git", "branch", "--show-current") != BRANCH:
        raise SystemExit(f"expected branch {BRANCH!r}")

    audit = AUDIT.read_text(encoding="utf-8")
    required_audit = (
        "def load_intentional_omission_policy(",
        "def apply_intentional_omission_policy(",
        "if len(components) != len(component_list):",
    )
    for marker in required_audit:
        if marker not in audit:
            raise SystemExit(f"runtime-font provenance checkpoint is incomplete: missing {marker!r}")
    if "expected_component_count" in audit:
        raise SystemExit("runtime-font provenance checkpoint still contains a fixed component count")

    for relative, digest in FONT_STATES.items():
        body = (ROOT / relative).read_text(encoding="utf-8")
        if f'imported_tree_digest = "{digest}"' not in body:
            raise SystemExit(f"{relative}: canonical projected digest checkpoint is missing")
        if f'imported_tree_digest_algorithm = "{SELECTED_ALGORITHM}"' not in body:
            raise SystemExit(f"{relative}: generic selected-tree digest algorithm is missing")


def block_bounds(text: str, start_marker: str, end_marker: str, label: str) -> tuple[int, int]:
    start = text.find(start_marker)
    if start < 0:
        raise SystemExit(f"resume helper is missing {label} start marker")
    end = text.find(end_marker, start + len(start_marker))
    if end < 0:
        raise SystemExit(f"resume helper is missing {label} end marker")
    return start, end


def ensure_lines_in_block(
    text: str,
    start_marker: str,
    end_marker: str,
    anchor: str,
    entries: tuple[str, ...],
    label: str,
) -> str:
    start, end = block_bounds(text, start_marker, end_marker, label)
    block = text[start:end]
    missing = [entry for entry in entries if entry not in block]
    if not missing:
        return text
    if block.count(anchor) != 1:
        raise SystemExit(
            f"resume helper {label} anchor multiplicity is {block.count(anchor)}, expected 1"
        )
    insertion = anchor + "".join(missing)
    block = block.replace(anchor, insertion, 1)
    return text[:start] + block + text[end:]


def patch_resume_helper() -> None:
    text = RESUME.read_text(encoding="utf-8")

    # Extend the deletion set in the HELPERS declaration only. Do not infer
    # anything from identical path strings in the later git-add argv.
    helper_entries = tuple(f'    ROOT / "{path}",\n' for path in TEMP_HELPERS[3:])
    text = ensure_lines_in_block(
        text,
        "HELPERS = [\n",
        "]\n\n\ndef output",
        '    ROOT / "DevUtils/resume_vendor_cosmic_tweaks.py",\n',
        helper_entries,
        "HELPERS declaration",
    )

    # Permit exactly the known continuation-state modifications. Scope this to
    # verify_partial_state's allowed tuple so repeated path strings elsewhere
    # cannot confuse recovery logic.
    allowed_entries = (
        '        "upstream/state/noto-sans-mono.toml",\n',
        '        "upstream/state/open-sans.toml",\n',
        '        "DevUtils/resume_vendor_cosmic_tweaks.py",\n',
    )
    text = ensure_lines_in_block(
        text,
        "    allowed = (\n",
        "    )\n    status = subprocess.check_output(",
        '        "upstream/state/cosmic-tweaks.toml",\n',
        allowed_entries,
        "verify_partial_state allowed tuple",
    )

    # The fixed inventory-size rewrite was a bootstrap mistake. Keep only the
    # dynamic report denominator. This handles either the old or already-fixed
    # local resume helper without reintroducing a numeric count.
    old_function = '''def finish_provenance_audit() -> None:\n    path = ROOT / "DevUtils/test_vendored_source_provenance.py"\n    ensure_once(\n        path,\n        "    expected_component_count = 63",\n        "    expected_component_count = 64",\n        "vendored component count",\n    )\n    ensure_once(\n        path,\n        '    print(f"components verified: {verified}/47")',\n        '    print(f"components verified: {verified}/{len(component_list)}")',\n        "dynamic provenance audit denominator",\n    )\n'''
    new_function = '''def finish_provenance_audit() -> None:\n    path = ROOT / "DevUtils/test_vendored_source_provenance.py"\n    text = path.read_text(encoding="utf-8")\n    old = '    print(f"components verified: {verified}/47")'\n    new = '    print(f"components verified: {verified}/{len(component_list)}")'\n    if old in text:\n        if text.count(old) != 1:\n            raise SystemExit("provenance audit denominator marker is not unique")\n        path.write_text(text.replace(old, new, 1), encoding="utf-8")\n    elif new not in text:\n        raise SystemExit("provenance audit dynamic denominator is missing")\n'''
    if old_function in text:
        text = text.replace(old_function, new_function, 1)
    elif new_function not in text:
        raise SystemExit("resume helper provenance function is in an unexpected state")

    # Add font-state migrations and every temporary-helper deletion to the final
    # integration commit's explicit staging argv. Scope this to validate_and_publish.
    start, end = block_bounds(
        text,
        "def validate_and_publish(app) -> None:\n",
        "\n\ndef main() -> None:\n",
        "validate_and_publish",
    )
    block = text[start:end]
    add_start = block.find('    run(\n        "git",\n        "add",\n        "-A",\n        "--",\n')
    if add_start < 0:
        raise SystemExit("resume helper final git-add block is missing")
    add_end = block.find('    run("git", "diff", "--cached", "--check")', add_start)
    if add_end < 0:
        raise SystemExit("resume helper final git-add block end is missing")
    add_block = block[add_start:add_end]

    final_entries = (
        '        "upstream/state/noto-sans-mono.toml",\n',
        '        "upstream/state/open-sans.toml",\n',
        '        "DevUtils/repair_runtime_font_provenance.py",\n',
        '        "DevUtils/continue_vendor_cosmic_tweaks.py",\n',
        '        "DevUtils/finalize_vendor_cosmic_tweaks.py",\n',
    )
    missing = [entry for entry in final_entries if entry not in add_block]
    if missing:
        anchor = '        "upstream/state/cosmic-tweaks.toml",\n'
        if add_block.count(anchor) != 1:
            raise SystemExit("resume helper final git-add state anchor is not unique")
        # Put state records next to state records; helper deletion paths may also
        # live here because git add -A accepts deleted tracked paths.
        add_block = add_block.replace(anchor, anchor + "".join(missing), 1)
        block = block[:add_start] + add_block + block[add_end:]
        text = text[:start] + block + text[end:]

    RESUME.write_text(text, encoding="utf-8")


def main() -> None:
    verify_checkpoint()
    patch_resume_helper()

    # Reuse the existing runner: it removes proven cargo-fmt collateral, fixes
    # stage-graph regression expectations idempotently, and invokes the resume
    # helper. On successful publication, resume deletes every bootstrap helper,
    # including this file, from the real integration commit.
    subprocess.run(
        ["python3", str(RUNNER.relative_to(ROOT))],
        cwd=ROOT,
        check=True,
    )


if __name__ == "__main__":
    main()
