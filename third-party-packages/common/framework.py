"""Small, deterministic framework for external native MattOS packages.

Recipes run outside the MattOS build DAG.  They download verified upstream
source and build it only inside a disposable workspace in the MattOS builder
container (`mattos-build builder-image`), so packages link against the MattOS
libraries they will run with.  A finished .deb is published through the
existing vendored repository client.
"""

from __future__ import annotations

import argparse
import gzip
import hashlib
import importlib.util
import json
import os
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile
import time
import tomllib
import urllib.error
import urllib.request
from dataclasses import dataclass, field
from pathlib import Path
from typing import Iterable, Sequence
from urllib.request import Request, urlopen


VALID_REPOSITORIES = frozenset(("mattos", "mattpackages"))
PUBLISHER_RELATIVE = Path("src/infrastructure/LinuxScripts/GenericScripts/ManageMattOSRepository.py")
RECIPES_RELATIVE = Path("third-party-packages")
BUILDER_IMAGE_METADATA = Path("out/images/mattos-builder.json")
MATTOS_INVENTORY = Path("out/packages/inventory.toml")
# Third-party revisions sort below a MattOS-built package of the same upstream
# version (`-1mattos1`), so MattOS can always take a package over.
REVISION_PREFIX = "0mattos"
BUILD_INPUTS_FIELD = "X-MattOS-Build-Inputs"


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
    `revision` numbers packaging-only changes of the same upstream release.
    """

    version: str
    provenance: dict[str, str]
    revision: int = 1

    @property
    def package_version(self) -> str:
        return f"{self.version}-{REVISION_PREFIX}{self.revision}"


@dataclass(frozen=True)
class PublishedPackage:
    version: str
    build_inputs: str = ""


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
        revision = value.get("revision", 1)
        if not isinstance(revision, int) or isinstance(revision, bool) or revision < 1:
            raise RecipeError(f"release selection revision for {name} must be a positive integer")
        provenance = {key: item for key, item in value.items() if key not in ("version", "revision")}
        if not all(isinstance(key, str) and isinstance(item, str) and item for key, item in provenance.items()):
            raise RecipeError(f"release selection provenance for {name} must contain non-empty strings")
        sha256 = provenance.get("source_sha256")
        if sha256 is not None and not re.fullmatch(r"[0-9a-f]{64}", sha256):
            raise RecipeError(f"release selection source_sha256 for {name} must be 64 lowercase hex digits")
        selections[name] = ReleaseSelection(version, provenance, revision)
    return selections


def selected_release(root: Path, recipe: PackageRecipe) -> ReleaseSelection:
    try:
        return release_selections(root)[recipe.name]
    except KeyError as exc:
        raise RecipeError(
            f"recipe {recipe.name} has no selected release in third-party-packages/releases.json"
        ) from exc


def mattos_package_names(root: Path) -> frozenset[str]:
    """Package names the MattOS build itself produces."""
    path = root / MATTOS_INVENTORY
    try:
        inventory = tomllib.loads(path.read_text(encoding="utf-8"))
    except (OSError, tomllib.TOMLDecodeError) as exc:
        raise RecipeError(
            f"MattOS package inventory is unavailable ({path}); build MattOS first "
            "(the builder image is built from its packages)"
        ) from exc
    return frozenset(entry["name"] for entry in inventory.get("package", []))


def ensure_not_a_mattos_package(root: Path, recipe: PackageRecipe) -> None:
    """A package name belongs to exactly one producer: MattOS or one recipe."""
    if recipe.name in mattos_package_names(root):
        raise RecipeError(
            f"{recipe.name} is built by MattOS itself; a third-party recipe must not publish it"
        )


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
                  priority: str = "optional", build_inputs: str = "") -> None:
    control = staging / "DEBIAN/control"
    control.parent.mkdir(parents=True, exist_ok=True)
    lines = [
        "Package: " + name,
        "Version: " + version,
        "Section: " + section,
        "Priority: " + priority,
        "Architecture: " + architecture,
        "Maintainer: MattOS third-party packages <packages@mattos.invalid>",
    ]
    if depends:
        lines.append("Depends: " + ", ".join(depends))
    if provides:
        lines.append("Provides: " + ", ".join(provides))
    if build_inputs:
        lines.append(f"{BUILD_INPUTS_FIELD}: {build_inputs}")
    lines.append("Description: " + description)
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


def _is_elf(path: Path) -> bool:
    try:
        with path.open("rb") as handle:
            return handle.read(4) == b"\x7fELF"
    except OSError:
        return False


def elf_runtime_dependencies(staging: Path) -> list[str]:
    """MattOS packages owning every shared library the staged ELF files load.

    Runs inside the builder container, whose dpkg database is the MattOS
    package set: each library `ldd` resolves is mapped to its owning package
    with `dpkg-query -S`.  A library no package owns fails the build.
    """
    libraries: set[str] = set()
    for path in sorted(staging.rglob("*")):
        if path.is_symlink() or not path.is_file() or "DEBIAN" in path.parts or not _is_elf(path):
            continue
        result = subprocess.run(["ldd", str(path)], text=True, stdout=subprocess.PIPE,
                                stderr=subprocess.STDOUT, check=False)
        if "not a dynamic executable" in result.stdout:
            continue
        for line in result.stdout.splitlines():
            if "=> not found" in line:
                raise RecipeError(f"{path.relative_to(staging)} needs a library MattOS does not provide: {line.strip()}")
            match = re.search(r"=>\s+(/\S+)", line)
            if match:
                libraries.add(os.path.realpath(match.group(1)))
    packages: set[str] = set()
    for library in sorted(libraries):
        candidates = {library, library.replace("/usr/lib/", "/lib/", 1)}
        owner = ""
        for candidate in sorted(candidates):
            query = subprocess.run(["dpkg-query", "-S", candidate], text=True,
                                   stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, check=False)
            if query.returncode == 0:
                owner = query.stdout.split(":", 1)[0].split(",")[0].strip()
                break
        if not owner:
            raise RecipeError(f"no MattOS package owns {library}")
        packages.add(owner)
    return sorted(packages)


def finalize_package(recipe: PackageRecipe, staging: Path, workspace: Path,
                     version: str, provenance: dict[str, str]) -> BuildResult:
    """Render policy-owned metadata, provenance, and the canonical .deb.

    In the builder container the package's library dependencies come from its
    ELF files (`elf_runtime_dependencies`) in addition to the declared ones.
    """
    repository = validate_repository(recipe)
    provenance = {
        **provenance,
        "package": recipe.name,
        "version": version,
        "architecture": recipe.architecture,
        "repository": repository,
    }
    metadata = recipe.metadata(version)
    if os.environ.get("MATTOS_BUILDER_CONTAINER") == "1":
        metadata["depends"] = tuple(sorted(set(metadata["depends"]) | set(elf_runtime_dependencies(staging))))
    write_control(staging, **metadata, build_inputs=provenance.get("build_inputs", ""))
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


def make_build_install(source: Path, staging: Path, *, options: Sequence[str] = (),
                       install_options: Sequence[str] = ()) -> None:
    """Build and DESTDIR-install a project with its own Makefile."""
    require_tools(["make", "cc", "c++"])
    command(["make", "-j", str(os.cpu_count() or 1), *options], cwd=source)
    command(["make", *options, *install_options, f"DESTDIR={staging}", "install"], cwd=source)


class SourceReleaseRecipe(PackageRecipe):
    """A recipe that builds one verified upstream release archive.

    Subclasses declare where the archive lives and how it builds; any method
    can be overridden for an unusual package.  The archive is verified against
    the `source_sha256` pinned for the selected release in releases.json.
    """

    github: tuple[str, str] = ("", "")
    # Formatted with {version} and {tag} (the selected release_tag).
    source_url: str = ""
    build_system: str = "autotools"  # "autotools", "cmake" or "make"
    build_options: Sequence[str] = ()
    install_options: Sequence[str] = ()

    def discover_version(self) -> tuple[str, dict[str, str]]:
        owner, repository = self.github
        if not owner:
            raise NotImplementedError(f"{self.name} must set github or override discover_version")
        version, provenance = github_latest_release(owner, repository)
        if not re.fullmatch(r"[0-9]+(?:\.[0-9]+)*", version):
            raise RecipeError(f"{self.name} release API returned an invalid stable tag {version!r}")
        return version, provenance

    def source_archive_url(self, version: str, provenance: dict[str, str]) -> str:
        return self.source_url.format(version=version, tag=provenance.get("release_tag", version))

    def build_source(self, source: Path, workspace: Path, staging: Path) -> None:
        if self.build_system == "cmake":
            cmake_build_install(source, workspace / "build", staging, options=self.build_options)
        elif self.build_system == "autotools":
            autotools_build_install(source, workspace / "build", staging, options=self.build_options)
        elif self.build_system == "make":
            make_build_install(source, staging, options=self.build_options, install_options=self.install_options)
        else:
            raise RecipeError(f"{self.name} declares unknown build system {self.build_system!r}")

    def post_install(self, staging: Path, source: Path) -> None:
        """Adjust the staged payload before packaging (optional)."""

    def build(self, workspace: Path, version: str, provenance: dict[str, str]) -> BuildResult:
        upstream = provenance.get("upstream_version", version)
        url = self.source_archive_url(upstream, provenance)
        expected = provenance.get("source_sha256")
        if not expected:
            raise RecipeError(f"{self.name} has no source_sha256 pinned in releases.json")
        archive = workspace / url.rsplit("/", 1)[-1]
        download(url, archive, sha256=expected)
        source = extract_archive(archive, workspace / "source")
        staging = workspace / "package"
        self.build_source(source, workspace, staging)
        self.post_install(staging, source)
        return finalize_package(self, staging, workspace, version, {**provenance, "source_url": url})


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


def no_new_privileges() -> bool:
    try:
        return any(line.split() == ["NoNewPrivs:", "1"]
                   for line in Path("/proc/self/status").read_text(encoding="utf-8").splitlines())
    except OSError:
        return False


def ensure_rootless_engine_can_start(engine: str) -> None:
    """Rootless Podman needs the setuid newuidmap helper, which cannot gain
    privileges in a process with no_new_privs set (as every terminal inside
    some editor sessions is).  Say so instead of failing inside Podman."""
    if Path(engine).name == "podman" and os.getuid() != 0 and no_new_privileges():
        raise RecipeError(
            "rootless Podman cannot start here: this process has no_new_privs set "
            "(NoNewPrivs: 1 in /proc/self/status), which blocks newuidmap. It is "
            "inherited from the program that started this terminal, for example an "
            "editor launched from a URL handler. Run from a terminal where "
            "`grep NoNewPrivs /proc/self/status` prints 0."
        )


def builder_image(root: Path, engine: str) -> tuple[str, str]:
    """Build (or reuse) the MattOS builder image and make it available to the
    container engine.  Returns its reference and manifest digest."""
    ensure_rootless_engine_can_start(engine)
    command(["cargo", "run", "-q", "-p", "mattos-build", "--", "builder-image"], cwd=root)
    try:
        metadata = json.loads((root / BUILDER_IMAGE_METADATA).read_text(encoding="utf-8"))
        reference = str(metadata["reference"])
        digest = str(metadata["manifest_digest"])
        archive = root / str(metadata["archive"])
    except (OSError, KeyError, TypeError, json.JSONDecodeError) as exc:
        raise RecipeError(f"mattos-build produced no valid builder image metadata: {exc}") from exc
    present = subprocess.run([engine, "image", "exists", reference], check=False).returncode == 0
    if not present:
        command([engine, "load", "--input", str(archive)], cwd=root)
    return reference, digest


def recorded_builder_image_digest(root: Path) -> str:
    """The last built builder image's digest, without building anything."""
    try:
        return str(json.loads((root / BUILDER_IMAGE_METADATA).read_text(encoding="utf-8"))["manifest_digest"])
    except (OSError, KeyError, TypeError, json.JSONDecodeError):
        return ""


def build_inputs_digest(root: Path, script: Path, selection: ReleaseSelection, image_digest: str) -> str:
    """Identity of everything a package build consumes: the recipe, the shared
    framework, the pinned release selection and the builder image.  Equal
    digests mean a rebuild would produce the published package again."""
    digest = hashlib.sha256()
    for path in (script, Path(__file__).resolve()):
        digest.update(path.name.encode() + b"\0" + path.read_bytes() + b"\0")
    digest.update(json.dumps({"version": selection.version, "revision": selection.revision,
                              **selection.provenance}, sort_keys=True).encode())
    digest.update(b"\0" + image_digest.encode())
    return digest.hexdigest()


def build_in_container(root: Path, recipe: PackageRecipe, script: Path, workspace: Path,
                       version: str, provenance: dict[str, str],
                       image: tuple[str, str] | None = None) -> BuildResult:
    """Run recipe payload assembly in the MattOS builder container.

    The container sees only the disposable workspace (read-write) and the
    recipe directory (read-only): upstream build scripts cannot touch the rest
    of the repository.  The host performs repository lookup and publishing, so
    repository credentials never enter the build environment.
    """
    engine = container_engine()
    reference, _digest = image or builder_image(root, engine)
    request = workspace / "container-request.json"
    request.write_text(json.dumps({"version": version, "provenance": provenance}, sort_keys=True), encoding="utf-8")
    recipes = (root / RECIPES_RELATIVE).resolve()
    relative_script = script.resolve().relative_to(recipes)
    # Rootless Podman's container root maps to the calling host user. Docker
    # needs an explicit UID/GID to avoid leaving root-owned temporary files.
    user_args: list[str] = []
    if Path(engine).name != "podman":
        user_args = ["--user", f"{os.getuid()}:{os.getgid()}"]
    args = [
        engine, "run", "--rm", *user_args,
        "--volume", f"{workspace.resolve()}:/work:rw",
        "--volume", f"{recipes}:/recipes:ro",
        "--workdir", "/work",
        "--env", "PYTHONDONTWRITEBYTECODE=1",
        "--env", "MATTOS_BUILDER_CONTAINER=1",
        reference, "python3", f"/recipes/{relative_script.as_posix()}",
        "__container-build", "--workspace", "/work",
        "--request", "/work/container-request.json",
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


def _publisher_config(root: Path, repository: str):
    """The vendored publisher's own view of a repository (URL, suite, ...)."""
    spec = importlib.util.spec_from_file_location("mattos_repository_publisher", root / PUBLISHER_RELATIVE)
    if spec is None or spec.loader is None:
        raise RecipeError(f"publisher not found: {root / PUBLISHER_RELATIVE}")
    module = importlib.util.module_from_spec(spec)
    # Dataclasses in the publisher resolve their module through sys.modules.
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module.Config.from_env(repository)


def repository_index_url(root: Path, repository: str) -> str:
    config = _publisher_config(root, repository)
    return f"{config.public_url}/dists/{config.suite}/{config.component}/binary-amd64/Packages.gz"


def parse_packages_index(text: str) -> dict[str, list[PublishedPackage]]:
    published: dict[str, list[PublishedPackage]] = {}
    for stanza in re.split(r"\n\s*\n", text):
        fields = {}
        for line in stanza.splitlines():
            key, separator, value = line.partition(":")
            if separator and not line.startswith((" ", "\t")):
                fields[key] = value.strip()
        if "Package" in fields and "Version" in fields:
            published.setdefault(fields["Package"], []).append(
                PublishedPackage(fields["Version"], fields.get(BUILD_INPUTS_FIELD, ""))
            )
    return published


def repository_inventory(root: Path, repository: str) -> dict[str, list[PublishedPackage]]:
    """One snapshot of a repository's published packages: a single fetch of
    its public index, including each package's recorded build inputs."""
    if repository not in VALID_REPOSITORIES:
        raise RecipeError(f"unsupported repository {repository!r}")
    url = repository_index_url(root, repository)
    try:
        request = Request(url, headers={"User-Agent": "MattOS-third-party-packages/2"})
        with urlopen(request, timeout=60) as response:
            text = gzip.decompress(response.read()).decode("utf-8")
    except urllib.error.HTTPError as exc:
        if exc.code == 404:
            return {}
        raise RecipeError(f"could not read the {repository} package index {url}: {exc}") from exc
    except OSError as exc:
        raise RecipeError(f"could not read the {repository} package index {url}: {exc}") from exc
    return parse_packages_index(text)


def write_repository_inventory(path: Path, repository: str, inventory: dict[str, list[PublishedPackage]]) -> None:
    if repository not in VALID_REPOSITORIES:
        raise RecipeError(f"unsupported repository {repository!r}")
    path.write_text(json.dumps({
        "format": 2,
        "repository": repository,
        "packages": {
            name: [{"version": entry.version, "build_inputs": entry.build_inputs} for entry in entries]
            for name, entries in inventory.items()
        },
    }, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def read_repository_inventory(path: Path, repository: str) -> dict[str, list[PublishedPackage]]:
    try:
        document = json.loads(path.read_text(encoding="utf-8"))
        packages = document["packages"]
    except (OSError, KeyError, TypeError, json.JSONDecodeError) as exc:
        raise RecipeError(f"repository inventory is invalid ({path}): {exc}") from exc
    if (not isinstance(document, dict) or document.get("format") != 2
            or document.get("repository") != repository or not isinstance(packages, dict)):
        raise RecipeError(f"repository inventory is invalid ({path}): repository identity mismatch")
    inventory: dict[str, list[PublishedPackage]] = {}
    for package, entries in packages.items():
        try:
            inventory[package] = [PublishedPackage(str(entry["version"]), str(entry.get("build_inputs", "")))
                                  for entry in entries]
        except (KeyError, TypeError) as exc:
            raise RecipeError(f"repository inventory is invalid ({path}): malformed package versions") from exc
    return inventory


def published_packages(root: Path, package: str, repository: str,
                       inventory_path: Path | None = None) -> list[PublishedPackage]:
    inventory = (read_repository_inventory(inventory_path, repository)
                 if inventory_path else repository_inventory(root, repository))
    return inventory.get(package, [])


def repository_versions(root: Path, package: str, repository: str,
                        inventory_path: Path | None = None) -> list[str]:
    return [entry.version for entry in published_packages(root, package, repository, inventory_path)]


def newest_debian_version(versions: Sequence[str]) -> str | None:
    """Select a Debian version with dpkg's comparison semantics."""
    newest: str | None = None
    for version in versions:
        if newest is None:
            newest = version
        elif subprocess.run(["dpkg", "--compare-versions", version, "gt", newest], check=False).returncode == 0:
            newest = version
    return newest


def release_state(upstream: str, selected: str, published: Sequence[str],
                  *, selected_package_version: str | None = None,
                  published_inputs: str | None = None, build_inputs: str = "") -> tuple[str, str | None]:
    """Classify the upstream release, the selected release and the repository.

    `pending-publish`: the selected package version is not published.
    `rebuild-pending`: it is published, but from different build inputs.
    `upstream-newer`: published as built, but upstream has a newer release.
    """
    package_version = selected_package_version or selected
    repository = newest_debian_version(published)
    if package_version not in published:
        return "pending-publish", repository
    if build_inputs and published_inputs is not None and published_inputs != build_inputs:
        return "rebuild-pending", repository
    if upstream != selected:
        return "upstream-newer", repository
    if repository != package_version:
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
    version = selection.package_version
    if args.describe_json:
        print(json.dumps({
            "package": recipe.name,
            "repository": repository,
            "selected_version": version,
        }, sort_keys=True))
        return 0
    print(f"[{recipe.name}] mode: {args.command}", flush=True)
    print(f"[{recipe.name}] declared repository: {repository}", flush=True)
    print(f"[{recipe.name}] selected MattOS release: {selection.version} (package {version})", flush=True)
    print(f"[{recipe.name}] discovering upstream latest release...", flush=True)
    upstream_version, upstream_provenance = recipe.discover_version()
    provenance = {
        **upstream_provenance, **selection.provenance,
        "upstream_version": selection.version,
        "package": recipe.name, "version": version,
        "architecture": recipe.architecture, "repository": repository,
    }
    existing: list[PublishedPackage] = []
    if args.command in ("check", "update", "publish"):
        existing = published_packages(root, recipe.name, repository, args.repository_inventory)
    published_versions = [entry.version for entry in existing]
    published_inputs = next((entry.build_inputs for entry in existing if entry.version == version), None)
    image_digest = recorded_builder_image_digest(root)
    expected_inputs = build_inputs_digest(root, script, selection, image_digest) if image_digest else ""
    state, repository_version = release_state(
        upstream_version, selection.version, published_versions,
        selected_package_version=version, published_inputs=published_inputs, build_inputs=expected_inputs,
    )
    result = {
        "package": recipe.name, "upstream_version": upstream_version,
        "selected_version": version, "repository_version": repository_version,
        "published_versions": published_versions,
    }
    if args.command == "check":
        print(f"[{recipe.name}] upstream latest: {upstream_version}", flush=True)
        print(f"[{recipe.name}] published versions in {repository}: {', '.join(published_versions) or 'not published'}", flush=True)
        print(f"[{recipe.name}] release status: {state.replace('-', ' ')}", flush=True)
        print(f"[{recipe.name}] check complete; no build or upload performed", flush=True)
        if args.result_json:
            args.result_json.write_text(json.dumps({**result, "status": "checked", "release_state": state},
                                                   sort_keys=True) + "\n", encoding="utf-8")
        return 0
    if args.command != "build":
        ensure_not_a_mattos_package(root, recipe)
    engine = container_engine()
    image = builder_image(root, engine)
    build_inputs = build_inputs_digest(root, script, selection, image[1])
    if args.command == "update" and published_inputs == build_inputs:
        print(f"[{recipe.name}] {version} is already published from identical build inputs; nothing to do", flush=True)
        if args.result_json:
            args.result_json.write_text(json.dumps({**result, "status": "up-to-date"}, sort_keys=True) + "\n",
                                        encoding="utf-8")
        return 0
    provenance = {**provenance, "build_inputs": build_inputs, "builder_image": image[1]}
    print(f"[{recipe.name}] building {version} in the MattOS builder container", flush=True)
    output = (args.output or root / "third-party-packages" / "dist").resolve()
    if args.command == "build":
        output.mkdir(parents=True, exist_ok=True)
    try:
        recipe.preflight(root)
        (root / "out/tmp").mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(prefix=f"mattos-{recipe.name}-", dir=root / "out/tmp") as temporary:
            workspace = Path(temporary).resolve()
            result_build = build_in_container(root, recipe, script, workspace, version, provenance, image)
            artifact = result_build.artifact.resolve()
            if not artifact.is_relative_to(workspace):
                raise RecipeError("recipe artifact must remain inside its disposable workspace")
            validate_package_artifact(recipe, result_build)
            print(f"[{recipe.name}] package built: {result_build.artifact.name}", flush=True)
            print(f"[{recipe.name}] artifact SHA-256: {sha256_file(result_build.artifact)}", flush=True)
            if args.command in ("publish", "update"):
                print(f"[{recipe.name}] uploading to {repository}...", flush=True)
                publish(root, result_build.artifact, repository=repository, dry_run=args.dry_run)
                print(f"[{recipe.name}] upload command completed", flush=True)
            elif args.command == "build":
                destination = output / result_build.artifact.name
                shutil.copy2(result_build.artifact, destination)
                print(f"[{recipe.name}] local artifact saved: {destination}", flush=True)
    except FileNotFoundError as exc:
        raise RecipeError(f"out/tmp is required and the tool is missing: {exc}") from exc
    if args.result_json:
        status = "built" if args.command == "build" else ("dry-run" if args.dry_run else "uploaded")
        args.result_json.write_text(json.dumps({**result, "status": status}, sort_keys=True) + "\n", encoding="utf-8")
    return 0
