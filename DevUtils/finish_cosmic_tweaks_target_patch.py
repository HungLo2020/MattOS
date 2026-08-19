#!/usr/bin/env python3
from __future__ import annotations

import hashlib
import subprocess
from pathlib import Path

BRANCH = "agent/vendor-cosmic-tweaks"
ROOT = Path(__file__).resolve().parents[1]
SELF = Path(__file__).resolve()
SOURCES = ROOT / "upstream/sources.toml"
STATE = ROOT / "upstream/state/cosmic-tweaks.toml"
PATCH_REL = "upstream/patches/cosmic-tweaks/0001-honor-cargo-target-dir.patch"
PATCH = ROOT / PATCH_REL
MANIFEST_REL = "upstream/patches/cosmic-tweaks/manifest.toml"
MANIFEST = ROOT / MANIFEST_REL
PATCH_SHA256 = "497d7d367f42676063f16d0427a811642fe061cc9fcaec05d1863776d965f8e6"
MANIFEST_SHA256 = "aad3b0f4f0a9fe35aab78e4abbf420a3d67322c2010bcaf05929fae9a035915d"
TWEAKS_COMMIT = "069c31b7b1beffddf744b28f8f056ace972830bc"


def run(*args: str) -> None:
    print("+", " ".join(args), flush=True)
    subprocess.run(args, cwd=ROOT, check=True)


def output(*args: str) -> str:
    return subprocess.check_output(args, cwd=ROOT, text=True).strip()


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def require_safe_checkout() -> None:
    branch = output("git", "branch", "--show-current")
    if branch != BRANCH:
        raise SystemExit(f"expected branch {BRANCH!r}, got {branch!r}")
    status = subprocess.check_output(
        ["git", "status", "--porcelain=v1", "--untracked-files=no"],
        cwd=ROOT,
        text=True,
    )
    if status:
        raise SystemExit("refusing to continue with dirty tracked files:\n" + status)


def verify_patch_inputs() -> None:
    if sha256(PATCH) != PATCH_SHA256:
        raise SystemExit("COSMIC Tweaks target-dir patch checksum mismatch")
    if sha256(MANIFEST) != MANIFEST_SHA256:
        raise SystemExit("COSMIC Tweaks patch manifest checksum mismatch")
    manifest = MANIFEST.read_text(encoding="utf-8")
    required = (
        'component = "cosmic-tweaks"',
        f'upstream_commit = "{TWEAKS_COMMIT}"',
        f'path = "{PATCH_REL}"',
        f'sha256 = "{PATCH_SHA256}"',
        'application = "output-mirror-only"',
    )
    for marker in required:
        if marker not in manifest:
            raise SystemExit(f"COSMIC Tweaks patch manifest is missing {marker!r}")


def component_block(text: str, name: str) -> tuple[int, int, str]:
    marker = f'[[component]]\nname = "{name}"\n'
    start = text.find(marker)
    if start < 0:
        raise SystemExit(f"sources.toml is missing component {name}")
    end = text.find("\n[[component]]\n", start + len(marker))
    if end < 0:
        end = len(text)
    return start, end, text[start:end]


def register_manifest_in_sources() -> None:
    text = SOURCES.read_text(encoding="utf-8")
    start, end, block = component_block(text, "cosmic-tweaks")
    if f'revision = "{TWEAKS_COMMIT}"' not in block:
        raise SystemExit("cosmic-tweaks revision changed unexpectedly")
    lines = block.splitlines(keepends=True)
    fields = {
        "patch_manifest": MANIFEST_REL,
        "patch_manifest_sha256": MANIFEST_SHA256,
    }
    for key, value in fields.items():
        prefix = f"{key} = "
        matches = [i for i, line in enumerate(lines) if line.startswith(prefix)]
        replacement = f'{key} = "{value}"\n'
        if len(matches) == 1:
            lines[matches[0]] = replacement
        elif len(matches) == 0:
            sync_matches = [i for i, line in enumerate(lines) if line.startswith("sync = ")]
            if len(sync_matches) != 1:
                raise SystemExit("cosmic-tweaks sync field is not unique")
            insert_at = sync_matches[0] + 1
            while insert_at < len(lines) and lines[insert_at].startswith("patch_manifest"):
                insert_at += 1
            lines.insert(insert_at, replacement)
        else:
            raise SystemExit(f"cosmic-tweaks has duplicate {key} fields")
    block = "".join(lines)
    SOURCES.write_text(text[:start] + block + text[end:], encoding="utf-8")


def set_state_field(key: str, value: str) -> None:
    text = STATE.read_text(encoding="utf-8")
    lines = text.splitlines(keepends=True)
    prefix = f"{key} = "
    matches = [i for i, line in enumerate(lines) if line.startswith(prefix)]
    if len(matches) != 1:
        raise SystemExit(f"state field {key} is not unique")
    lines[matches[0]] = f'{key} = "{value}"\n'
    STATE.write_text("".join(lines), encoding="utf-8")


def register_manifest_in_state() -> None:
    state = STATE.read_text(encoding="utf-8")
    if f'imported_commit = "{TWEAKS_COMMIT}"' not in state:
        raise SystemExit("cosmic-tweaks state revision changed unexpectedly")
    set_state_field("patch_manifest", MANIFEST_REL)
    set_state_field("patch_manifest_sha256", MANIFEST_SHA256)


def publish_metadata() -> None:
    run("python3", "DevUtils/test_vendored_source_provenance.py")
    SELF.unlink()
    run(
        "git",
        "add",
        "-A",
        "--",
        "upstream/sources.toml",
        "upstream/state/cosmic-tweaks.toml",
        str(SELF.relative_to(ROOT)),
    )
    run("git", "diff", "--cached", "--check")
    run("git", "commit", "-m", "Register COSMIC Tweaks target-dir patch")
    run("git", "push", "origin", f"HEAD:{BRANCH}")


def targeted_validation() -> None:
    run("cargo", "run", "-p", "mattos-build", "--", "build", "cosmic-tweaks")
    required = (
        ROOT / "out/build/cosmic-tweaks/install/usr/bin/cosmic-ext-tweaks",
        ROOT / "out/build/cosmic-tweaks/install/usr/share/applications/dev.edfloreshz.CosmicTweaks.desktop",
        ROOT / "out/build/cosmic-tweaks/install/usr/share/icons/hicolor/scalable/apps/dev.edfloreshz.CosmicTweaks.svg",
    )
    for path in required:
        if not path.is_file():
            raise SystemExit(f"targeted COSMIC Tweaks build is missing {path.relative_to(ROOT)}")
    run("cargo", "run", "-p", "mattos-build", "--", "build", "cosmic-desktop")
    run("cargo", "run", "-p", "mattos-build", "--", "package", "build", "cosmic-desktop")
    print("COSMIC Tweaks target-dir patch is provenance-valid and targeted validation passed.")


def main() -> None:
    require_safe_checkout()
    verify_patch_inputs()
    register_manifest_in_sources()
    register_manifest_in_state()
    publish_metadata()
    targeted_validation()


if __name__ == "__main__":
    main()
