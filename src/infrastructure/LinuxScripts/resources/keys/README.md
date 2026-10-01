# MattPackages public signing key

`mattpackages-archive-keyring.asc` is the existing public archive signing key shared
by MattPackages and MattOS. It contains no private key or publishing credentials.
It was retrieved from the repository management service and matched against the
existing MattOS public archive key. An isolated APT refresh verified the public
MattPackages `stable` metadata with this key during implementation.

Interactive setup installs this key for the MattPackages source only. Its SHA-256
digest is pinned in `src/mattpackages.py`. When rotating archive signing keys,
update this public key and that digest together, and verify signed repository
metadata before distributing the change.
