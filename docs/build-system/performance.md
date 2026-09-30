# Build performance and cache model

MattOS uses fail-closed, content-addressed stage and package manifests to make warm builds incremental without weakening source closure, package ownership, Debian compatibility, ELF validation, or release reproducibility.

## State and reports

Generated state is ignored by Git and never enters a package, rootfs, initramfs, or ISO:

```text
out/state/stages/<stage>.json
out/state/packages/<package>.json
out/state/package-facts/<deb-sha256>.json
out/state/audits/package-global.json
out/state/elf/<elf-sha256>.json
out/logs/<stage>.log
out/reports/build-timings.json
out/reports/build-timings.txt
out/reports/stage-memory.json
```

Stage manifests use schema version 3. They contain the stage identifier; source, configuration, tool, normalized environment, and dependency-output digests; the full input digest; the exact field identities used for diagnostics; a deterministic expected-output inventory; and the output-content digest. Output inventory entries include normalized path, kind, mode, owner UID/GID, size, file SHA-256 or symlink target. Maps use canonical key ordering. Wall-clock timestamps exist only in timing reports.

Manifests are written to a sibling temporary path and atomically renamed only after a successful stage and complete output inventory. Repository, rootfs, live-root, initramfs, and ISO misses also build into sibling temporary outputs. The active output is moved aside only at publication, restored if publication fails, and the previous copy is removed only after the validated replacement is active. Incomplete temporary paths are ignored and replaced on the next miss. A failed or interrupted stage never publishes a successful manifest.

## Input digest rules

A compilation stage key includes:

- tracked and non-ignored source inputs belonging to that component;
- MattOS build recipe source and configuration;
- target/build/host choices represented by the recipe;
- relevant compiler, assembler, linker, Make, Cargo, and Rust identities;
- an explicit allowlist of caller-controlled, output-affecting environment;
- the output-content digest of each declared dependency.

The environment allowlist is `CC`, `CXX`, `AR`, `AS`, `LD`, `NM`, `RANLIB`, `STRIP`, `OBJCOPY`, `PKG_CONFIG`, `CFLAGS`, `CXXFLAGS`, `CPPFLAGS`, `LDFLAGS`, `RUSTFLAGS`, `CARGO_TARGET_DIR`, `PKG_CONFIG_PATH`, and `LIBRARY_PATH`. Build subprocesses always receive `LC_ALL=C`, `LANG=C`, `TZ=UTC`, and `SOURCE_DATE_EPOCH=1767225600`; the cache records that fixed policy rather than the caller's locale, timezone, or epoch. `PATH` is used only to resolve a selected tool. The identity stored for that tool is its canonical executable path, executable SHA-256, stable first version line, and target triple where applicable. Raw `PATH`, `HOME`, `PWD`, terminal/color/dimension variables, logging verbosity, QEMU variables, temporary/log paths, shell state, and launcher options are not output identities.

Dependency manifests retain both upstream input and output digests in their diagnostic details, but only the output-content digest participates in a consumer key. This preserves true dependency invalidation while preventing a rebuild that republishes byte-identical output from causing a false cascade.

Documentation directories and conventional upstream README/change-log/license files are excluded from compilation source digests. Package keys separately include source documentation because licenses or other source documents can be installed into package payloads. Timestamps are not cache keys.

A stage is reused only when its schema and full input digest match, every expected output exists, the complete output inventory/content/mode/symlink digest matches, and stage-specific lightweight validation succeeds. Missing files, unexpected files, mode changes, symlink changes, compiler changes, configuration changes, dependency changes, or content corruption force a rebuild.

## Foundational key audit

All nine foundational stages use schema 3 and the normalized environment/tool policy above. Their remaining stage-specific inputs are:

| Stage | Source (output-affecting) | Configuration (output-affecting) | Tools | Dependency outputs | Validated outputs |
| --- | --- | --- | --- | --- | --- |
| `cross-toolchain` | Binutils and GCC trees; glibc version header | none | GCC, G++, assembler, linker, Make | none | stage-0 cross Binutils and pass-1 GCC install, prerequisite install, hardening specs, configure record |
| `linux` | Linux tree; `x86_64_mattos.config` and policy | none | GCC, G++, assembler, linker, Make | cross-toolchain | `bzImage`, module tree, kernel release |
| `linux-headers` | selected Linux x86 UAPI files | none | Make, GCC | glibc | exported headers and header inventory |
| `glibc` | glibc and Linux trees | none | GCC, G++, assembler, linker, Make | cross-toolchain | glibc install, UAPI export/inventory, controlled sysroot libc/loader |
| `gcc-runtime` | GCC tree | none | GCC, G++, assembler, linker, Make | glibc, Linux headers, cross-toolchain | runtime and development install trees plus sysroot `libgcc_s`/`libstdc++` |
| `binutils` | Binutils tree | none | GCC, G++, assembler, linker, Make | GCC runtime, cross-toolchain | native Binutils install tree |
| `gcc-compiler` | GCC tree | none | GCC, G++, assembler, linker, Make | Binutils, GCC runtime, cross-toolchain | native GCC compiler install tree |
| `make` | GNU Make and gnulib trees | none | GCC, G++, assembler, linker, Make | GCC compiler, Binutils, GCC runtime, cross-toolchain | native GNU Make install tree |
| `formal-sysroot` | none | none | GCC, linker | Linux headers, glibc, GCC runtime | representative UAPI/glibc/GCC headers, loader, and libraries |

No foundational stage has separate configuration files: the recipe implementation (`stages/toolchain.rs`, projected per stage as described in [Build-System Architecture](architecture.md#cache-contract)) is part of the source input, and the recipe string, recipe revision, and manifest schema are part of the key. The builder's own `Cargo.toml`/`Cargo.lock` are not inputs.

Source, recipe/configuration, normalized build environment, selected tool identities, schema, and dependency output bytes are genuinely output-affecting. Expected-output inventories and stage-specific checks are validation-only and never create an input miss. Quiet logging and `MATTOS_VERBOSE_BUILD_OUTPUT` are logging-only. Python launcher arguments, QEMU settings, and terminal state are launcher-only. Raw `PATH`, process IDs, temporary paths, and log paths are volatile or path-normalizable and are excluded. All sets/maps are stored in deterministic order; diagnostics explicitly report that no ordering-only difference exists.

## Foundation and consumers

The cached foundation covers the stage-0 cross toolchain, Linux and its UAPI export, glibc, GCC runtime, Binutils, native GCC/G++, GNU Make, and the formal `out/sysroot` inventory. The formal sysroot depends on Linux headers, glibc, and GCC runtime manifests. Foundation hits validate required headers, loader/runtime libraries, native compiler/binutils/Make files, and complete stored inventories without rerunning the full rootfs ELF audit.

Native consumers declare the formal sysroot plus the source-built libraries they actually consume. The former broad consumer-output deletion is gone. An unchanged stage is skipped; a changed stage retains its build directory and lets Make, Ninja, CMake, Meson, Autotools, or Cargo perform their own incremental work. A dependency digest change forces the consumer stage even when the native build system would not notice an external library replacement. Components whose own build functions require a clean rebuild retain that local behavior; no cache miss deletes unrelated component outputs.

## Repository and image layers

The dependency chain is `source stages -> .deb artifacts -> repository -> rootfs -> live-root -> ISO`, with the early initramfs built separately from `formal-sysroot` and `linux` and the repository also placed directly on the ISO. The repository key is the ordered package inventory (name, version, architecture, artifact SHA-256 and repository-visible metadata), trixie/main/amd64 identity, Origin/Label, deterministic gzip and Release settings, tool versions, epoch, recipe, and manifest schema. A hit validates the complete stored tree, pool/index consistency, `Packages.gz`, and every SHA-256/size entry in `Release`; it does not copy packages, scan indexes, compress, or regenerate Release.

The rootfs uses safe in-place reuse: timestamps are deliberately outside content identity. Hits validate the complete path/content/mode/owner/symlink inventory plus required dpkg, repository, Brush `sh`/`bash`, systemd, and mutable-state invariants. Misses assemble a fresh sibling tree and atomically swap it into place. No hard links, reflinks, privileged mounts, or filesystem-specific feature are required. The rootfs is still one tree that the `live-root` stage compresses into the SquashFS the live medium mounts beneath a tmpfs overlay; `rootfs-base` and `rootfs-live` are diagnostic aliases for this single `rootfs` stage, and the key/recipe boundary leaves room for a future immutable package base plus installed/live overlays.

The `live-root` stage compresses the rootfs into `out/build/live-root.squashfs` (zstd level 12, 1 MiB blocks, all files root-owned); its identity is the rootfs output digest, the `mksquashfs`/`unsquashfs` tool identities, and its recipe revision. Early-initramfs identity is the `formal-sysroot` and `linux` output digests, `src/boot/live-init.c`, the module loader header, selected firmware, the image recipe, and the `gcc`/`cpio`/`xz`/`modinfo` tool identities; the archive is `out/build/early-initramfs.cpio.xz`, built with timestamps set to the fixed epoch, sorted cpio order, root ownership, and `xz -1 --check=crc32`. ISO identity covers the Linux, live-root, initramfs, GRUB, and repository output digests, the authoritative GRUB configuration, xorriso, and the image recipe. Hits verify stored inventories and stage-specific checks and do not run mksquashfs, cpio, xz, GRUB staging, or xorriso.

## Package reuse

Each package has an independent cache manifest. Its key includes schema, package definition, name/version/architecture, source component content, payload-producing stage digests, resolved dependency/control metadata, provenance inputs, `SOURCE_DATE_EPOCH`, and `dpkg-deb` format/ownership/zstd level 19 settings.

Reuse requires the staging payload inventory/content/modes, artifact SHA-256, expected artifact path, package control name/version/architecture, and stored inventory entry to validate. A hit does not recreate staging or recompress the `.deb`. A miss rebuilds only that package and atomically publishes its manifest after archive verification. Structured package facts, keyed by `.deb` SHA-256, record control fields, conffiles, payload paths/modes/symlinks, ELF members, SONAME/NEEDED data, dependencies, installed size, and provenance. The global collision, SONAME ownership, dependency, and compatibility result is reused only when every package fact key and the validation policy match; changed graphs rerun the complete checks.

## ELF inspection facts

Each distinct ELF byte stream has one `out/state/elf/<sha256>.json` record. A single readelf invocation records ELF type/machine, interpreter, SONAME, `DT_NEEDED`, RPATH/RUNPATH, GNU symbol-version names, build ID, readelf version, target architecture, schema, and policy. Identical content at different package/rootfs paths shares that fact. Rootfs package ownership, provider resolution, duplicate SONAME detection, host-build-path checks, and loader execution remain path-sensitive and are recomputed for the current graph. Loader `--list` remains separate because it validates actual runtime resolution; `ldd` is not used for final rootfs validation.

## Quiet native-build logs

Cache misses run under a stage log at `out/logs/<stage>.log`. Default console output is limited to cache reason, start, completion/elapsed time, and the final timing summary. Commands executed through the common runner append their full stdout/stderr and invocation to the log. Failures retain the log and print its last 40 lines. Set `MATTOS_VERBOSE_BUILD_OUTPUT=1` to stream native subprocess output while diagnosing a build. Logs are ignored, and never participate in keys or shipped payloads.

Changing one package definition changes that package's definition digest. Changing a payload-producing stage changes packages that declare that stage. Corrupting a `.deb`, staging file, mode, or symlink forces the owning package to rebuild.

## Commands

Show the most recent report without building:

```text
cargo run -p mattos-build -- timings
```

Inspect cache state or one decision:

```text
cargo run -p mattos-build -- cache status
cargo run -p mattos-build -- cache explain glibc
cargo run -p mattos-build -- cache explain glibc --details
cargo run -p mattos-build -- cache explain linux --details
cargo run -p mattos-build -- cache explain gcc-compiler
cargo run -p mattos-build -- cache explain package:libc6
cargo run -p mattos-build -- cache explain repository
cargo run -p mattos-build -- cache explain rootfs-live
cargo run -p mattos-build -- cache explain live-root
cargo run -p mattos-build -- cache explain initramfs
cargo run -p mattos-build -- cache explain iso
cargo run -p mattos-build -- cache explain elf-facts
cargo run -p mattos-build -- cache explain package-audit
```

`rootfs-live` and `rootfs-base` are aliases for the `rootfs` stage in both
`cache explain` and `cache invalidate`; the SquashFS stage is `live-root`.

Invalidate a manifest while preserving outputs:

```text
cargo run -p mattos-build -- cache invalidate glibc
cargo run -p mattos-build -- cache invalidate --dependents glibc
cargo run -p mattos-build -- cache invalidate package:libc6
cargo run -p mattos-build -- cache invalidate --dependents repository
cargo run -p mattos-build -- cache invalidate rootfs-live
cargo run -p mattos-build -- cache invalidate elf-facts
cargo run -p mattos-build -- cache invalidate package-audit
```

Invalidation is deliberately scoped. It removes state, not build output, so the next dependency-correct build validates and refreshes the selected stage. There is no casual global cache-wipe command.

## Warm, cold, and release builds

A warm build has existing outputs and matching manifests. A cold build has missing output/state and executes every required builder. The timing summary labels cache hits and misses and explains each decision. Use the exact command and cache state whenever reporting performance.

Cache reuse is a development optimization, not a reduction in release validation. Release/milestone validation still requires the complete package, source-closure, Debian, protected-package, ownership, ELF, native-toolchain, normal/offline/rescue boot, and double-build byte-reproducibility checks. A fresh rebuild can be requested through the existing explicit clean workflow; cached and freshly rebuilt package/repository/initramfs/ISO hashes must agree for the same inputs.

## Historical milestone measurements (2026-08)

The measurements and corrections in this section and the next two were
recorded during the cache milestones in August 2026, when the build graph had
far fewer stages and packages (for example 65 packages and 112 to 177 cache
records) and the initramfs still contained the complete root filesystem
compressed with gzip. They are kept as a dated record of what those
milestones achieved. They are not current timings, hit counts, or artifact
hashes, and the layer names they use (rootfs, initramfs, ISO) predate the
current `rootfs -> live-root -> iso` chain and the separate early initramfs.

### First milestone measurements

The pre-milestone read-only audit measured a 53:00.44 warm `build all` and a 1:38.02 unchanged `package build --all`. The final measurement used the same complete, image-producing build path with all package, compatibility, ownership, rootfs, and ELF validation enabled:

```text
/usr/bin/time -v cargo run -p mattos-build -- build all
```

After a manifest-refresh run, the unchanged warm run completed in 4:04.45 with 112 cache hits, zero misses, and seven deliberately non-cacheable timing records. A second warm smoke run after two independent release builds completed in 4:15.86 with the same 112/0/7 result. Relative to the audited warm baseline, the primary measurement saved 48:55.99, or 92.3 percent. The primary run reused every toolchain foundation and native-consumer stage and all 65 packages. Its largest remaining work was rootfs assembly (108.9 seconds), initramfs compression (56.0 seconds), rootfs ELF audit (21.0 seconds), rootfs package audit (18.2 seconds), and repository/package audits.

The standalone package measurements used:

```text
/usr/bin/time -v cargo run -p mattos-build -- package build --all
```

Two unchanged runs completed in 41.08 and 41.24 seconds. Each reported 65 package hits, zero misses, and one non-cacheable global audit record, a reduction of about 58 percent from the audited 1:38.02 path.

Two separate release output trees were then built with no stage manifests, package manifests, stage outputs, packages, sysroot, repository, initramfs, or ISO available. They completed in 52:21.77 and 53:09.91, each with zero hits, 177 misses, and seven non-cacheable records. Sorted SHA-256 lists for all 3,334 generated package-staging and shipped files matched. The 137 shipped package, package-inventory, repository, initramfs, and ISO hashes also matched both the earlier cached warm build and the following warm smoke build. Timing/state files were excluded from those payloads and comparisons.

Current launcher behavior (not historical): `DevUtils/run_qemu.py` invokes one image-producing command, `mattos-build build all`, and does not follow it with a redundant `image`. `--build-only` provides a noninteractive build-and-exit path; `--no-build`, `--no-network`, graphical, serial, and custom QEMU argument behavior remain available.

### Fresh-process cache-stability correction (historical)

The first ordinary launcher invocation after the second milestone exposed two defects. Schema-2 stage keys hashed raw inherited `PATH` and `LC_ALL`; the previous validation process used a VS Code/Copilot terminal `PATH` with no `LC_ALL`, while the later process used a Codex temporary-bin `PATH` and `LC_ALL=C.UTF-8`. The launcher itself did not add arguments or environment: it inherited its parent environment, changed only the child working directory to the repository, ran `doctor`, and invoked the same `cargo run -p mattos-build -- build all` command as the direct path. Replaying the stored digest under the exact earlier terminal environment made Linux reusable, proving the two unstable fields. `doctor` performed no stage-relevant mutation.

The previous warm validation also preceded the final quiet-logging implementation edit. Because `performance.rs` was conservatively included as configuration for every stage, that logging-only source change left the manifests describing the pre-edit tree. Schema 3 removes the logging/cache-observability module from compiled-output configuration while retaining output-producing recipe source. Finally, schema 2 included a dependency's full input digest alongside its output digest, so a Linux miss that republished the same `bzImage` bytes could falsely invalidate glibc. Schema 3 keys consumers by dependency output bytes only.

The schema-2 to schema-3 transition intentionally required one manifest-establishing build. It completed in 3,135.76 seconds. Two consecutive new-process `python3 DevUtils/run_qemu.py --build-only` runs then completed in 347.80 and 336.19 seconds; each reported eight foundational hits and zero misses. A subsequent new-process direct `build all` completed in 335.96 seconds with the same eight/zero result. This establishes direct-to-launcher and launcher-to-direct equivalence. Detailed Linux and glibc explanations reported every schema/source/configuration/environment/tool/dependency/full digest unchanged, every detail group unchanged, and no ordering-only differences.

At the time, the migration reproduced the pre-fix shipped artifacts exactly: initramfs SHA-256 `a685aff1b1280997370fce739ebd847a52479815f8b426ccab07ad861b7a9b92` and ISO SHA-256 `88e974775a81da75fd045f5c18667f652323745f1656b84995dcd7ea164220f4`. Fixture tests cover source/configuration and genuine environment changes, tool executable/version changes, PATH noise, terminal/logging/QEMU-only changes, true dependency-output invalidation, and an upstream input change that republishes identical bytes without cascading.

### Second milestone measurements (historical)

The first untouched warm run with repository, rootfs, initramfs, ISO, package-audit, and ELF-fact reuse completed in 3:52.93. The required second unchanged run completed in 3:50.94 with 116 hits, zero misses, and zero non-cacheable timing records. `/usr/bin/time -v` reported 219.89 seconds user time, 11.41 seconds system time, 253,560 KiB maximum RSS, 1,558,488 filesystem input blocks, and 9,824 output blocks. This is 13.51 seconds (5.5 percent) faster than the first-milestone 4:04.45 warm result and 49:09.50 (92.7 percent) faster than the original 53:00.44 audit baseline.

The second warm run's formerly expensive layers were:

| Work | First-milestone behavior | Second-milestone warm time |
| --- | --- | ---: |
| repository | regenerated and audited | 7.78 seconds, hit |
| rootfs | 108.9-second reconstruction | 31.92 seconds, full inventory validation hit |
| rootfs package audit | 18.2 seconds | cached global audit hit, below timer resolution |
| rootfs ELF audit | 21.0 seconds | served by cached content facts; no separate warm audit |
| initramfs | 56.0-second generation | 38.14 seconds, artifact/inventory validation hit |
| ISO | regenerated | 37.27 seconds, artifact/inventory validation hit |

At the time, the remaining hit time was fail-closed integrity hashing, especially the 181 MB initramfs, 183 MB ISO, 397 MB rootfs, and large compiler/toolchain inventories. It was not package installation, cpio/gzip, GRUB, xorriso, or native compilation. Direct validation commands measured 8.19 seconds for `package repo`, 2:37.73 for `image`, and 3:47.76 for the doctor plus `build all` path used by `DevUtils/run_qemu.py --build-only`.

A controlled `cache invalidate repository --dependents` removed only the repository, rootfs, initramfs, and ISO manifests. All source/toolchain manifests and all 65 package manifests remained. The dependency-correct layer rebuild completed in 4:58.77: repository 9.27 seconds, rootfs 115.81 seconds, initramfs 99.39 seconds, and ISO 37.94 seconds. An exact 141-line comparison then matched every `.deb`, every repository file, package and rootfs inventory identity, ELF inventory, initramfs, ISO, and all four layer input/output content digests. Unit fixtures separately cover dependency/profile/GRUB targeting, corruption, mode/symlink changes, atomic replacement, failed-stage publication, and identical ELF content at different paths.

The rootfs of that milestone contained 555 ELF objects represented by 552 content-addressed ELF fact files, demonstrating same-content reuse across paths, and there were 65 package fact files. A clean native refresh retained 51,986 lines / 27,181,420 bytes in the Linux, glibc, GCC runtime, Binutils, GCC compiler, systemd, dpkg, and APT logs while the console showed only stage boundaries. The largest single example was glibc at 24,787 lines / 20,473,752 bytes. Normal and `--no-network` boots reached the live Brush prompt, the rescue entry reached the exact rescue-init handoff, and an offline guest passed native C, C++ exception, `.deb` construction, and Brush `sh`/`bash` checks.

## Child-job borrowing experiments

This section records a dated (2026-08) experiment plan and its baseline
data. Production scheduling has since moved to host-derived budgets and the
shared jobserver described in [Build-System
Architecture](architecture.md#resource-aware-scheduler-foundation); the
per-stage job counts below are not current settings.

The completed isolated calibration measurements are retained as empirical
scaling data, not production scheduler limits: Make j4, glibc j8, GCC runtime
j6, Binutils j6, and GCC compiler j6. The resource-aware scheduler consumes no
per-stage count from this table; it uses generic launch-time resource profiles
and host/cgroup CPU/RAM budgets. Future measurements may become an exceptional
profile override only when they demonstrate a stable, reproducible benefit.

The successful isolated cold DAG run at `cold-dag-20260808T174207Z` was the
performance baseline at the time, when the scheduler used a fixed 12-token
budget. Its scheduler action spans showed that the candidate stages had the
following otherwise-unused global capacity while running with the child
limits of that time:

| Stage | Jobs at the time | Action wall | Average unused tokens | Minimum unused tokens |
| --- | ---: | ---: | ---: | ---: |
| glibc | 4 | 453.080 s | 4.09 | 4 |
| GCC runtime | 4 | 773.452 s | 8.00 | 8 |
| Binutils | 4 | 144.898 s | 8.00 | 8 |
| GCC compiler | 4 | 647.434 s | 8.00 | 8 |
| Make | 4 | 17.947 s | 8.00 | 8 |
| APT | 2 | 168.123 s | 9.42 | 6 |

This established opportunity, not speedup. The planned follow-up measurements
were focused single-stage A/B runs; another full cold build is not justified until one of
them demonstrates a concrete improvement. Each variant must use a frozen
`mattos-build` binary and a separate private `out` and Cargo `target` bind mount
under the benchmark directory. Seed every variant from one byte-identical
prerequisite snapshot, remove only the candidate's output and manifest, run as
effective UID/GID 1000, and retain the effective argv/child-limit command log,
scheduler trace, `/usr/bin/time -v`, `mpstat`, `vmstat`, and `pidstat` output.
Never reuse or modify the successful cold baseline directory.

The experimental control is the guarded single-stage override
`build <stage> --experimental-child-jobs <n>`. It is accepted only for
`cross-toolchain`, `glibc`, `gcc-runtime`, `binutils`, `gcc-toolchain`,
`make`, and `apt`, and `n` must be above the stage's safe baseline grant and
at most the effective CPU budget (`budget.cpu_tokens` from host/cgroup
discovery). Unlike normal scheduler normalization, it replaces the recipes'
explicit job counts only for that selected experiment. The override and
effective value are recorded in the timing metadata and command log.

Run each accepted pair in alternating order with at least three measured
repetitions after one discarded setup run:

| Stage | Variants | Advancement rule |
| --- | --- | --- |
| glibc | 4, 6, 8 jobs | Test 12 only if 8 has clear headroom and no swap |
| GCC runtime | 4, 6, 8 jobs | Test 12 only if 8 has clear headroom and no swap |
| Binutils | 4, 6, 8, 12 jobs | Retain the lowest count within 2% of the fastest |
| GCC compiler | 4, 6, 8 jobs | Test 12 only if 8 has clear headroom and no swap |
| Make | 4, 6, 8 jobs | Require five repetitions because the action is short |
| APT | 2, 4, 6, 8 jobs | Test 12 only if 8 remains CPU-bound with headroom |

A variant advances only when median action wall improves by at least 5%, every
run succeeds, output inventory and content digests match the control, swap-in
and swap-out remain zero, no sustained CPU I/O wait regression appears, and
peak memory leaves at least 2 GiB available. Record user/system CPU, maximum
RSS, major faults, filesystem I/O, context switches, and host-wide CPU/memory
samples. A full cold confirmation may follow only after choosing a specific
stage and borrowing limit from this evidence.

Production elastic borrowing is now implemented through the shared GNU make
jobserver (`jobserver.rs`): stages are admitted at their minimum grant, grow
into idle CPUs as tokens are released, and the token pool shrinks under memory
pressure. The CPU budget is derived from host/cgroup discovery rather than a
fixed 12 tokens, and the concurrent memory-heavy limit is the CPU-token count
under healthy pressure, one when constrained, and zero when critical. A fixed
larger `-j` remains an isolated experiment rather than a production scheduler
change.

The command runner removes a recipe's explicit `-j` arguments and hands each
child the jobserver through `MAKEFLAGS`, so anything else a command adds to
`MAKEFLAGS` must extend that value rather than replace it. The kernel recipe's
`KBUILD_CPPFLAGS` assignment (propagated through `MAKEFLAGS` so Kbuild's
recursive makes keep it; `propagate_kbuild_cppflags` in
`stages/helpers/command.rs`) once rebuilt `MAKEFLAGS` without the jobserver,
leaving the kernel with neither `-j` nor tokens: it compiled one file at a time
and took up to three hours instead of about five minutes.

## Per-stage memory containment

Each stage that runs its recipe gets its own transient systemd user scope
(`stage_memory.rs`). Its `memory.high` is three quarters of the memory
available when the stage starts, at least 1 GiB (the remaining quarter is
headroom for the desktop and the other running stages), and every command the stage runs moves itself into
the scope before `exec`. A stage that outgrows the memory it started with is
then reclaimed and throttled by the kernel inside its own cgroup, instead of
pushing other processes (the user's desktop, the editor) into swap: during
the 7.2.8 rebuild LLVM ran with 18 GiB of the host in swap because nothing
bounded it below the whole session.

Swap is not forbidden inside a scope (`MemorySwapMax=0`): once a stage's
anonymous memory is above `memory.high`, a swap-free scope has nothing left to
reclaim and the kernel throttles its processes indefinitely. That stalled LLVM
with eleven compilers in uninterruptible sleep for 13 minutes. Keeping a stage
below its limit is the scheduler's job instead: LLVM has its own profile
(`StageResourceProfile::llvm`, 1.25 GiB per compile job, since its C++
translation units peak at 0.5-1.3 GiB against the generic 768 MiB estimate),
and its configuration caps concurrent `llvm-tblgen` runs
(`LLVM_PARALLEL_TABLEGEN_JOBS`, half the memory at 2 GiB each), whose burst of
about a dozen 0.7-1.4 GiB processes otherwise filled the scope.

`memory.high` alone would only move the excess from other programs into the
stage's own swap, so the scheduler also reads each running stage's cgroup
(`stage_pressure`). A stage within 10% of its `memory.high` counts as
constrained memory pressure, which halves the jobserver tokens before reclaim
starts; a stage the kernel throttled at `memory.high` since the last sample
counts as critical, so no new compile job starts until its memory falls. The
same two-sample recovery applies as for host-wide pressure.

The scope's `memory.peak` is the stage's true aggregate peak, page cache
included; a successful stage records it in `out/reports/stage-memory.json`
for diagnosis. It is not turned into a per-job reservation: under the shared
jobserver a stage's parallelism varies while it runs and only the pool's
ceiling is known, so the peak cannot be divided into a per-job cost. The live
`stage_pressure` feedback acts on the stage's actual memory instead.

Scopes need a systemd user manager with the memory controller delegated, as
in a normal desktop session. Without one (CI, containers, root builds) the
build prints one notice and runs stages uncontained, as before;
`MATTOS_STAGE_CGROUPS=0` opts out explicitly. The scope's holder process dies
with the build, so an interrupted build leaves no scope behind.

LLVM's Ninja link pool (`LLVM_PARALLEL_LINK_JOBS`) is one link per 6 GiB of
the memory the build may use (host memory or its cgroup limit), from 1 to 4.
It is fixed per host because it is baked into the configured build.

## Remaining work

Validation-only checks remain deliberately executable rather than becoming mutable artifact stages. Debian compatibility validation is always read and checked; rootfs package/ELF validation runs on a rootfs miss and its proven output inventory is revalidated on hits; loader execution remains path-sensitive; QEMU boot validation always boots the selected image. These checks consume cached package/ELF facts where their inputs are path-independent. Canonical tool path identity remains a conservative false-positive risk when identical executable bytes move to a different absolute path. Repeated full-tree output hashing is mitigated by the checksummed persistent integrity index (`integrity_index.rs`), which still falls back to byte hashing whenever a file's fingerprint does not match. An immutable package-installed base rootfs plus explicit installed overlays remains possible future work. The resource-aware DAG scheduler, shared jobserver, and ccache compiler cache are implemented (see [Build-System Architecture](architecture.md)); remote caches, QEMU snapshots, lower compression, and reduced release tests remain out of scope.
