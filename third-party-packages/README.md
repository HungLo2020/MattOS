# MattOS third-party native packages

This directory contains independently maintained package recipes. They are
not `BuildStage`s and are never fetched or built by a normal MattOS ISO build.
Each recipe downloads source or an upstream binary into a disposable
`out/tmp/mattos-*` directory inside the local `Containerfile` builder image,
verifies what the upstream publishes, creates a native `.deb`, and can upload
it through the existing vendored `ManageMattOSRepository.py` client. The
container never receives repository credentials: repository lookup and upload
remain host-side after the artifact has passed Debian metadata validation.

## Commands

From the repository root:

```text
python3 third-party-packages/fastfetch.py check
python3 third-party-packages/fastfetch.py build
python3 third-party-packages/firefox.py update
python3 third-party-packages/firefox.py publish --dry-run
python3 DevUtils/BuildAndUploadThirdPartyPackages.py --check
python3 DevUtils/BuildAndUploadThirdPartyPackages.py
python3 DevUtils/BuildAndUploadThirdPartyPackages.py --dry-run --recipe htop
```

With no command, a recipe performs the read-only `check` operation. Every
recipe has three deliberately separate versions: the **upstream latest**,
the checked-in **MattOS selected release** in `releases.json`, and the newest
version already **published** in that recipe's declared repository. `check`
reports all three and never builds anything. The selection is the version
that `build`/`update` use; seeing a newer upstream release is a maintainer
signal, not permission for a script to publish it automatically.

`DevUtils/BuildAndUploadThirdPartyPackages.py --check` is the efficient
whole-tree version of that report. It obtains one validated inventory per
repository, passes a temporary snapshot to every recipe, and therefore does
not query the package server once per recipe or compile packages just to
decide their status. Its default `update` mode uses the same snapshot and
only builds when a selected MattOS release is absent from its repository.
Use `--dry-run` to exercise the publish command for such a missing selected
release without uploading it.

`build` creates a local package without credentials or repository access;
only the requested `.deb` is retained in `--output` (or `dist/`). `update` is
the idempotent lifecycle: it checks the recipe's declared repository, skips a
selected release already published there, or downloads, verifies, builds,
stages, validates, and publishes the selected release. `publish` performs
the same lifecycle without the version-skip policy. `--dry-run` passes
through to the repository client, but tests should mock it; no real upload is
performed by the package tests.

All `check`, `update`, and `publish` operations invoke the repository declared
by the recipe as `ManageMattOSRepository.py --repo <repository>`. Update and
publish workspaces are disposable even when a download, build, validation, or
upload fails. A local build is also disposable except for its final `.deb`.

Recipes keep package-specific release URLs, build/install policy, and unusual
verification rules small. `PackageRecipe` owns ordinary Debian metadata;
`common/` owns temporary workspace cleanup, retries, archive traversal checks,
canonical control/provenance rendering, `.deb` construction, idempotency, and
invocation of the existing repository publisher. For a normal CMake package,
the recipe can be as small as:

```python
class ExampleRecipe(PackageRecipe):
    name = "example"
    repository = "mattos"  # REQUIRED: "mattos" or "mattpackages"
    description = "An ordinary native application"
    depends = ("libc6",)

    def discover_version(self):
        return github_latest_release("owner", "example")

    def build(self, workspace, version, provenance):
        archive = workspace / "source.tar.gz"
        tag = provenance["release_tag"]
        url = github_source_archive("owner", "example", tag)
        download(url, archive)
        source = extract_archive(archive, workspace / "source")
        staging = workspace / "package"
        cmake_build_install(source, workspace / "build", staging)
        provenance = {**provenance, "source_url": url,
                      "source_sha256": sha256_file(archive)}
        return finalize_package(self, staging, workspace, version, provenance)
```

Versions are upstream versions; a package rebuild with the same upstream
version is intentionally rejected/skipped by repository identity. A recipe
must use checksums or signatures whenever its upstream provides them and must
fail closed when a declared verification step fails. Runtime dependencies are
declared in `Depends`; host libraries are never copied into a package.

The publisher requires the usual MattOS repository configuration/token and
uses the vendored LinuxScripts implementation. Publication is per complete
`.deb`; the server atomically validates and indexes it. Tests use local
fixtures and mocked publisher/build functions; this suite never uploads.

`repository` is required on every recipe and is not a command-line option.
Use `mattos` for packages intended for the MattOS distribution. Select
`mattpackages` only for a package intentionally maintained in that separate
repository; the framework accepts no other value and never silently defaults.

## Builder image and bulk updates

`third-party-packages/Containerfile` is the canonical, rootless-compatible
build environment for every recipe. The framework builds or reuses its local
image through Podman (preferred) or Docker; set
`MATTOS_THIRD_PARTY_CONTAINER_ENGINE` only to explicitly choose one. A recipe
does not need to implement container handling.

`DevUtils/BuildAndUploadThirdPartyPackages.py` discovers immediate recipe
modules, groups them by their recipe-owned repository, and prints a colored
table with **upstream**, **selected**, and **repo** versions. Results include
**UP TO DATE**, **UPSTREAM NEWER**, **PENDING PUBLISH**, **UPLOADED**,
**DRY RUN**, or **FAILED**. `--recipe NAME` restricts the run; `--check`
is strictly read-only; and `--dry-run` preserves the normal publication
command without uploading.
