#!/usr/bin/env python3
"""Direct Python replacement for the legacy AUTOMATIC1111 launcher."""

from __future__ import annotations

import sys
from pathlib import Path


sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from containers.workloads import main_for_workload


if __name__ == "__main__":
    raise SystemExit(main_for_workload("stable-diffusion"))