# MattOS Wiki

MattOS is a from-source Linux distribution: every executable, library and tool it installs is built by the MattOS build system from source kept in one repository. It uses Debian's package formats and tooling and is currently similar to, and partly binary-compatible with, Debian 13 (Trixie); that compatibility is not a design goal or a promise, and MattOS can and will diverge from Debian.

Source code: <https://github.com/HungLo2020/MattOS>

The wiki is organized as a strict hierarchy: every directory has an index page like this one that links to every note in it and to the index of each subdirectory.

## Sections

- [Project](project/index.md): What MattOS is trying to be and the rules every change must follow.
- [Guides](guides/index.md): Using MattOS: installing it and working with an installed system.
- [Build System](build-system/index.md): Building MattOS with the Rust `mattos-build` orchestrator: commands, the stage graph and cache contract, and performance.
- [Sources](sources/index.md): How upstream source is imported, pinned, owned and kept in sync.
- [Packaging](packaging/index.md): Debian-format packages, the MattOS APT repository, current (not guaranteed) Debian 13 compatibility, and publishing.
- [System](system/index.md): The components of a running MattOS system.
