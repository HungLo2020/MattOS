# MattOS

MattOS is a Linux-compatible OS project with upstream source imported directly as ordinary tracked files in one repository.

- **Wiki:** <https://hunglo2020.github.io/MattOS/>
- **Source:** <https://github.com/HungLo2020/MattOS>

## Documentation

All documentation lives in the wiki under `docs/`, organized as a strict
hierarchy of systems and subsystems: every directory has an `index.md` that
links to each note in it and to the index of each subdirectory. The hosted wiki
above renders the same files for easier reading and navigation; preview it
locally with `python3 DevUtils/RunWiki.py`.

- [Wiki home](docs/index.md)
- [Project](docs/project/index.md): goals, rules, repository layout, licensing, milestones
- [Guides](docs/guides/index.md): installing and using MattOS
- [Build System](docs/build-system/index.md): building MattOS, the stage graph and cache, the toolchain bootstrap
- [Sources](docs/sources/index.md): importing, pinning, owning and syncing upstream source
- [Packaging](docs/packaging/index.md): Debian-format packages, the APT repository, current Debian compatibility, publishing
- [System](docs/system/index.md): boot, installer, services, networking, userland

To build MattOS, start with [Building MattOS](docs/build-system/building.md)
and the [build commands](docs/build-system/commands.md).

## Agents

`AGENTS.md` is a symlink to this README. Agents working in this repository
follow the guidance below.

* Before working on anything, read the documentation for it: start at the [wiki home](docs/index.md), always read the [project rules](docs/project/rules.md) and [goals](docs/project/goals.md), then follow the index pages down to every system and subsystem the task touches and read all of their related notes. Search `docs/` for the components, files and terms involved, and read what you find before changing code.
* Keep the documentation current: when a change alters documented behavior, update the affected notes in the same change.
* Keep the wiki's strict hierarchy: put each new note in the directory for its system or subsystem (creating a subdirectory with its own `index.md` when a topic grows), link it from that directory's `index.md`, and link between notes with relative Markdown links. Only top-level sections are linked from this README. `python3 DevUtils/check_wiki_structure.py` checks the hierarchy.
* MattOS is a monorepo Linux distribution intended to contain editable source for its primary runtime libraries, system components, tools, and first-class programs.
* Transitive dependencies do not all need vendored source. For example, Rust crates statically linked into a first-class MattOS program may be fetched normally.
* MattOS should eventually be self-hosting, but rebuilding MattOS may require network access.
* The installer ISO itself must contain everything needed to install its supported base profiles without internet access.
* Prefer Rust for new MattOS-owned software where practical, but do not rewrite mature upstream software merely for language purity.
* Vendored upstream source must be pinned to exact immutable commits and kept as close to upstream as practical.
* Vendored source may be deliberately pruned during import when MattOS does not support that functionality. Omissions must be explicit, reproducible, provenance-tracked, and must not impair supported builds or future upstream updates.
* Prefer deterministic import policies over manually deleting files from vendored trees. Unsupported architectures, platforms, tests, tooling, documentation, or other upstream content may be excluded only through documented source-selection policy.
* Avoid modifying retained vendored source directly. Prefer small, documented patches applied to output-owned source mirrors.
* Generated files and build outputs must never be written into authoritative vendored source trees.
* MattOS targets broad binary compatibility with the broader linux ecosystem with mattos-compat. basically allows for hosting other distros userland in /compat. for ease of maintainance howerver the mattos "distro" itself should still try and follow debian.
* The Rust MattOS build tooling is the canonical build orchestration layer. Reuse it instead of creating parallel ad-hoc build systems.
* Never solve target dependencies by copying host binaries or runtime libraries into MattOS.
* Prefer root-cause fixes, preserve reproducibility and source provenance, and add focused regression tests for defects.
* Do not modify or publish through LinuxScripts unless explicitly instructed.
* Never stage, commit, stash, reset, clean, merge, rebase, tag, push, publish, or otherwise alter Git history/index unless explicitly instructed in the current session.
* Leave changes unstaged and uncommitted by default.
* Do not stop a session merely because a required healthy build or test is still running.
* Give final session reports directly in chat, not in report files, unless explicitly requested.
