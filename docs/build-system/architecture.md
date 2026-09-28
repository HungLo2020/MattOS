# MattOS Build-System Architecture and Invalidation Contract

## Ownership

The canonical orchestrator is `src/tools/mattos-build`.

`src/tools/mattos-build/src` contains a set of Rust modules declared from
`main.rs`, plus recipe and command files that `main.rs` pulls in with
`include!` and that therefore share its namespace.

| Module | Responsibility |
| --- | --- |
| `main.rs` | CLI definition and dispatch, `build all` node construction for the scheduler, single-stage execution, and the `include!` list for the files below. |
| `commands/*.rs` (`include!`d) | `doctor`, `cache`, artifact reports and `image`, and WSL helper commands. |
| `source/*.rs` (`include!`d) | Upstream import/sync (`import.rs`), with source selection, provenance, Git LFS, and patch application in `selection.rs`, `provenance.rs`, `lfs.rs`, and `patches.rs`. |
| `stages/*.rs` (`include!`d) | Stage recipes grouped by area (`toolchain.rs`, `libraries.rs`, `qt.rs`, `plasma.rs`, `image.rs`, ...). `stages/registry.rs` builds each stage's `StageSpec` (outputs, tools, virtual `linux-headers`/`formal-sysroot` specs, target-toolchain cache key) and dispatches to recipes; `stages/helpers/` holds the shared Autotools, Meson, Cargo, pkg-config, native-build, and command-runner helpers. |
| `stage_graph.rs` | `BuildStage` identities, stage IDs, direct dependencies (`direct_dependencies`), deterministic build order, package/repository artifact graph, and graph-level invalidation analysis. |
| `stage_inputs.rs` | Per-stage source roots, configuration inputs, tool names, and recipe revisions. |
| `recipe_projection.rs` | Per-stage projection of shared `stages/*.rs` recipe files (see Cache Contract). |
| `cache_manifest.rs` | Serialized `StageSpec`, input, tool, dependency, and manifest types, and the manifest schema version. |
| `stage_cache.rs` | Stage input evaluation, cache decisions, cached execution, manifest migration, and explanations. |
| `performance.rs` | Timing/telemetry, stage logs and logged command execution, integrity-cache coordination, output inventories and digests, and atomic publication helpers. |
| `timing.rs` | Timing report record types. |
| `scheduler.rs` | Resource-aware DAG executor for `build all`, admission, child-job policy, failure policy, and scheduler trace. |
| `jobserver.rs` | Shared GNU make jobserver for one invocation. |
| `resources.rs` | Host/cgroup resource discovery, CPU/RAM budgets, and runtime memory-pressure sampling. |
| `integrity_index.rs` | Checksummed persistent output-file fingerprints and content digests under `out`. |
| `packaging.rs` and `packaging/*.rs` | Package build, staging (`staging.rs`), package definitions (`registry.rs`), package cache (`cache.rs`), audits (`audit.rs`), and repository generation (`repository.rs`). |
| `elf_cache.rs` | Content-addressed ELF inspection facts. |
| `source_identity.rs` | Invocation-scoped Git index and working-tree source selection, canonical serialization, and digest generation. |
| `tool_identity.rs` | Canonical executable resolution and stable version/target probing. |

Further decomposition (for example turning the `include!`d files into real
modules) must be introduced incrementally. Moving a function is not
sufficient: its inputs and outputs must become explicit, and stage IDs, recipe
strings, normalized environment, and manifest schemas must remain stable unless
a deliberate migration is supplied. Because recipe files are cache inputs,
moving code between them also changes the affected stages' identities.

## Resource-aware scheduler foundation

`build all` snapshots host capacity once before it evaluates the DAG. The
snapshot records logical CPUs, cpuset and CPU-quota restrictions, physical and
currently available RAM, cgroup memory limits/current use, swap use, and the
cumulative kernel swap counters. Linux cgroup v2 is preferred, with cgroup v1
controller-file fallbacks. `MATTOS_RESERVED_MEMORY_MIB` optionally sets the
RAM headroom; otherwise the scheduler reserves the larger of 1 GiB and 12.5%
of the effective memory limit (never more than half the limit).

The scheduler derives CPU tokens from the minimum of logical CPUs, cpuset, and
quota, and derives build memory from current effective availability minus the
reserve. A stage has a generic resource profile: minimum safe CPU grant,
optional useful ceiling, estimated memory, memory-heavy status, idle-CPU
borrowing permission, and child-job policy. Profiles are resource *classes*,
not machine-specific `-j` assignments: standard and memory-heavy work can use
additional capacity on a large, healthy host; the libcap serial profile remains
one job.

### Shared jobserver

`build all` runs one GNU make jobserver (a FIFO token pool under `out/tmp`)
for the whole invocation. Make (4.4+), Ninja (1.13+, and so CMake and Meson)
and Cargo join it through `MAKEFLAGS`/`CARGO_MAKEFLAGS`, and explicit `-jN`
arguments are removed for them (an explicit `-j1` serialization request is
kept). Stages are admitted at their minimum grant, so memory reservations
still gate admission, and then grow into idle CPUs as tokens are released.
This replaces launch-time-only grants, under which a large stage that started
during contention kept one or two jobs for its whole run. Every scheduler pass
sets the pool to the idle CPU count while memory pressure is healthy, half of
it when constrained, and zero when critical; tokens held by running jobs are
reclaimed as they are returned. The pool is also memory-aware: it never
exceeds the tokens already held by running jobs plus the measured build-memory
headroom divided by the largest per-job memory estimate among the building
stages (at least 512 MiB). The pool grows by at most one token per pass and
shrinks immediately. Sustained swap-in counts as memory pressure (1,024
pages/s, 4 MiB/s with 4 KiB pages, is constrained; 8,192 pages/s, 32 MiB/s, is
critical), since pages faulting back from swap mean the working set no longer
fits. Sustained swap-out also counts (256 pages/s constrained, 4,096 pages/s
critical). Serial and capped stages keep fixed limits.
Single-stage `build <stage>` runs do not use the jobserver.

### Compiler cache

When a host `ccache` is installed (and `MATTOS_CCACHE` is not `0`), target
C/C++ compiles are cached in `out/cache/ccache` (40 GB cap). Compiler-named
links to ccache in `out/toolchain/ccache-bin` precede the MattOS compiler
wrappers on `PATH`, and CMake builds that name their compiler explicitly get
`CMAKE_<LANG>_COMPILER_LAUNCHER`. Entries are keyed to the MattOS toolchain
identity (`CCACHE_COMPILERCHECK=string:<identity>`: cross-toolchain and
compiler configurations plus the hardening specs), so a changed compiler never
reuses objects. Hits are byte-identical objects, so ccache accelerates rebuilds
without affecting outputs. It does not cache Rust (rustc).

Admission is deterministic for a fixed snapshot and ready-set order. It always
requires a profile's minimum CPU and memory estimate, sets aside the minimum
grants of ready peers that could run (capped at half of the idle CPUs) before
lending idle capacity, and prevents a new action from crossing the memory
budget. Borrowing idle CPU beyond a profile's preferred baseline is allowed
only while pressure is healthy. Without the shared
jobserver (single-stage builds) a grant is chosen only at launch; with it,
running stages grow and shrink through jobserver tokens instead of any
mid-command resizing.

The scheduler also samples the resource envelope before each launch decision.
It refreshes effective available/cgroup memory, `pswpin`/`pswpout`, and Linux
memory PSI `some avg10` when available. Existing swap occupancy is telemetry,
not pressure: no scheduler decision reads it. Only swap-in/swap-out rates, PSI
(`some avg10` of 5% or more is constrained), or low build-memory headroom can
raise the pressure level. Levels are `healthy`, `constrained`, and `critical`.
Worsening is immediate; recovery requires two lower-pressure samples. Healthy
pressure allows as many concurrent memory-heavy actions as there are CPU
tokens (memory admission remains the real limit) and lets stages borrow idle
CPU; constrained pressure uses only preferred baseline grants and allows one
memory-heavy action; critical pressure admits no new memory-heavy action.
Running child processes are never killed or resized.

Each stage trace record includes its profile estimate, start/end available RAM,
cgroup memory current where known, and start/end pressure. This is intentionally
observation-only groundwork for later empirical memory estimates; one run never
changes production profiles automatically.

The Make/glibc/GCC runtime/Binutils/GCC compiler j4/j8/j6/j6/j6 results remain
benchmark calibration metadata in [Build performance and cache model](performance.md); they are not read by
the production admission algorithm. New stacks and components (for example further KDE applications) should select
one of the generic classes (or a measured exceptional override) rather than
adding machine-specific job values for every new DAG node.

## Cache Contract

A stage is reusable only when all source, configuration, tool, environment,
recipe, and direct dependency-output identities match and all declared outputs
pass fail-closed inventory and semantic validation.

Dependency keys contain the dependency's output digest, not its complete input
identity. Therefore an upstream stage whose inputs changed but whose published
bytes are identical does not invalidate consumers. If published bytes change,
the exact transitive downstream closure in `stage_graph` must miss. Missing or
corrupted outputs invalidate their owner; downstream work is necessary only if
the repaired output digest differs.

Target stages also carry a `<target-toolchain-v2>` pseudo-dependency (`TARGET_TOOLCHAIN_CACHE_KEY` in `stages/registry.rs`; the version suffix lets manifests keyed by an older definition migrate): the output
digests of `cross-toolchain` and `gcc-runtime` plus the compiler wrappers in
`out/toolchain/bin`. Target code reaches the compiler through PATH rather than
through a published dependency, so without it a toolchain change that leaves
the sysroot bytes unchanged would not invalidate them. The toolchain stages
themselves (`cross-toolchain`, `glibc`, `gcc-runtime`), `linux` (built by the
pass-1 compiler, a direct dependency), the virtual `linux-headers` and
`formal-sysroot` nodes, `packages` and `repository` do not carry it. A manifest
written before the key existed adopts it without a rebuild only when the
workspace guard's marker (`out/state/toolchain-markers/<stage>`) records the
current toolchain for that stage.

Recipe implementation files under `src/tools/mattos-build/src/stages/` are
hashed per stage. When a file defines other stages' recipe functions (the
functions `build_stage_recipe` dispatches to), a stage hashes the file with
those functions removed, unless its own retained code calls them. Shared
helpers, constants and imports stay in every projection, so editing them still
invalidates every stage in the file; editing one recipe invalidates only its
stage. Files that define no other stage's recipe are hashed whole.

Cargo-built userland (`brush`, `coreutils`, `grep`, `sed`, `findutils`,
`diffutils`, `init`, `sudo-rs`, `greetd`, `cozy`, `installer`, and the
`mattos-compat` package) and Meson builds with Rust components (`dbus-broker`,
`mesa`, `gstreamer`) are compiled by the MattOS rustc from the `rust` stage,
which is their declared dependency. The
rootfs audit rejects any ELF whose `.comment` names a GCC, rustc or Clang other
than the MattOS-built ones.

The package layer is an explicit artifact node. Package producer output or
package metadata changes invalidate `packages`; changed package inventory or
artifacts invalidate `repository` and `rootfs`; changed rootfs bytes invalidate
`live-root`; changed live-root, repository, initramfs, GRUB, or Linux bytes
invalidate `iso`. The early `initramfs` does not consume the rootfs: it
depends on `formal-sysroot` and `linux`, so changed Linux bytes invalidate
`initramfs` and `installer` as well as `iso`.

## Stage Contracts

`configuration` below lists inputs beyond source roots. All ordinary native
stages also include normalized host-tool identities. Rust stages additionally
include their Cargo manifest inputs. Recipe files under `stages/` are source
inputs through the per-stage projection described above. Every target stage
not in the exempt list above also carries the `<target-toolchain-v2>` key.

The table covers the foundational, base-system, and image stages. It is not
exhaustive: the graph also contains the language toolchains, graphics, Qt,
KDE, Plasma, and application stages. The dependency column is derived from
`direct_dependencies` in `stage_graph.rs` (virtual `linux-headers` and
`formal-sysroot` from their specs in `stages/registry.rs`), which is the
authority when this table and the code disagree.

| Stage | Source roots | Configuration | Direct dependency outputs | Produced outputs |
| --- | --- | --- | --- | --- |
| `cross-toolchain` | Binutils, GCC, glibc version header | none | none | stage-0 cross Binutils/pass-1 GCC install, prerequisite install, hardening specs, configure record |
| `linux` | Linux tree and x86_64 MattOS config/policy | none | `cross-toolchain` | x86_64 `bzImage`, module tree, kernel release |
| `glibc` | glibc plus selected Linux x86 UAPI | none | `cross-toolchain` | glibc install, Linux headers, sysroot libc/loader |
| `linux-headers` (virtual) | selected Linux x86 UAPI | none | `glibc` publication | installed headers and inventory |
| `gcc-runtime` | GCC | none | `glibc`, `linux-headers`, `cross-toolchain` | runtime install, ABI report, sysroot libgcc/libstdc++ |
| `binutils` | Binutils | none | `gcc-runtime`, `cross-toolchain` | cross/native installs and configure record |
| `gcc-compiler` | GCC | none | `binutils`, `gcc-runtime`, `cross-toolchain` | native compiler install and configure record |
| `make` | Make and gnulib | none | `gcc-compiler`, `binutils`, `gcc-runtime`, `cross-toolchain` | native Make install |
| `formal-sysroot` (virtual) | none | none | `linux-headers`, `glibc`, `gcc-runtime` | declared sysroot boundary files |
| `brush`, `coreutils`, `grep`, `sed`, `findutils`, `diffutils`, `init` | named component tree (Brush also its patches) | component `Cargo.toml`/`Cargo.lock` and Cargo ownership contract (`init`: `Cargo.toml` only) | formal sysroot, `rust` | release binary (`coreutils` and `diffutils` multicall; `init` the `mattos-init` binary) |
| `expat`, `libcap`, `attr`, `zlib`, `bzip2`, `lz4`, `xz`, `xxhash`, `zstd`, `pcre2`, `libxcrypt`, `libmd`, `ncurses`, `iputils`, `libffi` | named component tree | none | formal sysroot | component install |
| `kmod` | kmod | none | formal sysroot, zstd | kmod install |
| `acl` | ACL | none | formal sysroot, Attr | ACL install |
| `openssl` | OpenSSL | none | formal sysroot, zlib, zstd | OpenSSL install |
| `elfutils` | elfutils | none | formal sysroot, zlib, zstd | elfutils install |
| `selinux` | SELinux | none | formal sysroot, PCRE2 | SELinux install |
| `libbsd` | libbsd | none | formal sysroot, libmd | libbsd install |
| `tar` | tar, paxutils, gnulib | none | formal sysroot, ACL, Attr | tar install |
| `procps-ng` | procps-ng | none | formal sysroot, ncurses | procps install |
| `iproute2` | iproute2 | none | formal sysroot, libcap, zlib, zstd, elfutils, PCRE2, SELinux | iproute2 install |
| `curl` | curl | none | formal sysroot, OpenSSL, zlib, zstd | curl install |
| `linux-pam` | Linux-PAM | none | formal sysroot, libxcrypt | PAM install |
| `util-linux` | util-linux and patches | none | formal sysroot, PAM, SELinux, PCRE2, ncurses, libxcrypt | util-linux install |
| `shadow` | shadow | none | formal sysroot, PAM, libbsd, libmd, libxcrypt | shadow install |
| `sudo-rs` | sudo-rs | component `Cargo.toml`/`Cargo.lock` and Cargo ownership contract | formal sysroot, PAM, `rust` | release binary |
| `llvm` | LLVM project | none | formal sysroot, zlib, zstd | LLVM/Clang/LLD install |
| `rust` | Rust source release and release-archive policy | none | formal sysroot, `llvm`, OpenSSL, zlib, curl | rustc, standard library, rustdoc, and Cargo install |
| `dbus` | D-Bus | none | formal sysroot, Expat | D-Bus install |
| `systemd` | systemd | none | formal sysroot, D-Bus, kmod, util-linux, PAM, libcap, OpenSSL, PCRE2 | systemd install |
| `dbus-broker` | dbus-broker and patches | none | formal sysroot, systemd, Expat, `rust` | dbus-broker install |
| `dpkg` | dpkg | none | formal sysroot, zlib, bzip2, xz, zstd, libmd, SELinux, PCRE2 | dpkg install |
| `apt` | APT and patches | none | formal sysroot, dpkg, OpenSSL, zlib, bzip2, xz, zstd, systemd | APT install |
| `grub` | GRUB, its gnulib, autoconf-archive, font | none | formal sysroot, zlib, xz, FreeType | GRUB install |
| `installer` | installer, storage tools, firmware, `image.rs` | installer `Cargo.toml`, storage-tool ownership contracts | GRUB, formal sysroot, util-linux, e2fsprogs, zlib, zstd, libxcrypt, `linux`, `rust` | installer install, installed-system initramfs, storage tools, `BOOTX64.EFI` |
| `packages` | package definitions and payload configuration | package metadata/policy | package-producer outputs | staging trees, `.deb` files, inventory and facts |
| `repository` | none | package inventory and repository policy | package artifacts | Debian repository tree |
| `rootfs` | `image.rs` | skeleton, live profile, units, network/session configuration, package inventory | APT, dpkg, systemd, dbus-broker, grep, sed, findutils, diffutils, init, installer, repository | root filesystem tree |
| `live-root` | `image.rs` | recipe revision | `rootfs` | zstd (level 12) SquashFS `out/build/live-root.squashfs` and inventory report |
| `initramfs` | `live-init.c`, module loader, firmware, `image.rs` | recipe revision | formal sysroot, `linux` | static early `/init` plus boot module closure as reproducible xz-compressed cpio `out/build/early-initramfs.cpio.xz` |
| `iso` | GRUB configuration, `image.rs` | recipe revision | `linux`, `live-root`, `initramfs`, `grub`, `repository` | ISO staging tree, bootable ISO, and reports |

## Invalidation Rules

- Irrelevant documentation changes: no misses.
- Relevant source, configuration, or recipe change: owner misses.
- Missing/corrupt output: owner misses and republishes.
- Dependency input change with byte-identical output: consumers remain hits.
- Dependency output change: exact transitive consumers miss.
- Linux x86_64 config: `linux`, then `initramfs`, `installer`, and `iso` only if Linux output bytes change (and the rootfs, live root, and ISO further downstream only if those outputs change).
- Linux UAPI change: `linux`, `glibc`, and `linux-headers`; consumers follow only changed published outputs.
- Rootfs configuration: `rootfs`, then `live-root` and ISO only when bytes change.
- Initramfs source/recipe: `initramfs`, then ISO only when bytes change.
- GRUB configuration (`src/boot/grub/grub.cfg`): `iso` only.

No stage may depend on another stage merely because it ran earlier. A direct
edge is valid only when the consumer reads that producer's published bytes.

## Source Identity Design

### Measured Input Path

The fully cached 2026-08-07 baseline spent 21.52 seconds in 51 stage input
evaluations. Source identity had 114 invocation-cache misses. The repository
contained approximately 377,000 tracked imported-source paths, and each source
root query scanned the complete Git index even when the selected root contained
only a few files. Package evaluation requests many roots a second time with
documentation included, while stage evaluation excludes documentation. The
largest repeated scans were GCC (6.38 seconds over two queries), Linux (6.75
seconds over three queries), Binutils (2.28 seconds over two queries), and glibc
(1.04 seconds over two queries), plus the combined glibc/UAPI query.

Clean stage source files were not byte-read in this baseline. Their identities
were the Git index mode, object ID, and stage tuple. Unstaged tracked files and
untracked files were inventoried from the working tree and byte-hashed. Stage
configuration files were inventoried directly. Output files under `out` used
the separately checksummed persistent integrity index where its full
device/inode/type/size/mtime/ctime fingerprint matched; every mismatch fell
back to byte hashing.

### Considered Designs

| Design | Correctness | Warm cost | Decision |
| --- | --- | --- | --- |
| Direct byte hashing for every source path | Content-authoritative and independent of Git, but must still inventory types, modes, symlinks, and directory entries. | Re-reads hundreds of thousands of vendored files per invocation. | Retained as the fail-closed fallback, not the clean-tree fast path. |
| Git-object-assisted identity | A clean stage-0 index entry supplies immutable blob identity and tracked mode. Staged content/mode changes alter that identity. Unstaged, untracked, deleted, replaced, symlinked, conflicted, or unparsable paths require direct working-tree inventory and byte hashing. | Three Git commands per invocation plus work proportional to selected roots. Prefix-indexing avoids scanning the complete index for every root and is expected to remove most of the 21.52-second input cost. | Selected. |
| Persistent source-integrity metadata | Filesystem fingerprints can detect ordinary changes but are not content identity. Trusting unchanged metadata could reuse stale inputs after adversarial metadata restoration or external mutation; verifying safety requires rereading bytes. Persisting Git/index state also does not remove the need to discover dirty and untracked paths. | Potentially low only if metadata is trusted, which violates the cache contract; otherwise little benefit over the selected design. | Rejected. No source-input digest is persisted. |

The selected design must fail closed. Only unambiguous stage-0 index entries
may use Git identity. Selection/parsing failure or uncertain index state falls
back to direct filesystem identity. Working-tree paths never become trusted
merely because size or timestamps match. Tests must cover ordinary and
same-size edits, restored timestamps, chmod, symlink and rename replacement,
staged and unstaged changes, deletion, untracked files, conflicts, and
fresh-process reevaluation before the indexed identity is accepted.

Function-level profiling of the first ordered-map implementation showed that
prefix lookup was not the bottleneck. Across a fully cached invocation, prefix
range setup took about 1 millisecond, while snapshot map construction took
2.47 seconds, selected-entry insertion/sorting took 1.45 seconds, working-tree
overlay construction took 2.54 seconds, JSON serialization took 6.97 seconds,
and SHA-256 took 6.42 seconds. The snapshot accessor also deep-cloned all
approximately 377,000 index entries for each uncached semantic-root query.

The corrected representation keeps one immutable ordered snapshot behind an
invocation-owned `Rc`. Prefix queries borrow it instead of rebuilding it. This
preserves the existing canonical JSON and SHA-256 digest byte for byte, so
stage and package identities require no schema migration. `sha2` and
`serde_json` alone are optimized in the development profile used to run the
orchestrator; this changes execution speed, not serialized bytes or hashes.

Query-reuse profiling observed 165 source requests: 51 exact invocation-cache
hits and 114 misses. Canonicalizing root order, duplicate roots, and nested
roots did not merge any misses. The misses comprised 70 exclude-documentation
queries and 44 include-documentation queries, and all 114 produced distinct
canonical digests. Reusing results across different semantic queries therefore
offers no measured opportunity. The canonical query key is retained so root
ordering and redundant nested roots cannot create accidental duplicate work.

The selected entries are now merged directly from ordered index and untracked
ranges. Each path is JSON-escaped with `serde_json`, while the fixed Git header
or working-tree digest value is emitted directly into a SHA-256 writer. This
produces the exact legacy byte sequence
`["git-index-and-working-tree",{...}]` without a selected `BTreeMap`, one
formatted `String` per clean entry, a complete JSON `Vec<u8>`, or a second hash
pass. A test-only legacy full-scan serializer proves byte-for-byte equality,
not only digest equality, across clean, filtered, overlapping, dirty, staged,
mode, symlink, replacement, deletion, untracked, conflict, and fresh-process
states.

A Merkle directory aggregate was considered. It would make clean subtree
lookup constant-time, but SHA-256 of the existing canonical JSON map cannot be
composed from child SHA-256 values. Adopting a Merkle digest would therefore
change every source identity and invalidate stage and package caches. The
shared-snapshot approach already beats the pre-change input-hashing baseline
without that migration, so the schema-changing design was rejected. A slow
full-index reference implementation remains in the test build and must match
the optimized range selection across overlapping roots and adversarial Git and
working-tree states.

## Incremental and Cold-Build Audit

An input change always rebuilds its direct owner. Downstream stages are only
candidates until the owner republishes a different output digest. A
byte-identical rebuild stops propagation. Representative graph tests therefore
assert both the candidate closure and the output-sensitive required rebuild
set; real-spec tests separately prove the source/configuration owners.

No representative dependency edge was removable in the 2026-08-08 audit.
In the current graph Linux feeds the initramfs, installer, and ISO; package
producers feed individual package artifacts and the ordered inventory;
repository consumes those artifacts; rootfs consumes repository bytes plus
selected direct install trees; and the live root, initramfs, GRUB, and
repository feed the ISO. Two conservative boundaries remain explicit:

- Dependency identity is stage-wide. A consumer of one output subset can
  become a candidate when another output in the same producer changes. The
  `linux-headers` view of the aggregate glibc publication is the clearest
  example; its refresh is virtual, and propagation continues only if its own
  published subset changes.
- The graph's `packages` node represents changed package inventory/artifacts.
  Individual package cache keys remain independent; a Brush change does not
  rebuild unrelated packages merely because `packages` appears in the graph
  closure.

### Historical cold-build baseline (2026-08-08)

The figures in this subsection describe the build graph and scheduler as they
were on 2026-08-08, when the graph had roughly 50 build nodes and the
executor used a fixed token budget. They are retained as a dated reference,
not as a description of current behavior or performance.

The cold DAG baseline of that time is the successful isolated build captured
at `cold-dag-20260808T174207Z`. It completed in 2,912.818 scheduler seconds
(48:32.98 wall), produced ISO SHA-256
`f43630631d8daca8e74d474235b8664e682075a7bb412633e4ff0acfa7b1aa84`, and
booted to an interactive MattOS shell in QEMU. This is 25.59% faster than the
65.25-minute serialized baseline. The checked simulation used that
baseline's `build-start` to `stage-end` action spans for all 48 real build
nodes of the time. Their serial total was 4,883.713 seconds (81.395 minutes),
the dependency-only critical path was 2,666.283 seconds (44.438 minutes), and
the then-implemented graph and weights produced a 2,987.354-second
(49.789-minute) simulated schedule. Cache evaluation, resource-request arrival
timing, and orchestration were outside that action-only model.

### DAG executor

`build all` uses a deterministic, resource-bounded DAG executor
(`scheduler.rs`). Linux is independent of glibc and can run beside it. After
GCC runtime publishes the formal sysroot, Binutils/GCC compiler can run beside
the broad fan-out of libraries, and the Rust userland follows the `rust`
stage. Shared sysroot writers remain ordered (`glibc -> gcc-runtime`), and
package inventory publication, repository, rootfs, live root, and ISO remain
barriers.

1. Construct nodes from stage specs. Map virtual `linux-headers` and
	`formal-sysroot` dependencies to their atomic glibc and Make publishers.
	Package publication and repository generation run inside the `rootfs`
	stage's scheduled action, which depends on every package-producing stage;
	a `packages` or `repository` dependency maps to `rootfs`.
2. Validate acyclicity, known dependencies, and overlapping output ownership
	before executing anything.
3. Maintain a stable ready queue ordered by stage identifier, but dispatch
	any ready node whose resource profile fits the budget, as described in
	[Resource-aware scheduler foundation](#resource-aware-scheduler-foundation).
4. The CPU-token and memory budget come from the startup host/cgroup snapshot
	(`resources.rs`), not a fixed constant. Under healthy pressure the number of
	concurrent memory-heavy jobs is bounded only by the CPU-token count
	(`heavy_limit` in `scheduler.rs`), so memory admission is the effective
	limit; constrained pressure allows one and critical pressure none.
	Child-process parallelism is a separate per-stage contract:
	`SchedulerGrant` uses the scheduler's grant, `Capped(n)` uses the lower of
	the grant and the explicit cap, and `Serial` uses one job. Under the shared
	jobserver, `SchedulerGrant` stages have explicit `-jN`/`--jobs`/`--parallel`
	counts removed from their commands (explicit `-j1` is kept) and take
	parallelism from the token pool (`stages/helpers/command.rs`). Without
	the jobserver (single-stage builds), explicit counts are reduced to the
	child-job limit. Libcap is explicitly `Serial` because its upstream
	Makefile omits the generated `cap_names.h` dependency needed by
	`cap_magic.o`.
5. Publish each stage atomically, compute its output digest, and wake
	consumers only after successful validation. Cache hits consume a small
	evaluation token and do not block unrelated actions.
6. On failure, the default policy stops dispatching new nodes, waits for
	active jobs, preserves their logs, and publishes no partial output or
	manifest. With `build all --keep-going` (`FailurePolicy::KeepGoing`),
	every stage that does not depend on a failed stage still builds, and the
	run ends by reporting each failure and the stages it blocked.

The scheduler starts ready nodes with one evaluation token. A cache miss
releases that token while waiting in the stable resource queue, then acquires
its full stage weight immediately before its action. Cache hits never acquire
the larger allocation.

Set `MATTOS_SCHEDULER_TRACE` to retain scheduler telemetry. Each completed node
emits `stage-metrics` with whether a build action executed, its resource weight,
effective child-job limit, resource-wait and action wall seconds, and the
average and minimum globally unused tokens during its action, and CPU use
(`cpu_user_seconds`, `cpu_system_seconds`, `cpu_seconds`, `cpu_cores_avg`)
read from the `stage-cpu-accounting` record in the stage's log. Each logged
command's CPU time comes from `wait4` on that command's own child, so it
includes its waited-for descendants without attributing other stages'
concurrent process trees. The CPU fields read `unavailable` only when the log
holds no complete accounting record. Stage command logs include a
`mattos-command` record containing the effective child-job limit and normalized
argv actually executed after scheduler normalization.
