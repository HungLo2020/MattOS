# Goals and Vision

- MattOS should eventually be fully self-hosting: a running MattOS system must contain the compilers, linkers, interpreters, package tools, and other development utilities required to rebuild MattOS and generate its packages, repository, and bootable ISO.
- Self-hosting does not require a completely offline build. MattOS may download pinned source and build dependencies through normal systems such as Cargo or project build tools.
- Builds should also be possible from an already populated local dependency cache when network access is unavailable.
- MattOS targets binary package compatibility with Debian 13 (Trixie) while retaining a MattOS-built and MattOS-controlled critical base. MattOS packages take precedence over Debian packages, and Debian repositories are used only to supplement optional software without replacing protected system infrastructure.
- MattOS targets broad binary compatibility with the wider Linux ecosystem through `mattos-compat`, which hosts other distributions' userlands under `/compat`; for ease of maintenance the MattOS distribution itself follows Debian.
- arm64, x86_64, risc-v, and UML are our intended architectures this project supports.
- Tentatively planning on using the COSMIC desktop stack, login, etc.
