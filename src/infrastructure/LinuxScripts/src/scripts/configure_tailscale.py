#!/usr/bin/env python3
"""Enable Tailscale and optionally complete interactive device authentication."""

from __future__ import annotations

import json
import subprocess
import sys


def tailscale_status() -> dict[str, object] | None:
    """Return Tailscale status JSON, or None when it is not available yet."""

    result = subprocess.run(
        ("tailscale", "status", "--json"),
        text=True,
        capture_output=True,
        check=False,
    )
    if result.returncode != 0:
        return None
    try:
        status = json.loads(result.stdout)
    except json.JSONDecodeError:
        return None
    return status if isinstance(status, dict) else None


def is_connected(status: dict[str, object] | None) -> bool:
    """Return whether Tailscale has an online identity and running backend."""

    if status is None or status.get("BackendState") != "Running":
        return False
    self_status = status.get("Self")
    return isinstance(self_status, dict) and self_status.get("Online") is True


def confirm_enrollment() -> bool:
    """Require an explicit answer before opening an interactive browser login."""

    try:
        return input("Tailscale is not connected. Start interactive sign-in now? [y/N]: ").strip().lower() in {"y", "yes"}
    except EOFError:
        return False


def main() -> int:
    subprocess.run(("sudo", "systemctl", "enable", "--now", "tailscaled"), check=True)
    if is_connected(tailscale_status()):
        print("Tailscale is already connected; skipping interactive enrollment.")
        return 0
    if not confirm_enrollment():
        print("Tailscale is enabled but not connected. Run 'sudo tailscale up' when ready to enroll this device.")
        return 0

    print("Starting Tailscale authentication. Complete the browser sign-in if prompted.")
    subprocess.run(("sudo", "tailscale", "up"), check=True)
    subprocess.run(("tailscale", "status"), check=True)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except subprocess.CalledProcessError as error:
        print(f"Error: Tailscale setup failed: {error}", file=sys.stderr)
        raise SystemExit(error.returncode or 1) from error