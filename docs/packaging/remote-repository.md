# MattOS remote repository integration

MattOS builds and validates `.deb` files. The imported LinuxScripts publisher
uploads explicitly approved artifacts to the home repository service, which
publishes them to Cloudflare R2 for `https://packages.mattsherfey.com`
(`trixie`, `main`, `amd64`/`all`).

The authoritative upstream is imported as ordinary source, without a nested
Git repository:

```text
repository: https://github.com/HungLo2020/LinuxScripts.git
branch: master
commit: d1e85219c8f86ceaa1135312126d02fa4dbee623
destination: src/infrastructure/LinuxScripts
sync method: copy
```

`upstream/state/linuxscripts.toml` records the import timestamp and commit.
`upstream/policies/linuxscripts.toml` pins the authoritative publisher:

```text
src/infrastructure/LinuxScripts/GenericScripts/ManageMattOSRepository.py
SHA-256: 0b0be18e1164481612aa41ab6300c301b5d1088f86f9f653aed5a516ed50f35c
```

The imported component is externally maintained and read-only in MattOS.
Agents must not patch, format, rename, or relocate the publisher. A required
change must be reproduced and fixed in LinuxScripts upstream, then imported by
the normal sync workflow. `package compatibility-audit` verifies the state,
checksum, policy, and absence of `.git` anywhere in the imported tree.

## Publishing generated packages

After a successful build, upload every generated `amd64` binary package with:

```text
python3 DevUtils/PublishPackages.py
```

The script first reuses `run_qemu.py`'s `build_if_needed` path (`mattos-build
doctor`, then `build all`) unless `--no-build` is given; `--clean` runs
`mattos-build clean artifacts` before that build. It then reads
`out/packages/inventory.toml` with `tomllib` and takes the `artifact_path` of
each top-level `[[package]]` entry. Every artifact must resolve inside
`out/packages/amd64`, be a regular (non-symlink) `.deb`, and appear only once;
a duplicate or malformed entry is an error. Stale `.deb` files left in
`out/packages/amd64` but absent from the inventory are ignored, and package
names are not maintained in the script. It then compares that set with the
hosted repository's `Packages` index: packages identical to the published
ones are skipped, and packages older than a published version are refused (see
[Publishing Packages](publishing.md#what-gets-uploaded)). It passes the new,
newer and rebuilt packages, sorted, to the vendored publisher as
`ManageMattOSRepository.py --non-interactive --repo mattos upload ...`.

`--dry-run` still builds (unless `--no-build` is also given) and still runs the
publisher, now with `--non-interactive --dry-run`. The publisher validates each
package with `dpkg-deb --show`, checks its architecture, rejects duplicate
name/version/architecture entries, prints
`Dry run: upload N validated package(s)`, and exits without uploading.

## Manual validation handoff

To validate approved build outputs and print—without executing—the publisher
command:

```text
cargo run -p mattos-build -- package publish-plan \
  out/packages/amd64/<package>_<version>_amd64.deb
```

Every selected file must exist, end in `.deb`, resolve beneath the canonical
`out/packages/` directory, and exactly match the path and SHA-256 in
`out/packages/inventory.toml`. Symlink escapes, missing files, directories,
non-package files, and unrecorded or changed artifacts are rejected. Naming
the same artifact more than once is an error ("duplicate publication artifacts
are not allowed"). A successful command only prints the exact
`python3 .../ManageMattOSRepository.py --repo mattos upload ...` invocation; it does not run
the script, access credentials, or mutate the imported source. Use
`DevUtils/PublishPackages.py` for the actual build-and-upload workflow.

`PublishPackages.py` invokes only the publisher's `upload` command. Repository
administration commands such as `init`, `remove`, and `publish` remain manual
operations.

The hosted deb822 APT source at `https://packages.mattsherfey.com` is enabled
on both the live image and installed systems and is verified with
`Signed-By: /usr/share/keyrings/mattos-archive-keyring.asc`. Its published
Release metadata must use `Origin: MattOS`, `Label: MattOS`, `Suite: trixie`,
and `Codename: trixie`; the local repository keeps the distinct
`Label: MattOS Local` identity. Both are pinned at `990`, so a newer hosted
version is a normal upgrade candidate while the local repository still
provides a complete offline closure. See
[APT source and pin policy](debian-packaging.md#apt-source-and-pin-policy).
