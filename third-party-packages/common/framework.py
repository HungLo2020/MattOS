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
import urllib.error
import urllib.request
from dataclasses import dataclass
from pathlib import Path
from typing import Callable, Iterable, Sequence


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
    architecture: str = "amd64"

    def discover_version(self) -> tuple[str, dict[str, str]]:
        raise NotImplementedError

    def build(self, workspace: Path, version: str, provenance: dict[str, str]) -> BuildResult:
        raise NotImplementedError

    def dependency_names(self) -> Sequence[str]:
        return ()


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
    print("+", " ".join(str(arg) for arg in args))
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
                  depends: Sequence[str], provides: Sequence[str] = ()) -> None:
    control = staging / "DEBIAN/control"
    control.parent.mkdir(parents=True, exist_ok=True)
    lines = [
        "Package: " + name,
        "Version: " + version,
        "Section: web" if name == "firefox" else "Section: utils",
        "Priority: optional",
        "Architecture: amd64",
        "Maintainer: MattOS third-party packages <packages@mattos.local>",
        "Description: " + description,
    ]
    if depends:
        lines.insert(5, "Depends: " + ", ".join(depends))
    if provides:
        lines.insert(6 if depends else 5, "Provides: " + ", ".join(provides))
    control.write_text("\n".join(lines) + "\n", encoding="utf-8")


def write_provenance(staging: Path, provenance: dict[str, str]) -> None:
    path = staging / "usr/share/mattos/third-party/provenance.json"
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(provenance, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def package_staging(staging: Path, output: Path, *, name: str, version: str, architecture: str = "amd64") -> Path:
    output.mkdir(parents=True, exist_ok=True)
    if architecture != "amd64":
        raise RecipeError(f"unsupported package architecture: {architecture}")
    artifact = output / f"{name}_{version}_{architecture}.deb"
    command(["dpkg-deb", "--root-owner-group", "--build", str(staging), str(artifact)])
    return artifact


def repository_versions(root: Path, package: str) -> list[str]:
    publisher = root / "src/infrastructure/LinuxScripts/GenericScripts/ManageMattOSRepository.py"
    if not publisher.is_file():
        raise RecipeError(f"publisher not found: {publisher}")
    try:
        output = command([sys.executable, str(publisher), "--non-interactive", "list"], cwd=root)
    except RecipeError:
        return []
    return [line.split("\t", 2)[1] for line in output.splitlines()
            if line.startswith(package + "\t") and len(line.split("\t", 2)) == 3]


def publish(root: Path, artifact: Path, *, dry_run: bool) -> None:
    publisher = root / "src/infrastructure/LinuxScripts/GenericScripts/ManageMattOSRepository.py"
    args = [sys.executable, str(publisher), "--non-interactive"]
    if dry_run:
        args.append("--dry-run")
    args += ["upload", str(artifact)]
    command(args, cwd=root)


def run_recipe(recipe: PackageRecipe, argv: Sequence[str], script: Path) -> int:
    parser = argparse.ArgumentParser(description=f"Maintain the MattOS {recipe.name} package")
    parser.add_argument("command", nargs="?", choices=("check", "build", "publish", "update"), default="update")
    parser.add_argument("--dry-run", action="store_true", help="validate publication without uploading")
    parser.add_argument("--output", type=Path, help="local output directory for the .deb")
    args = parser.parse_args(list(argv))
    root = repo_root(script)
    version, provenance = recipe.discover_version()
    provenance = {**provenance, "package": recipe.name, "version": version, "architecture": recipe.architecture}
    existing = repository_versions(root, recipe.name)
    if args.command in ("check", "update") and version in existing:
        print(f"{recipe.name} {version} is already published; skipping rebuild")
        return 0
    if args.command == "check":
        print(f"{recipe.name}: upstream={version}; repository={'none' if not existing else ','.join(existing)}")
        return 0
    output = (args.output or root / "third-party-packages" / "dist").resolve()
    output.mkdir(parents=True, exist_ok=True)
    try:
        (root / "out/tmp").mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(prefix=f"mattos-{recipe.name}-", dir=root / "out/tmp") as temporary:
            result = recipe.build(Path(temporary), version, provenance)
            print(f"built {result.artifact} sha256={sha256_file(result.artifact)}")
            if args.command in ("publish", "update"):
                publish(root, result.artifact, dry_run=args.dry_run)
    except FileNotFoundError as exc:
        raise RecipeError(f"out/tmp is required and the tool is missing: {exc}") from exc
    return 0
