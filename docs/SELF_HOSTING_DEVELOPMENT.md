# Self-hosting development foundation

MattOS imports and builds its first complete installed language-development
foundation from pinned source: CPython, LLVM/Clang/LLD, and Rust/Cargo. These
components extend the existing native GCC/Binutils/Make toolchain; they do not
yet claim that every image-construction step is executable inside MattOS.

## Provenance and bootstrap boundary

`upstream/sources.toml` pins every imported Git tree to an exact commit. Import
materialization reads Git blobs directly, preserving bytes, executable modes,
and symlinks without allowing checkout attributes or component `.gitignore`
rules to alter the authoritative copy. Import and sync commands update only the
working tree and `upstream/state`; they never stage paths in the caller's Git
index.

CPython 3.14 is built against MattOS-owned OpenSSL, zlib, bzip2, xz, Expat,
ncurses, and libffi. The install is split into runtime, shared-library, virtual
environment/ensurepip, and development ownership boundaries.

LLVM builds Clang and LLD with X86, AArch64, and RISC-V backends. Only X86_64 is
an executable MattOS target in this milestone; retaining the other backend
descriptions avoids baking the current architecture into the stage model.

Rust is built from the checksummed official source release, whose vendored
Cargo dependency closure and stage-0 metadata are part of the release. The
host/bootstrap compiler is an explicit build input. The installed compiler uses
the MattOS LLVM, linker, C runtime, and GCC runtime closure. Bootstrap downloads
remain output-owned and are never copied into the imported Rust tree.

## Package ownership

Development files are deliberately separate from runtime ABI owners. Runtime
packages own versioned shared objects; development packages own headers,
unversioned linker names, pkg-config/CMake metadata, and compiler support files.
The interpreter/compiler packages own commands and their language-specific
runtime trees. No package is permitted to overlap another package's path.

The protected-package and APT pinning policies reserve these MattOS-owned paths
against replacement by a supplemental Debian repository. Compatibility entries
document where the newer language ABI is a MattOS extension rather than a claim
to be the exact Debian 13 default.

## Validation boundary

The milestone is complete only when a fresh image boots and performs native
Python, Clang C/C++, LLD, rustc, Cargo, and existing-MattOS-Rust-component smoke
tests. Building `mattos-build` inside the guest is attempted separately because
full image/package construction may still require host-only namespace, mount,
QEMU, or ISO tooling. Any such boundary must be reported rather than hidden.

Per-stage source inputs and dependency manifests keep changes scoped: CPython
changes do not rebuild LLVM or Rust, while LLVM changes invalidate Rust because
Rust deliberately consumes the installed MattOS LLVM. Linux and GCC remain
unaffected unless their own source or output identities change.
