#!/usr/bin/env python3
"""Build MattOS and upload every generated binary package.

The build invocation intentionally reuses the same helper as ``run_qemu.py``.
Package selection is discovered from the canonical build output directory so
new package definitions do not require edits to this script.
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

import run_qemu
from common import RepoError, find_repo_root, run_command


PACKAGE_OUTPUT_RELATIVE = Path("out/packages/amd64")
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
    """Return every regular amd64 .deb beneath the canonical package output."""
    package_root = (repo_root / PACKAGE_OUTPUT_RELATIVE).resolve()
    if not package_root.is_dir():
        raise RepoError(f"package output directory does not exist: {package_root}")

    packages: list[Path] = []
    for candidate in sorted(package_root.rglob("*.deb")):
        if candidate.is_symlink() or not candidate.is_file():
            raise RepoError(f"package output contains a non-regular .deb: {candidate}")
        resolved = candidate.resolve()
        if package_root not in resolved.parents:
            raise RepoError(f"package output escapes its canonical directory: {candidate}")
        packages.append(resolved)

    if not packages:
        raise RepoError(f"no .deb packages found beneath {package_root}")
    return packages


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
    command.extend(["upload", *(str(package) for package in packages)])
    run_command(command, cwd=repo_root)


def main() -> int:
    args = parse_args()
    repo_root = find_repo_root(Path(__file__).resolve().parent)
    ensure_build(repo_root, clean=args.clean, no_build=args.no_build)
    packages = discover_packages(repo_root)
    print(f"Discovered {len(packages)} package(s) beneath {PACKAGE_OUTPUT_RELATIVE}")
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
