# MattOS source ownership

MattOS treats every first-class source tree listed in `upstream/sources.toml` as the authoritative copy of that project for in-tree builds.

## Invariant

If a MattOS subproject depends on a project MattOS already owns, the build must consume the MattOS-owned source or the output built from that source. It must not silently download another Git revision, crates.io copy, system `-dev` package, Meson wrap, CMake `FetchContent` checkout, or equivalent duplicate of an owned package that is present locally.

The vendored source manifest is authoritative for the dependency version. Consumers do not duplicate or pin a second MattOS-local version number. Updating an owned source tree therefore moves all in-tree consumers to that source together; an incompatible consumer must be patched or fail explicitly instead of falling back to another copy.

## Cargo

`DevUtils/generate_source_overrides.py` reads `upstream/sources.toml`, enumerates tracked `Cargo.toml` files through Git, discovers Cargo packages under each first-class source root, and writes an ignored repo-local `.cargo/config.toml` containing Cargo `[patch]` entries.

A package becomes a project-wide canonical Cargo owner only when it is the root package of a first-class component. Nested crates inside large imported projects remain private implementation details unless another manifest explicitly depends on that component's Git repository and requests that nested package by name. This prevents Rust compiler shims, fixtures, tests, and deliberately duplicated package names from becoming accidental global owners.

This distinction also gives COSMIC the intended behavior: the first-class `src/desktop/cosmic/iced` component owns the `iced` root package, so libcosmic cannot keep using its embedded iced copy. Git dependencies on an owned component repository are rebound to packages that actually exist in the corresponding local source tree.

`src/tools/mattos-build/build.rs` regenerates the override configuration before MattOS child Cargo builds and tracks `upstream/sources.toml`, the generator, and tracked Cargo manifests as build-script inputs. The generated config is ignored because it is derived state.

If an owned package exists locally but an in-tree path dependency resolves to another copy, generation fails. If a Git dependency points at an owned repository but requests a package that is not present in MattOS's imported copy of that repository, the generator leaves that edge external because there is no local source to substitute; importing that package into MattOS is the prerequisite to claiming ownership of it.

## Native build systems

Native libraries remain source-owned through the MattOS build graph and staged sysroot. Meson, CMake, Autotools, Make, and Rust build scripts that consume native libraries must resolve headers, pkg-config metadata, link libraries, and runtime closure from MattOS-built component outputs rather than matching host development packages.

A runtime relationship is not automatically a rebuild relationship. For example, PipeWire being part of the complete COSMIC desktop runtime does not by itself make PipeWire source or package output a compile-time input to `cosmic-panel`. Stage dependency/cache graphs must model actual build and ABI inputs separately from runtime/image composition dependencies.

## Cache identity

Source ownership changes dependency identity. Stage cache inputs must ultimately describe the canonical MattOS source/output that was used to produce an artifact, not the external Git or registry reference that was overridden. This keeps source closure and incremental correctness aligned: changing an owned library invalidates real consumers, while changing an unrelated runtime component does not fan out into needless recompilation.

## Maintenance

Run:

```text
python3 DevUtils/generate_source_overrides.py
python3 DevUtils/test_source_ownership_overrides.py
```

The first command regenerates the ignored Cargo override file. The second exercises the source-ownership invariants, including the libcosmic/iced collapse that motivated this mechanism.
