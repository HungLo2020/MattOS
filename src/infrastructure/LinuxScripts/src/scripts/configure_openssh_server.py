#!/usr/bin/env python3
"""Enable the OpenSSH service after the Linux package is installed."""

from __future__ import annotations

import os
import subprocess
import sys


def privileged_command(*arguments: str) -> tuple[str, ...]:
    """Use sudo only when the package setup process is not already root."""

    return arguments if os.geteuid() == 0 else ("sudo", *arguments)


def main() -> int:
    """Match the legacy headless setup's SSH service enable/start behavior."""

    for service in ("ssh", "sshd"):
        result = subprocess.run(privileged_command("systemctl", "enable", "--now", service), check=False)
        if result.returncode == 0:
            print("OpenSSH server is enabled and running.")
            return 0
    raise RuntimeError("Could not enable an OpenSSH systemd service ('ssh' or 'sshd').")


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (RuntimeError, subprocess.CalledProcessError) as error:
        print(f"Error: OpenSSH service setup failed: {error}", file=sys.stderr)
        raise SystemExit(getattr(error, "returncode", None) or 1) from error