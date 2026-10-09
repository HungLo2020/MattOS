# Slate text editor

Slate is MattOS's default terminal and graphical text editor. Its complete
upstream source is imported at `src/userland/slate`, with the exact commit in
`upstream/sources.toml` and its tree digest in `upstream/state/slate.toml`.
The current pin is Slate 0.1.8 at
[`32df63dc111cf923db37668c48e142b1777f26ba`](https://github.com/HungLo2020/Slate/commit/32df63dc111cf923db37668c48e142b1777f26ba).

## Packages and builds

The canonical Rust builder has independent `slate` and `slate-gui` stages.
Both copy the pinned workspace into their own output-owned source mirrors
and build with `--locked --release` and the MattOS-built Rust compiler.
The GUI's C++/Qt adapter uses MattOS's GCC, sysroot, Qt Base and Qt
Declarative outputs under a closed CMake package-search policy. Production
packages do not enable upstream's GUI smoke instrumentation.

| Package | Profile | Payload |
| --- | --- | --- |
| `slate` | Base, CLI and Plasma | `/usr/bin/slate`, `editor` symlink, `slate-visual` launcher, upstream manual and documentation |
| `slate-gui` | Plasma | `/usr/bin/slate-gui`, upstream desktop entry, AppStream metadata and icon, system MIME defaults |

The terminal package does not depend on Qt. The graphical package depends
exactly on the terminal package and MattOS Qt Base, Declarative, SVG and
Wayland runtime packages, including the QML modules and platform plugins.
Both packages enter MattOS's normal package inventory and the repository
carried by the installer ISO. MattOS does not consume upstream's prebuilt
Debian package or run its mattpackages build/publishing scripts.

The pinned upstream tree contains no project license declaration. Package
copyright notes record that fact; the MattOS license is not assigned to
imported Slate code. The imported desktop metadata's CC0 declaration applies
to that metadata.

## Defaults and commands

The PAM `/etc/environment` policy, login shells and Plasma sessions default `EDITOR` to `/usr/bin/slate` and
`VISUAL` to `/usr/bin/slate-visual`. Existing environment overrides are
preserved. The visual launcher uses `slate-gui --wait` when a graphical
display is available, so Git and other editor callers wait for their named
documents to close even if Slate is already running. Without the GUI package,
without a display, or in an SSH session, it executes the terminal editor.

Use `slate FILE` for the terminal interface, `slate-gui FILE` for the graphical
interface, or `slate --gui FILE` to hand over to the GUI executable. Use
`slate-gui --wait FILE` for an explicit blocking graphical editor invocation.
Upstream requires named file arguments for `--wait`; directories and stdin
are not blocking document requests. `+LINE:COL` positions the cursor.

The graphical package owns `/etc/xdg/mimeapps.list` as a conffile, selecting
`slate.desktop` for the editor's advertised text and source formats. HTML
keeps its browser default. Per-user MIME preferences take precedence over
these distribution defaults. Kate and Cozy remain available as separately
packaged editors.

Slate can launch optional language servers, formatters, debuggers and external
clipboard tools when installed. Making it the default editor does not install
every optional development tool. Its embedded terminal uses the user's shell.

See [Building MattOS](../../build-system/building.md),
[upstream synchronization](../../sources/upstream-sync.md), and
[Debian packaging](../../packaging/debian-packaging.md) for the build,
provenance and package contracts.

## Local verification

The installed-system QEMU checks run the upstream PTY smoke test shipped as
`/usr/share/doc/slate/examples/tui-smoke.py`. It exercises editing, saving,
find/replace, splits, the embedded shell and layout persistence. CLI verification
checks that the graphical package is absent. Plasma verification checks the MIME
defaults and GUI library closure, opens a text file through `gio`, edits and
saves with real keyboard input, and proves that a second visual-editor caller
waits until the document closes. These checks use the normal production binary.

Run local image and installed-system validation through
`python3 DevUtils/run_qemu.py --build-only` and
`python3 DevUtils/run_qemu.py --install --install-profile plasma`
(or `--install-profile cli` for the terminal-only boundary).
