"""SMB3 client and kernel mount validation inside an ephemeral MattOS VM."""
from __future__ import annotations

import argparse
import base64
from pathlib import Path
import shutil
import tarfile
import tempfile

from common import RepoError, ensure_project_temp_root
from qemu_test_control import serial_command_stream, wait_for_socket


def prepare_fixture(repo: Path, destination: Path) -> Path:
    source = repo / "out/build/samba/validation"
    payload = destination / "fixture"
    (payload / "bin").mkdir(parents=True)
    (payload / "lib").mkdir()
    for name, relative in (("smbd", "usr/sbin/smbd"), ("smbpasswd", "usr/bin/smbpasswd")):
        candidate = source / relative
        if not candidate.is_file():
            raise RepoError(f"Build the Samba validation fixture first: missing {candidate}")
        shutil.copy2(candidate, payload / "bin" / name)
    # The builder retained only source-built libraries required by the fixture.
    # Do not copy anything from the host's /usr/lib or /lib.
    for library in (source / "usr/lib/x86_64-linux-gnu/samba").glob("*.so*"):
        resolved = library.resolve(strict=True)
        if not resolved.is_relative_to(source.resolve()):
            raise RepoError(f"SMB fixture library leaves its source build: {library}")
        if resolved.is_file():
            shutil.copy2(resolved, payload / "lib" / library.name)
    archive = destination / "fixture.tar.gz"
    with tarfile.open(archive, "w:gz") as tar:
        tar.add(payload, arcname=".")
    return archive


SCRIPT = r'''#!/bin/sh
set -eu
user=$1
t=$(mktemp -d /tmp/mattos-smb-test.XXXXXX)
pid=
cleanup() {
    result=$?
    if [ "$result" -ne 0 ]; then
        cat "$t/server.log" "$t/log.smbd" 2>/dev/null || true
        dmesg | tail -n 50 || true
    fi
    mountpoint -q "$t/mount" && umount "$t/mount" || true
    if [ -n "$pid" ]; then kill "$pid" 2>/dev/null || true; wait "$pid" 2>/dev/null || true; fi
    rm -rf "$t"
}
trap cleanup EXIT INT TERM
tar -xzf /tmp/mattos-smb-fixture.tar.gz -C "$t"
mkdir -p "$t/share" "$t/client" "$t/mount" "$t/private" "$t/state" "$t/cache" "$t/lock" "$t/pid" "$t/ncalrpc"
chmod 755 "$t"
chmod 700 "$t/private"
chown "$user" "$t/share" "$t/client"
chmod 700 "$t/client"
client() { sudo -u "$user" env -u LD_LIBRARY_PATH smbclient "$@"; }
cat > "$t/smb.conf" <<EOF
[global]
    server role = standalone server
    security = user
    workgroup = WORKGROUP
    interfaces = 127.0.0.1
    bind interfaces only = yes
    server min protocol = SMB3_11
    private dir = $t/private
    state directory = $t/state
    cache directory = $t/cache
    lock directory = $t/lock
    pid directory = $t/pid
    ncalrpc dir = $t/ncalrpc
    passdb backend = smbpasswd:$t/private/smbpasswd
    load printers = no
    printing = bsd
    printcap name = /dev/null
    disable spoolss = yes
    log file = $t/log.smbd
[storage]
    path = $t/share
    read only = no
    valid users = $user
    smb encrypt = required
EOF
printf '%s\n' MattOS-SMB-test-only MattOS-SMB-test-only | LD_LIBRARY_PATH="$t/lib" "$t/bin/smbpasswd" -c "$t/smb.conf" -a "$user" -s
credentials="$t/client/credentials"
printf 'username=%s\npassword=MattOS-SMB-test-only\ndomain=WORKGROUP\n' "$user" > "$credentials"
chown "$user" "$credentials"
chmod 600 "$credentials"
LD_LIBRARY_PATH="$t/lib" "$t/bin/smbd" --foreground --no-process-group --configfile="$t/smb.conf" --port=1445 > "$t/server.log" 2>&1 < /dev/null &
pid=$!
ready=no
for n in $(seq 1 30); do
    if client //127.0.0.1/storage -p 1445 -A "$credentials" --client-protection=encrypt -c ls; then ready=yes; break; fi
    kill -0 "$pid" || { cat "$t/server.log" "$t/log.smbd"; exit 1; }
    sleep 1
done
test "$ready" = yes
client --version
mount.cifs -V
printf 'MattOS SMB3 encrypted round trip\n' > "$t/client/input"
client //127.0.0.1/storage -p 1445 -A "$credentials" --client-protection=encrypt -c "put $t/client/input client-file; get client-file $t/client/output; del client-file"
cmp "$t/client/input" "$t/client/output"
mount -t cifs //127.0.0.1/storage "$t/mount" -o "credentials=$credentials,port=1445,vers=3.1.1,seal,uid=$(id -u "$user"),gid=$(id -g "$user")"
cp "$t/client/input" "$t/mount/kernel-file"
cmp "$t/client/input" "$t/share/kernel-file"
printf 'server-to-kernel\n' > "$t/share/server-file"
cmp "$t/share/server-file" "$t/mount/server-file"
findmnt -n -o FSTYPE,OPTIONS "$t/mount" | grep 'vers=3.1.1' | grep seal
umount "$t/mount"
echo '[smb-test] PASS authenticated encrypted SMB3 client transfer and CIFS mount/read/write'
'''


def run(repo: Path, iso: Path, args: argparse.Namespace) -> int:
    import run_qemu
    options = argparse.Namespace(**vars(args))
    options.test_control = True
    options.no_install_disk = True
    options.install_disk = None
    log = repo / "out/logs/smb-runtime-test.log"
    log.write_text("MattOS source-built SMB runtime validation\n")
    with tempfile.TemporaryDirectory(prefix="smb-validation-", dir=ensure_project_temp_root(repo)) as raw:
        directory = Path(raw)
        prepare_fixture(repo, directory)
        server, port = run_qemu.serve_test_directory(directory)
        try:
            def lifecycle(_proc, paths):
                wait_for_socket(paths[0], 120)

                def stream(chunk):
                    print(chunk, end="", flush=True)
                    with log.open("a") as output:
                        output.write(chunk)

                def command(body):
                    return serial_command_stream(paths[1].resolve(), body, 180, on_output=stream)

                command("test -e /run/mattos-live && smbclient --version && sudo mount.cifs -V && "
                        "test ! -e /usr/sbin/smbd && test ! -e /usr/bin/smbd")
                command("mkdir -p /tmp/mattos-smb-apt-sources && "
                        "for file in /etc/apt/sources.list.d/*.sources; do "
                        "if grep -q 'file:/usr/share/mattos/repository' \"$file\"; then "
                        "cp \"$file\" /tmp/mattos-smb-apt-sources/; fi; done")
                apt = ("sudo apt-get -o Dir::Etc::sourcelist=/dev/null "
                       "-o Dir::Etc::sourceparts=/tmp/mattos-smb-apt-sources")
                command(f"{apt} update")
                command(f"{apt} -y --no-remove --reinstall install cifs-utils smbclient")
                command(f"curl --retry 15 --retry-delay 1 --retry-all-errors -f "
                        f"http://10.0.2.2:{port}/fixture.tar.gz -o /tmp/mattos-smb-fixture.tar.gz")
                encoded = base64.b64encode(SCRIPT.encode()).decode()
                # Keep the UART command line small: send the script in chunks.
                command(": > /tmp/mattos-smb-validation.b64")
                for at in range(0, len(encoded), 512):
                    command(f"printf '%s' '{encoded[at:at + 512]}' >> /tmp/mattos-smb-validation.b64")
                command("base64 -d /tmp/mattos-smb-validation.b64 > /tmp/mattos-smb-validation.sh && "
                        "sudo sh /tmp/mattos-smb-validation.sh \"$(id -un)\"")
                command(run_qemu.scheduled_poweroff_command())

            return run_qemu._launch_one(repo, iso, options, boot_iso=True, install_disk=None, lifecycle=lifecycle)
        finally:
            server.shutdown()
            server.server_close()
