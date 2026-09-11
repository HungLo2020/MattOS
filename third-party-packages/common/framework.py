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
CONTAINERFILE_RELATIVE = Path("third-party-packages/Containerfile")
CONTAINER_IMAGE = "localhost/mattos-third-party-builder:1"


class RecipeError(RuntimeError):
    pass


@dataclass(frozen=True)
class BuildResult:
    package: str
    version: str
    architecture: str
    artifact: Path
    provenance: dict[str, str]


@dataclass(frozen=True)
class ReleaseSelection:
    """The deliberate MattOS release chosen for one recipe.

    Upstream discovery is informative: it tells maintainers whether a newer
    release exists.  This selection is authoritative for builds, so an
    ordinary check or update never silently changes what MattOS publishes.
    """

    version: str
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

    def preflight(self, root: Path) -> None:
        """Reject a recipe whose declared target closure is not available."""

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


def release_selections(root: Path) -> dict[str, ReleaseSelection]:
    """Load the checked-in, auditable third-party release selections."""
    path = root / "third-party-packages/releases.json"
    try:
        document = json.loads(path.read_text(encoding="utf-8"))
        packages = document["packages"]
    except (OSError, KeyError, TypeError, json.JSONDecodeError) as exc:
        raise RecipeError(f"release selections are invalid ({path}): {exc}") from exc
    if not isinstance(document, dict) or document.get("format") != 1 or not isinstance(packages, dict):
        raise RecipeError(f"release selections are invalid ({path}): expected format 1 package map")
    selections: dict[str, ReleaseSelection] = {}
    for name, value in packages.items():
        if not isinstance(name, str) or not isinstance(value, dict):
            raise RecipeError(f"release selections are invalid ({path}): malformed package entry")
        version = value.get("version")
        if not isinstance(version, str) or not version.strip():
            raise RecipeError(f"release selection for {name} must contain a non-empty version")
        provenance = {key: item for key, item in value.items() if key != "version"}
        if not all(isinstance(key, str) and isinstance(item, str) and item for key, item in provenance.items()):
            raise RecipeError(f"release selection provenance for {name} must contain non-empty strings")
        selections[name] = ReleaseSelection(version, provenance)
    return selections


def selected_release(root: Path, recipe: PackageRecipe) -> ReleaseSelection:
    try:
        return release_selections(root)[recipe.name]
    except KeyError as exc:
        raise RecipeError(
            f"recipe {recipe.name} has no selected release in third-party-packages/releases.json"
        ) from exc


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
        output = (getattr(exc, "stdout", "") or getattr(exc, "output", "")
                  or getattr(exc, "stderr", "") or "")
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
    require_tools(["cmake", "make", "cc", "c++"])
    command([
        "cmake", "-S", str(source), "-B", str(build),
        "-DCMAKE_BUILD_TYPE=Release", "-DCMAKE_INSTALL_PREFIX=/usr", *options,
    ])
    command(["cmake", "--build", str(build), "--parallel"])
    env = {**os.environ, "DESTDIR": str(staging)}
    command(["cmake", "--install", str(build)], env=env)


def autotools_build_install(source: Path, build: Path, staging: Path,
                            *, options: Sequence[str] = ()) -> None:
    """Build and DESTDIR-install a conventional Autotools release archive."""
    require_tools(["make", "cc", "c++", "sh"])
    configure = source / "configure"
    if not configure.is_file():
        require_tools(["autoreconf"])
        command(["autoreconf", "--install", "--force"], cwd=source)
    build.mkdir(parents=True, exist_ok=True)
    command([
        str(configure), "--prefix=/usr", "--disable-dependency-tracking", *options,
    ], cwd=build)
    command(["make", "-j", str(os.cpu_count() or 1)], cwd=build)
    command(["make", f"DESTDIR={staging}", "install"], cwd=build)


def container_engine() -> str:
    """Return the explicitly selected or first available rootless engine."""
    selected = os.environ.get("MATTOS_THIRD_PARTY_CONTAINER_ENGINE")
    if selected:
        if shutil.which(selected) is None:
            raise RecipeError(f"requested container engine is unavailable: {selected}")
        return selected
    for candidate in ("podman", "docker"):
        if shutil.which(candidate):
            return candidate
    raise RecipeError(
        "third-party builds require Podman or Docker; install one or set "
        "MATTOS_THIRD_PARTY_CONTAINER_ENGINE"
    )


def ensure_builder_image(root: Path, engine: str) -> None:
    containerfile = root / CONTAINERFILE_RELATIVE
    if not containerfile.is_file():
        raise RecipeError(f"third-party builder definition is missing: {containerfile}")
    command([
        engine, "build", "--tag", CONTAINER_IMAGE, "--file", str(containerfile), str(containerfile.parent),
    ], cwd=root)


def _container_workspace(root: Path, workspace: Path) -> str:
    relative = workspace.resolve().relative_to(root.resolve())
    return "/workspace/" + relative.as_posix()


def build_in_container(root: Path, recipe: PackageRecipe, script: Path, workspace: Path,
                       version: str, provenance: dict[str, str]) -> BuildResult:
    """Run recipe payload assembly in the pinned local builder image.

    The host performs repository lookup and publishing.  The container only
    receives source/build workspace access, so repository credentials never
    enter the build environment.
    """
    engine = container_engine()
    ensure_builder_image(root, engine)
    request = workspace / "container-request.json"
    request.write_text(json.dumps({"version": version, "provenance": provenance}, sort_keys=True), encoding="utf-8")
    relative_script = script.resolve().relative_to(root.resolve())
    container_workspace = _container_workspace(root, workspace)
    # Rootless Podman's container root maps to the calling host user. Docker
    # needs an explicit UID/GID to avoid leaving root-owned temporary files.
    user_args: list[str] = []
    if Path(engine).name != "podman":
        user_args = ["--user", f"{os.getuid()}:{os.getgid()}"]
    args = [
        engine, "run", "--rm", *user_args,
        "--volume", f"{root.resolve()}:/workspace:rw", "--workdir", "/workspace",
        CONTAINER_IMAGE, "python3", f"/workspace/{relative_script.as_posix()}",
        "__container-build", "--workspace", container_workspace,
        "--request", f"{container_workspace}/container-request.json",
    ]
    command(args, cwd=root)
    result_path = workspace / "container-result.json"
    try:
        data = json.loads(result_path.read_text(encoding="utf-8"))
        artifact = (workspace / str(data["artifact"])).resolve()
    except (OSError, KeyError, TypeError, json.JSONDecodeError) as exc:
        raise RecipeError("container build completed without a valid result manifest") from exc
    if not artifact.is_relative_to(workspace.resolve()) or not artifact.is_file():
        raise RecipeError("container build returned an invalid package artifact path")
    return BuildResult(recipe.name, version, recipe.architecture, artifact, provenance)


def validate_package_artifact(recipe: PackageRecipe, result: BuildResult) -> None:
    """Fail closed on a malformed or mismatched package before persistence/upload."""
    require_tools(["dpkg-deb"])
    fields = command([
        "dpkg-deb", "--show", "--showformat=${Package}\\n${Version}\\n${Architecture}\\n",
        str(result.artifact),
    ]).splitlines()
    expected = [recipe.name, result.version, recipe.architecture]
    if fields != expected:
        raise RecipeError(f"built package metadata mismatch: expected {expected}, got {fields}")
    contents = command(["dpkg-deb", "--contents", str(result.artifact)])
    provenance_path = f"./usr/share/mattos/third-party/{recipe.name}/provenance.json"
    if provenance_path not in contents:
        raise RecipeError(f"built package omitted package-scoped provenance: {provenance_path}")


def _write_container_result(recipe: PackageRecipe, workspace: Path,
                            version: str, provenance: dict[str, str]) -> int:
    result = recipe.build(workspace, version, provenance)
    artifact = result.artifact.resolve()
    if not artifact.is_relative_to(workspace.resolve()):
        raise RecipeError("recipe artifact must remain inside its disposable workspace")
    (workspace / "container-result.json").write_text(
        json.dumps({"artifact": artifact.relative_to(workspace.resolve()).as_posix()}, sort_keys=True),
        encoding="utf-8",
    )
    return 0


def repository_inventory(root: Path, repository: str) -> dict[str, list[str]]:
    """Return one verified package/version snapshot for a Matt repository."""
    if repository not in VALID_REPOSITORIES:
        raise RecipeError(f"unsupported repository {repository!r}")
    publisher = root / PUBLISHER_RELATIVE
    if not publisher.is_file():
        raise RecipeError(f"publisher not found: {publisher}")
    output = command([
        sys.executable, str(publisher), "--non-interactive", "--repo",
        repository, "list",
    ], cwd=root)
    inventory: dict[str, list[str]] = {}
    for line in output.splitlines():
        fields = line.split("\t", 2)
        if len(fields) != 3 or not fields[0] or not fields[1]:
            continue
        inventory.setdefault(fields[0], []).append(fields[1])
    return {package: sorted(set(versions)) for package, versions in inventory.items()}


def write_repository_inventory(path: Path, repository: str, inventory: dict[str, list[str]]) -> None:
    if repository not in VALID_REPOSITORIES:
        raise RecipeError(f"unsupported repository {repository!r}")
    path.write_text(json.dumps({
        "format": 1,
        "repository": repository,
        "packages": inventory,
    }, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def read_repository_inventory(path: Path, repository: str) -> dict[str, list[str]]:
    try:
        document = json.loads(path.read_text(encoding="utf-8"))
        packages = document["packages"]
    except (OSError, KeyError, TypeError, json.JSONDecodeError) as exc:
        raise RecipeError(f"repository inventory is invalid ({path}): {exc}") from exc
    if (not isinstance(document, dict) or document.get("format") != 1
            or document.get("repository") != repository or not isinstance(packages, dict)):
        raise RecipeError(f"repository inventory is invalid ({path}): repository identity mismatch")
    inventory: dict[str, list[str]] = {}
    for package, versions in packages.items():
        if not isinstance(package, str) or not isinstance(versions, list) or not all(
                isinstance(version, str) and version for version in versions):
            raise RecipeError(f"repository inventory is invalid ({path}): malformed package versions")
        inventory[package] = list(versions)
    return inventory


def repository_versions(root: Path, package: str, repository: str,
                        inventory_path: Path | None = None) -> list[str]:
    inventory = (read_repository_inventory(inventory_path, repository)
                 if inventory_path else repository_inventory(root, repository))
    return inventory.get(package, [])


def newest_debian_version(versions: Sequence[str]) -> str | None:
    """Select a Debian version with dpkg's comparison semantics."""
    newest: str | None = None
    for version in versions:
        if newest is None:
            newest = version
        elif subprocess.run(["dpkg", "--compare-versions", version, "gt", newest], check=False).returncode == 0:
            newest = version
    return newest


def release_state(upstream: str, selected: str, published: Sequence[str]) -> tuple[str, str | None]:
    """Classify the three independently meaningful package versions."""
    repository = newest_debian_version(published)
    if selected not in published:
        return "pending-publish", repository
    if upstream != selected:
        return "upstream-newer", repository
    if repository != selected:
        return "repository-diverged", repository
    return "up-to-date", repository


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
    parser.add_argument("command", nargs="?", choices=("check", "build", "publish", "update", "__container-build"), default="check")
    parser.add_argument("--dry-run", action="store_true", help="validate publication without uploading")
    parser.add_argument("--output", type=Path, help="local output directory for the .deb")
    parser.add_argument("--workspace", type=Path, help=argparse.SUPPRESS)
    parser.add_argument("--request", type=Path, help=argparse.SUPPRESS)
    parser.add_argument("--result-json", type=Path, help=argparse.SUPPRESS)
    parser.add_argument("--repository-inventory", type=Path, help=argparse.SUPPRESS)
    parser.add_argument("--describe-json", action="store_true", help=argparse.SUPPRESS)
    args = parser.parse_args(list(argv))
    if args.command == "__container-build":
        if not args.workspace or not args.request:
            raise RecipeError("container build requires --workspace and --request")
        try:
            request = json.loads(args.request.read_text(encoding="utf-8"))
            version = str(request["version"])
            provenance = dict(request["provenance"])
        except (OSError, KeyError, TypeError, json.JSONDecodeError) as exc:
            raise RecipeError(f"container build request is invalid ({args.request}): {exc}") from exc
        return _write_container_result(recipe, args.workspace, version, provenance)
    root = repo_root(script)
    repository = validate_repository(recipe)
    selection = selected_release(root, recipe)
    if args.describe_json:
        print(json.dumps({
            "package": recipe.name,
            "repository": repository,
            "selected_version": selection.version,
        }, sort_keys=True))
        return 0
    print(f"[{recipe.name}] mode: {args.command}", flush=True)
    print(f"[{recipe.name}] declared repository: {repository}", flush=True)
    print(
        f"[{recipe.name}] commands: check (default, read-only), "
        "build (local .deb), update (build and publish if new), "
        "publish (build and publish)",
        flush=True,
    )
    print(f"[{recipe.name}] selected MattOS release: {selection.version}", flush=True)
    print(f"[{recipe.name}] discovering upstream latest release...", flush=True)
    upstream_version, upstream_provenance = recipe.discover_version()
    version = selection.version
    provenance = {
        **upstream_provenance, **selection.provenance,
        "package": recipe.name, "version": version,
        "architecture": recipe.architecture, "repository": repository,
    }
    existing: list[str] = []
    if args.command in ("check", "update", "publish"):
        existing = repository_versions(root, recipe.name, repository, args.repository_inventory)
    state, repository_version = release_state(upstream_version, version, existing)
    if args.command == "check":
        published = ", ".join(existing) if existing else "not published"
        print(f"[{recipe.name}] upstream latest: {upstream_version}", flush=True)
        print(f"[{recipe.name}] MattOS selected: {version}", flush=True)
        print(f"[{recipe.name}] published versions in {repository}: {published}", flush=True)
        print(f"[{recipe.name}] newest published: {repository_version or 'not published'}", flush=True)
        print(f"[{recipe.name}] release status: {state.replace('-', ' ')}", flush=True)
        print(f"[{recipe.name}] check complete; no build or upload performed", flush=True)
        if args.result_json:
            args.result_json.write_text(json.dumps({
                "status": "checked", "package": recipe.name,
                "upstream_version": upstream_version, "selected_version": version,
                "repository_version": repository_version, "release_state": state,
                "published_versions": existing,
            }, sort_keys=True) + "\n", encoding="utf-8")
        return 0
    if args.command == "update" and version in existing:
        print(f"[{recipe.name}] selected release {version} is already published in {repository}; no build or upload needed", flush=True)
        if args.result_json:
            args.result_json.write_text(json.dumps({
                "status": state, "package": recipe.name,
                "upstream_version": upstream_version, "selected_version": version,
                "repository_version": repository_version, "published_versions": existing,
            }, sort_keys=True) + "\n", encoding="utf-8")
        return 0
    print(f"[{recipe.name}] selected release to build: {version}", flush=True)
    if args.command == "build":
        print(f"[{recipe.name}] starting local build; only the final .deb will be retained", flush=True)
    else:
        print(f"[{recipe.name}] version is not present in {repository}; starting disposable build", flush=True)
    output = (args.output or root / "third-party-packages" / "dist").resolve()
    if args.command == "build":
        output.mkdir(parents=True, exist_ok=True)
    try:
        recipe.preflight(root)
        (root / "out/tmp").mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(prefix=f"mattos-{recipe.name}-", dir=root / "out/tmp") as temporary:
            workspace = Path(temporary).resolve()
            result = build_in_container(root, recipe, script, workspace, version, provenance)
            artifact = result.artifact.resolve()
            if not artifact.is_relative_to(workspace):
                raise RecipeError("recipe artifact must remain inside its disposable workspace")
            validate_package_artifact(recipe, result)
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
    if args.result_json:
        status = "built" if args.command == "build" else ("dry-run" if args.dry_run else "uploaded")
        args.result_json.write_text(json.dumps({
            "status": status, "package": recipe.name,
            "upstream_version": upstream_version, "selected_version": version,
            "repository_version": repository_version, "published_versions": existing,
        }, sort_keys=True) + "\n", encoding="utf-8")
    return 0
