---
name: vendored-source-validation
description: Audit MattOS vendored source against pinned upstream revisions, selection policies, gitlinks, patches, and provenance state.
---

# MattOS vendored-source validation

Use this skill for source provenance audits, upstream synchronization reviews,
output-mirror patch validation, or questions about whether imported source still
matches its declared upstream state.

## Authority chain

Treat these as the source of truth, in combination:

1. `upstream/sources.toml` declares each component's immutable repository,
   revision, destination path, sync method, source-selection policy, gitlink
   policy, omission policy, and output-mirror patch manifest.
2. `upstream/state/<component>.toml` records the imported commit/tree,
   imported-tree digest, destination, policies, and patch-manifest identity.
3. The destination under `src/` is authoritative imported source and must remain
   unchanged by build-time adaptation.
4. `upstream/patches/<component>/manifest.toml` and listed patch SHA-256 values
   describe permitted `output-mirror-only` adaptations.
5. `DevUtils/generate_source_overrides.py` generates the Cargo ownership catalog
   and contracts under `out/source-ownership/cargo/`.
6. `DevUtils/source_ownership_graph.py` and
   `DevUtils/cargo_source_owned.py` construct and validate derived output mirrors;
   Cargo must consume those mirrors, never rewritten authoritative source.

## Checks

For a metadata/catalog check:

```bash
python3 DevUtils/generate_source_overrides.py --check
```

For the full provenance audit (network metadata may be required):

```bash
python3 DevUtils/audits/test_vendored_source_provenance.py
```

Useful focused regressions include:

```bash
python3 -m unittest discover -s DevUtils/audits -p 'test_*.py'
python3 DevUtils/audits/test_imported_source_immutability.py --help
```

The provenance audit compares upstream Git tree entries and raw worktree blob
content, validates exact pins and policy records, checks missing/stale paths and
gitlinks, and verifies patch provenance. It may write derived audit cache/state
under `out/tmp`; it must not modify imported source or Git index/history.

When investigating a mismatch, first classify it as an incorrect pin/state
record, an undocumented source-selection/omission decision, an upstream gitlink
requiring an explicit policy, a legitimate output-mirror patch, or generated
residue. Do not edit vendored source directly or copy host-generated files into
it. Fix the declaration/provenance or mirror-construction logic at the proper
layer, then add a regression that proves authoritative source remains pristine.

Keep provenance identity separate from derived Cargo mirror identity. A mirror
must include the declared source, patch manifest and patch bytes, ownership
contract, and relevant rewrite inputs in its validity contract; stale or
partially prepared mirrors must fail closed before Cargo consumes them.
