# MattOS third-party native packages

This directory contains independently maintained package recipes. They are
not `BuildStage`s and are never fetched or built by a normal MattOS ISO build.
Each recipe downloads source or an upstream binary into a disposable
`out/tmp/mattos-*` directory, verifies what the upstream publishes, creates a
native `.deb`, and can upload it through the existing vendored
`ManageMattOSRepository.py` client.

## Commands

From the repository root:

```text
python3 third-party-packages/fastfetch.py check
python3 third-party-packages/fastfetch.py build
python3 third-party-packages/firefox.py update
python3 third-party-packages/firefox.py publish --dry-run
```

`check` performs version discovery and reports the repository state. `build`
creates a local package without publishing. `publish` builds and uploads, and
`update` is the normal idempotent mode: it skips a version already present in
the MattOS repository. `--dry-run` still validates the package through the
real publisher without changing the server.

Recipes keep package-specific release URLs, build/install policy, metadata,
runtime dependencies, and verification rules small. `common/` owns temporary
workspace cleanup, retries, archive traversal checks, package metadata,
provenance, idempotency checks, and invocation of the existing repository
publisher. Never add downloaded source or build directories here.

Versions are upstream versions; a package rebuild with the same upstream
version is intentionally rejected/skipped by repository identity. A recipe
must use checksums or signatures whenever its upstream provides them and must
fail closed when a declared verification step fails. Runtime dependencies are
declared in `Depends`; host libraries are never copied into a package.

The publisher requires the usual MattOS repository configuration/token and
uses the vendored LinuxScripts implementation. Publication is per complete
`.deb`; the server atomically validates and indexes it. Tests should use local
fixtures and fake publisher/build functions; network publication tests are
explicit integration tests only.
