#!/usr/bin/env python3
from __future__ import annotations

import hashlib
import subprocess
import urllib.request
from pathlib import Path

BRANCH = "agent/vendor-cosmic-tweaks"
ROOT = Path(__file__).resolve().parents[1]
SOURCES = ROOT / "upstream/sources.toml"
RESUME = ROOT / "DevUtils/resume_vendor_cosmic_tweaks.py"
FINALIZER = ROOT / "DevUtils/finalize_vendor_cosmic_tweaks.py"
SELF_REL = "DevUtils/complete_vendor_cosmic_tweaks.py"

COSMIC_COMP_COMMIT = "693d1ad775193deb95a33810b5eac59684ca2ac0"
COSMIC_COMP_TREE = "0017bc3f47da8be017d4d332ab76c99049486a9f"
COSMIC_COMP_MANIFEST = ROOT / "upstream/patches/cosmic-comp/manifest.toml"
COSMIC_COMP_STATE = ROOT / "upstream/state/cosmic-comp.toml"

GREETER_COMMIT = "d39915ae2381424d406cd511a2310ef928144f4c"
GREETER_LFS_PATH = "res/background.jpg"
GREETER_LFS_SHA256 = "7500f702f0488d4a8df2c5abeb7ca9107a7ca7998e0441727cbaf79465b02388"
GREETER_LFS_SIZE = 3839900
GREETER_POLICY_REL = "upstream/policies/cosmic-greeter-lfs.toml"
GREETER_POLICY = ROOT / GREETER_POLICY_REL
GREETER_STATE = ROOT / "upstream/state/cosmic-greeter.toml"
GREETER_PAYLOAD = ROOT / "src/desktop/cosmic/cosmic-greeter" / GREETER_LFS_PATH
GREETER_MEDIA_URL = (
    "https://media.githubusercontent.com/media/pop-os/cosmic-greeter/"
    f"{GREETER_COMMIT}/{GREETER_LFS_PATH}"
)


def output(*args: str) -> str:
    return subprocess.check_output(args, cwd=ROOT, text=True).strip()


def sha256_bytes(payload: bytes) -> str:
    return hashlib.sha256(payload).hexdigest()


def component_block(text: str, name: str) -> tuple[int, int, str]:
    marker = f'[[component]]\nname = "{name}"\n'
    start = text.find(marker)
    if start < 0:
        raise SystemExit(f"sources.toml is missing component {name}")
    end = text.find("\n[[component]]\n", start + len(marker))
    if end < 0:
        end = len(text)
    return start, end, text[start:end]


def set_block_field(block: str, key: str, value: str, *, after_key: str | None = None) -> str:
    prefix = f"{key} = "
    lines = block.splitlines(keepends=True)
    matches = [i for i, line in enumerate(lines) if line.startswith(prefix)]
    replacement = f'{key} = "{value}"\n'
    if len(matches) == 1:
        lines[matches[0]] = replacement
        return "".join(lines)
    if matches:
        raise SystemExit(f"component block has duplicate {key} fields")
    if after_key is None:
        raise SystemExit(f"component block is missing {key} and no insertion anchor was supplied")
    anchor = f"{after_key} = "
    anchors = [i for i, line in enumerate(lines) if line.startswith(anchor)]
    if len(anchors) != 1:
        raise SystemExit(f"component block insertion anchor {after_key} is not unique")
    lines.insert(anchors[0] + 1, replacement)
    return "".join(lines)


def replace_component_block(text: str, name: str, new_block: str) -> str:
    start, end, _ = component_block(text, name)
    return text[:start] + new_block + text[end:]


def set_state_field(path: Path, key: str, value: str) -> None:
    text = path.read_text(encoding="utf-8")
    prefix = f"{key} = "
    lines = text.splitlines(keepends=True)
    matches = [i for i, line in enumerate(lines) if line.startswith(prefix)]
    if len(matches) != 1:
        raise SystemExit(f"{path.relative_to(ROOT)}: expected exactly one {key} field")
    lines[matches[0]] = f'{key} = "{value}"\n'
    path.write_text("".join(lines), encoding="utf-8")


def fix_cosmic_comp_patch_manifest() -> str:
    text = COSMIC_COMP_MANIFEST.read_text(encoding="utf-8")
    required = (
        f'component = "cosmic-comp"',
        f'upstream_commit = "{COSMIC_COMP_COMMIT}"',
        'application = "output-mirror-only"',
    )
    for marker in required:
        if marker not in text:
            raise SystemExit(f"cosmic-comp patch manifest is missing {marker!r}")

    tree_line = f'upstream_tree = "{COSMIC_COMP_TREE}"'
    if tree_line not in text:
        if "upstream_tree = " in text:
            raise SystemExit("cosmic-comp patch manifest records an unexpected upstream tree")
        anchor = f'upstream_commit = "{COSMIC_COMP_COMMIT}"\n'
        if text.count(anchor) != 1:
            raise SystemExit("cosmic-comp patch manifest commit anchor is not unique")
        text = text.replace(anchor, anchor + tree_line + "\n", 1)
        COSMIC_COMP_MANIFEST.write_text(text, encoding="utf-8")

    digest = sha256_bytes(COSMIC_COMP_MANIFEST.read_bytes())
    sources = SOURCES.read_text(encoding="utf-8")
    _, _, block = component_block(sources, "cosmic-comp")
    if f'revision = "{COSMIC_COMP_COMMIT}"' not in block:
        raise SystemExit("cosmic-comp sources.toml revision changed unexpectedly")
    block = set_block_field(block, "patch_manifest_sha256", digest)
    SOURCES.write_text(replace_component_block(sources, "cosmic-comp", block), encoding="utf-8")
    set_state_field(COSMIC_COMP_STATE, "patch_manifest_sha256", digest)
    print(f"Pinned cosmic-comp patch manifest to upstream tree {COSMIC_COMP_TREE}", flush=True)
    return digest


def greeter_policy_bytes() -> bytes:
    return (
        'schema_version = 1\n'
        'component = "cosmic-greeter"\n'
        f'upstream_commit = "{GREETER_COMMIT}"\n'
        f'source = "https://media.githubusercontent.com/media/pop-os/cosmic-greeter/{GREETER_COMMIT}/{{path}}"\n'
        '\n'
        '[[object]]\n'
        f'path = "{GREETER_LFS_PATH}"\n'
        f'sha256 = "{GREETER_LFS_SHA256}"\n'
        f'size = {GREETER_LFS_SIZE}\n'
    ).encode()


def install_greeter_lfs_policy() -> str:
    payload = greeter_policy_bytes()
    if GREETER_POLICY.exists() and GREETER_POLICY.read_bytes() != payload:
        raise SystemExit("existing cosmic-greeter LFS policy differs from the expected exact policy")
    GREETER_POLICY.parent.mkdir(parents=True, exist_ok=True)
    GREETER_POLICY.write_bytes(payload)
    digest = sha256_bytes(payload)

    sources = SOURCES.read_text(encoding="utf-8")
    _, _, block = component_block(sources, "cosmic-greeter")
    if f'revision = "{GREETER_COMMIT}"' not in block:
        raise SystemExit("cosmic-greeter sources.toml revision changed unexpectedly")
    block = set_block_field(block, "lfs_policy", GREETER_POLICY_REL, after_key="sync")
    block = set_block_field(block, "lfs_policy_sha256", digest, after_key="lfs_policy")
    SOURCES.write_text(replace_component_block(sources, "cosmic-greeter", block), encoding="utf-8")

    set_state_field(GREETER_STATE, "lfs_policy", GREETER_POLICY_REL)
    set_state_field(GREETER_STATE, "lfs_policy_sha256", digest)
    print(f"Declared cosmic-greeter LFS object policy {digest}", flush=True)
    return digest


def payload_is_hydrated() -> bool:
    if not GREETER_PAYLOAD.is_file():
        return False
    body = GREETER_PAYLOAD.read_bytes()
    return len(body) == GREETER_LFS_SIZE and sha256_bytes(body) == GREETER_LFS_SHA256


def hydrate_greeter_background() -> None:
    if payload_is_hydrated():
        print("cosmic-greeter background is already hydrated and verified", flush=True)
        return

    current = GREETER_PAYLOAD.read_bytes() if GREETER_PAYLOAD.exists() else b""
    expected_pointer = (
        "version https://git-lfs.github.com/spec/v1\n"
        f"oid sha256:{GREETER_LFS_SHA256}\n"
        f"size {GREETER_LFS_SIZE}\n"
    ).encode()
    if current and current != expected_pointer:
        raise SystemExit(
            "cosmic-greeter background is neither the exact upstream LFS pointer nor the verified payload"
        )

    tmp_dir = ROOT / "out/tmp"
    tmp_dir.mkdir(parents=True, exist_ok=True)
    tmp = tmp_dir / "cosmic-greeter-background.jpg.part"
    print(f"Downloading exact cosmic-greeter LFS payload from {GREETER_MEDIA_URL}", flush=True)
    with urllib.request.urlopen(GREETER_MEDIA_URL, timeout=120) as response:
        body = response.read()
    if len(body) != GREETER_LFS_SIZE:
        raise SystemExit(
            f"cosmic-greeter background size mismatch: got {len(body)}, expected {GREETER_LFS_SIZE}"
        )
    actual = sha256_bytes(body)
    if actual != GREETER_LFS_SHA256:
        raise SystemExit(
            f"cosmic-greeter background SHA-256 mismatch: got {actual}, expected {GREETER_LFS_SHA256}"
        )
    tmp.write_bytes(body)
    GREETER_PAYLOAD.parent.mkdir(parents=True, exist_ok=True)
    tmp.replace(GREETER_PAYLOAD)
    print("Hydrated and verified cosmic-greeter res/background.jpg", flush=True)


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
    block = block.replace(anchor, anchor + "".join(missing), 1)
    return text[:start] + block + text[end:]


def patch_resume_bookkeeping() -> None:
    text = RESUME.read_text(encoding="utf-8")

    text = ensure_lines_in_block(
        text,
        "HELPERS = [\n",
        "]\n\n\ndef output",
        '    ROOT / "DevUtils/resume_vendor_cosmic_tweaks.py",\n',
        (f'    ROOT / "{SELF_REL}",\n',),
        "HELPERS declaration",
    )

    allowed = (
        '        "upstream/patches/cosmic-comp/manifest.toml",\n',
        '        "upstream/state/cosmic-comp.toml",\n',
        f'        "{GREETER_POLICY_REL}",\n',
        '        "upstream/state/cosmic-greeter.toml",\n',
        '        "src/desktop/cosmic/cosmic-greeter/res/background.jpg",\n',
    )
    text = ensure_lines_in_block(
        text,
        "    allowed = (\n",
        "    )\n    status = subprocess.check_output(",
        '        "upstream/state/cosmic-tweaks.toml",\n',
        allowed,
        "verify_partial_state allowed tuple",
    )

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
        '        "upstream/patches/cosmic-comp/manifest.toml",\n',
        '        "upstream/state/cosmic-comp.toml",\n',
        f'        "{GREETER_POLICY_REL}",\n',
        '        "upstream/state/cosmic-greeter.toml",\n',
        '        "src/desktop/cosmic/cosmic-greeter/res/background.jpg",\n',
        f'        "{SELF_REL}",\n',
    )
    missing = [entry for entry in final_entries if entry not in add_block]
    if missing:
        anchor = '        "upstream/state/cosmic-tweaks.toml",\n'
        if add_block.count(anchor) != 1:
            raise SystemExit("resume helper final git-add state anchor is not unique")
        add_block = add_block.replace(anchor, anchor + "".join(missing), 1)
        block = block[:add_start] + add_block + block[add_end:]
        text = text[:start] + block + text[end:]

    RESUME.write_text(text, encoding="utf-8")


def verify_repair_metadata(manifest_digest: str, policy_digest: str) -> None:
    manifest = COSMIC_COMP_MANIFEST.read_text(encoding="utf-8")
    if f'upstream_tree = "{COSMIC_COMP_TREE}"' not in manifest:
        raise SystemExit("cosmic-comp manifest tree repair did not persist")
    sources = SOURCES.read_text(encoding="utf-8")
    _, _, comp = component_block(sources, "cosmic-comp")
    if f'patch_manifest_sha256 = "{manifest_digest}"' not in comp:
        raise SystemExit("cosmic-comp sources.toml patch-manifest checksum was not updated")
    _, _, greeter = component_block(sources, "cosmic-greeter")
    for expected in (
        f'lfs_policy = "{GREETER_POLICY_REL}"',
        f'lfs_policy_sha256 = "{policy_digest}"',
    ):
        if expected not in greeter:
            raise SystemExit(f"cosmic-greeter sources.toml is missing {expected}")
    if not payload_is_hydrated():
        raise SystemExit("cosmic-greeter background is not the verified hydrated payload")


def main() -> None:
    if output("git", "branch", "--show-current") != BRANCH:
        raise SystemExit(f"expected branch {BRANCH!r}")
    if not FINALIZER.is_file() or not RESUME.is_file():
        raise SystemExit("COSMIC Tweaks continuation helpers are missing")

    manifest_digest = fix_cosmic_comp_patch_manifest()
    policy_digest = install_greeter_lfs_policy()
    hydrate_greeter_background()
    verify_repair_metadata(manifest_digest, policy_digest)
    patch_resume_bookkeeping()

    print("Pre-existing provenance blockers repaired; resuming COSMIC Tweaks finalization.", flush=True)
    subprocess.run(
        ["python3", str(FINALIZER.relative_to(ROOT))],
        cwd=ROOT,
        check=True,
    )


if __name__ == "__main__":
    main()
