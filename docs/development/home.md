# Development

Start here for building MattOS and for the design documents behind its build
system, bootstrap, packaging and system components.

- [Building MattOS](../BUILD.md)
- Build system:
  [architecture and invalidation contract](../BUILD_SYSTEM_ARCHITECTURE.md),
  [performance and cache model](../BUILD_PERFORMANCE.md),
  [cache audit](../BUILD_CACHE_AUDIT.md)
- Toolchain and bootstrap:
  [native toolchain](../NATIVE_TOOLCHAIN.md),
  [glibc](../GLIBC_BOOTSTRAP.md),
  [GCC runtime](../GCC_RUNTIME_BOOTSTRAP.md),
  [bootstrap runtime audit](../BOOTSTRAP_RUNTIME.md),
  [self-hosting](../SELF_HOSTING_DEVELOPMENT.md)
- Sources:
  [source closure](../SOURCE_CLOSURE.md),
  [source ownership](../SOURCE_OWNERSHIP.md),
  [upstream synchronization](../UPSTREAM_SYNC.md)
- Packaging:
  [Debian packaging](../PACKAGING.md),
  [Debian 13 compatibility](../DEBIAN_COMPATIBILITY.md),
  [remote repository](../REMOTE_REPOSITORY.md)
- System: see the System section in the navigation.

## Editing this wiki

The wiki is the Markdown under `docs/`, built with MkDocs Material.
Preview it locally with `python3 DevUtils/RunWiki.py` (serves on
<http://127.0.0.1:8000>); `python3 DevUtils/RunWiki.py build` runs the same
strict build as CI. Pushing changes to `docs/` on `main` publishes the site.
