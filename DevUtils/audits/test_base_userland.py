#!/usr/bin/env python3
"""Exercise MattOS base-userland commands in an isolated built rootfs.

This test uses bubblewrap to execute the package-owned MattOS binaries against
the MattOS rootfs and libraries. It never substitutes host commands inside the
test environment. Pass ``--https`` to include a live Git HTTPS remote check.
"""

from __future__ import annotations

import argparse
import shutil
import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]

REQUIRED_COMMANDS = (
    "lsblk dmesg fdisk cfdisk sfdisk wipefs blkid findmnt losetup mountpoint "
    "blockdev flock lscpu lslocks lsns nsenter unshare taskset chrt ionice "
    "prlimit uuidgen gzip gunzip zcat bzip2 bunzip2 bzcat bzip2recover xz "
    "unxz xzcat lzma unlzma lzcat zstd unzstd zstdcat patch file less lesskey "
    "git scalar ssh scp sftp ssh-add ssh-agent ssh-keygen ssh-keyscan sshd"
).split()


def command_path(rootfs: Path, command: str) -> Path:
    for directory in ("usr/bin", "usr/sbin", "usr/libexec"):
        candidate = rootfs / directory / command
        if candidate.exists():
            return candidate
    raise AssertionError(f"required MattOS command is missing: {command}")


def bwrap_prefix(rootfs: Path) -> list[str]:
    return [
        "bwrap",
        "--unshare-all",
        "--share-net",
        "--die-with-parent",
        "--tmpfs",
        "/",
        "--ro-bind",
        str(rootfs / "usr"),
        "/usr",
        "--ro-bind",
        str(rootfs / "etc"),
        "/etc",
        "--ro-bind",
        str(rootfs / "var"),
        "/var",
        "--ro-bind",
        str(rootfs / "home"),
        "/home",
        "--symlink",
        "usr/bin",
        "/bin",
        "--symlink",
        "usr/sbin",
        "/sbin",
        "--symlink",
        "usr/lib",
        "/lib",
        "--symlink",
        "usr/lib64",
        "/lib64",
        "--dev",
        "/dev",
        "--proc",
        "/proc",
        "--tmpfs",
        "/tmp",
        "--dir",
        "/run",
        "--dir",
        "/run/systemd",
        "--dir",
        "/run/systemd/resolve",
        "--ro-bind",
        "/etc/resolv.conf",
        "/run/systemd/resolve/stub-resolv.conf",
    ]


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--rootfs", type=Path, default=ROOT / "out/build/rootfs")
    parser.add_argument("--https", action="store_true")
    args = parser.parse_args()
    rootfs = args.rootfs.resolve()
    if not (rootfs / "usr/bin/brush").is_file():
        parser.error(f"MattOS rootfs is not built at {rootfs}")
    if shutil.which("bwrap") is None:
        parser.error("bubblewrap (bwrap) is required for isolated rootfs execution")
    for command in REQUIRED_COMMANDS:
        command_path(rootfs, command)

    script = r"""
set -eu
export HOME=/tmp/home TERM=xterm-256color PATH=/usr/sbin:/usr/bin:/sbin:/bin LESSOPEN=
mkdir -p "$HOME" /tmp/git
printf 'MattOS base userland payload\n' >/tmp/payload
for codec in gzip bzip2 xz zstd; do
    "$codec" -c /tmp/payload >"/tmp/payload.$codec"
    case "$codec" in
        gzip) gzip -dc "/tmp/payload.$codec" ;;
        bzip2) bzip2 -dc "/tmp/payload.$codec" ;;
        xz) xz -dc "/tmp/payload.$codec" ;;
        zstd) zstd -q -dc "/tmp/payload.$codec" ;;
    esac >"/tmp/roundtrip.$codec"
    cmp /tmp/payload "/tmp/roundtrip.$codec"
done

printf 'before\n' >/tmp/patch-target
printf '%s\n' '--- patch-target' '+++ patch-target' '@@ -1 +1 @@' '-before' '+after' |
    (cd /tmp && patch -s patch-target)
grep -qx after /tmp/patch-target
file /tmp/patch-target | grep -q 'ASCII text'
less -F -X /tmp/patch-target | grep -qx after

git -C /tmp/git init -q
git -C /tmp/git config user.name 'MattOS Test'
git -C /tmp/git config user.email test@mattos.invalid
printf 'tracked\n' >/tmp/git/tracked
git -C /tmp/git add tracked
git -C /tmp/git commit -qm initial
test "$(git -C /tmp/git log -1 --format=%s)" = initial

ssh -G -F /etc/ssh/ssh_config localhost >/tmp/ssh-effective
grep -q '^hostname localhost$' /tmp/ssh-effective
ssh-keygen -q -t ed25519 -N '' -f /tmp/test-key
ssh-keygen -lf /tmp/test-key.pub >/dev/null

for command in lsblk dmesg fdisk cfdisk sfdisk wipefs blkid findmnt losetup \
    mountpoint blockdev flock lscpu lslocks lsns nsenter unshare taskset chrt \
    ionice prlimit uuidgen; do
    "$command" --version >/dev/null
done

/bin/sh -c 'value=posix; case "$value" in posix) exit 0;; *) exit 1;; esac'
if /bin/sh -c 'items=(one two)' >/dev/null 2>&1; then
    echo '/bin/sh incorrectly accepted Bash array syntax' >&2
    exit 1
fi
/bin/bash -c 'items=(one two); [[ ${items[1]} == two ]]'
printf '__MATTOS_BASE_USERLAND_OK__\n'
"""
    if args.https:
        script += "git ls-remote https://github.com/git/git.git HEAD >/tmp/git-head\n"
        script += "test -s /tmp/git-head\n"

    completed = subprocess.run(
        [*bwrap_prefix(rootfs), "/bin/sh", "-c", script],
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    print(completed.stdout, end="")
    if completed.returncode != 0:
        raise SystemExit(f"base-userland isolated runtime test failed: {completed.returncode}")
    if "__MATTOS_BASE_USERLAND_OK__" not in completed.stdout:
        raise SystemExit("base-userland success marker missing")
    print(f"validated {len(REQUIRED_COMMANDS)} MattOS base-userland commands in {rootfs}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
