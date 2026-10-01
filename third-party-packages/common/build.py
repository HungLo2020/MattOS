"""The third-party build code that runs inside the MattOS builder container.

Everything a package build executes lives here: the recipe base classes,
the build-system helpers, staging, the ELF dependency resolution and the
control-file rendering.  This file is part of every package's build inputs
(`X-MattOS-Build-Inputs`), so a change here rebuilds every package.  Code
that only runs on the host -- upstream discovery (`discovery.py`), the
repository inventory and publishing (`framework.py`) -- is deliberately kept
out of it, so changing it rebuilds nothing.
"""

from __future__ import annotations

import hashlib
import json
import os
import re
import shutil
import subprocess
import tarfile
import urllib.error
import urllib.request
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable, Mapping, Sequence


VALID_REPOSITORIES = frozenset(("mattos", "mattpackages"))


BUILD_INPUTS_FIELD = "X-MattOS-Build-Inputs"


# Fingerprint of the builder-image packages a build used: the MattOS packages
# the package depends on plus its compiler toolchain.  Unlike the build
# inputs, a change here makes a package stale, not due for a rebuild.
BUILD_ENVIRONMENT_FIELD = "X-MattOS-Build-Environment"


# The builder-image packages each toolchain a recipe declares builds with.
# Rust links through the C toolchain; Go is pinned in common/go.py and needs
# nothing from the image unless cgo is enabled (declare "c" as well then).
C_TOOLCHAIN = (
    "binutils", "cpp", "gcc", "g++", "libc6-dev", "linux-libc-dev",
    "mattos-gcc-common", "mattos-libgcc-dev", "mattos-libstdc++-dev",
)
TOOLCHAIN_PACKAGES: dict[str, tuple[str, ...]] = {
    "c": C_TOOLCHAIN,
    "rust": (*C_TOOLCHAIN, "rustc", "cargo"),
    "go": (),
}


# Debian's multiarch library directory, which the MattOS loader searches.
MULTIARCH_LIBDIR = "lib/x86_64-linux-gnu"


# Workspace directory holding third-party build dependencies to install.
THIRD_PARTY_DEPENDENCY_DIR = "third-party-deps"


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
    conflicts: Sequence[str] = ()
    # Other third-party packages (published in the same repository) that must
    # be installed in the builder container to build this one, for example a
    # library it links.  They are installed from the repository at their
    # selected release, and the bulk runner publishes them first.
    build_depends: Sequence[str] = ()
    # The compilers the build uses (keys of TOOLCHAIN_PACKAGES).  Together
    # with the packages the result depends on, they decide which builder
    # image changes make the published package stale.
    toolchains: Sequence[str] = ("c",)

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
            "conflicts": tuple(self.conflicts),
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


def _skip_unsafe_links(member: tarfile.TarInfo, path: str) -> tarfile.TarInfo | None:
    """tarfile's `data` filter, except that a link pointing outside the tree
    (typically upstream test data) is left out instead of failing the whole
    extraction.  Anything else the filter rejects still fails."""
    try:
        return tarfile.data_filter(member, path)
    except (tarfile.AbsoluteLinkError, tarfile.LinkOutsideDestinationError):
        if member.issym() or member.islnk():
            return None
        raise


def extract_archive(archive: Path, destination: Path) -> Path:
    destination.mkdir(parents=True, exist_ok=True)
    with tarfile.open(archive, "r:*") as tar:
        members = tar.getmembers()
        for member in members:
            target = (destination / member.name).resolve()
            if not target.is_relative_to(destination.resolve()):
                raise RecipeError(f"archive contains path traversal: {member.name}")
        tar.extractall(destination, filter=_skip_unsafe_links)
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
                  conflicts: Sequence[str] = (),
                  architecture: str = "amd64", section: str = "utils",
                  priority: str = "optional", build_inputs: str = "",
                  build_environment: str = "") -> None:
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
    if conflicts:
        lines.append("Conflicts: " + ", ".join(conflicts))
        lines.append("Replaces: " + ", ".join(conflicts))
    if build_inputs:
        lines.append(f"{BUILD_INPUTS_FIELD}: {build_inputs}")
    if build_environment:
        lines.append(f"{BUILD_ENVIRONMENT_FIELD}: {build_environment}")
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
    # A package's programs may load the package's own libraries: resolve
    # those from the staging tree, and depend only on what lies outside it.
    own = [staging / "usr" / MULTIARCH_LIBDIR, staging / "usr/lib"]
    env = {**os.environ, "LD_LIBRARY_PATH": ":".join(str(path) for path in own if path.is_dir())}
    staged = Path(os.path.abspath(staging))
    for path in sorted(staging.rglob("*")):
        if path.is_symlink() or not path.is_file() or "DEBIAN" in path.parts or not _is_elf(path):
            continue
        result = subprocess.run(["ldd", str(path)], text=True, stdout=subprocess.PIPE,
                                stderr=subprocess.STDOUT, check=False, env=env)
        if "not a dynamic executable" in result.stdout:
            continue
        for line in result.stdout.splitlines():
            if "=> not found" in line:
                raise RecipeError(f"{path.relative_to(staging)} needs a library MattOS does not provide: {line.strip()}")
            match = re.search(r"=>\s+(/\S+)", line)
            if match and not Path(os.path.realpath(match.group(1))).is_relative_to(staged):
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


def dependency_package_names(depends: Iterable[str]) -> list[str]:
    """Package names in Depends entries (`a (>= 1) | b` names a and b)."""
    names = set()
    for entry in depends:
        for clause in entry.split(","):
            for alternative in clause.split("|"):
                name = alternative.strip().split(" ", 1)[0].split("(", 1)[0].split(":", 1)[0]
                if name:
                    names.add(name)
    return sorted(names)


def build_environment_digest(recipe: PackageRecipe, depends: Iterable[str],
                             package_sha256: Mapping[str, str]) -> str:
    """Fingerprint of the builder-image packages one build relied on.

    Only the image packages the result depends on (what it links) and the
    packages of its declared toolchains count, so an image change elsewhere
    leaves the package current.  Empty when the image digests are unknown.
    """
    if not package_sha256:
        return ""
    names = set(dependency_package_names(depends))
    for toolchain in recipe.toolchains:
        if toolchain not in TOOLCHAIN_PACKAGES:
            raise RecipeError(f"{recipe.name} declares unknown toolchain {toolchain!r}")
        names.update(TOOLCHAIN_PACKAGES[toolchain])
    digest = hashlib.sha256(b"mattos-third-party-build-environment-v1")
    for name in sorted(names & package_sha256.keys()):
        digest.update(f"\n{name}={package_sha256[name]}".encode())
    return digest.hexdigest()


# The builder image's package digests, set by `container_build` from the
# host's request.
_image_package_sha256: dict[str, str] = {}


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
    environment = build_environment_digest(recipe, metadata["depends"], _image_package_sha256)
    write_control(staging, **metadata, build_inputs=provenance.get("build_inputs", ""),
                  build_environment=environment)
    write_provenance(staging, recipe.name, provenance)
    artifact = package_staging(
        staging, workspace, name=recipe.name, version=version,
        architecture=recipe.architecture,
    )
    return BuildResult(recipe.name, version, recipe.architecture, artifact, provenance)


def github_source_archive(owner: str, repository: str, release_tag: str) -> str:
    return f"https://github.com/{owner}/{repository}/archive/refs/tags/{release_tag}.tar.gz"


def cmake_build_install(source: Path, build: Path, staging: Path,
                        *, options: Sequence[str] = ()) -> None:
    """Configure, build, and DESTDIR-install a conventional CMake project."""
    require_tools(["cmake", "make", "cc", "c++"])
    command([
        "cmake", "-S", str(source), "-B", str(build),
        "-DCMAKE_BUILD_TYPE=Release", "-DCMAKE_INSTALL_PREFIX=/usr",
        f"-DCMAKE_INSTALL_LIBDIR={MULTIARCH_LIBDIR}", *options,
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
        str(configure), "--prefix=/usr", f"--libdir=/usr/{MULTIARCH_LIBDIR}",
        "--disable-dependency-tracking", *options,
    ], cwd=build)
    command(["make", "-j", str(os.cpu_count() or 1)], cwd=build)
    command(["make", f"DESTDIR={staging}", "install"], cwd=build)


def meson_build_install(source: Path, build: Path, staging: Path,
                        *, options: Sequence[str] = ()) -> None:
    """Configure, build, and DESTDIR-install a Meson project."""
    require_tools(["meson", "ninja", "cc"])
    command([
        "meson", "setup", str(build), str(source), "--prefix=/usr",
        f"--libdir={MULTIARCH_LIBDIR}", "--buildtype=release", "--wrap-mode=nodownload", *options,
    ])
    command(["ninja", "-C", str(build)])
    command(["meson", "install", "-C", str(build), "--destdir", str(staging)])


def make_build_install(source: Path, staging: Path, *, options: Sequence[str] = (),
                       install_options: Sequence[str] = ()) -> None:
    """Build and DESTDIR-install a project with its own Makefile."""
    require_tools(["make", "cc", "c++"])
    command(["make", "-j", str(os.cpu_count() or 1), *options], cwd=source)
    command(["make", *options, *install_options, f"DESTDIR={staging}", "install"], cwd=source)


def cargo_environment(workspace: Path) -> dict[str, str]:
    """Environment for `cargo build` with the MattOS Rust toolchain.  Crates
    are fetched from crates.io at the versions pinned by Cargo.lock."""
    return {
        **os.environ,
        "CARGO_HOME": str(workspace / "cargo-home"),
        "CARGO_TARGET_DIR": str(workspace / "cargo-target"),
        "CARGO_NET_RETRY": "5",
        # Bounded parallelism: large workspaces (codex) otherwise run one
        # rustc per CPU thread and exhaust memory.
        "CARGO_BUILD_JOBS": "6",
        "HOME": str(workspace),
    }


def install_file(source: Path, destination: Path, mode: int = 0o644) -> None:
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source, destination)
    destination.chmod(mode)


def install_license(staging: Path, package: str, source: Path, names: Sequence[str]) -> None:
    """The package's copyright notice, from the named upstream license files."""
    text = "\n".join((source / name).read_text(encoding="utf-8", errors="replace") for name in names)
    destination = staging / "usr/share/doc" / package / "copyright"
    destination.parent.mkdir(parents=True, exist_ok=True)
    destination.write_text(text, encoding="utf-8")


class SourceReleaseRecipe(PackageRecipe):
    """A recipe that builds one verified upstream release archive.

    Subclasses declare where the archive lives and how it builds; any method
    can be overridden for an unusual package.  The archive is verified against
    the `source_sha256` pinned for the selected release in releases.json.
    """

    github: tuple[str, str] = ("", "")
    # Release tags are listed from this Git repository (default: the GitHub
    # repository); `tag_pattern` full-matches a release tag, its group the
    # version.  Pre-releases do not match the default pattern.
    git_url: str = ""
    tag_pattern: str = r"v?([0-9]+(?:\.[0-9]+)*)"
    # Formatted with {version} and {tag} (the selected release_tag).
    source_url: str = ""
    build_system: str = "autotools"  # "autotools", "cmake", "meson", "make" or custom
    build_options: Sequence[str] = ()
    install_options: Sequence[str] = ()

    def discover_version(self) -> tuple[str, dict[str, str]]:
        owner, repository = self.github
        url = self.git_url or (f"https://github.com/{owner}/{repository}.git" if owner else "")
        if not url:
            raise NotImplementedError(f"{self.name} must set github or git_url, or override discover_version")
        from .discovery import git_latest_tag  # host-only; not part of the build
        version, tag = git_latest_tag(url, self.tag_pattern)
        upstream = url.removesuffix(".git")
        return version, {"upstream": upstream, "release_tag": tag}

    def source_archive_url(self, version: str, provenance: dict[str, str]) -> str:
        return self.source_url.format(version=version, tag=provenance.get("release_tag", version))

    def build_source(self, source: Path, workspace: Path, staging: Path) -> None:
        if self.build_system == "cmake":
            cmake_build_install(source, workspace / "build", staging, options=self.build_options)
        elif self.build_system == "autotools":
            autotools_build_install(source, workspace / "build", staging, options=self.build_options)
        elif self.build_system == "meson":
            meson_build_install(source, workspace / "build", staging, options=self.build_options)
        elif self.build_system == "make":
            make_build_install(source, staging, options=self.build_options, install_options=self.install_options)
        else:
            raise RecipeError(f"{self.name} declares unknown build system {self.build_system!r}")

    def post_install(self, staging: Path, source: Path) -> None:
        """Adjust the staged payload before packaging (optional)."""

    def build(self, workspace: Path, version: str, provenance: dict[str, str]) -> BuildResult:
        upstream = provenance.get("upstream_version", version)
        # The upstream release being built, for recipes that stamp it into
        # the binaries they build.
        self.upstream_version = upstream
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


def container_build(recipe: PackageRecipe, workspace: Path, request_path: Path) -> int:
    """The build run inside the builder container: install the third-party
    build dependencies, build, and report the artifact to the host."""
    try:
        request = json.loads(request_path.read_text(encoding="utf-8"))
        version = str(request["version"])
        provenance = dict(request["provenance"])
        package_sha256 = {str(name): str(sha) for name, sha in dict(request.get("package_sha256", {})).items()}
    except (OSError, KeyError, TypeError, ValueError) as exc:
        raise RecipeError(f"container build request is invalid ({request_path}): {exc}") from exc
    _image_package_sha256.clear()
    _image_package_sha256.update(package_sha256)
    dependencies = sorted((workspace / THIRD_PARTY_DEPENDENCY_DIR).glob("*.deb"))
    if dependencies:
        command(["dpkg", "--install", *map(str, dependencies)])
    result = recipe.build(workspace, version, provenance)
    artifact = result.artifact.resolve()
    if not artifact.is_relative_to(workspace.resolve()):
        raise RecipeError("recipe artifact must remain inside its disposable workspace")
    (workspace / "container-result.json").write_text(
        json.dumps({"artifact": artifact.relative_to(workspace.resolve()).as_posix()}, sort_keys=True),
        encoding="utf-8",
    )
    return 0
