# Third-Party Packages

`third-party-packages/` holds independently maintained recipes for software
MattOS publishes but does not build as part of its own image: btop,
fastfetch and htop. They are not build stages
and are never fetched or built by `mattos-build build`. Each recipe builds a
native `.deb` from a verified upstream release inside the MattOS builder
container and publishes it through the vendored repository client.

## Commands

From the repository root:

```text
python3 third-party-packages/htop.py check
python3 third-party-packages/htop.py build
python3 third-party-packages/htop.py update
python3 third-party-packages/htop.py publish --dry-run
python3 DevUtils/BuildAndUploadThirdPartyPackages.py --check
python3 DevUtils/BuildAndUploadThirdPartyPackages.py
python3 DevUtils/BuildAndUploadThirdPartyPackages.py --dry-run --recipe htop
```

With no command a recipe runs the read-only `check`. `build` makes a local
`.deb` in `--output` (default `third-party-packages/dist/`) without touching a
repository. `update` builds and publishes only when needed (below); `publish`
builds and publishes unconditionally. `--dry-run` passes through to the
repository client. `DevUtils/BuildAndUploadThirdPartyPackages.py` runs every
recipe (or `--recipe NAME`), fetches each repository's index once, and prints
a status table: **UP TO DATE**, **UPSTREAM NEWER**, **PENDING PUBLISH**,
**REBUILD PENDING**, **UPLOADED**, **DRY RUN** or **FAILED**.

## Releases, versions and source verification

`third-party-packages/releases.json` is the checked-in selection for every
recipe: the upstream `version`, its `release_tag`, the `source_sha256` of the
release archive, and an optional packaging `revision` (default 1). The package
version is `<version>-0mattos<revision>`: bump `revision` for a packaging-only
change. A `0mattos` revision sorts below a MattOS-built `-1mattos1` package of
the same upstream version, so MattOS can always take a package over. Upstream
discovery (GitHub's latest release) is only a signal that a newer release
exists; a build always uses the selection.

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

The framework runs `mattos-build builder-image`, loads the archive into Podman
(or Docker, or `MATTOS_THIRD_PARTY_CONTAINER_ENGINE`) when the engine lacks
the image, and runs the recipe with only two mounts: the disposable workspace
(read-write, `/work`) and `third-party-packages/` (read-only, `/recipes`).
Upstream build scripts cannot reach the rest of the repository, and
repository credentials stay on the host, which performs lookup and upload.

Rootless Podman needs the setuid `newuidmap` helper, which cannot work in a
process with `no_new_privs` set. That flag is inherited: a terminal inside an
editor that was itself started with it (for example VS Code launched through a
`vscode://` URL handler) cannot start rootless containers. The framework
checks `/proc/self/status` and says so; run recipes from a terminal where
`grep NoNewPrivs /proc/self/status` prints `0`.

## Skipping unchanged packages

Every package records `X-MattOS-Build-Inputs`: a SHA-256 of the recipe file,
the shared framework, its `releases.json` selection and the builder image's
manifest digest. The repository index carries the field, so `update` compares
it with the published package after one index fetch per repository:

- identical inputs: nothing is built or uploaded;
- the selected version is published from different inputs: rebuilt and
  republished (the repository replaces it in place);
- the version is not published: built and published.

## Package ownership

A package name has one producer. Recipes refuse to publish a name the MattOS
build produces (read from `out/packages/inventory.toml`), and
`DevUtils/PublishPackages.py` refuses MattOS packages whose name a recipe
claims in `releases.json`.

## Writing a recipe

Most recipes only declare their archive and build system; `SourceReleaseRecipe`
(`common/framework.py`) downloads and verifies the archive, extracts it,
builds it with Autotools, CMake or the project's own Makefile, installs into a
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
unusual packages. Tests in `third-party-packages/tests/` use local fixtures and
mocked publication; they never upload.
