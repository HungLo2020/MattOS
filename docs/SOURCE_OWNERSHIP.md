# MattOS source ownership

MattOS treats every first-class source tree listed in `upstream/sources.toml` as the authoritative copy of that project for in-tree builds.

## Invariant

If a MattOS subproject depends on a project MattOS already owns, the build must consume the MattOS-owned source or the output built from that source. It must not silently download another Git revision, crates.io copy, system `-dev` package, Meson wrap, CMake `FetchContent` checkout, or equivalent duplicate of an owned package that is present locally.

The vendored source manifest is authoritative for dependency identity and version. Consumers do not duplicate or pin a second MattOS-local version number. Updating an owned source tree therefore moves in-tree consumers to that source together; an incompatible consumer must fail explicitly instead of falling back to another copy.

When a consumer is incompatible with an owned lower-level project, MattOS prefers moving the consumer forward over patching an older snapshot. The maintenance order is: keep the MattOS-owned dependency authoritative; inspect the current upstream consumer; if upstream already supports the owned dependency revision, advance the vendored consumer to that compatible upstream revision; only carry an output-mirror compatibility patch when no suitable upstream consumer revision exists. A consumer's historical Cargo.lock or Git pin never authorizes a second copy of a project MattOS already owns.

This policy also expresses the general freshness preference: use current upstream revisions where practical, while keeping one canonical MattOS-owned revision for each first-class project. Freshness is subordinate to source coherence, not an excuse to introduce duplicate versions.

## Cargo ownership catalog

`DevUtils/generate_source_overrides.py` reads `upstream/sources.toml`, enumerates tracked Cargo manifests through Git, and writes the derived ownership catalog at `out/source-ownership/cargo/index.json`.

The catalog records:

- first-class component repository identities and pinned revisions;
- Cargo packages physically owned by each first-class component;
- canonical root-package ownership;
- declared gitlink replacements from `upstream/policies/gitlinks.toml` such as libcosmic's upstream `iced` gitlink being replaced by the first-class `cosmic-iced` component; and
- output-mirror patch metadata for components that carry MattOS patches.

Catalog generation validates every declared patch chain before any Cargo build starts: `sources.toml` must agree with the component provenance state, the patch-manifest bytes must match their SHA-256, the manifest must name the pinned component/revision and `output-mirror-only` application policy, and every patch payload must match its declared SHA-256.

The catalog is derived output. MattOS does not generate a repository-wide Cargo `[patch]` configuration.

## Structural Cargo rebinding

Cargo `[patch]` is intentionally not the ownership enforcement mechanism. `[patch]` participates in normal Cargo resolution and therefore cannot express MattOS's stronger rule that an owned dependency must use one exact local source and may not fall back to another source identity.

`DevUtils/source_ownership_graph.py` instead rewrites dependency declarations in derived build mirrors before Cargo resolves them. The authoritative imported trees under `src/` remain pristine.

The consumer being built remains in its normal `out/build/...` source mirror. Additional owned dependencies are materialized under canonical, reusable `out/source-ownership/sources/<component>` mirrors. Shared mirrors never record a path into another stage's private `out/build/...` tree; only the top-level consumer's own manifests may use its private consumer path. Per-component filesystem locks serialize shared-mirror mutation so multiple build processes cannot copy, patch, or rewrite the same canonical mirror concurrently.

MattOS output-only patches are part of output-mirror creation, not an incidental side effect of a later build frontend. A stage that copies an authoritative first-class source into an `out/build/...` mirror must apply that component's validated registered patch chain immediately after the pristine copy and before Cargo isolation, configuration, or compilation. Canonical dependency mirrors use the same rule while they are materialized. `DevUtils/cargo_source_owned.py` then independently validates the top-level consumer patch state before ownership graph preparation; repeated Cargo invocations detect an already-applied chain with `git apply --reverse --check`. This dispatcher check is an idempotent fail-closed enforcement layer, not the primary mechanism that makes a stage mirror buildable. Patch manifests contain Git-format diffs, so source ownership uses `git apply --check` followed by `git apply --whitespace=error-all`, matching MattOS's existing output-patch regression semantics. It does not use GNU `patch`, whose interpretation of Git metadata can differ for valid Git-format patches.

Ownership decisions are source-qualified:

- **Git dependency:** repository identity is matched first, then package identity. A same-name crate from a different Git repository is not captured. If the repository is owned and the requested package exists in that component or its declared gitlink-replacement closure, the Git/revision/branch/tag fields are removed and replaced with an explicit canonical `path` dependency.
- **Registry/version dependency:** package identity may resolve to a unique first-class root package owned by MattOS.
- **Existing path dependency:** it is preserved unless it crosses a declared replaced gitlink or already identifies a canonical ownership mirror.

Nested crates are not globally claimed merely because a `Cargo.toml` exists somewhere inside a large imported tree. This prevents compiler fixtures, tests, shims, and unrelated same-name packages from becoming accidental project-wide owners.

For COSMIC this means, for example, a Git edge requesting `libcosmic` from the libcosmic repository is rebound to MattOS's libcosmic mirror, while iced-family packages exposed through libcosmic's upstream gitlink are routed through the declared first-class `cosmic-iced` replacement. Conversely, a crate named `cosmic-settings-daemon` coming from `dbus-settings-bindings` remains that external crate; it is not confused with MattOS's separately owned `pop-os/cosmic-settings-daemon` project.

## Fail-closed verification

`DevUtils/cargo_source_owned.py` is the Cargo dispatcher used by the MattOS launcher. For Cargo commands operating on an `out/build/...` mirror it:

1. identifies the first-class component represented by the build mirror;
2. validates that consumer's registered output-only patch chain and applies it only if the mirror has not already been prepared;
3. prepares the transitive MattOS-owned canonical source mirrors and rewrites dependency edges;
4. runs `cargo metadata` against the rewritten graph;
5. verifies that an owned Git package did not remain external and that canonical first-class path/registry packages resolve from their expected MattOS mirror; and
6. only after verification runs the original Cargo command, including its original `--locked` policy.

A requested package that is not actually present in an owned repository or its declared replacement closure is not invented. It may remain external until MattOS imports/owns that source. Once the matching source is owned, external fallback is forbidden.

The dispatcher never rewrites authoritative imported source under `src/`. Source-ownership transformations happen only in derived output mirrors.

## Diagnostics

Ownership-enabled Cargo invocations write detailed traces under `out/source-ownership/logs/<component>.log`. These logs include consumer patch state, graph preparation, metadata verification and final Cargo diagnostics.

`DevUtils/run_qemu.py` automatically prints source-ownership failure logs generated during the current failed build command. Runtime diagnostics remain under `out/`; builds do not dirty the Git-tracked tree merely to expose an error.

## Native build systems

Native libraries remain source-owned through the MattOS build graph and staged sysroot. Meson, CMake, Autotools, Make, and Rust build scripts that consume native libraries must resolve headers, pkg-config metadata, link libraries, and runtime closure from MattOS-built component outputs rather than matching host development packages or downloading duplicate owned projects.

A runtime relationship is not automatically a rebuild relationship. For example, PipeWire being part of the complete COSMIC desktop runtime does not by itself make PipeWire source or package output a compile-time input to `cosmic-panel`. Stage dependency/cache graphs must model actual build and ABI inputs separately from runtime/image composition dependencies.

## Cache identity

Source ownership changes dependency identity. Stage cache inputs must ultimately describe the canonical MattOS source/output that was used to produce an artifact, not an external reference that existed in the imported upstream manifest before mirror rewriting. Shared mirror content is consumer-independent, so one canonical mirror fingerprint represents one deterministic rewritten source graph. Changing an owned library invalidates real consumers, while changing an unrelated runtime component does not fan out into needless recompilation.

## Maintenance

Run:

```text
python3 DevUtils/generate_source_overrides.py
python3 DevUtils/test_source_ownership_overrides.py
```

The first command validates source/patch provenance and regenerates the derived ownership catalog. The second exercises source-qualified resolution, canonical/private mirror separation, Git-format output-patch application, idempotent consumer patching, build-mirror patch ordering, gitlink replacement behavior, metadata fail-closed checks, provenance agreement, and preservation of pristine imported manifests.
