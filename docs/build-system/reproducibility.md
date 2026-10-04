# Reproducibility across build machines

A package's bytes must depend only on the MattOS source and the pinned
toolchain, not on the machine, checkout, user, clock, IDE or earlier
interrupted builds. Publishing compares each package with its hosted copy
(see [Publishing Packages](../packaging/publishing.md)), so a difference
between two machines' builds of the same commit becomes a needless packaging
revision on every package that differs, and on every package pinning one of
them exactly.

The rules below come from comparing two machines' builds of one commit. Each
one closes a specific way in which host state reached package payloads.

## Build command environment

Every build command (`run_logged_command` in
`src/tools/mattos-build/src/performance.rs`) gets the fixed `LC_ALL`, `LANG`,
`TZ` and `SOURCE_DATE_EPOCH` (see [performance](performance.md)), and:

- It cannot see the MattOS checkout's Git repository.
  - `GIT_CEILING_DIRECTORIES` is the checkout root, and no system or global
    Git configuration applies.
  - Vendored trees have no `.git`, so `git describe` in FFmpeg, mpv,
    iputils, libass (Meson `vcs_tag`), Duktape, OpenCV and brush
    (`git_version!`) used to find the checkout. It embedded the checkout's
    commit and dirty state.
- It does not inherit the IDE detection variables `VSCODE_CLI`, `QTC_RUN`
  and `CLION_IDE`.
  - When QtBase sees one, it builds `syncqt` at configure time and leaves it
    out of its SBOM.

## Generated names and stamps

- **pkg-config overlays.** The directory name of a consumer's overlay
  (`out/build/.pkgconfig-overlays/<digest>`) is derived only from:
  - the repo-relative descriptor directories;
  - each `.pc` file's name and bytes, with the checkout path replaced by a
    placeholder.

  It is not derived from file owners, modes, the checkout path or producer
  manifests. CPython records its configure environment, including
  `PKG_CONFIG_PATH`, in `sysconfig`.
- **Qt SBOMs.** The document namespace is fixed with
  `QT_SBOM_FORCE_DOCUMENT_NAMESPACE_INFIX_UUID_CONTENT=mattos`.
  - Otherwise Qt hashes the host CMake version, and, on a fresh configure,
    an architecture probe that has not run yet.
  - Every Qt module's SBOM refers to qtbase's.
- **KDevelop app templates.** ECM is given the build's GNU tar
  (`_tar_executable`), so the template archives are sorted, owned by root
  and dated `SOURCE_DATE_EPOCH`.
  - Under the isolated CMake search policy, ECM's own search found no tar
    and fell back to `cmake -E tar`, which follows directory order and
    records the builder's uid and gid.
- **Build dates.** Output-mirror patches make two sources use
  `SOURCE_DATE_EPOCH`:
  - keyutils' `keyutils_build_string`;
  - KSyntaxHighlighting's generated Jinja syntax versions.

  GRUB's manual sources get the normalized mtime, because `mdate-sh` dates
  the info manuals from it.
- **Kernel and NVIDIA.**
  - Kconfig's host-tool probes are disabled with `RUSTC=false`,
    `BINDGEN=false` and `PAHOLE=false`, so the host rustc version no longer
    reaches `kheaders.ko`.
  - The NVIDIA build stamp's user, host and date are pinned.

## Host tools that must not decide outputs

- **Shaders.** `qsb -O` runs whatever `spirv-opt` `PATH` provides and keeps
  unoptimized SPIR-V when it fails.
  - MattOS does not build SPIRV-Tools.
  - Qt and KDE builds put a failing `spirv-opt` first on `PATH`
    (`out/host-tools/qt-build-overrides/bin`), so every machine ships the
    same unoptimized shaders.
- **Meson dependencies.** Meson builds get a machine file with
  `cmake = 'false'`.
  - A dependency missing from a recipe used to fall back to CMake discovery
    of the host's library and headers. Polkit and Mesa linked the host's
    Expat this way, and Mesa compiled against host Wayland headers.
  - A missing declared dependency now fails the build.
- **KDE ECM.** Only the pinned ECM, or an explicit `MATTOS_HOST_ECM_DIR`, is
  used, never one installed on a KDE desktop.
- **cargo.** cargo's curl-sys always builds its vendored libcurl
  (`LIBCURL_NO_PKG_CONFIG=1`). It used to link the system libcurl only when
  the host's `curl-config` reported HTTP2.
- **GnuPG.** The mail program is fixed (`--with-mailprog=/usr/sbin/sendmail`,
  as in Debian) instead of whatever `sendmail` the host has.
- **cryptsetup.** Translations are required outputs, built with gettext's
  `msgfmt` (provisioned by `DevUtils/setup.py`). Configure used to build
  them only when the host happened to have `msgfmt`.

## Build order and RPATHs

- **QML.** Qt and KDE builds first build every QML module's type
  registration and copied QML files, then the rest.
  - qmlcachegen depends only on its own module's types.
  - A file importing a sibling module could be compiled before that module's
    types existed, so the AOT-compiled code depended on job count and
    timing.
- **RPATHs.** CMake components keep `CMAKE_SKIP_RPATH=ON`.
  - Exceptions: components whose programs load private libraries (Discover,
    Elisa). They link with their install RPATH
    (`CMAKE_BUILD_WITH_INSTALL_RPATH`), so no absolute build-tree RUNPATH
    reaches the linked bytes or the GNU build ID.
  - An unconditional upstream `$ORIGIN/../lib/<multiarch>` (Exiv2,
    KDSingleApplication) is not shipped.

## Interrupted builds

- **Kernel.**
  - The kernel stage marks its build tree incomplete until the build
    finishes, and discards a tree an unfinished build touched.
  - Kbuild writes an object before appending its symbol-version CRCs, so a
    build killed in between left objects whose exports got CRC 0 in every
    later build of that tree.
  - The build also fails if `Module.symvers` has an export without a CRC.
- **Checkout line endings.** The repository stores every file byte for byte
  (`* -text`).
  - A checkout made before that rule may still have converted working files
    (`git ls-files --eol` shows `i/lf w/crlf`), which Qt installs.
  - Re-checking out those files restores the committed bytes.
