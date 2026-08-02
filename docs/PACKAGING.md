# MattOS Debian Packaging

MattOS uses the Debian binary package format, `dpkg` as the low-level installer and package database, and APT as the planned dependency-resolving frontend. This choice provides a mature archive format, file ownership database, conffile semantics, and repository metadata without making Debian or Ubuntu a source of MattOS base-system binaries.

The source monorepo and the generated package repository have different roles. Editable component source and MattOS packaging policy live in this repository. Binary packages and APT indexes are reproducible build artifacts under `out/`; they are not source and remain ignored by Git.

## Imported package-manager sources

Both imports are ordinary editable files with no nested `.git` directories:

| Component | Official repository | Branch | Imported commit | Destination |
| --- | --- | --- | --- | --- |
| dpkg | `https://git.dpkg.org/git/dpkg/dpkg.git` | `main` | `ff7e9d8bf01379e8b022028a65afaa262e2c25cd` | `src/system/packages/dpkg/` |
| APT | `https://salsa.debian.org/apt-team/apt.git` | `main` | `5e6dcc8d0c8bdce61e9cc7f497abadb5349d509a` | `src/system/packages/apt/` |

`upstream/sources.toml` is the manifest and `upstream/state/{dpkg,apt}.toml` records the URL, branch, exact commit, import time, destination, and copy-sync method. The normal component-scoped `upstream import`, `upstream status`, and conflict-exposing `upstream sync` commands apply.

## Naming, versions, and architecture

MattOS-owned binary packages use the `mattos-` prefix. Names follow Debian's lowercase package-name grammar. The initial architecture is Debian's `amd64`, even though ISO filenames use `x86_64`.

Versions are deterministic:

```
<upstream-version>-<MattOS-revision>
```

The first revision is `1mattos1`, producing versions such as `0.4.0-1mattos1`. Mutable timestamps are never package versions. Source repository, exact commit, MattOS path, build configuration, runtime library inventory, package version, and architecture are installed in `/usr/share/doc/<package>/mattos-build-info.toml`.

## Commands and output

```
cargo run -p mattos-build -- package build mattos-brush
cargo run -p mattos-build -- package build --all
cargo run -p mattos-build -- package repo
cargo run -p mattos-build -- package inspect mattos-brush
cargo run -p mattos-build -- package status
```

Staging trees and packages are written to:

```
out/packages/staging/<package>/
out/packages/amd64/<package>_<version>_amd64.deb
out/packages/inventory.toml
```

The inventory records name, version, architecture, artifact path, source component, package dependencies, legacy runtime-library closure, file count, and SHA-256. Package timestamps are normalized with a fixed `SOURCE_DATE_EPOCH`; `dpkg-deb --root-owner-group` records root ownership without requiring root on the host. Package modes are normalized, symlinks remain symlinks, and no maintainer scripts are used.

## Prototype ownership

| Package | Owned content | Runtime relationship |
| --- | --- | --- |
| `mattos-filesystem` | core directories and merged-`/usr` structural symlinks | no package dependency |
| `mattos-base-files` | MattOS identity, hostname default, issue, profile, shells, and local-only APT source | depends on `mattos-filesystem` |
| `mattos-brush` | `/usr/bin/brush` | depends on `mattos-filesystem`; library SONAMEs are recorded as a bootstrap closure |
| `mattos-coreutils` | `/usr/bin/coreutils` and discovered non-conflicting applet symlinks | depends on `mattos-filesystem`; provides/conflicts/replaces `coreutils` |
| `mattos-curl` | `/usr/bin/curl`, matching `libcurl.so.4` runtime | depends on `mattos-filesystem`; provides/conflicts/replaces `curl`; CA bundle remains legacy-owned |

Directories may be shared. Two packages may not own the same regular file, symlink, configuration file, or command alias. The builder rejects non-directory collisions before archive creation. The coreutils package consumes the existing `coreutils --list` inventory and excludes commands supplied by another selected component; there is no second hand-maintained applet list.

`mattos-base-files` marks mutable baseline files as conffiles. `/etc/os-release` is immutable package identity. Live-only passwd/shadow state, autologin overrides, the live sudo policy, live notice, live home skeleton, and live tmpfiles policy remain in `src/system/profiles/live` and are not permanent package payloads. Maintainer scripts are prohibited for this prototype; future scripts must be minimal, deterministic, idempotent, documented, and offline.

Dynamic library package dependencies cannot yet be expressed as `Depends` because glibc, OpenSSL, systemd, and other runtime libraries are still installed by legacy rootfs assembly rather than owning MattOS packages. `mattos-curl` carries the `libcurl.so.4` built alongside its executable so the ABI pair cannot drift; the rest of its closure remains legacy-owned. The exact SONAME closure is therefore recorded in `X-MattOS-Legacy-Runtime-Libraries` and provenance instead of inventing Debian package dependencies. Converting those libraries to MattOS packages is required before these fields can become normal dependency relationships.

## Local APT repository

`package repo` creates only MattOS content:

```
out/repository/
├── pool/main/*.deb
└── dists/mattos/
    ├── Release
    └── main/binary-amd64/
        ├── Packages
        └── Packages.gz
```

`dpkg-scanpackages` generates `Packages`; deterministic gzip generates `Packages.gz`; `apt-ftparchive` generates a `Release` file with checksums and MattOS origin/suite/codename fields. The repository is embedded at `/usr/share/mattos/repository`. `/etc/apt/sources.list.d/mattos.sources` uses only `file:/usr/share/mattos/repository`.

The repository is unsigned for this local bootstrap and is explicitly marked `Trusted: yes` only in the local file source. This exception must never be copied to an HTTP(S) repository. Online publication requires repository signing, key rotation policy, release promotion, and removal of the local trust exception.

## Bootstrap tools and source builds

Package construction currently uses host `dpkg-deb` and `dpkg`; repository indexing uses host `dpkg-scanpackages`, `apt-ftparchive`, and `gzip`. `zstd`, `xz`, and `tar` are setup prerequisites used by the package toolchain. Root ownership is encoded by `dpkg-deb`, so fakeroot is not required by the implemented path. This is an explicit bootstrap boundary, not self-hosting.

`mattos build dpkg` builds the imported source with Autotools into `out/build/dpkg/install`. A deterministic `.dist-version` and exact `.dist-vcs-id` are added only to its isolated build copy because an ordinary imported Git snapshot has no `.git` directory. The built `dpkg`, `dpkg-deb`, and `dpkg-query` are staged into the image and use `/etc/dpkg` and `/var/lib/dpkg`.

`mattos build apt` builds imported APT 3.3.2 with CMake/Ninja into `out/build/apt/install`, with docs, tests, and NLS disabled and the MattOS vendor selected. APT runtime is intentionally not installed yet. Its current output needs the built `libapt-pkg`/`libapt-private`, method/helper trees, C++ runtime, compression libraries, libudev/libsystemd, OpenSSL, and xxHash. Those dependencies are not yet represented by MattOS packages, so copying them as another unowned closure would weaken the package boundary and risk the bootable baseline.

## Hybrid rootfs migration

The prototype replaces these direct-copy operations:

| Previous rootfs action | Package replacement |
| --- | --- |
| create merged-`/usr` hierarchy and symlinks | `mattos-filesystem` |
| copy baseline identity/profile files from the skeleton | `mattos-base-files` |
| copy Brush binary | `mattos-brush` |
| copy coreutils multicall binary and create applet links | `mattos-coreutils` |
| copy curl command through the component manifest | `mattos-curl` |

Authentication, systemd, D-Bus, networking, non-curl administration tools, runtime libraries, live overlay, rescue init, initramfs, and ISO assembly stay on their proven legacy paths.

Rootfs assembly creates an empty staging root, builds the packages/repository, initializes `/var/lib/dpkg`, installs all five packages through real host `dpkg` semantics, snapshots every package-owned path, and then layers only non-migrated content. Explicit destination checks reject known legacy/package overlaps and the final snapshot rejects any silent change to package-owned files. The real database contains `status`, `available`, `info/`, `updates/`, file lists, md5sums, conffile state, and ownership records. MattOS-built `dpkg-query` can perform `-W`, `-L`, and `-S` inside the guest.

Future milestones can package the runtime libraries, certificates, dpkg itself, APT and its helpers, then progressively remove the remaining legacy-copy boundary. A future installer should assemble targets from this same repository rather than copying the live rootfs. Persistent installation, an online repository, signing infrastructure, and automatic updates are deliberately outside this milestone.
