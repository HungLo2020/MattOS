#!/usr/bin/env python3
"""Build MattOS and upload every generated binary package.

The build invocation intentionally reuses the same helper as ``run_qemu.py``.
Package selection is discovered from the canonical build output directory so
new package definitions do not require edits to this script.

Before uploading, each package is compared with the published repository
index.  Builds are reproducible, so a package whose version and SHA-256 both
match the published one is truly unchanged and is skipped; a new or newer
version is uploaded, and so is a rebuild whose bytes differ under the same
version (the publisher replaces it in place, which testing relies on).  A
version older than the published one is refused.
"""

from __future__ import annotations

import argparse
import gzip
import re
import subprocess
import sys
import tomllib
import urllib.request
from dataclasses import dataclass, field
from pathlib import Path

import run_qemu
from common import RepoError, find_repo_root, run_command


PACKAGE_OUTPUT_RELATIVE = Path("out/packages/amd64")
PACKAGE_INVENTORY_RELATIVE = Path("out/packages/inventory.toml")
PUBLISHER_RELATIVE = Path(
    "src/infrastructure/LinuxScripts/GenericScripts/ManageMattOSRepository.py"
)
REPOSITORY_SOURCES_RELATIVE = Path("src/system/packages/config/apt/mattos-hosted.sources")
THIRD_PARTY_RELEASES_RELATIVE = Path("third-party-packages/releases.json")


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


@dataclass(frozen=True)
class InventoryPackage:
    name: str
    version: str
    architecture: str
    sha256: str
    artifact: Path
    dependencies: tuple[str, ...]


@dataclass(frozen=True)
class PublishedPackage:
    version: str
    sha256: str


@dataclass
class PublicationPlan:
    new: list[InventoryPackage] = field(default_factory=list)
    rebuilt: list[InventoryPackage] = field(default_factory=list)
    unchanged: list[InventoryPackage] = field(default_factory=list)
    older_than_published: list[tuple[InventoryPackage, str]] = field(default_factory=list)

    @property
    def upload(self) -> list[InventoryPackage]:
        return self.new + self.rebuilt


def inventory_packages(repo_root: Path) -> list[InventoryPackage]:
    inventory = tomllib.loads((repo_root / PACKAGE_INVENTORY_RELATIVE).read_text(encoding="utf-8"))
    packages = []
    for entry in inventory.get("package", []):
        try:
            packages.append(
                InventoryPackage(
                    name=entry["name"],
                    version=entry["version"],
                    architecture=entry["architecture"],
                    sha256=entry["sha256"],
                    artifact=repo_root / entry["artifact_path"],
                    dependencies=tuple(entry.get("dependencies", [])),
                )
            )
        except KeyError as exc:
            raise RepoError(f"package inventory entry lacks {exc}: {entry!r}") from exc
    return packages


def third_party_package_names(repo_root: Path) -> set[str]:
    """Names published by third-party recipes rather than the MattOS build."""
    path = repo_root / THIRD_PARTY_RELEASES_RELATIVE
    if not path.is_file():
        return set()
    import json

    return set(json.loads(path.read_text(encoding="utf-8")).get("packages", {}))


def reject_third_party_names(repo_root: Path, packages: list[InventoryPackage]) -> None:
    """A package name belongs to exactly one producer: MattOS or one recipe."""
    claimed = sorted(package.name for package in packages if package.name in third_party_package_names(repo_root))
    if claimed:
        raise RepoError(
            "MattOS packages also claimed by a third-party recipe: " + ", ".join(claimed)
            + f"; remove the recipe from {THIRD_PARTY_RELEASES_RELATIVE} before publishing"
        )


def published_index_url(repo_root: Path) -> str:
    """The binary Packages index of the hosted repository the installed system uses."""
    fields: dict[str, str] = {}
    for line in (repo_root / REPOSITORY_SOURCES_RELATIVE).read_text(encoding="utf-8").splitlines():
        key, separator, value = line.partition(":")
        if separator:
            fields[key.strip()] = value.strip()
    try:
        return (
            f"{fields['URIs'].rstrip('/')}/dists/{fields['Suites']}/{fields['Components']}"
            f"/binary-{fields['Architectures']}/Packages.gz"
        )
    except KeyError as exc:
        raise RepoError(f"{REPOSITORY_SOURCES_RELATIVE} lacks {exc}") from exc


def fetch_published_index(url: str) -> str:
    try:
        # The CDN refuses urllib's default User-Agent.
        request = urllib.request.Request(url, headers={"User-Agent": "MattOS-PublishPackages/1"})
        with urllib.request.urlopen(request, timeout=60) as response:
            return gzip.decompress(response.read()).decode("utf-8")
    except OSError as exc:
        raise RepoError(f"could not read the published package index {url}: {exc}") from exc


def parse_packages_index(text: str) -> dict[tuple[str, str], list[PublishedPackage]]:
    published: dict[tuple[str, str], list[PublishedPackage]] = {}
    for stanza in re.split(r"\n\s*\n", text):
        fields = {}
        for line in stanza.splitlines():
            key, separator, value = line.partition(":")
            if separator and not line.startswith((" ", "\t")):
                fields[key] = value.strip()
        if {"Package", "Version", "Architecture", "SHA256"} <= fields.keys():
            published.setdefault((fields["Package"], fields["Architecture"]), []).append(
                PublishedPackage(fields["Version"], fields["SHA256"])
            )
    return published


def version_compare(left: str, operator: str, right: str) -> bool:
    return subprocess.run(["dpkg", "--compare-versions", left, operator, right], check=False).returncode == 0


def plan_publication(
    packages: list[InventoryPackage],
    published: dict[tuple[str, str], list[PublishedPackage]],
) -> PublicationPlan:
    plan = PublicationPlan()
    for package in packages:
        versions = published.get((package.name, package.architecture), [])
        same = [entry for entry in versions if version_compare(entry.version, "eq", package.version)]
        if same:
            if all(entry.sha256 == package.sha256 for entry in same):
                plan.unchanged.append(package)
            else:
                plan.rebuilt.append(package)
            continue
        newer = [entry.version for entry in versions if version_compare(entry.version, "gt", package.version)]
        if newer:
            newest = newer[0]
            for version in newer[1:]:
                if version_compare(version, "gt", newest):
                    newest = version
            plan.older_than_published.append((package, newest))
        else:
            plan.new.append(package)
    return plan


def main() -> int:
    args = parse_args()
    repo_root = find_repo_root(Path(__file__).resolve().parent)
    ensure_build(repo_root, clean=args.clean, no_build=args.no_build)
    packages = discover_packages(repo_root)
    print(
        f"Discovered {len(packages)} package(s) from {PACKAGE_INVENTORY_RELATIVE}"
    )
    entries = inventory_packages(repo_root)
    reject_third_party_names(repo_root, entries)
    url = published_index_url(repo_root)
    plan = plan_publication(entries, parse_packages_index(fetch_published_index(url)))
    for package, newest in plan.older_than_published:
        print(f"  refused: {package.name} {package.version} is older than published {newest}")
    if plan.older_than_published:
        raise RepoError(
            "the packages above are older than the published versions; publishing "
            "them would not replace what clients install. Rebuild from a current checkout"
        )
    print(
        f"{len(plan.new)} new or newer, {len(plan.rebuilt)} rebuilt under a published "
        f"version (replaced), {len(plan.unchanged)} identical to the published package (skipped)"
    )
    if not plan.upload:
        print("Nothing to upload")
        return 0
    for package in plan.upload:
        print(f"  {package.artifact.relative_to(repo_root)}")
    upload_packages(repo_root, sorted(package.artifact for package in plan.upload), dry_run=args.dry_run)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except RepoError as exc:
        print(f"error: {exc}", file=sys.stderr)
        raise SystemExit(1) from exc
