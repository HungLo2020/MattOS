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
commit: bccc8041ce8e37ab993a418504ddde95cdfccc8c
destination: src/infrastructure/LinuxScripts
sync method: copy
```

`upstream/state/linuxscripts.toml` records the import timestamp and commit.
`upstream/policies/linuxscripts.toml` pins the authoritative publisher:

```text
src/infrastructure/LinuxScripts/GenericScripts/ManageMattOSRepository.py
SHA-256: ff56c6cb56951543dfb8eb0298f424d34517a1d87175a44060ef6f97d6a51cd4
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

The script reuses `run_qemu.py`'s doctor and `build all` path, recursively
reads the authoritative artifact list from `out/packages/inventory.toml`, and
passes exactly that set to the vendored `ManageMattOSRepository.py upload`
command. Stale `.deb` files left in `out/packages/amd64` are ignored. Package
names are not maintained in the script. `--no-build` uses existing artifacts
and `--dry-run` validates and prints the manager invocation without uploading.

## Manual validation handoff

To validate approved build outputs and print—without executing—the future
publisher command:

```text
cargo run -p mattos-build -- package publish-plan \
  out/packages/amd64/<package>_<version>_amd64.deb
```

Every selected file must exist, end in `.deb`, resolve beneath the canonical
`out/packages/` directory, and exactly match the path and SHA-256 in
`out/packages/inventory.toml`. Symlink escapes, missing files, directories,
non-package files, and unrecorded or changed artifacts are rejected. Duplicates
are removed deterministically. A successful command only prints the exact
`python3 .../ManageMattOSRepository.py upload ...` invocation; it does not run
the script, access credentials, or mutate the imported source. Use
`DevUtils/PublishPackages.py` for the actual build-and-upload workflow.

`PublishPackages.py` invokes only the publisher's `upload` command. Repository
administration commands such as `init`, `remove`, and `publish` remain manual
operations.

The hosted deb822 APT source is an intentionally disabled, signed scaffold at
`https://packages.mattsherfey.com`. Its published Release metadata must use
`Origin: MattOS`, `Label: MattOS`, `Suite: trixie`, and `Codename: trixie`;
local media retains the distinct `Label: MattOS Local` identity and higher pin.
