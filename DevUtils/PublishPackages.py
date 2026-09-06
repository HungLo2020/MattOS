#!/usr/bin/env python3
"""Build MattOS and upload every generated binary package.

The build invocation intentionally reuses the same helper as ``run_qemu.py``.
Package selection is discovered from the canonical build output directory so
new package definitions do not require edits to this script.
"""

from __future__ import annotations

import argparse
import sys
import tomllib
from pathlib import Path

import run_qemu
from common import RepoError, find_repo_root, run_command


PACKAGE_OUTPUT_RELATIVE = Path("out/packages/amd64")
PACKAGE_INVENTORY_RELATIVE = Path("out/packages/inventory.toml")
PUBLISHER_RELATIVE = Path(
    "src/infrastructure/LinuxScripts/GenericScripts/ManageMattOSRepository.py"
)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Build MattOS and upload every generated .deb package"
    )
    parser.add_argument(
        "--clean",
        action="store_true",
        help="clean build artifacts before running the same build as run_qemu.py",
    )
    parser.add_argument(
        "--no-build",
        action="store_true",
        help="use the existing package output without rebuilding",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="validate and print the upload command without uploading",
    )
    return parser.parse_args()


def discover_packages(repo_root: Path) -> list[Path]:
    """Return exactly the artifacts approved by the package inventory."""
    package_root = (repo_root / PACKAGE_OUTPUT_RELATIVE).resolve()
    if not package_root.is_dir():
        raise RepoError(f"package output directory does not exist: {package_root}")
    inventory_path = repo_root / PACKAGE_INVENTORY_RELATIVE
    if not inventory_path.is_file():
        raise RepoError(f"package inventory does not exist: {inventory_path}")
    try:
        inventory = tomllib.loads(inventory_path.read_text(encoding="utf-8"))
    except (OSError, tomllib.TOMLDecodeError) as exc:
        raise RepoError(f"could not read package inventory: {inventory_path}") from exc

    entries = inventory.get("package")
    if not isinstance(entries, list) or not entries:
        raise RepoError(f"package inventory has no package entries: {inventory_path}")

    packages: list[Path] = []
    seen: set[Path] = set()
    for entry in entries:
        if not isinstance(entry, dict) or not isinstance(entry.get("artifact_path"), str):
            raise RepoError(f"package inventory contains a malformed artifact entry: {inventory_path}")
        relative = Path(entry["artifact_path"])
        candidate = (repo_root / relative).resolve()
        try:
            candidate.relative_to(package_root)
        except ValueError as exc:
            raise RepoError(
                f"inventory artifact is outside {PACKAGE_OUTPUT_RELATIVE}: {relative}"
            ) from exc
        if candidate in seen:
            raise RepoError(f"package inventory contains a duplicate artifact: {relative}")
        if candidate.suffix != ".deb" or candidate.is_symlink() or not candidate.is_file():
            raise RepoError(f"inventory artifact is not a regular .deb: {relative}")
        seen.add(candidate)
        packages.append(candidate)

    return sorted(packages)


def ensure_build(repo_root: Path, *, clean: bool, no_build: bool) -> None:
    """Use run_qemu.py's exact doctor/build path without launching QEMU."""
    if no_build:
        return
    build_args = argparse.Namespace(no_build=False, clean=clean, dry_run=False)
    run_qemu.build_if_needed(repo_root, build_args)


def publisher_path(repo_root: Path) -> Path:
    publisher = repo_root / PUBLISHER_RELATIVE
    if not publisher.is_file():
        raise RepoError(f"vendored repository publisher is missing: {publisher}")
    return publisher


def upload_packages(repo_root: Path, packages: list[Path], *, dry_run: bool) -> None:
    command = [
        sys.executable,
        str(publisher_path(repo_root)),
        "--non-interactive",
    ]
    if dry_run:
        command.append("--dry-run")
    command.extend(["--repo", "mattos", "upload", *(str(package) for package in packages)])
    run_command(command, cwd=repo_root)


def main() -> int:
    args = parse_args()
    repo_root = find_repo_root(Path(__file__).resolve().parent)
    ensure_build(repo_root, clean=args.clean, no_build=args.no_build)
    packages = discover_packages(repo_root)
    print(
        f"Discovered {len(packages)} package(s) from {PACKAGE_INVENTORY_RELATIVE}"
    )
    for package in packages:
        print(f"  {package.relative_to(repo_root)}")
    upload_packages(repo_root, packages, dry_run=args.dry_run)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except RepoError as exc:
        print(f"error: {exc}", file=sys.stderr)
        raise SystemExit(1) from exc
