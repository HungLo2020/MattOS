#!/usr/bin/env python3
"""Configure RustDesk unattended access, direct IP access, and its service."""

from __future__ import annotations

import getpass
import os
import re
import subprocess
import sys
from pathlib import Path


SOURCE_DIRECTORY = Path(__file__).resolve().parents[1]
if str(SOURCE_DIRECTORY) not in sys.path:
    sys.path.insert(0, str(SOURCE_DIRECTORY))

from bitwarden import BitwardenClient, BitwardenError


DEFAULT_BITWARDEN_ITEM = "PCPassword"
PASSWORD_FILE = Path(__file__).resolve().parents[2] / ".bw_master_password"
CONFIG_PATH = "/root/.config/rustdesk/RustDesk2.toml"


def bitwarden_password(item_name: str) -> str | None:
    """Return a Bitwarden password without hiding the unlock prompt."""

    try:
        return BitwardenClient(password_file=PASSWORD_FILE).password(item_name)
    except BitwardenError as error:
        print(f"Bitwarden password lookup failed: {error}", file=sys.stderr)
        return None


def choose_password() -> str:
    """Use the configured Bitwarden item or require a non-empty local entry."""

    item_name = os.environ.get("BITWARDEN_RUSTDESK_ITEM", DEFAULT_BITWARDEN_ITEM)
    password = bitwarden_password(item_name)
    if password:
        print(f"Using RustDesk password from Bitwarden item '{item_name}'.")
        return password
    while not (password := getpass.getpass("RustDesk permanent password for unattended access: ")):
        print("A RustDesk permanent password is required.")
    return password


def config_contents(password: str) -> str:
    """Merge unattended and direct-IP settings into the current configuration."""

    escaped_password = password.replace("\\", "\\\\").replace('"', '\\"')
    current = subprocess.run(
        ("sudo", "cat", CONFIG_PATH),
        text=True,
        capture_output=True,
        check=False,
    ).stdout
    entries = f'permanent-password = "{escaped_password}"\ndirect-server = "Y"'
    if re.search(r"^\[options\]$", current, re.MULTILINE):
        current = re.sub(r'^permanent-password\s*=.*$', f'permanent-password = "{escaped_password}"', current, flags=re.MULTILINE)
        current = re.sub(r'^direct-server\s*=.*$', 'direct-server = "Y"', current, flags=re.MULTILINE)
        missing_entries = [entry for entry in entries.splitlines() if entry.split(" = ", 1)[0] not in current]
        return current.rstrip() + ("\n" + "\n".join(missing_entries) if missing_entries else "") + "\n"
    return current.rstrip() + f"\n[options]\n{entries}\n"


def main() -> int:
    password = choose_password()
    subprocess.run(("sudo", "systemctl", "stop", "rustdesk"), check=False)
    subprocess.run(("sudo", "systemctl", "disable", "rustdesk"), check=False)
    subprocess.run(("sudo", "install", "-d", "-m", "0700", "/root/.config/rustdesk"), check=True)
    subprocess.run(("sudo", "tee", CONFIG_PATH), input=config_contents(password), text=True, stdout=subprocess.DEVNULL, check=True)
    subprocess.run(("sudo", "chmod", "0600", CONFIG_PATH), check=True)
    subprocess.run(("sudo", "systemctl", "enable", "--now", "rustdesk"), check=True)
    Path("/tmp/linuxscripts-rustdesk-amd64.deb").unlink(missing_ok=True)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except subprocess.CalledProcessError as error:
        print(f"Error: RustDesk configuration failed: {error}", file=sys.stderr)
        raise SystemExit(error.returncode or 1) from error