"""Configure and maintain a Tailscale-gated CIFS storage mount on Linux."""

from __future__ import annotations

import argparse
import getpass
import json
import os
import pwd
import subprocess
import sys
from dataclasses import asdict, dataclass
from pathlib import Path


DEFAULT_SERVER = "100.72.33.98"
DEFAULT_SHARE = "storage"
DEFAULT_MOUNT_POINT = "/mnt/storage"
DEFAULT_BITWARDEN_ITEM = "PCPassword"
CONFIG_DIRECTORY = Path("/etc/linuxscripts")
CONFIG_PATH = CONFIG_DIRECTORY / "storage-smb-mount.json"
HELPER_DIRECTORY = Path("/usr/local/lib/linuxscripts")
HELPER_PATH = HELPER_DIRECTORY / "storage_smb_mount.py"
LEGACY_HELPER_PATH = Path("/usr/local/sbin/storage-smb-mount.sh")
SERVICE_PATH = Path("/etc/systemd/system/storage-smb-mount.service")
SERVICE_NAME = "storage-smb-mount.service"


@dataclass(frozen=True)
class MountConfiguration:
    """Root-owned settings needed by the systemd CIFS mount helper."""

    server: str
    share: str
    mount_point: str
    credentials_file: str
    uid: int
    gid: int


def sudo(command: tuple[str, ...], **kwargs) -> subprocess.CompletedProcess:
    """Run a privileged command directly when root, otherwise through sudo."""

    prefix = () if os.geteuid() == 0 else ("sudo",)
    check = kwargs.pop("check", True)
    return subprocess.run((*prefix, *command), check=check, **kwargs)


def prompt_value(label: str, default: str) -> str:
    """Prompt for a non-empty setting while preserving the legacy default."""

    entered = input(f"{label} [{default}]: ").strip()
    return entered or default


def tailscale_connected() -> bool:
    """Return whether the local Tailscale daemon has an online node identity."""

    try:
        result = subprocess.run(("tailscale", "status", "--json"), text=True, capture_output=True, check=False)
    except OSError:
        return False
    if result.returncode != 0:
        return False
    try:
        status = json.loads(result.stdout)
    except json.JSONDecodeError:
        return False
    self_status = status.get("Self") if isinstance(status, dict) else None
    return status.get("BackendState") == "Running" and isinstance(self_status, dict) and self_status.get("Online") is True


def bitwarden_password(item_name: str) -> str | None:
    """Retrieve an SMB password through the shared visible-prompt helper."""

    from bitwarden import BitwardenClient, BitwardenError

    try:
        return BitwardenClient(password_file=Path(__file__).resolve().parents[1] / ".bw_master_password").password(item_name)
    except BitwardenError as error:
        print(f"Bitwarden password lookup failed: {error}", file=sys.stderr)
        return None


def choose_password() -> str:
    """Use Bitwarden when possible, otherwise request the SMB password locally."""

    item_name = os.environ.get("SMB_BITWARDEN_ITEM", DEFAULT_BITWARDEN_ITEM)
    password = bitwarden_password(item_name)
    if password:
        print(f"Using SMB password from Bitwarden item '{item_name}'.")
        return password
    while not (password := getpass.getpass("SMB password: ")):
        print("An SMB password is required.")
    return password


def write_root_file(path: Path, contents: str, mode: str) -> None:
    """Write a root-owned file through sudo without sending content to stdout."""

    sudo(("install", "-d", "-m", "0755", str(path.parent)))
    sudo(("tee", str(path)), input=contents, text=True, stdout=subprocess.DEVNULL)
    sudo(("chmod", mode, str(path)))


def install_prerequisites() -> None:
    """Install the SMB client and mount tools needed by the generated helper.

    Do not refresh every configured APT source here.  A mount is independent of
    unrelated third-party repositories, and ``apt-get install`` can use the
    package lists already present on the machine (and is a no-op when these
    packages are installed).
    """

    sudo(("apt-get", "install", "-y", "smbclient", "cifs-utils"))


def install_helper() -> None:
    """Install a root-owned copy so systemd never executes user-writable code."""

    sudo(("install", "-d", "-m", "0755", str(HELPER_DIRECTORY)))
    sudo(("install", "-m", "0700", str(Path(__file__).resolve()), str(HELPER_PATH)))


def retire_legacy_implementation(user: str, active_credentials: Path) -> None:
    """Replace the legacy shell service without unmounting an active share.

    The legacy and Python implementations use the same systemd service name.
    Stopping the oneshot service does not unmount CIFS; the new helper verifies
    an existing matching mount and exits successfully when it restarts.
    """

    legacy_credentials = Path(f"/etc/samba/credentials-{DEFAULT_SHARE}-{user}")
    sudo(("systemctl", "disable", "--now", SERVICE_NAME), check=False)
    sudo(("rm", "-f", str(LEGACY_HELPER_PATH)))
    if legacy_credentials != active_credentials:
        sudo(("rm", "-f", str(legacy_credentials)))


def service_contents() -> str:
    """Return the retrying systemd service definition for the CIFS helper."""

    return f"""[Unit]
Description=Mount Tailscale SMB storage share
After=network-online.target tailscaled.service
Wants=network-online.target tailscaled.service
StartLimitIntervalSec=0

[Service]
Type=oneshot
ExecStart=/usr/bin/python3 {HELPER_PATH} --mount --config {CONFIG_PATH}
RemainAfterExit=yes
Restart=on-failure
RestartSec=20

[Install]
WantedBy=multi-user.target
"""


def configure_interactively() -> None:
    """Install a persistent SMB mount service using the legacy mount defaults."""

    if not tailscale_connected():
        raise RuntimeError("Tailscale must be connected before configuring the storage mount.")
    user = os.environ.get("SUDO_USER") or getpass.getuser()
    account = pwd.getpwnam(user)
    server = prompt_value("Server Tailscale IP", DEFAULT_SERVER)
    share = prompt_value("SMB share name", DEFAULT_SHARE)
    mount_point = prompt_value("Local mount point", DEFAULT_MOUNT_POINT)
    password = choose_password()
    credentials_path = Path(f"/etc/samba/credentials-{share}-{user}")
    configuration = MountConfiguration(server, share, mount_point, str(credentials_path), account.pw_uid, account.pw_gid)

    install_prerequisites()
    write_root_file(credentials_path, f"username={user}\npassword={password}\n", "0600")
    write_root_file(CONFIG_PATH, json.dumps(asdict(configuration), indent=2) + "\n", "0600")
    retire_legacy_implementation(user, credentials_path)
    install_helper()
    write_root_file(SERVICE_PATH, service_contents(), "0644")
    sudo(("systemctl", "daemon-reload"))
    sudo(("systemctl", "reset-failed", SERVICE_NAME), check=False)
    sudo(("systemctl", "enable", SERVICE_NAME))
    # Do not make interactive setup wait for a potentially unavailable server.
    # The service's Restart=on-failure policy handles boot-time and transient
    # Tailscale/SMB availability; --no-block only queues its first attempt.
    sudo(("systemctl", "restart", "--no-block", SERVICE_NAME))
    print(f"Storage mount service enabled: {SERVICE_NAME}")
    print(f"It retries every 20 seconds until //{server}/{share} is mounted at {mount_point}.")


def mount_from_config(configuration: MountConfiguration) -> int:
    """Mount the configured CIFS share or fail for systemd to retry later."""

    if not tailscale_connected():
        raise RuntimeError("Tailscale is not connected.")
    mount_point = Path(configuration.mount_point)
    mount_point.mkdir(parents=True, exist_ok=True)
    # ``--target`` reports the filesystem *containing* a directory (usually
    # the root filesystem).  We need to detect only a mount whose mountpoint
    # is exactly the configured directory, otherwise every normal directory
    # is incorrectly rejected as already in use after boot.
    existing = subprocess.run(("findmnt", "-rn", "--mountpoint", str(mount_point), "-o", "SOURCE"), text=True, capture_output=True, check=False)
    expected_source = f"//{configuration.server}/{configuration.share}"
    if existing.returncode == 0:
        if existing.stdout.strip() == expected_source:
            return 0
        raise RuntimeError(f"Mount point is already in use by {existing.stdout.strip()!r}.")
    subprocess.run(
        (
            "mount",
            "-t",
            "cifs",
            expected_source,
            str(mount_point),
            "-o",
            f"credentials={configuration.credentials_file},uid={configuration.uid},gid={configuration.gid},iocharset=utf8,noperm,vers=3.1.1,_netdev",
        ),
        check=True,
    )
    return 0


def load_configuration(path: Path) -> MountConfiguration:
    """Load the root-owned service configuration and reject missing settings."""

    values = json.loads(path.read_text(encoding="utf-8"))
    return MountConfiguration(**values)


def main(argv: list[str] | None = None) -> int:
    """Run the service helper command line."""

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--mount", action="store_true", help="Mount the configured share for systemd.")
    parser.add_argument("--config", type=Path, default=CONFIG_PATH, help="Root-owned mount configuration path.")
    args = parser.parse_args(argv)
    if not args.mount:
        parser.error("--mount is required")
    return mount_from_config(load_configuration(args.config))


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, subprocess.CalledProcessError, json.JSONDecodeError, TypeError) as error:
        print(f"Error: {error}", file=sys.stderr)
        raise SystemExit(1) from error
