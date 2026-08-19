#!/usr/bin/env python3
from __future__ import annotations

import re
import subprocess
from pathlib import Path

BRANCH = "agent/vendor-cosmic-tweaks"
ROOT = Path(__file__).resolve().parents[1]
AUDIT = ROOT / "DevUtils/test_vendored_source_provenance.py"
RESUME = ROOT / "DevUtils/resume_vendor_cosmic_tweaks.py"
SELF_REL = "DevUtils/repair_runtime_font_provenance.py"
GENERIC_SELECTED_ALGORITHM = "sha256-selected-git-ls-tree-no-gitlinks-v1"
LEGACY_SELECTED_ALGORITHM = "sha256-selected-runtime-fonts-v1"
FONT_COMPONENTS = ("noto-sans-mono", "open-sans")


def output(*args: str) -> str:
    return subprocess.check_output(args, cwd=ROOT, text=True).strip()


def replace_once(path: Path, old: str, new: str, label: str) -> None:
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


def insert_audit_support() -> None:
    text = AUDIT.read_text(encoding="utf-8")
    if "def load_intentional_omission_policy(" not in text:
        marker = "\ndef load_lfs_policy(component: dict, state: dict) -> tuple[dict[str, dict], list[str]]:\n"
        if text.count(marker) != 1:
            raise SystemExit("provenance audit omission-policy insertion marker is not unique")
        support = r'''
def _safe_policy_relative_path(value: object) -> str | None:
    if not isinstance(value, str) or not value or value.startswith("/") or "\\" in value:
        return None
    parts = value.split("/")
    if any(part in ("", ".", "..") for part in parts):
        return None
    return value


def load_intentional_omission_policy(
    component: dict, state: dict
) -> tuple[dict | None, list[str]]:
    """Load a standalone, declarative projection of an upstream source tree.

    Gitlink omission fragments remain owned by the existing gitlink policy
    machinery. Standalone omission policies may either select one upstream
    subtree (whose prefix is stripped in the MattOS import) or retain an
    explicit set of upstream-relative files/directories.
    """
    name = component["name"]
    policy_name = component.get("intentional_omission_policy", "none")
    failures: list[str] = []
    if state.get("intentional_omission_policy", "none") != policy_name:
        failures.append(f"{name}: state intentional_omission_policy does not match sources.toml")
    if policy_name == "none" or "#" in policy_name:
        return None, failures

    policy_path = ROOT / policy_name
    if not policy_path.is_file():
        return None, [*failures, f"{name}: intentional-omission policy is missing: {policy_name}"]
    policy = load_toml(policy_path)
    if policy.get("schema_version") != 1:
        failures.append(f"{name}: unsupported intentional-omission policy schema")
    if policy.get("component") != name:
        failures.append(f"{name}: intentional-omission policy component mismatch")
    if policy.get("upstream_commit") != component.get("revision"):
        failures.append(f"{name}: intentional-omission policy commit mismatch")
    if not isinstance(policy.get("reason"), str) or not policy.get("reason", "").strip():
        failures.append(f"{name}: intentional-omission policy lacks a reason")

    subtree = policy.get("upstream_subtree")
    retained = policy.get("retained_paths")
    if (subtree is None) == (retained is None):
        failures.append(
            f"{name}: intentional-omission policy must declare exactly one of "
            "upstream_subtree or retained_paths"
        )
        return policy, failures

    if subtree is not None and _safe_policy_relative_path(subtree) is None:
        failures.append(f"{name}: intentional-omission upstream_subtree is unsafe")

    if retained is not None:
        if not isinstance(retained, list) or not retained:
            failures.append(f"{name}: intentional-omission retained_paths must be a non-empty list")
        else:
            normalized = [_safe_policy_relative_path(path) for path in retained]
            if any(path is None for path in normalized):
                failures.append(f"{name}: intentional-omission retained_paths contains an unsafe path")
            if len(set(retained)) != len(retained):
                failures.append(f"{name}: intentional-omission retained_paths contains duplicates")

    expected = policy.get("expected_runtime_files")
    if expected is not None:
        if subtree is None:
            failures.append(f"{name}: expected_runtime_files requires upstream_subtree")
        elif not isinstance(expected, list) or not expected:
            failures.append(f"{name}: expected_runtime_files must be a non-empty list")
        else:
            normalized = [_safe_policy_relative_path(path) for path in expected]
            if any(path is None for path in normalized):
                failures.append(f"{name}: expected_runtime_files contains an unsafe path")
            if len(set(expected)) != len(expected):
                failures.append(f"{name}: expected_runtime_files contains duplicates")
    return policy, failures


def apply_intentional_omission_policy(
    component: dict,
    entries: list[tuple[str, str, str, str]],
    policy: dict | None,
) -> tuple[list[tuple[str, str, str, str]], list[str]]:
    if policy is None:
        return entries, []
    name = component["name"]
    failures: list[str] = []
    subtree = policy.get("upstream_subtree")
    if isinstance(subtree, str):
        prefix = subtree.rstrip("/") + "/"
        selected = [
            (mode, object_type, oid, path[len(prefix):])
            for mode, object_type, oid, path in entries
            if path.startswith(prefix) and path != prefix
        ]
        if not selected:
            failures.append(f"{name}: intentional-omission upstream_subtree selects no upstream files")
        expected = policy.get("expected_runtime_files")
        if isinstance(expected, list):
            actual = sorted(path for mode, _, _, path in selected if mode != "160000")
            if actual != sorted(expected):
                failures.append(
                    f"{name}: intentional-omission expected_runtime_files no longer matches upstream subtree"
                )
        return selected, failures

    retained = policy.get("retained_paths", [])
    selected: list[tuple[str, str, str, str]] = []
    matched = {path: False for path in retained if isinstance(path, str)}
    for entry in entries:
        upstream_path = entry[3]
        keep = False
        for selector in matched:
            if upstream_path == selector or upstream_path.startswith(selector.rstrip("/") + "/"):
                matched[selector] = True
                keep = True
        if keep:
            selected.append(entry)
    for selector, found in matched.items():
        if not found:
            failures.append(
                f"{name}: intentional-omission retained path does not exist upstream: {selector}"
            )
    if not selected:
        failures.append(f"{name}: intentional-omission retained_paths selects no upstream files")
    return selected, failures

'''
        text = text.replace(marker, support + marker, 1)
        AUDIT.write_text(text, encoding="utf-8")

    replace_once(
        AUDIT,
        "    source_selection: dict | None,\n    lfs_objects: dict[str, dict],\n) -> tuple[int, list[str]]:",
        "    source_selection: dict | None,\n    intentional_omission: dict | None,\n    lfs_objects: dict[str, dict],\n) -> tuple[int, list[str]]:",
        "verify_component_tree omission argument",
    )
    replace_once(
        AUDIT,
        "        SELECTED_IMPORTED_DIGEST_ALGORITHM if source_selection is not None else IMPORTED_DIGEST_ALGORITHM\n",
        "        SELECTED_IMPORTED_DIGEST_ALGORITHM\n        if source_selection is not None or intentional_omission is not None\n        else IMPORTED_DIGEST_ALGORITHM\n",
        "selected digest algorithm decision",
    )

    emit_old = '''            source_selection, selection_failures = load_source_selection_policy(component, state)\n            failures.extend(selection_failures)\n            entries = [\n                entry for entry in entries if source_selection_retains(source_selection, entry[3])\n            ]\n            print(f"{name}\\t{tree}\\t{imported_digest(entries)}")'''
    emit_new = '''            source_selection, selection_failures = load_source_selection_policy(component, state)\n            failures.extend(selection_failures)\n            intentional_omission, omission_failures = load_intentional_omission_policy(component, state)\n            failures.extend(omission_failures)\n            entries = [\n                entry for entry in entries if source_selection_retains(source_selection, entry[3])\n            ]\n            entries, projection_failures = apply_intentional_omission_policy(\n                component, entries, intentional_omission\n            )\n            failures.extend(projection_failures)\n            print(f"{name}\\t{tree}\\t{imported_digest(entries)}")'''
    replace_once(AUDIT, emit_old, emit_new, "emit-state omission projection")

    verify_old = '''        source_selection, selection_failures = load_source_selection_policy(component, state)\n        failures.extend(selection_failures)\n        lfs_objects, lfs_failures = load_lfs_policy(component, state)'''
    verify_new = '''        source_selection, selection_failures = load_source_selection_policy(component, state)\n        failures.extend(selection_failures)\n        intentional_omission, omission_failures = load_intentional_omission_policy(component, state)\n        failures.extend(omission_failures)\n        lfs_objects, lfs_failures = load_lfs_policy(component, state)'''
    replace_once(AUDIT, verify_old, verify_new, "verification omission policy load")

    filter_old = '''        entries = [\n            entry for entry in entries if source_selection_retains(source_selection, entry[3])\n        ]\n        ignored_count, tree_failures = verify_component_tree(\n            component, state, tree, entries, gitlink_by_path, source_selection, lfs_objects\n        )'''
    filter_new = '''        entries = [\n            entry for entry in entries if source_selection_retains(source_selection, entry[3])\n        ]\n        entries, projection_failures = apply_intentional_omission_policy(\n            component, entries, intentional_omission\n        )\n        failures.extend(projection_failures)\n        ignored_count, tree_failures = verify_component_tree(\n            component,\n            state,\n            tree,\n            entries,\n            gitlink_by_path,\n            source_selection,\n            intentional_omission,\n            lfs_objects,\n        )'''
    replace_once(AUDIT, filter_old, filter_new, "verification omission projection")


def migrate_font_state_digests() -> None:
    completed = subprocess.run(
        ["python3", str(AUDIT.relative_to(ROOT)), "--emit-state-values"],
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if completed.returncode != 0:
        raise SystemExit(
            "unable to derive canonical projected source digests:\n" + completed.stderr.strip()
        )
    values: dict[str, str] = {}
    for line in completed.stdout.splitlines():
        fields = line.split("\t")
        if len(fields) == 3:
            values[fields[0]] = fields[2]

    for name in FONT_COMPONENTS:
        digest = values.get(name)
        if not digest or not re.fullmatch(r"[0-9a-f]{64}", digest):
            raise SystemExit(f"missing canonical projected digest for {name}")
        state_path = ROOT / "upstream/state" / f"{name}.toml"
        text = state_path.read_text(encoding="utf-8")
        algorithm_match = re.search(r'^imported_tree_digest_algorithm = "([^"]+)"$', text, re.MULTILINE)
        if algorithm_match is None:
            raise SystemExit(f"{name}: state lacks imported_tree_digest_algorithm")
        algorithm = algorithm_match.group(1)
        if algorithm not in (LEGACY_SELECTED_ALGORITHM, GENERIC_SELECTED_ALGORITHM):
            raise SystemExit(f"{name}: refusing unexpected imported-tree digest algorithm {algorithm!r}")
        text, digest_count = re.subn(
            r'^imported_tree_digest = "[0-9a-f]{64}"$',
            f'imported_tree_digest = "{digest}"',
            text,
            count=1,
            flags=re.MULTILINE,
        )
        if digest_count != 1:
            raise SystemExit(f"{name}: unable to update imported_tree_digest exactly once")
        text, algorithm_count = re.subn(
            r'^imported_tree_digest_algorithm = "[^"]+"$',
            f'imported_tree_digest_algorithm = "{GENERIC_SELECTED_ALGORITHM}"',
            text,
            count=1,
            flags=re.MULTILINE,
        )
        if algorithm_count != 1:
            raise SystemExit(f"{name}: unable to update imported_tree_digest_algorithm exactly once")
        state_path.write_text(text, encoding="utf-8")
        print(f"Migrated {name} to canonical projected digest {digest}", flush=True)


def patch_resume_helper() -> None:
    text = RESUME.read_text(encoding="utf-8")
    if 'ROOT / "DevUtils/repair_runtime_font_provenance.py"' not in text:
        marker = '    ROOT / "DevUtils/resume_vendor_cosmic_tweaks.py",\n'
        if text.count(marker) != 1:
            raise SystemExit("resume helper bootstrap-list marker is not unique")
        text = text.replace(
            marker,
            marker + '    ROOT / "DevUtils/repair_runtime_font_provenance.py",\n',
            1,
        )

    allowed_marker = '        "upstream/state/cosmic-tweaks.toml",\n'
    allowed_addition = (
        '        "upstream/state/noto-sans-mono.toml",\n'
        '        "upstream/state/open-sans.toml",\n'
    )
    if '        "upstream/state/noto-sans-mono.toml",\n' not in text:
        if text.count(allowed_marker) != 1:
            raise SystemExit("resume helper allowed-state marker is not unique")
        text = text.replace(allowed_marker, allowed_marker + allowed_addition, 1)

    stage_marker = '        "upstream/state/cosmic-tweaks.toml",\n        "src/desktop/cosmic/cosmic-tweaks",\n'
    stage_new = (
        '        "upstream/state/cosmic-tweaks.toml",\n'
        '        "upstream/state/noto-sans-mono.toml",\n'
        '        "upstream/state/open-sans.toml",\n'
        '        "src/desktop/cosmic/cosmic-tweaks",\n'
    )
    # The marker appears once in final integration staging and once in the
    # catalog-only staging call. Font state migration belongs only in the final
    # commit, so patch the last occurrence.
    if '        "upstream/state/noto-sans-mono.toml",\n        "upstream/state/open-sans.toml",\n        "src/desktop/cosmic/cosmic-tweaks",\n' not in text:
        index = text.rfind(stage_marker)
        if index < 0:
            raise SystemExit("resume helper final staging marker is missing")
        text = text[:index] + text[index:].replace(stage_marker, stage_new, 1)

    RESUME.write_text(text, encoding="utf-8")


def main() -> None:
    if output("git", "branch", "--show-current") != BRANCH:
        raise SystemExit(f"expected branch {BRANCH!r}")
    insert_audit_support()
    migrate_font_state_digests()
    patch_resume_helper()
    print("Runtime-font omission provenance is now expressed through the generic selected-tree model.")


if __name__ == "__main__":
    main()
