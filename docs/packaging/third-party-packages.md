# Third-Party Packages

`third-party-packages/` holds independently maintained recipes for software
MattOS publishes but does not build as part of its own image. They are not
build stages
and are never fetched or built by `mattos-build build`. Each recipe builds a
native `.deb` from a verified upstream release inside the MattOS builder
container and publishes it through the vendored repository client.

| Group | Packages |
| --- | --- |
| Tools | btop, fastfetch, htop, ripgrep, wget, gh, tailscale, codex |
| Podman | podman, conmon, crun, netavark, aardvark-dns, passt, catatonit, containers-common, libseccomp2 |
| nftables (rootful container networking) | nftables, libnftnl11, libmnl0 |
| QEMU | qemu-system-x86 (x86_64 system emulator with qemu-img), libslirp0, libsdl2-2.0-0 |
| ImageMagick | imagemagick, libwebp7, libtiff6, libopenjp2-7 |

Only software MattOS needs to build itself is vendored; everything here is
third-party because MattOS's own build does not need it. A library a vendored program
needs is vendored even when a third-party package also uses it: ImageMagick
links MattOS's libpng, libjpeg-turbo, FreeType, Fontconfig, Little CMS,
libxml2, zlib, bzip2, xz and zstd through their `-dev` packages, and only the
image codecs nothing in MattOS uses (libwebp, libtiff, OpenJPEG) are
third-party. libjpeg-turbo (Gwenview) and Jansson (PackageKit) were once
third-party recipes; now that vendored programs need them they are MattOS
packages (`libjpeg62-turbo`, `libjansson4`, each carrying its development
files) in the builder image, so libtiff6, ImageMagick and nftables build
against them without listing them as third-party build dependencies. ImageMagick draws text with any installed font named
by `-font` (for example `-font Fira-Sans-Regular`); with no `-font` it looks
for a Helvetica- or Arial-class font, which MattOS does not ship.

## Commands

From the repository root:

```text
python3 third-party-packages/htop.py check
python3 third-party-packages/htop.py build
python3 third-party-packages/htop.py update
python3 third-party-packages/htop.py publish --dry-run
python3 DevUtils/BuildAndUploadThirdPartyPackages.py --check
python3 DevUtils/BuildAndUploadThirdPartyPackages.py
python3 DevUtils/BuildAndUploadThirdPartyPackages.py --rebuild-stale
python3 DevUtils/BuildAndUploadThirdPartyPackages.py --dry-run --recipe htop
```

With no command a recipe runs the read-only `check`. `build` makes a local
`.deb` in `--output` (default `third-party-packages/dist/`) without touching a
repository. `update` builds and publishes only when needed (below);
`update --rebuild-stale` also rebuilds packages built with an older builder
image; `publish` builds and publishes unconditionally. `--dry-run` passes
through to the repository client. `DevUtils/BuildAndUploadThirdPartyPackages.py` runs every
recipe (or `--recipe NAME`), fetches each repository's index once, and prints
a status table: **UP TO DATE**, **UPSTREAM NEWER**, **PENDING PUBLISH**,
**REBUILD PENDING**, **STALE ENVIRONMENT**, **UPLOADED**, **DRY RUN** or
**FAILED**.

## Releases, versions and source verification

`third-party-packages/releases.json` is the checked-in selection for every
recipe: the upstream `version`, its `release_tag`, the `source_sha256` of the
release archive, and an optional packaging `revision` (default 1). The package
version is `<version>-0mattos<revision>`; the framework raises `revision`
itself when a rebuild would otherwise reuse a published version (see below). A `0mattos` revision sorts below a MattOS-built `-1mattos1` package of
the same upstream version, so MattOS can always take a package over. Upstream
discovery (GitHub's latest release) is only a signal that a newer release
exists; a build always uses the selection. Discovery lists the upstream
Git repository's release tags (`git ls-remote`, no API quota), matched by the
recipe's `tag_pattern`, so a check of every recipe works in one run.

Every recipe downloads a release archive and refuses it unless its SHA-256
matches `source_sha256`. Prefer official release archives (they carry a
generated `configure`, so no Autotools are needed) over GitHub's generated tag
archives.

## The MattOS builder container

Recipes build inside `localhost/mattos-builder:<tag>`, an OCI image made from
the built MattOS packages by `mattos-build builder-image`
(`packaging/builder_image.rs`). It installs `mattos-build-essential` (GCC,
G++, Make, Binutils, pkgconf, CMake, dash, GNU sed, mawk), grep, findutils
and diffutils, Perl, m4, Autoconf, Automake, Libtool, Meson and Ninja, the
MattOS `-dev` packages recipes need, Python and the fetch and archive tools
(but not the service policy of `mattos-base-runtime`), with the
same chrootless dpkg install as the root filesystem. The image is one
deterministic layer written as `out/images/mattos-builder.oci.tar`; its tag is
its manifest digest, and an unchanged package set reuses the archive.

Packages link against the MattOS libraries they will run with. Inside the
container the image's dpkg database is the MattOS package set, so the
framework resolves every shared library each staged ELF file loads (`ldd`)
and adds the MattOS package owning it (`dpkg-query -S`) to `Depends`; a
library MattOS does not provide fails the build. A recipe's `depends` adds
anything else, such as commands it runs.

The image also carries Rust (`rustc`, `cargo`) for Rust recipes and the MattOS
`-dev` packages third-party builds link against: zlib, OpenSSL, PCRE2, zstd,
GLib, pixman, Wayland (with `wayland-scanner` and `wayland-protocols`),
xkbcommon, alongside libjpeg-turbo and Jansson (whose runtime packages carry
their headers), json-c (whose runtime package carries its headers), ncurses, libcap, libnl, systemd, acl, attr and
libglvnd. Go recipes (gh, tailscale, podman) build with a pinned,
checksum-verified official Go release downloaded into the build
(`go_environment`, pinned in `common/go.py`); Go is not needed to build
MattOS, so it is not packaged.
Rust crates and Go modules are fetched at the versions their lock files pin.

## Third-party build dependencies

A recipe may link a library that is itself a third-party package (crun, conmon
and podman link libseccomp; QEMU links libslirp and SDL2). It lists them in
`build_depends`: the framework resolves each at its `releases.json` release in
the recipe's repository, downloads the published `.deb` into the workspace,
and the container installs it before building, so `ldd` then resolves the
library to that package. Their identity is part of the build inputs, so a
rebuilt library rebuilds its users. The bulk runner orders recipes after the
packages they build-depend on and refreshes its repository snapshot after each
upload, so one run publishes a library and then the packages that need it.

The framework runs `mattos-build builder-image`, loads the archive into Podman
(or Docker, or `MATTOS_THIRD_PARTY_CONTAINER_ENGINE`) when the engine lacks
the image, and runs the recipe with only two mounts: the disposable workspace
(read-write, `/work`) and `third-party-packages/` (read-only, `/recipes`).
Upstream build scripts cannot reach the rest of the repository, and
repository credentials stay on the host, which performs lookup and upload.
Under Podman the container's `memory.high` is three quarters of the memory
available when it starts (`--cgroup-conf`), as for a MattOS build stage, so a
large build (codex's Rust link) is reclaimed and throttled in its own cgroup
instead of pushing the desktop into swap. Cargo runs at most six jobs (codex
two: its largest crates need about 3 GiB each, and it builds without LTO).

Rootless Podman needs the setuid `newuidmap` helper, which cannot work in a
process with `no_new_privs` set. That flag is inherited: a terminal inside an
editor that was itself started with it (for example VS Code launched through a
`vscode://` URL handler) cannot start rootless containers. The framework
checks `/proc/self/status` and says so; run recipes from a terminal where
`grep NoNewPrivs /proc/self/status` prints `0`.

## Skipping unchanged packages

A package records two fingerprints in its control file, and the repository
index carries both, so `update` compares them with the published package after
one index fetch per repository.

`X-MattOS-Build-Inputs` is what the package is made from: a SHA-256 of the
recipe file, the shared code that runs inside the build, its `releases.json`
selection, and the published versions and build inputs of its third-party
build dependencies. The shared code is split so that only the build side is
hashed:

| File | Runs | Hashed |
| --- | --- | --- |
| `common/build.py` | in the container: recipe base classes, build-system helpers, Cargo, staging, ELF dependency resolution, control files | every recipe |
| `common/go.py` | in the container: the pinned Go toolchain | recipes declaring the `go` toolchain |
| `common/discovery.py` | on the host: upstream release discovery | no |
| `common/framework.py` | on the host: release selection, repository inventory, fingerprints, starting the container, publishing | no |

Changing discovery or publishing therefore rebuilds nothing, and a Go update
rebuilds only the Go packages. `build.py` must not import the host modules at
module level (a test checks this).

`X-MattOS-Build-Environment` is which builder image the package was built
with, narrowed to what it relied on: the inventory SHA-256 of the MattOS
packages in its `Depends` (the libraries it links, as resolved in the
container, plus its declared dependencies) and of the compiler toolchain it
declares in `toolchains` (`c`, the default: GCC, Binutils, the C and C++
runtime development packages and kernel headers; `rust`: those plus `rustc`
and `cargo`; `go`: nothing from the image, since Go is pinned). The image
metadata (`out/images/mattos-builder.json`, `package_sha256`) lists each image
package's digest. A changed wget library does not touch codex unless codex
links it.

`update` then decides:

- identical inputs and environment: nothing is built or uploaded (UP TO DATE);
- identical inputs, different environment: the package is reported as
  STALE ENVIRONMENT and left alone. MattOS libraries keep their SONAMEs, so a
  package built against an older image keeps working, as Debian packages do
  when their build environment moves on. `--rebuild-stale` rebuilds it;
- the selected version is published from different inputs (a recipe,
  selection, build code or build dependency change): rebuilt as the next
  revision;
- the version is not published: built and published.

Different bytes are never published under a version that is already in the
repository, since installed systems would never upgrade to them. Whenever
`update` or `publish` is about to upload a version that is published (changed
inputs, `--rebuild-stale`, or an explicit `publish`), the build becomes
`<version>-0mattos<N+1>`, N being the highest published revision of that
upstream release, and `releases.json` records the new revision (commit it).
A dry run builds the next version without recording it. A package that
build-depends on the rebuilt one then sees a new dependency version, so its
inputs change and it gets its own next revision in the same run.

Repository requests (reading the index, downloading a build dependency,
uploading) that fail for a transient reason are retried ten times, ten seconds
apart, as for MattOS's own packages
([transient repository failures](publishing.md#transient-repository-failures)).
A package whose upload fails is kept in `out/third-party/unpublished/`, and
the failure says so. The next `update` or `publish` uploads it instead of
building again when its version, build inputs and build environment still
match; otherwise it is rebuilt. A dry run never keeps a package.

`check` compares with the last built builder image without building one;
`update` first brings the image up to date. A rebuilt third-party build
dependency keeps its build inputs when only its environment changed, so
`--rebuild-stale` on a library does not cascade into its users.

## Package ownership

A package name has one producer. Recipes refuse to publish a name the MattOS
build produces (read from `out/packages/inventory.toml`), and
`DevUtils/PublishPackages.py` refuses MattOS packages whose name a recipe
claims in `releases.json`.

## Writing a recipe

Most recipes only declare their archive and build system; `SourceReleaseRecipe`
(`common/build.py`) downloads and verifies the archive, extracts it,
builds it with Autotools, CMake, Meson or the project's own Makefile, installs into a
staging tree, and writes the control file and provenance:

```python
class HtopRecipe(SourceReleaseRecipe):
    name = "htop"
    repository = "mattos"  # "mattos" or "mattpackages"
    description = "Interactive process viewer"
    depends = ("libc6", "libcap2", "libncursesw6", "mattos-libtinfow6",
               "libnl-3-200", "libnl-genl-3-200")
    github = ("htop-dev", "htop")
    source_url = "https://github.com/htop-dev/htop/releases/download/{version}/htop-{version}.tar.xz"
    build_options = ("--enable-unicode", "--enable-capabilities")
```

Override `discover_version`, `build_source`, `post_install` or `build` for
unusual packages; `self.upstream_version` is the release being built.
`cargo_environment`, `go_environment`, `install_file` and `install_license`
cover Rust and Go builds; a Rust recipe declares `toolchains = ("rust",)`, a
Go recipe `("go",)` (with `"c"` too when it enables cgo). A recipe can write `DEBIAN/conffiles` and maintainer
scripts into the staging tree (tailscale enables `tailscaled.service` on a
running system; like MattOS's own scripts they exit when `DPKG_ROOT` is set). Tests in `third-party-packages/tests/` use local fixtures and
mocked publication; they never upload.
