# MattOS source-closure policy

MattOS source closure applies to the software MattOS intentionally installs in
its defined base image. It does not require recursively copying every build
dependency into the monorepo.

Classify a dependency with two questions:

1. Does it independently produce a program, script, or runtime shared library
   intentionally installed in the MattOS base image?
2. If not, is it a major source subsystem that MattOS intentionally expects to
   modify or update independently?

A yes answer makes first-class source ownership appropriate. Two no answers
make the dependency normal build dependency state. Static linking or embedding
data does not change that result; an independently installed runtime `.so`
does.

## Source categories

### First-class source-owned component

An installed base program, runtime shared library, script, or intentionally
source-owned major subsystem. Authoritative modifiable source belongs under
`src/` where appropriate and receives normal immutable upstream provenance,
import-fidelity, patch, and update records.

### Build-only or transitive dependency

A crate, helper library, generator, or implementation dependency used only to
produce another component. Its package-manager lockfile, exact Git commit,
registry checksum, and normal download cache identify it reproducibly. It does
not receive an authoritative first-class tree merely because it is fetched
from Git or statically linked.

### Bootstrap or toolchain dependency

An input needed to construct the system or toolchain but not independently
installed as a MattOS base runtime component. It may be fetched or staged by a
documented reproducible bootstrap policy. Special output-mirror policies are
kept separate from authoritative imported source.

### Firmware or microcode blob

Binary firmware and microcode use MattOS's existing explicit, checksummed
binary exception. They are not represented as source-built programs. The
pinned `linux-firmware` source tree remains authoritative for the binary
payload, `WHENCE`, and redistribution licenses; packages must be derived from
that tree without substituting firmware from the build host.

### User-installed Debian or Flatpak package

Software installed later by a user is outside the MattOS-defined base source
closure. Its distribution/package provenance applies instead.

## Cargo dependencies and offline operation

Committed Cargo lockfiles pin Git dependencies to exact revisions and registry
packages to checksums. Network access may populate Cargo's normal cache. A
populated cache can then support `--locked --offline` builds, but downloaded
cache trees remain ignored cache/output state and must never be copied into
authoritative `src/`.

Source-ownership verification must preserve the Cargo resolution policy of the
command it verifies. In particular, a locked build is verified with
`cargo metadata --locked`; an offline/frozen build remains offline/frozen during
the strict verification step. The verifier does not force ordinary online
locked builds offline, because first-time cache population for normal registry
build dependencies is an allowed part of this policy.

Structural source ownership may change an owned dependency from an upstream
Git/registry identity to a MattOS output-mirror path. That derived graph cannot
reuse the copied upstream lock byte-for-byte because Cargo records source
identity in `Cargo.lock`. MattOS may therefore reconcile the copied lock only in
the output mirror before strict verification. This reconciliation is not an
upstream dependency update and never edits the authoritative lock under `src/`.
After reconciliation, the rewritten graph must be stable under the caller's
original `--locked`/`--offline`/`--frozen` policy before the build may proceed.

The COSMIC desktop hierarchy therefore contains recognizable platform and
desktop projects, not every crate in their recursive dependency graph. Future
COSMIC imports must apply the two-question test before creating a source tree
or upstream state record.
