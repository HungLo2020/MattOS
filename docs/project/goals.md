# Goals and Vision

- MattOS should eventually be fully self-hosting: a running MattOS system must contain the compilers, linkers, interpreters, package tools, and other development utilities required to rebuild MattOS and generate its packages, repository, and bootable ISO.
- Self-hosting does not require a completely offline build. MattOS may download pinned source and build dependencies through normal systems such as Cargo or project build tools.
- Builds should also be possible from an already populated local dependency cache when network access is unavailable.
- MattOS is its own distribution with a MattOS-built and MattOS-controlled critical base. It uses Debian's package formats and tooling (`.deb`, `dpkg`, APT) and follows Debian conventions where that eases maintenance, so it is currently similar to, and partly binary-compatible with, Debian 13 (Trixie). That compatibility is not a goal or a promise: MattOS can and will diverge from Debian. MattOS packages take precedence over Debian packages, and Debian repositories, where used, only supplement optional software without replacing protected system infrastructure.
- MattOS targets broad binary compatibility with the wider Linux ecosystem through `mattos-compat`, which hosts other distributions' userlands under `/compat`; for ease of maintenance the MattOS distribution itself follows Debian.
- arm64, x86_64, risc-v, and UML are our intended architectures this project supports.
- The desktop is KDE Plasma on Wayland, with the Plasma Login Manager greeter and the Calamares graphical installer, all built from source.
