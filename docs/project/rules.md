# Project Rules

- Every executable, script, and runtime-loaded library installed in MattOS must be built from source as part of the MattOS build process.
- Build-only dependencies that are statically linked into a final artifact, used only during compilation, or fetched through a project’s normal dependency system do not need to become separate installed MattOS components or first-class MattOS packages.
- Every installed file must have a clear source, build path, and package owner. Host binaries and runtime libraries may be used only as explicitly documented temporary bootstrap dependencies.
- Downloaded build dependencies that do not become separate runtime artifacts do not need to be individually installed or managed through APT.
- All source code for MattOS's own components and the software it installs, including the compilers and toolchains that build MattOS, must be contained in this repo. The only exception is build-only dependencies fetched through a project's normal dependency system, as described above.
- The installer ISO itself must contain everything needed to install its supported profiles without internet access.
