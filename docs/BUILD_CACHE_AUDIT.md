# Build Cache Audit

Date: 2026-07-31
Scope: read-only audit of current cache and incremental behavior after source layout migration to `src/`.

## Current Behavior

### Cargo and Rust

| Mechanism | Path | Component | Persistent | Invalidated by | Assessment |
|---|---|---|---|---|---|
| Workspace target dir | `target/` | `mattos-build`, `mattos-init` | Yes | `cargo clean`, profile/flag changes, compiler version changes, source changes | Correct and standard |
| Brush crate target dir | `src/userland/brush/target/` | Brush upstream shell build | Yes | `cargo clean` in Brush dir, source changes, release profile changes | Correct but separate from workspace cache |
| Coreutils target dir | `src/userland/coreutils/target/` | uutils/coreutils build | Yes | `cargo clean` in coreutils dir, feature changes (`unix,feat_Tier1`), source changes | Correct but separate from workspace cache |
| Rust incremental metadata | under each target dir | all Rust components | Yes | profile, rustc changes, source graph changes | Used implicitly; no custom override |
| `CARGO_TARGET_DIR` override | not configured | all MattOS-owned crates | N/A | N/A | Not used |
| `sccache`/`RUSTC_WRAPPER` | not configured in MattOS-owned tooling | all MattOS-owned crates | N/A | N/A | Not used |

Notes:
- `mattos-build` invokes Brush/Coreutils builds from their own directories, so they do not share `target/` with workspace crates.
- `mattos-build` and `mattos-init` do share root workspace `target/`.

### Linux kernel

| Mechanism | Path | Component | Persistent | Invalidated by | Assessment |
|---|---|---|---|---|---|
| In-tree Kbuild outputs | `src/kernel/linux/` | Linux kernel | Yes | config changes, source changes, toolchain changes, explicit clean targets | Correct but monolithic |
| Seed config staging | `src/kernel/config/x86_64_mattos.config` copied to `src/kernel/linux/.config` each build | Linux kernel | `.config` persists | seed config content changes | Correct for reproducibility |
| `make olddefconfig` | in `src/kernel/linux` | Linux kernel | N/A | rerun each kernel build | Correct; may touch config-derived artifacts |

Notes:
- Build uses in-tree output (no `O=` out-of-tree object directory).
- `clean artifacts` does not remove kernel build objects, preserving Kbuild reuse.

### Brush and uutils

| Mechanism | Path | Component | Persistent | Invalidated by | Assessment |
|---|---|---|---|---|---|
| Brush release build cache | `src/userland/brush/target/release/` | Brush | Yes | Brush source/feature/toolchain changes | Correct |
| Coreutils release build cache | `src/userland/coreutils/target/release/` | Coreutils | Yes | Coreutils source/feature/toolchain changes | Correct |

Notes:
- ISO assembly step copies prebuilt binaries; if binaries are unchanged, Cargo should reuse prior artifacts.

### Rootfs, initramfs, and ISO

| Mechanism | Path | Component | Persistent | Invalidated by | Assessment |
|---|---|---|---|---|---|
| Rootfs staging | `out/build/rootfs` | initramfs assembly | Recreated | every `build rootfs` / `image` | Deterministic but always rebuilt |
| Initramfs archive | `out/build/initramfs.cpio.gz` | boot artifact | Recreated | every `build initramfs` / `image` | Deterministic but always rebuilt |
| ISO staging | `out/build/iso` | ISO assembly | Recreated | every `build iso` / `image` | Deterministic but always rebuilt |
| ISO artifact | `out/images/mattos-x86_64.iso` | boot artifact | Rewritten | every `build iso` / `image` | Correct output location |

Notes:
- No timestamp/freshness graph exists for rootfs/initramfs/ISO; these layers are rebuilt each image run.
- Staging directories are fully recreated, so stale staging files are unlikely.

### Upstream import/sync tooling

| Mechanism | Path | Component | Persistent | Invalidated by | Assessment |
|---|---|---|---|---|---|
| Temporary clones/merge repos | `upstream/.tmp/` | upstream import/sync | Ephemeral | per import/sync run or `clean all` | Correct for safety, not optimized for repeated fetches |
| Upstream state metadata | `upstream/state/*.toml` | provenance tracking | Yes | successful import/sync metadata updates | Correct |
| Imported source trees | `src/kernel/linux`, `src/userland/brush`, `src/userland/coreutils` | working source | Yes | explicit import/sync or manual edits | Correct |

### Other cache layers

| Mechanism | Path | Component | Persistent | Invalidated by | Assessment |
|---|---|---|---|---|---|
| Rootless local toolchain | `.tools/rootless/usr` | host build tooling fallback | Yes | manual replacement/removal | Correct and intentionally persistent |
| QEMU logs | `out/logs/` | runtime diagnostics | Yes | `clean logs` / manual removal | Correct |
| Ignored generated outputs | `.gitignore` entries for `out/`, `target/`, `upstream/.tmp/`, kernel byproducts | repository hygiene | N/A | N/A | Correct |

## Confirmed Problems

1. No build-graph freshness tracking for `rootfs`, `initramfs`, and `iso`; these are always regenerated when `image` runs.
2. Upstream import/update uses fresh shallow clones in `upstream/.tmp/` and does not keep a reusable local mirror cache.
3. Kernel build is in-tree; build artifacts remain in imported Linux tree and are not isolated in a dedicated out-of-tree output dir.
4. Rust caches are split between workspace (`target/`) and upstream userland subprojects (`src/userland/brush/target`, `src/userland/coreutils/target`), which is functional but duplicates cache surfaces.

## Potential Improvements (Future Task)

1. Add optional freshness checks for `rootfs`, `initramfs`, and ISO generation to skip unnecessary rebuilds.
2. Add optional `O=` kernel out-of-tree build mode while preserving current default behavior.
3. Add optional reusable upstream bare-mirror cache to reduce repeated network transfer during import/sync.
4. Evaluate optional shared `CARGO_TARGET_DIR` strategy for selected sub-builds if cache duplication becomes a bottleneck.
5. Evaluate optional `sccache` integration for local developer builds and CI.

## Current Verdict

Current cache behavior is consistent and reproducible for a baseline milestone build system. The main tradeoff is predictable full regeneration of post-compile image layers rather than maximal incremental optimization.
