#!/usr/bin/env python3
"""Build MattOS and upload every generated binary package.

The build invocation intentionally reuses the same helper as ``run_qemu.py``.
Package selection is discovered from the canonical build output directory so
new package definitions do not require edits to this script.

Before uploading, each package is compared with the published repository
index.  Builds are reproducible, so a package whose version and SHA-256 both
match the published one is truly unchanged and is skipped; a new or newer
version is uploaded.  Different bytes are never uploaded under a version that
is already published, since installed systems would never upgrade to them:
the package's packaging revision (`<upstream>-1mattos<N>`, recorded in
src/system/packages/revisions.toml) is raised instead, together with every
published package that depends on it with an exact version, the packages are
rebuilt, and the new versions are uploaded.  A version older than the
published one is refused.

Repository requests that fail for a transient reason (the server unreachable,
a timeout, HTTP 5xx) are retried ten times, ten seconds apart; a rejection
fails at once.
"""

from __future__ import annotations

import argparse
import gzip
import json
import re
import subprocess
import sys
import time
import tomllib
import urllib.error
import urllib.request
from dataclasses import dataclass, field
from pathlib import Path

import run_qemu
from common import RepoError, find_repo_root


PACKAGE_OUTPUT_RELATIVE = Path("out/packages/amd64")
PACKAGE_INVENTORY_RELATIVE = Path("out/packages/inventory.toml")
PUBLISHER_RELATIVE = Path(
    "src/infrastructure/LinuxScripts/GenericScripts/ManageMattOSRepository.py"
)
REPOSITORY_SOURCES_RELATIVE = Path("src/system/packages/config/apt/mattos-hosted.sources")
THIRD_PARTY_RELEASES_RELATIVE = Path("third-party-packages/releases.json")
REVISION_LEDGER_RELATIVE = Path("src/system/packages/revisions.toml")
MATTOS_VERSION = re.compile(r"^(?P<upstream>.+)-1mattos(?P<revision>[1-9][0-9]*)$")
EXACT_DEPENDENCY = re.compile(r"^(?P<name>[a-z0-9][a-z0-9+.-]*) \(= (?P<version>[^)]+)\)$")
# Bumping a package and the packages that pin it exactly settles in one round;
# a second is allowed for safety before suspecting nondeterministic packaging.
MAX_BUMP_ROUNDS = 3
REPOSITORY_RETRIES = 10
REPOSITORY_RETRY_DELAY = 10
# Publisher output that means the server could not be reached or failed
# while handling the request, rather than rejecting it.
TRANSIENT_PUBLISHER_OUTPUT = re.compile(
    r"server is unreachable|ConnectionResetError|ConnectionRefusedError|TimeoutError|timed out"
    r"|RemoteDisconnected|BrokenPipeError|IncompleteRead|returned invalid JSON"
)


class TransientRepositoryError(RepoError):
    """A repository request failed in a way a later attempt may not."""


def transient_http_status(code: int) -> bool:
    return code >= 500 or code in (408, 429)


def transient_publisher_failure(output: str) -> bool:
    """The publisher reports every server problem as a remote error, so the
    HTTP status in its message decides: 4xx (but 408 and 429) is a refusal."""
    status = re.search(r"returned HTTP (\d{3})", output)
    if status:
        return transient_http_status(int(status.group(1)))
    return bool(TRANSIENT_PUBLISHER_OUTPUT.search(output))


def with_repository_retries(action, what: str):
    """Run `action`, retrying a TransientRepositoryError REPOSITORY_RETRIES
    times REPOSITORY_RETRY_DELAY seconds apart before giving up."""
    for attempt in range(REPOSITORY_RETRIES + 1):
        try:
            return action()
        except TransientRepositoryError as exc:
            if attempt == REPOSITORY_RETRIES:
                raise RepoError(f"{exc} (gave up after {REPOSITORY_RETRIES} retries)") from exc
            print(f"{what} failed ({exc}); retry {attempt + 1}/{REPOSITORY_RETRIES} "
                  f"in {REPOSITORY_RETRY_DELAY} s", flush=True)
            time.sleep(REPOSITORY_RETRY_DELAY)
    raise AssertionError("unreachable")


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
        help="validate and print the upload command without uploading (and without raising revisions)",
    )
    parser.add_argument(
        "--no-bump",
        action="store_true",
        help="refuse, instead of raising packaging revisions, when a package changed under a published version",
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

    def upload() -> None:
        print("+", " ".join(command), flush=True)
        try:
            completed = subprocess.run(command, cwd=repo_root, text=True, check=False,
                                       stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        except OSError as exc:
            raise RepoError(f"failed to execute the publisher: {exc}") from exc
        print(completed.stdout, end="", flush=True)
        if completed.returncode:
            lines = [line.strip() for line in completed.stdout.splitlines() if line.strip()]
            reason = lines[-1] if lines else f"exit code {completed.returncode}"
            # Re-uploading a package with the same bytes is harmless, so the
            # whole batch is simply repeated.
            error = (TransientRepositoryError if transient_publisher_failure(completed.stdout) else RepoError)
            raise error(f"upload failed: {reason}")

    with_repository_retries(upload, "uploading packages")


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
        # A package rebuilt under a published version is never uploaded; it
        # needs a higher revision first (see `bump_targets`).
        return list(self.new)


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
    def fetch() -> str:
        try:
            # The CDN refuses urllib's default User-Agent.
            request = urllib.request.Request(url, headers={"User-Agent": "MattOS-PublishPackages/1"})
            with urllib.request.urlopen(request, timeout=60) as response:
                return gzip.decompress(response.read()).decode("utf-8")
        except urllib.error.HTTPError as exc:
            error = TransientRepositoryError if transient_http_status(exc.code) else RepoError
            raise error(f"could not read the published package index {url}: {exc}") from exc
        except OSError as exc:
            raise TransientRepositoryError(f"could not read the published package index {url}: {exc}") from exc

    return with_repository_retries(fetch, "reading the published package index")


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


def split_mattos_version(version: str) -> tuple[str, int]:
    """(upstream, revision) of a `<upstream>-1mattos<N>` version."""
    match = MATTOS_VERSION.match(version)
    if not match:
        raise RepoError(f"{version} is not a MattOS package version (<upstream>-1mattos<N>)")
    return match["upstream"], int(match["revision"])


def bump_targets(plan: PublicationPlan, packages: list[InventoryPackage]) -> list[InventoryPackage]:
    """The packages that need a higher packaging revision: each package whose
    bytes changed under its already-published version, and every package
    whose version is published and that depends on one of them with an exact
    version (its Depends changes with the new version, so its bytes do too)."""
    published = {package.name for package in plan.rebuilt + plan.unchanged}
    targets = {package.name for package in plan.rebuilt}
    grew = True
    while grew:
        grew = False
        for package in packages:
            if package.name in targets or package.name not in published:
                continue
            for dependency in package.dependencies:
                match = EXACT_DEPENDENCY.match(dependency.strip())
                if match and match["name"] in targets:
                    targets.add(package.name)
                    grew = True
                    break
    return sorted((package for package in packages if package.name in targets), key=lambda package: package.name)


def raise_revisions(repo_root: Path, packages: list[InventoryPackage]) -> list[tuple[str, str, str]]:
    """Record revision N+1 for each package in the revision ledger, keeping its
    header and other entries.  Returns (name, old version, new version)."""
    path = repo_root / REVISION_LEDGER_RELATIVE
    text = path.read_text(encoding="utf-8")
    ledger = tomllib.loads(text)
    if ledger.get("schema_version") != 1:
        raise RepoError(f"{REVISION_LEDGER_RELATIVE} has an unsupported schema_version")
    entries = {name: dict(entry) for name, entry in ledger.get("package", {}).items()}
    changes = []
    for package in packages:
        upstream, revision = split_mattos_version(package.version)
        entries[package.name] = {"upstream": upstream, "revision": revision + 1}
        changes.append((package.name, package.version, f"{upstream}-1mattos{revision + 1}"))
    lines = text.splitlines()
    try:
        header = lines[: lines.index("[package]") + 1]
    except ValueError as exc:
        raise RepoError(f"{REVISION_LEDGER_RELATIVE} lacks its [package] table") from exc
    body = [
        f'{json.dumps(name)} = {{ upstream = {json.dumps(entry["upstream"])}, revision = {entry["revision"]} }}'
        for name, entry in sorted(entries.items())
    ]
    path.write_text("\n".join(header + body) + "\n", encoding="utf-8")
    return changes


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
    published = parse_packages_index(fetch_published_index(url))
    for round_number in range(MAX_BUMP_ROUNDS + 1):
        plan = plan_publication(entries, published)
        for package, newest in plan.older_than_published:
            print(f"  refused: {package.name} {package.version} is older than published {newest}")
        if plan.older_than_published:
            raise RepoError(
                "the packages above are older than the published versions; publishing "
                "them would not replace what clients install. Rebuild from a current checkout "
                f"(is {REVISION_LEDGER_RELATIVE} up to date and committed?)"
            )
        if not plan.rebuilt:
            break
        targets = bump_targets(plan, entries)
        print(
            f"{len(plan.rebuilt)} package(s) changed under an already-published version; "
            f"raising the packaging revision of {len(targets)} (with their exact dependents):"
        )
        for package in targets:
            upstream, revision = split_mattos_version(package.version)
            reason = "changed" if package in plan.rebuilt else "pins a changed package exactly"
            print(f"  {package.name}: {package.version} -> {upstream}-1mattos{revision + 1} ({reason})")
        if args.no_bump:
            raise RepoError(
                "refusing to upload different bytes under published versions (--no-bump); "
                "rerun without --no-bump to raise their revisions"
            )
        if args.dry_run:
            print(f"Dry run: {REVISION_LEDGER_RELATIVE} is not changed and nothing is uploaded")
            return 0
        if round_number == MAX_BUMP_ROUNDS:
            raise RepoError(
                f"packages still changed under published versions after {MAX_BUMP_ROUNDS} revision "
                "rounds; their packaging may not be reproducible"
            )
        raise_revisions(repo_root, targets)
        print(f"Recorded the new revisions in {REVISION_LEDGER_RELATIVE} (commit it); rebuilding the packages")
        ensure_build(repo_root, clean=False, no_build=False)
        entries = inventory_packages(repo_root)
        reject_third_party_names(repo_root, entries)
    print(
        f"{len(plan.new)} new or newer, {len(plan.unchanged)} identical to the published package (skipped)"
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
