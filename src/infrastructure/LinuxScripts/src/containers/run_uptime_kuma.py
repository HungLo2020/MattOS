#!/usr/bin/env python3
"""Direct Python replacement for the legacy Uptime Kuma droplet launcher."""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path


sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from containers.workloads import UptimeKumaWorkload, parse_action


def main(argv: list[str] | None = None) -> int:
    """Retain the legacy no-argument, --on, --off, and -D interface."""

    try:
        return UptimeKumaWorkload().execute(parse_action(list(sys.argv[1:] if argv is None else argv)))
    except (OSError, RuntimeError, subprocess.CalledProcessError, ValueError) as error:
        print(f"Error: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())