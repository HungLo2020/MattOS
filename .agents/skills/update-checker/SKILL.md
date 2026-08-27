---
name: update-checker
description: Check every pinned MattOS vendored source component against its declared upstream ref and report commit distance without modifying the checkout.
---

# MattOS vendored-source update checker

Use this skill when the user wants to know whether imported MattOS source is
behind upstream before deciding which components to synchronize. The checker
discovers components from `upstream/sources.toml`; never maintain a package list
inside the script.

Run from the repository root:

```bash
python3 DevUtils/UpdateChecker.py
```

For a focused check or machine-readable output:

```bash
python3 DevUtils/UpdateChecker.py --component cosmic-files
python3 DevUtils/UpdateChecker.py --json
```

The default is a fast hash-only check. It reports each component as soon as its
remote ref completes, including progress, and does not fetch commit history.
Normal output distinguishes `up to date` from `update available`; exact commit
distance is intentionally omitted in this mode. `--json` includes full hashes,
repository, ref, source path, status, distance, and any error detail.

Use the slower exact mode only when commit counts are needed:

```bash
python3 DevUtils/UpdateChecker.py --exact
```

Exact mode fetches shallow history into temporary bare repositories and deepens
only when the pinned revision is not reachable. It can still be expensive for
old pins or very large repositories. `--timeout SECONDS` limits every Git
operation (45 seconds by default), and `--jobs N` controls concurrent checks.

The script uses `git ls-remote` to resolve the declared branch/ref. Only exact
mode uses temporary bare Git repositories under the system temporary directory
to obtain commit history and calculate ancestry/distance. It never fetches into
the MattOS checkout, changes `upstream/sources.toml`, updates state files,
changes source trees, writes the Git index, or synchronizes anything. A remote
failure or timeout is reported per component and causes a nonzero exit status;
do not interpret an error as “up to date.”

Interpret results carefully:

- `up to date`: pinned revision equals the declared upstream ref tip.
- `update available`: the upstream hash differs; rerun with `--exact` for ancestry and commit distance.
- `behind N commit(s)`: exact mode proved the pinned revision is an ancestor and N commits are available.
- `diverged`: the pinned revision is not an ancestor of the current ref; inspect history before updating.
- `local-ahead`: the pinned revision is newer than the declared ref; verify the ref declaration.
- `error`: the remote/ref/history could not be checked.

This is an availability/age report, not permission to update source. To update
a component, use the separate upstream synchronization workflow: deliberately
change its exact revision, run `cargo run -p mattos-build -- upstream sync
<component>`, perform provenance checks, and then use the canonical
`python3 DevUtils/run_qemu.py --build-only` plus appropriate runtime validation.
