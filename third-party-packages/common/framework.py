"""Small, deterministic framework for external native MattOS packages.

Recipes run outside the MattOS build DAG.  They download and build only in a
temporary directory, then publish a finished .deb through the existing
vendored repository client.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile
import time
import urllib.error
import urllib.request
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable, Sequence
from urllib.request import Request, urlopen


VALID_REPOSITORIES = frozenset(("mattos", "mattpackages"))
PUBLISHER_RELATIVE = Path("src/infrastructure/LinuxScripts/GenericScripts/ManageMattOSRepository.py")


class RecipeError(RuntimeError):
    pass


@dataclass(frozen=True)
class BuildResult:
    package: str
    version: str
    architecture: str
    artifact: Path
    provenance: dict[str, str]


class PackageRecipe:
    """Recipe interface implemented by one small package-specific module."""

    name: str
    repository: str
    architecture: str = "amd64"
    section: str = "utils"
    priority: str = "optional"
    description: str = ""
    depends: Sequence[str] = ()
    provides: Sequence[str] = ()

    def discover_version(self) -> tuple[str, dict[str, str]]:
        raise NotImplementedError

    def build(self, workspace: Path, version: str, provenance: dict[str, str]) -> BuildResult:
        raise NotImplementedError

    def dependency_names(self) -> Sequence[str]:
        return self.depends

    def metadata(self, version: str) -> dict[str, str | Sequence[str]]:
        return {
            "name": self.name,
            "version": version,
            "architecture": self.architecture,
            "section": self.section,
            "priority": self.priority,
            "description": self.description,
            "depends": tuple(self.dependency_names()),
            "provides": tuple(self.provides),
        }


def validate_repository(recipe: PackageRecipe) -> str:
    repository = getattr(recipe, "repository", None)
    if not isinstance(repository, str) or not repository:
        raise RecipeError(
            f"recipe {getattr(recipe, 'name', '<unnamed>')} must declare a non-empty repository"
        )
    if repository not in VALID_REPOSITORIES:
        allowed = ", ".join(sorted(VALID_REPOSITORIES))
        raise RecipeError(
            f"recipe {recipe.name} declares unsupported repository {repository!r}; "
            f"expected one of: {allowed}"
        )
    return repository


def repo_root(script: Path) -> Path:
    for candidate in (script.parent, *script.parents):
        if (candidate / "Cargo.toml").is_file() and (candidate / "upstream/sources.toml").is_file():
            return candidate
    raise RecipeError(f"cannot locate MattOS repository root from {script}")


def require_tools(names: Iterable[str]) -> None:
    missing = [name for name in names if shutil.which(name) is None]
    if missing:
        raise RecipeError("missing required tools: " + ", ".join(missing))


def command(args: Sequence[str], *, cwd: Path | None = None, env: dict[str, str] | None = None) -> str:
    try:
        result = subprocess.run(
            list(args), cwd=cwd, env=env, check=True, text=True,
            stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
        )
    except (OSError, subprocess.CalledProcessError) as exc:
        output = getattr(exc, "stdout", "") or ""
        raise RecipeError(f"command failed: {' '.join(args)}\n{output}") from exc
    return result.stdout


def download(url: str, destination: Path, *, sha256: str | None = None) -> None:
    destination.parent.mkdir(parents=True, exist_ok=True)
    request = urllib.request.Request(url, headers={"User-Agent": "MattOS-third-party-packages/1"})
    last: Exception | None = None
    for attempt in range(1, 4):
        try:
            with urllib.request.urlopen(request, timeout=60) as response, destination.open("wb") as out:
                shutil.copyfileobj(response, out, length=1024 * 1024)
            break
        except (OSError, urllib.error.URLError) as exc:
            last = exc
            if attempt == 3:
                raise RecipeError(f"download failed after retries: {url}") from exc
    if sha256:
        actual = hashlib.sha256(destination.read_bytes()).hexdigest()
        if actual.lower() != sha256.lower():
            raise RecipeError(f"checksum mismatch for {url}: expected {sha256}, got {actual}")


def fetch_json(url: str, *, headers: dict[str, str] | None = None, attempts: int = 3) -> dict:
    request_headers = {"User-Agent": "MattOS-third-party-packages/1"}
    if headers:
        request_headers.update(headers)
    last: Exception | None = None
    for attempt in range(1, attempts + 1):
        try:
            with urlopen(Request(url, headers=request_headers), timeout=30) as response:
                value = json.load(response)
            if not isinstance(value, dict):
                raise RecipeError(f"upstream API returned a non-object response: {url}")
            return value
        except (OSError, urllib.error.URLError, json.JSONDecodeError, RecipeError) as exc:
            last = exc
            if attempt < attempts:
                time.sleep(attempt)
    raise RecipeError(f"upstream API request failed after {attempts} attempts: {url}: {last}") from last


def extract_archive(archive: Path, destination: Path) -> Path:
    destination.mkdir(parents=True, exist_ok=True)
    with tarfile.open(archive, "r:*") as tar:
        members = tar.getmembers()
        for member in members:
            target = (destination / member.name).resolve()
            if not target.is_relative_to(destination.resolve()):
                raise RecipeError(f"archive contains path traversal: {member.name}")
        tar.extractall(destination, filter="data")
    roots = [path for path in destination.iterdir() if path.is_dir()]
    if len(roots) == 1:
        return roots[0]
    return destination


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def write_control(staging: Path, *, name: str, version: str, description: str,
                  depends: Sequence[str], provides: Sequence[str] = (),
                  architecture: str = "amd64", section: str = "utils",
                  priority: str = "optional") -> None:
    control = staging / "DEBIAN/control"
    control.parent.mkdir(parents=True, exist_ok=True)
    lines = [
        "Package: " + name,
        "Version: " + version,
        "Section: " + section,
        "Priority: " + priority,
        "Architecture: " + architecture,
        "Maintainer: MattOS third-party packages <packages@mattos.local>",
        "Description: " + description,
    ]
    if depends:
        lines.insert(5, "Depends: " + ", ".join(depends))
    if provides:
        lines.insert(6 if depends else 5, "Provides: " + ", ".join(provides))
    control.write_text("\n".join(lines) + "\n", encoding="utf-8")


def write_provenance(staging: Path, package: str, provenance: dict[str, str]) -> None:
    path = staging / "usr/share/mattos/third-party" / package / "provenance.json"
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(provenance, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def package_staging(staging: Path, output: Path, *, name: str, version: str, architecture: str = "amd64") -> Path:
    output.mkdir(parents=True, exist_ok=True)
    if architecture != "amd64":
        raise RecipeError(f"unsupported package architecture: {architecture}")
    artifact = output / f"{name}_{version}_{architecture}.deb"
    command(["dpkg-deb", "--root-owner-group", "--build", str(staging), str(artifact)])
    return artifact


def finalize_package(recipe: PackageRecipe, staging: Path, workspace: Path,
                     version: str, provenance: dict[str, str]) -> BuildResult:
    """Render policy-owned metadata, provenance, and the canonical .deb."""
    repository = validate_repository(recipe)
    provenance = {
        **provenance,
        "package": recipe.name,
        "version": version,
        "architecture": recipe.architecture,
        "repository": repository,
    }
    write_control(staging, **recipe.metadata(version))
    write_provenance(staging, recipe.name, provenance)
    artifact = package_staging(
        staging, workspace, name=recipe.name, version=version,
        architecture=recipe.architecture,
    )
    return BuildResult(recipe.name, version, recipe.architecture, artifact, provenance)


def github_latest_release(owner: str, repository: str) -> tuple[str, dict[str, str]]:
    url = f"https://api.github.com/repos/{owner}/{repository}/releases/latest"
    release = fetch_json(url, headers={"Accept": "application/vnd.github+json"})
    tag = str(release.get("tag_name", ""))
    if not tag:
        raise RecipeError(f"GitHub release API returned no stable tag: {url}")
    version = tag[1:] if tag.startswith("v") else tag
    return version, {
        "upstream": f"https://github.com/{owner}/{repository}",
        "release_tag": tag,
        "release_api": url,
    }


def github_source_archive(owner: str, repository: str, release_tag: str) -> str:
    return f"https://github.com/{owner}/{repository}/archive/refs/tags/{release_tag}.tar.gz"


def cmake_build_install(source: Path, build: Path, staging: Path,
                        *, options: Sequence[str] = ()) -> None:
    """Configure, build, and DESTDIR-install a conventional CMake project."""
    require_tools(["cmake", "make", "cc"])
    command([
        "cmake", "-S", str(source), "-B", str(build),
        "-DCMAKE_BUILD_TYPE=Release", "-DCMAKE_INSTALL_PREFIX=/usr", *options,
    ])
    command(["cmake", "--build", str(build), "--parallel"])
    env = {**os.environ, "DESTDIR": str(staging)}
    command(["cmake", "--install", str(build)], env=env)


def repository_versions(root: Path, package: str, repository: str) -> list[str]:
    if repository not in VALID_REPOSITORIES:
        raise RecipeError(f"unsupported repository {repository!r}")
    publisher = root / PUBLISHER_RELATIVE
    if not publisher.is_file():
        raise RecipeError(f"publisher not found: {publisher}")
    output = command([
        sys.executable, str(publisher), "--non-interactive", "--repo",
        repository, "list",
    ], cwd=root)
    return [line.split("\t", 2)[1] for line in output.splitlines()
            if line.startswith(package + "\t") and len(line.split("\t", 2)) == 3]


def publish(root: Path, artifact: Path, *, repository: str, dry_run: bool) -> None:
    if repository not in VALID_REPOSITORIES:
        raise RecipeError(f"unsupported repository {repository!r}")
    publisher = root / PUBLISHER_RELATIVE
    args = [sys.executable, str(publisher), "--non-interactive"]
    if dry_run:
        args.append("--dry-run")
    args += ["--repo", repository, "upload", str(artifact)]
    command(args, cwd=root)


def run_recipe(recipe: PackageRecipe, argv: Sequence[str], script: Path) -> int:
    parser = argparse.ArgumentParser(description=f"Maintain the MattOS {recipe.name} package")
    parser.add_argument("command", nargs="?", choices=("check", "build", "publish", "update"), default="check")
    parser.add_argument("--dry-run", action="store_true", help="validate publication without uploading")
    parser.add_argument("--output", type=Path, help="local output directory for the .deb")
    args = parser.parse_args(list(argv))
    root = repo_root(script)
    repository = validate_repository(recipe)
    print(f"[{recipe.name}] mode: {args.command}", flush=True)
    print(f"[{recipe.name}] declared repository: {repository}", flush=True)
    print(
        f"[{recipe.name}] commands: check (default, read-only), "
        "build (local .deb), update (build and publish if new), "
        "publish (build and publish)",
        flush=True,
    )
    if args.command == "build":
        print(f"[{recipe.name}] repository lookup: skipped (local build)", flush=True)
    else:
        print(f"[{recipe.name}] discovering upstream version and checking {repository}...", flush=True)
    version, provenance = recipe.discover_version()
    provenance = {
        **provenance, "package": recipe.name, "version": version,
        "architecture": recipe.architecture, "repository": repository,
    }
    existing: list[str] = []
    if args.command in ("check", "update", "publish"):
        existing = repository_versions(root, recipe.name, repository)
    if args.command == "check":
        published = ", ".join(existing) if existing else "not published"
        print(f"[{recipe.name}] upstream version: {version}", flush=True)
        print(f"[{recipe.name}] published versions in {repository}: {published}", flush=True)
        print(f"[{recipe.name}] check complete; no build or upload performed", flush=True)
        return 0
    if args.command in ("update", "publish") and version in existing:
        print(f"[{recipe.name}] version {version} is already published in {repository}; no build or upload needed", flush=True)
        return 0
    print(f"[{recipe.name}] upstream version selected: {version}", flush=True)
    if args.command == "build":
        print(f"[{recipe.name}] starting local build; only the final .deb will be retained", flush=True)
    else:
        print(f"[{recipe.name}] version is not present in {repository}; starting disposable build", flush=True)
    output = (args.output or root / "third-party-packages" / "dist").resolve()
    if args.command == "build":
        output.mkdir(parents=True, exist_ok=True)
    try:
        (root / "out/tmp").mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(prefix=f"mattos-{recipe.name}-", dir=root / "out/tmp") as temporary:
            workspace = Path(temporary).resolve()
            result = recipe.build(workspace, version, provenance)
            artifact = result.artifact.resolve()
            if not artifact.is_relative_to(workspace):
                raise RecipeError("recipe artifact must remain inside its disposable workspace")
            print(f"[{recipe.name}] package built: {result.artifact.name}", flush=True)
            print(f"[{recipe.name}] artifact SHA-256: {sha256_file(result.artifact)}", flush=True)
            if args.command in ("publish", "update"):
                print(f"[{recipe.name}] uploading to {repository}...", flush=True)
                publish(root, result.artifact, repository=repository, dry_run=args.dry_run)
                print(f"[{recipe.name}] upload command completed", flush=True)
            elif args.command == "build":
                destination = output / result.artifact.name
                shutil.copy2(result.artifact, destination)
                print(f"[{recipe.name}] local artifact saved: {destination}", flush=True)
    except FileNotFoundError as exc:
        raise RecipeError(f"out/tmp is required and the tool is missing: {exc}") from exc
    return 0
