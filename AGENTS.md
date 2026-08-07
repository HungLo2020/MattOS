# MattOS Agent Guidance

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
* MattOS targets broad binary compatibility with Debian 13 Trixie. MattOS packages should take precedence over Debian packages for the MattOS base system.
* The long-term installer should support desktop and terminal/server profiles. COSMIC is the planned desktop environment and COSMIC/Pop!_OS installer technology is the planned installer direction.
* The Rust MattOS build tooling is the canonical build orchestration layer. Reuse it instead of creating parallel ad-hoc build systems.
* Never solve target dependencies by copying host binaries or runtime libraries into MattOS.
* Prefer root-cause fixes, preserve reproducibility and source provenance, and add focused regression tests for defects.
* Do not modify or publish through LinuxScripts unless explicitly instructed.
* Never stage, commit, stash, reset, clean, merge, rebase, tag, push, publish, or otherwise alter Git history/index unless explicitly instructed in the current session.
* Leave changes unstaged and uncommitted by default.
* Do not stop a session merely because a required healthy build or test is still running.
* Give final session reports directly in chat, not in report files, unless explicitly requested.
