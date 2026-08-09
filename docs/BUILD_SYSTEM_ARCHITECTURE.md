# MattOS Build-System Architecture and Invalidation Contract

## Ownership

The canonical orchestrator is `src/tools/mattos-build`.

| Module | Responsibility |
| --- | --- |
| `main.rs` | CLI, stage execution dispatch, build recipes, source import/sync, rootfs and image assembly. |
| `stage_graph.rs` | Stage identity, deterministic build order, direct output dependencies, package/repository artifact graph, and graph-level invalidation analysis. |
| `performance.rs` | Invocation telemetry and integrity-cache coordination, configuration hashing, output inventories, diagnostics, logged commands, and atomic publication helpers. |
| `stage_cache.rs` | Stage input evaluation, manifests, cache decisions, execution, migration, and explanations. |
| `integrity_index.rs` | Checksummed persistent output-file fingerprints and content digests under `out`. |
| `packaging.rs` | Package definitions, package cache keys, staging, Debian artifacts, package facts, repository generation, and package installation. |
| `elf_cache.rs` | Content-addressed ELF inspection facts. |
| `source_identity.rs` | Invocation-scoped Git index and working-tree source selection, canonical serialization, and digest generation. |
| `tool_identity.rs` | Canonical executable resolution and stable version/target probing. |

The next safe decomposition steps are mechanical extractions behind existing APIs:

1. `stage_specs`: output declarations and recipe revisions, completing the existing `stage_inputs` extraction.
2. `timing`: reports, diagnostics, and logged command execution.
3. `toolchain`: Linux, glibc, GCC runtime/compiler, Binutils, Make, and sysroot recipes.
4. `source`: import, projection, provenance, and upstream synchronization.
5. `image`: rootfs, initramfs, ISO assembly, and semantic validators.
6. `packages`: package policy/cache, payload staging, repository, and installation submodules.

These boundaries must be introduced incrementally. Moving a function is not sufficient: its inputs and outputs must become explicit, and stage IDs, recipe strings, normalized environment, and manifest schemas must remain stable unless a deliberate migration is supplied.

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

The package layer is an explicit artifact node. Package producer output or
package metadata changes invalidate `packages`; changed package inventory or
artifacts invalidate `repository` and `rootfs`; changed rootfs bytes invalidate
`initramfs`; changed initramfs bytes invalidate `iso`.

## Stage Contracts

`configuration` below lists inputs beyond source roots. All ordinary native
stages also include normalized host-tool identities. Rust stages additionally
include workspace `Cargo.toml` and `Cargo.lock`.

| Stage | Source roots | Configuration | Direct dependency outputs | Produced outputs |
| --- | --- | --- | --- | --- |
| `linux` | Linux tree and x86_64 MattOS config | none | none | x86_64 `bzImage` |
| `glibc` | glibc plus selected Linux x86 UAPI | none | none | glibc install, Linux headers, sysroot libc/loader |
| `linux-headers` | selected Linux x86 UAPI | none | `glibc` publication | installed headers and inventory |
| `gcc-runtime` | GCC | none | `glibc`, `linux-headers` | runtime install, ABI report, sysroot libgcc/libstdc++ |
| `binutils` | Binutils | none | `gcc-runtime` | cross/native installs and configure record |
| `gcc-compiler` | GCC | none | `binutils`, `gcc-runtime` | native compiler install and configure record |
| `make` | Make and gnulib | none | compiler, Binutils, GCC runtime | native Make install |
| `formal-sysroot` | none | none | headers, glibc, GCC runtime | declared sysroot boundary files |
| `brush` | Brush and patches | Cargo workspace | formal sysroot | release binary |
| `coreutils` | uutils coreutils | Cargo workspace | formal sysroot | release multicall binary |
| `grep` | uutils grep | Cargo workspace | formal sysroot | release binary |
| `sed` | uutils sed | Cargo workspace | formal sysroot | release binary |
| `findutils` | uutils findutils | Cargo workspace | formal sysroot | release binary |
| `diffutils` | uutils diffutils | Cargo workspace | formal sysroot | release multicall binary |
| `expat`, `libcap`, `attr`, `zlib`, `bzip2`, `lz4`, `xz`, `xxhash`, `zstd`, `pcre2`, `libxcrypt`, `libmd`, `ncurses`, `iputils`, `kmod`, `init` | named component tree | Cargo workspace for `init`; otherwise none | formal sysroot | component install (or `mattos-init` binary) |
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
| `util-linux` | util-linux | none | formal sysroot, PAM, SELinux, PCRE2 | util-linux install |
| `shadow` | shadow | none | formal sysroot, PAM, libbsd, libmd, libxcrypt | shadow install |
| `sudo-rs` | sudo-rs | Cargo workspace | formal sysroot, PAM | release binary |
| `systemd` | systemd | none | formal sysroot, kmod, util-linux, PAM, libcap, OpenSSL, PCRE2 | systemd install |
| `dbus-broker` | dbus-broker and patches | none | formal sysroot, systemd, Expat | dbus-broker install |
| `dpkg` | dpkg | none | formal sysroot, zlib, bzip2, xz, zstd, libmd, SELinux, PCRE2 | dpkg install |
| `apt` | APT and patches | none | formal sysroot, dpkg, OpenSSL, zlib, bzip2, xz, zstd, systemd | APT install |
| `packages` | package definitions and payload configuration | package metadata/policy | package-producer outputs | staging trees, `.deb` files, inventory and facts |
| `repository` | none | package inventory and repository policy | package artifacts | Debian repository tree |
| `rootfs` | none | skeleton, live profile, units, network/session configuration, package inventory | APT, dpkg, systemd, dbus-broker, text tools, init, packages, repository | root filesystem tree |
| `initramfs` | none | recipe revision | rootfs | reproducible gzip-compressed cpio archive |
| `iso` | GRUB configuration | recipe revision | Linux image, initramfs | ISO staging tree and bootable ISO |

## Invalidation Rules

- Irrelevant documentation changes: no misses.
- Relevant source, configuration, or recipe change: owner misses.
- Missing/corrupt output: owner misses and republishes.
- Dependency input change with byte-identical output: consumers remain hits.
- Dependency output change: exact transitive consumers miss.
- Linux x86_64 config: `linux`, then `iso` only if kernel bytes change.
- Linux UAPI change: `linux`, `glibc`, and `linux-headers`; consumers follow only changed published outputs.
- Rootfs configuration: `rootfs`, then initramfs/ISO only when bytes change.
- Initramfs recipe/configuration: `initramfs`, then ISO only when bytes change.

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
Linux and initramfs feed ISO, package producers feed individual package
artifacts and the ordered inventory, repository consumes those artifacts, and
rootfs consumes repository bytes plus selected direct install trees. Two
conservative boundaries remain explicit:

- Dependency identity is stage-wide. A consumer of one output subset can
  become a candidate when another output in the same producer changes. The
  `linux-headers` view of the aggregate glibc publication is the clearest
  example; its refresh is virtual, and propagation continues only if its own
  published subset changes.
- The graph's `packages` node represents changed package inventory/artifacts.
  Individual package cache keys remain independent; a Brush change does not
  rebuild unrelated packages merely because `packages` appears in the graph
  closure.

The authoritative cold DAG baseline is the successful isolated build captured
at `cold-dag-20260808T174207Z`. It completed in 2,912.818 scheduler seconds
(48:32.98 wall), produced ISO SHA-256
`f43630631d8daca8e74d474235b8664e682075a7bb412633e4ff0acfa7b1aa84`, and
booted to an interactive MattOS shell in QEMU. This is 25.59% faster than the
65.25-minute serialized baseline. Linux is independent and can run beside glibc.
After GCC runtime publishes the formal sysroot, Binutils/GCC compiler can run
beside the broad fan-out of Rust userland and libraries. Shared sysroot writers
must remain ordered (`glibc -> gcc-runtime`), and package inventory publication,
repository, rootfs, initramfs, and ISO remain barriers.

`build all` now uses a deterministic bounded DAG executor rather than the old
serial stage loop:

1. Construct nodes from stage specs. Map virtual `linux-headers` and
	`formal-sysroot` dependencies to their atomic glibc and Make publishers.
	Keep package publication and repository generation inside the existing
	Rootfs transaction, after every package-producing stage.
2. Validate acyclicity, known dependencies, and overlapping output ownership
	before executing anything.
3. Maintain a stable ready queue ordered by stage identifier, but dispatch
	any ready node whose CPU and memory weights fit the global budget.
4. Use a 12-token budget. Linux, glibc, GCC runtime/compiler, Binutils, and the
	Rust/Cargo stages receive four tokens and a memory-heavy slot. Make, systemd,
	dbus-broker, util-linux, and the image tail receive four tokens. Other
	libraries receive two. At most two memory-heavy jobs run concurrently.
	Child-process parallelism is a separate per-stage contract: `SchedulerGrant`
	uses the granted token count, `Capped(n)` uses the lower of the grant and the
	explicit cap, and `Serial` uses one job. Explicit lower `-j`/`--parallel`
	limits are preserved, while higher limits and the Make, Cargo, CMake, Meson,
	and Ninja environment limits are reduced to the policy limit. Libcap is
	explicitly `Serial` because its upstream Makefile omits the generated
	`cap_names.h` dependency needed by `cap_magic.o`; all other audited stages use
	`SchedulerGrant`.
5. Publish each stage atomically as today, compute its output digest, and wake
	consumers only after successful validation. Cache hits consume a small
	evaluation token and do not block unrelated actions.
6. On failure, stop dispatching new nodes, wait for active jobs, preserve their
	logs, and publish no partial output or manifest.

The scheduler starts ready nodes with one evaluation token. A cache miss
releases that token while waiting in the stable resource queue, then acquires
its full stage weight immediately before its action. Cache hits never acquire
the larger allocation.

The checked simulation uses the successful baseline's `build-start` to
`stage-end` action spans for all 48 real build nodes. Their serial total is
4,883.713 seconds (81.395 minutes), the dependency-only critical path is
2,666.283 seconds (44.438 minutes), and the implemented graph and weights
produce a 2,987.354-second (49.789-minute) simulated schedule. Cache evaluation,
resource-request arrival timing, and orchestration are intentionally outside
this action-only model, so the successful trace remains the performance
baseline rather than treating the simulation as an exact replay.

Set `MATTOS_SCHEDULER_TRACE` to retain scheduler telemetry. Each completed node
emits `stage-metrics` with whether a build action executed, its resource weight,
effective child-job limit, resource-wait and action wall seconds, and the
average and minimum globally unused tokens during its action. Process CPU time
is explicitly reported as unavailable because overlapping child trees cannot
be attributed from process-wide counters. Stage command logs include a
`mattos-command` record containing the effective child-job limit and normalized
argv actually executed after scheduler capping.