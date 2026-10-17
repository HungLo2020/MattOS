# SMB network shares

MattOS builds `cifs-utils` and `smbclient` from pinned, editable upstream source
in `src/system/network/cifs-utils` and `src/system/network/samba`. They are
ordinary native packages in the ISO's offline repository and the hosted MattOS
repository. The installer does not configure a personal storage server or
credentials automatically.

`cifs-utils` supplies `mount.cifs`, `cifscreds` and the kernel-client utilities.
The Linux kernel includes the CIFS module. `smbclient` supplies the Samba
SMB2/SMB3 command-line client. No Samba server service is installed or enabled.
The initial scope supports password/guest access and SMB3 encryption; AD domain
management and the Kerberos upcall/idmapping utilities are not enabled.

Samba's Talloc library is built separately from its retained source subtree for
cifs-utils. Its GnuTLS, Nettle, libtasn1, GMP and other runtime libraries are
MattOS source-built packages, not copies from the build host. Verified release
archives supply generated crypto-library bootstrap inputs in output-owned
mirrors, overlaid with retained authoritative Git source. Development/test
GnuTLS submodules are explicitly excluded by source policy.

The Samba build bootstraps the host-only Parse::Yapp 1.21 PIDL generator under
`out/build/samba-host-tools` from its CPAN archive, SHA256
`3810e998308fba2e0f4f26043035032b027ce51ce5c8a52a8b8e340ca65f13e5`.
It runs the generator's own tests and does not install it into the host or image.

LinuxScripts interactive `Tools/Setup.py` offers its persistent Tailscale SMB
mount on both Linux and MattOS APT hosts after a successful or skipped package
profile step. It requires an authenticated Tailscale connection, installs the
SMB tools, prompts for the share and credentials, and creates a retrying
`storage-smb-mount.service`. Its defaults are `//100.72.33.98/storage` mounted
at `/mnt/storage`. Credentials stay in a root-only file; personal share policy
lives in LinuxScripts rather than the MattOS image.

For manual diagnostics, use `smbclient --version`, `sudo mount.cifs -V`, and
`modinfo cifs`. Listing a share and a real SMB3 mount/read/write round trip are
required runtime verification; a successful build or `--help` is insufficient.

After the canonical full build, run
`python3 DevUtils/run_qemu.py --smb-test --no-build`. This boots a disposable
live VM, reinstalls both client packages using only the offline ISO repository,
starts an isolated loopback-only Samba fixture built from the same
source, and verifies regular-user authenticated encrypted SMB 3.1.1 transfers and
kernel CIFS mount/read/write round trips. The fixture server is not shipped in
the ISO or installed client package. Results are in
`out/logs/smb-runtime-test.log`; no physical machine is used.
