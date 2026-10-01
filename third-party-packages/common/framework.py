"""Small, deterministic framework for external native MattOS packages.

Recipes run outside the MattOS build DAG.  They download verified upstream
source and build it only inside a disposable workspace in the MattOS builder
container (`mattos-build builder-image`), so packages link against the MattOS
libraries they will run with.  A finished .deb is published through the
existing vendored repository client.

This module is the host side: release selection, the repository inventory,
the build fingerprints, starting the container and publishing.  The code
that runs inside the build lives in `build.py` (and `go.py`), and only that
code is part of a package's build inputs.
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
import tempfile
import time
import tomllib
import urllib.error
from dataclasses import dataclass
from pathlib import Path
from typing import Sequence
from urllib.request import Request, urlopen

from . import build as build_module
from . import go as go_module
from .build import *  # noqa: F401,F403  (re-exported for recipes and tools)
from .build import (
    BUILD_ENVIRONMENT_FIELD, BUILD_INPUTS_FIELD, THIRD_PARTY_DEPENDENCY_DIR, VALID_REPOSITORIES,
    BuildResult, PackageRecipe, RecipeError, build_environment_digest, command, container_build,
    download, require_tools, sha256_file, validate_repository,
)
from .discovery import fetch_json, git_latest_tag, github_latest_release, version_key  # noqa: F401
from .go import GO_SHA256, GO_VERSION, go_environment  # noqa: F401


PUBLISHER_RELATIVE = Path("src/infrastructure/LinuxScripts/GenericScripts/ManageMattOSRepository.py")


RECIPES_RELATIVE = Path("third-party-packages")


BUILDER_IMAGE_METADATA = Path("out/images/mattos-builder.json")


MATTOS_INVENTORY = Path("out/packages/inventory.toml")


# How often, and how far apart, a repository request that failed for a
# transient reason (the server unreachable, a timeout, HTTP 5xx) is retried.
# A rejection (bad credentials, an invalid package, other HTTP 4xx) fails at once.
REPOSITORY_RETRIES = 10
REPOSITORY_RETRY_DELAY = 10


# Built packages whose upload failed, kept so the next update can publish
# them instead of rebuilding.
UNPUBLISHED_ARTIFACTS = Path("out/third-party/unpublished")


# Third-party revisions sort below a MattOS-built package of the same upstream
# version (`-1mattos1`), so MattOS can always take a package over.
REVISION_PREFIX = "0mattos"


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
    # Repository-relative path of the .deb (the index's Filename field).
    filename: str = ""
    # X-MattOS-Build-Environment and Depends of the published package.
    build_environment: str = ""
    depends: tuple[str, ...] = ()


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


def image_package_sha256(root: Path) -> dict[str, str]:
    """The inventory SHA-256 of each package in the last built builder image,
    without building anything (empty if unknown)."""
    try:
        metadata = json.loads((root / BUILDER_IMAGE_METADATA).read_text(encoding="utf-8"))
        return {str(name): str(sha) for name, sha in dict(metadata.get("package_sha256", {})).items()}
    except (OSError, TypeError, ValueError):
        return {}


def build_code_files(recipe: PackageRecipe) -> list[Path]:
    """The shared code that runs inside a recipe's build.  Host-only code
    (this module, discovery) is deliberately not among them."""
    files = [Path(build_module.__file__).resolve()]
    if "go" in recipe.toolchains:
        files.append(Path(go_module.__file__).resolve())
    return files


def build_inputs_digest(recipe: PackageRecipe, script: Path, selection: ReleaseSelection,
                        dependencies: Sequence[tuple[str, PublishedPackage]] = ()) -> str:
    """Identity of what a package build is made from: the recipe, the shared
    build code it runs, the pinned release selection and the third-party
    build dependencies.  A change to any of them means the published package
    no longer matches its recipe and must be rebuilt.

    The builder image is deliberately not part of it: MattOS libraries keep
    their SONAMEs, so a package built against an older image keeps working.
    Image changes are tracked separately (`build_environment_digest`) and
    only make a package stale."""
    digest = hashlib.sha256()
    for path in (script, *build_code_files(recipe)):
        digest.update(path.name.encode() + b"\0" + path.read_bytes() + b"\0")
    digest.update(json.dumps({"version": selection.version, "revision": selection.revision,
                              **selection.provenance}, sort_keys=True).encode())
    for name, package in dependencies:
        digest.update(f"\0{name}={package.version}:{package.build_inputs}".encode())
    return digest.hexdigest()


def resolve_build_dependencies(root: Path, recipe: PackageRecipe,
                               inventory: dict[str, list[PublishedPackage]]) -> list[tuple[str, PublishedPackage]]:
    """The published package of each third-party build dependency, at the
    release selected for it in releases.json."""
    selections = release_selections(root)
    resolved = []
    for name in recipe.build_depends:
        selection = selections.get(name)
        if selection is None:
            raise RecipeError(f"{recipe.name} build-depends on {name}, which has no selected release")
        wanted = selection.package_version
        match = next((entry for entry in inventory.get(name, []) if entry.version == wanted), None)
        if match is None or not match.filename:
            raise RecipeError(
                f"{recipe.name} build-depends on {name} {wanted}, which is not published in "
                f"{recipe.repository}; publish it first"
            )
        resolved.append((name, match))
    return resolved


def transient_http_status(code: int) -> bool:
    return code >= 500 or code in (408, 429)


def transient_request_error(exc: BaseException) -> bool:
    """Whether a failed request to the repository may succeed later."""
    while exc is not None:
        if isinstance(exc, urllib.error.HTTPError):
            return transient_http_status(exc.code)
        if isinstance(exc, OSError):  # URLError, timeouts, refused or reset connections
            return True
        exc = exc.__cause__
    return False


# Publisher output that means the server could not be reached or failed
# while handling the request, rather than rejecting it.
_TRANSIENT_PUBLISHER_OUTPUT = re.compile(
    r"server is unreachable|ConnectionResetError|ConnectionRefusedError|TimeoutError|timed out"
    r"|RemoteDisconnected|BrokenPipeError|IncompleteRead|returned invalid JSON"
)


def transient_publisher_failure(output: str) -> bool:
    """Whether a failed upload by the vendored publisher is worth retrying.
    The publisher reports every server problem as a remote error, so the
    HTTP status in its message decides: 4xx (but 408 and 429) is a refusal."""
    status = re.search(r"returned HTTP (\d{3})", output)
    if status:
        return transient_http_status(int(status.group(1)))
    return bool(_TRANSIENT_PUBLISHER_OUTPUT.search(output))


def with_repository_retries(action, what: str, transient):
    """Run `action`, retrying a transient failure REPOSITORY_RETRIES times
    REPOSITORY_RETRY_DELAY seconds apart before giving up."""
    for attempt in range(REPOSITORY_RETRIES + 1):
        try:
            return action()
        except RecipeError as exc:
            if attempt == REPOSITORY_RETRIES or not transient(exc):
                raise
            reason = [line.strip() for line in str(exc).splitlines() if line.strip()][-1:] or [type(exc).__name__]
            print(f"{what} failed ({reason[0]}); retry {attempt + 1}/{REPOSITORY_RETRIES} "
                  f"in {REPOSITORY_RETRY_DELAY} s", flush=True)
            time.sleep(REPOSITORY_RETRY_DELAY)
    raise AssertionError("unreachable")


def fetch_build_dependencies(root: Path, repository: str, workspace: Path,
                             dependencies: Sequence[tuple[str, PublishedPackage]]) -> None:
    """Download the dependency packages into the workspace, where the
    container build installs them before building."""
    if not dependencies:
        return
    base = _publisher_config(root, repository).public_url.rstrip("/")
    for name, package in dependencies:
        url = f"{base}/{package.filename}"
        destination = workspace / THIRD_PARTY_DEPENDENCY_DIR / Path(package.filename).name
        with_repository_retries(lambda: download(url, destination), f"downloading {name}", transient_request_error)


def container_memory_high() -> int | None:
    """Three quarters of MemAvailable (at least 2 GiB), or None if unknown."""
    try:
        for line in Path("/proc/meminfo").read_text(encoding="utf-8").splitlines():
            if line.startswith("MemAvailable:"):
                available = int(line.split()[1]) * 1024
                return max(available // 4 * 3, 2 << 30)
    except (OSError, ValueError, IndexError):
        pass
    return None


def build_in_container(root: Path, recipe: PackageRecipe, script: Path, workspace: Path,
                       version: str, provenance: dict[str, str],
                       image: tuple[str, str] | None = None,
                       package_sha256: dict[str, str] | None = None) -> BuildResult:
    """Run recipe payload assembly in the MattOS builder container.

    The container sees only the disposable workspace (read-write) and the
    recipe directory (read-only): upstream build scripts cannot touch the rest
    of the repository.  The host performs repository lookup and publishing, so
    repository credentials never enter the build environment.
    """
    engine = container_engine()
    reference, _digest = image or builder_image(root, engine)
    request = workspace / "container-request.json"
    request.write_text(json.dumps({
        "version": version, "provenance": provenance,
        "package_sha256": package_sha256 if package_sha256 is not None else image_package_sha256(root),
    }, sort_keys=True), encoding="utf-8")
    recipes = (root / RECIPES_RELATIVE).resolve()
    relative_script = script.resolve().relative_to(recipes)
    # Rootless Podman's container root maps to the calling host user. Docker
    # needs an explicit UID/GID to avoid leaving root-owned temporary files.
    user_args: list[str] = []
    if Path(engine).name != "podman":
        user_args = ["--user", f"{os.getuid()}:{os.getgid()}"]
    else:
        # Like a MattOS build stage, the container is reclaimed and
        # throttled at three quarters of the memory available when it
        # starts, instead of pushing the desktop into swap.
        high = container_memory_high()
        if high:
            user_args = [f"--cgroup-conf=memory.high={high}"]
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
            depends = tuple(entry.strip() for entry in fields.get("Depends", "").split(",") if entry.strip())
            published.setdefault(fields["Package"], []).append(
                PublishedPackage(fields["Version"], fields.get(BUILD_INPUTS_FIELD, ""), fields.get("Filename", ""),
                                 fields.get(BUILD_ENVIRONMENT_FIELD, ""), depends)
            )
    return published


def repository_inventory(root: Path, repository: str) -> dict[str, list[PublishedPackage]]:
    """One snapshot of a repository's published packages: a single fetch of
    its public index, including each package's recorded build inputs."""
    if repository not in VALID_REPOSITORIES:
        raise RecipeError(f"unsupported repository {repository!r}")
    url = repository_index_url(root, repository)

    def fetch() -> str | None:
        try:
            request = Request(url, headers={"User-Agent": "MattOS-third-party-packages/2"})
            with urlopen(request, timeout=60) as response:
                return gzip.decompress(response.read()).decode("utf-8")
        except urllib.error.HTTPError as exc:
            if exc.code == 404:
                return None
            raise RecipeError(f"could not read the {repository} package index {url}: {exc}") from exc
        except OSError as exc:
            raise RecipeError(f"could not read the {repository} package index {url}: {exc}") from exc

    text = with_repository_retries(fetch, f"reading the {repository} package index", transient_request_error)
    return {} if text is None else parse_packages_index(text)


def write_repository_inventory(path: Path, repository: str, inventory: dict[str, list[PublishedPackage]]) -> None:
    if repository not in VALID_REPOSITORIES:
        raise RecipeError(f"unsupported repository {repository!r}")
    path.write_text(json.dumps({
        "format": 3,
        "repository": repository,
        "packages": {
            name: [{"version": entry.version, "build_inputs": entry.build_inputs, "filename": entry.filename,
                    "build_environment": entry.build_environment, "depends": list(entry.depends)}
                   for entry in entries]
            for name, entries in inventory.items()
        },
    }, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def read_repository_inventory(path: Path, repository: str) -> dict[str, list[PublishedPackage]]:
    try:
        document = json.loads(path.read_text(encoding="utf-8"))
        packages = document["packages"]
    except (OSError, KeyError, TypeError, json.JSONDecodeError) as exc:
        raise RecipeError(f"repository inventory is invalid ({path}): {exc}") from exc
    if (not isinstance(document, dict) or document.get("format") != 3
            or document.get("repository") != repository or not isinstance(packages, dict)):
        raise RecipeError(f"repository inventory is invalid ({path}): repository identity mismatch")
    inventory: dict[str, list[PublishedPackage]] = {}
    for package, entries in packages.items():
        try:
            inventory[package] = [PublishedPackage(str(entry["version"]), str(entry.get("build_inputs", "")),
                                                   str(entry.get("filename", "")),
                                                   str(entry.get("build_environment", "")),
                                                   tuple(str(item) for item in entry.get("depends", ())))
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
                  published_inputs: str | None = None, build_inputs: str = "",
                  published_environment: str = "", build_environment: str = "") -> tuple[str, str | None]:
    """Classify the upstream release, the selected release and the repository.

    `pending-publish`: the selected package version is not published.
    `rebuild-pending`: it is published, but from different build inputs.
    `stale-environment`: published from the same inputs, but built with an
    older builder image (packages it links, or its compilers, changed since).
    It keeps working; `--rebuild-stale` rebuilds it.
    `upstream-newer`: published as built, but upstream has a newer release.
    """
    package_version = selected_package_version or selected
    repository = newest_debian_version(published)
    if package_version not in published:
        return "pending-publish", repository
    if build_inputs and published_inputs is not None and published_inputs != build_inputs:
        return "rebuild-pending", repository
    if published_environment and build_environment and published_environment != build_environment:
        return "stale-environment", repository
    if upstream != selected:
        return "upstream-newer", repository
    if repository != package_version:
        return "repository-diverged", repository
    return "up-to-date", repository


def kept_artifact(root: Path, recipe: PackageRecipe, version: str, build_inputs: str,
                  package_sha256: dict[str, str]) -> Path | None:
    """A package kept after a failed upload that a build now would
    reproduce: same version, build inputs and build environment."""
    path = root / UNPUBLISHED_ARTIFACTS / f"{recipe.name}_{version}_{recipe.architecture}.deb"
    if not path.is_file():
        return None
    try:
        output = command(["dpkg-deb", "--field", str(path), "Version", BUILD_INPUTS_FIELD,
                          BUILD_ENVIRONMENT_FIELD, "Depends"])
    except RecipeError:
        return None
    fields = {}
    for line in output.splitlines():
        key, separator, value = line.partition(":")
        if separator and not line.startswith((" ", "\t")):
            fields[key] = value.strip()
    depends = [entry for entry in fields.get("Depends", "").split(",") if entry.strip()]
    if (fields.get("Version") != version or fields.get(BUILD_INPUTS_FIELD) != build_inputs
            or fields.get(BUILD_ENVIRONMENT_FIELD, "")
            != build_environment_digest(recipe, depends, package_sha256)):
        return None
    return path


def publish_or_keep(root: Path, recipe: PackageRecipe, artifact: Path, *, repository: str, dry_run: bool) -> None:
    """Publish a built package.  If the upload fails, keep the package so a
    later update publishes it without rebuilding (codex takes two hours)."""
    kept = root / UNPUBLISHED_ARTIFACTS / artifact.name
    try:
        publish(root, artifact, repository=repository, dry_run=dry_run)
    except RecipeError as exc:
        if dry_run:
            raise
        if artifact.resolve() != kept.resolve():
            kept.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(artifact, kept)
        reason = next((line.strip() for line in reversed(str(exc).splitlines()) if line.strip()), "")
        raise RecipeError(
            f"{exc}\nerror: upload failed ({reason}); the built package is kept at "
            f"{kept.relative_to(root)} and the next update publishes it without rebuilding"
        ) from exc
    if not dry_run and kept.is_file():
        kept.unlink()


def publish(root: Path, artifact: Path, *, repository: str, dry_run: bool) -> None:
    if repository not in VALID_REPOSITORIES:
        raise RecipeError(f"unsupported repository {repository!r}")
    publisher = root / PUBLISHER_RELATIVE
    args = [sys.executable, str(publisher), "--non-interactive"]
    if dry_run:
        args.append("--dry-run")
    args += ["--repo", repository, "upload", str(artifact)]
    # Replacing a package with the same bytes is harmless, so a failed
    # upload can simply be repeated.
    with_repository_retries(lambda: command(args, cwd=root), f"uploading {artifact.name}",
                            lambda exc: transient_publisher_failure(str(exc)))


def run_recipe(recipe: PackageRecipe, argv: Sequence[str], script: Path) -> int:
    parser = argparse.ArgumentParser(description=f"Maintain the MattOS {recipe.name} package")
    parser.add_argument("command", nargs="?", choices=("check", "build", "publish", "update", "__container-build"), default="check")
    parser.add_argument("--dry-run", action="store_true", help="validate publication without uploading")
    parser.add_argument("--output", type=Path, help="local output directory for the .deb")
    parser.add_argument("--rebuild-stale", action="store_true",
                        help="update also rebuilds a package built with an older builder image")
    parser.add_argument("--workspace", type=Path, help=argparse.SUPPRESS)
    parser.add_argument("--request", type=Path, help=argparse.SUPPRESS)
    parser.add_argument("--result-json", type=Path, help=argparse.SUPPRESS)
    parser.add_argument("--repository-inventory", type=Path, help=argparse.SUPPRESS)
    parser.add_argument("--describe-json", action="store_true", help=argparse.SUPPRESS)
    args = parser.parse_args(list(argv))
    if args.command == "__container-build":
        if not args.workspace or not args.request:
            raise RecipeError("container build requires --workspace and --request")
        return container_build(recipe, args.workspace, args.request)
    root = repo_root(script)
    repository = validate_repository(recipe)
    selection = selected_release(root, recipe)
    version = selection.package_version
    if args.describe_json:
        print(json.dumps({
            "package": recipe.name,
            "repository": repository,
            "selected_version": version,
            "build_depends": list(recipe.build_depends),
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
    dependencies: list[tuple[str, PublishedPackage]] | None = []
    if recipe.build_depends:
        inventory = (read_repository_inventory(args.repository_inventory, repository)
                     if args.repository_inventory else repository_inventory(root, repository))
        try:
            dependencies = resolve_build_dependencies(root, recipe, inventory)
        except RecipeError:
            if args.command != "check":
                raise
            dependencies = None
    published_versions = [entry.version for entry in existing]
    current = next((entry for entry in existing if entry.version == version), None)
    published_inputs = current.build_inputs if current else None
    expected_inputs = (build_inputs_digest(recipe, script, selection, dependencies)
                       if dependencies is not None else "")

    def classify(package_sha256: dict[str, str]) -> tuple[str, str | None]:
        # The published package's environment, re-fingerprinted against the
        # image the next build would use.
        environment = (build_environment_digest(recipe, current.depends, package_sha256)
                       if current and current.build_environment else "")
        return release_state(
            upstream_version, selection.version, published_versions,
            selected_package_version=version, published_inputs=published_inputs, build_inputs=expected_inputs,
            published_environment=current.build_environment if current else "", build_environment=environment,
        )

    state, repository_version = classify(image_package_sha256(root))
    result = {
        "package": recipe.name, "upstream_version": upstream_version,
        "selected_version": version, "repository_version": repository_version,
        "published_versions": published_versions,
    }
    if args.command == "check":
        print(f"[{recipe.name}] upstream latest: {upstream_version}", flush=True)
        print(f"[{recipe.name}] published versions in {repository}: {', '.join(published_versions) or 'not published'}", flush=True)
        print(f"[{recipe.name}] release status: {state.replace('-', ' ')}", flush=True)
        if state == "stale-environment":
            print(f"[{recipe.name}] built with an older builder image; `update --rebuild-stale` rebuilds it",
                  flush=True)
        print(f"[{recipe.name}] check complete; no build or upload performed", flush=True)
        if args.result_json:
            args.result_json.write_text(json.dumps({**result, "status": "checked", "release_state": state},
                                                   sort_keys=True) + "\n", encoding="utf-8")
        return 0
    if args.command != "build":
        ensure_not_a_mattos_package(root, recipe)
    engine = container_engine()
    image = builder_image(root, engine)
    package_sha256 = image_package_sha256(root)
    build_inputs = build_inputs_digest(recipe, script, selection, dependencies or ())
    if args.command == "update" and published_inputs == build_inputs:
        state, repository_version = classify(package_sha256)
        if state != "stale-environment":
            print(f"[{recipe.name}] {version} is already published from identical build inputs; nothing to do",
                  flush=True)
            status = "up-to-date"
        elif not args.rebuild_stale:
            print(f"[{recipe.name}] {version} was built with an older builder image; it keeps working, "
                  "so it is not rebuilt (pass --rebuild-stale to rebuild it)", flush=True)
            status = "stale-environment"
        else:
            print(f"[{recipe.name}] {version} was built with an older builder image; rebuilding (--rebuild-stale)",
                  flush=True)
            status = ""
        if status:
            if args.result_json:
                args.result_json.write_text(json.dumps({**result, "status": status}, sort_keys=True) + "\n",
                                            encoding="utf-8")
            return 0
    if args.command in ("publish", "update"):
        kept = kept_artifact(root, recipe, version, build_inputs, package_sha256)
        if kept:
            print(f"[{recipe.name}] publishing the package kept after a failed upload "
                  f"({kept.relative_to(root)}); it matches the current build inputs and environment", flush=True)
            validate_package_artifact(recipe, BuildResult(recipe.name, version, recipe.architecture, kept, provenance))
            print(f"[{recipe.name}] uploading to {repository}...", flush=True)
            publish_or_keep(root, recipe, kept, repository=repository, dry_run=args.dry_run)
            print(f"[{recipe.name}] upload command completed", flush=True)
            if args.result_json:
                status = "dry-run" if args.dry_run else "uploaded"
                args.result_json.write_text(json.dumps({**result, "status": status}, sort_keys=True) + "\n",
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
            fetch_build_dependencies(root, repository, workspace, dependencies or ())
            result_build = build_in_container(root, recipe, script, workspace, version, provenance, image,
                                              package_sha256)
            artifact = result_build.artifact.resolve()
            if not artifact.is_relative_to(workspace):
                raise RecipeError("recipe artifact must remain inside its disposable workspace")
            validate_package_artifact(recipe, result_build)
            print(f"[{recipe.name}] package built: {result_build.artifact.name}", flush=True)
            print(f"[{recipe.name}] artifact SHA-256: {sha256_file(result_build.artifact)}", flush=True)
            if args.command in ("publish", "update"):
                print(f"[{recipe.name}] uploading to {repository}...", flush=True)
                publish_or_keep(root, recipe, result_build.artifact, repository=repository, dry_run=args.dry_run)
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

