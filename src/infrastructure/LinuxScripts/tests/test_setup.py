"""Regression coverage for interactive Setup workflow boundaries."""

from __future__ import annotations

import importlib.util
import os
import sys
import unittest
from pathlib import Path
from unittest.mock import patch


ROOT = Path(__file__).resolve().parents[1]


def load_setup_module():
    """Load Setup.py without allowing its virtual-environment re-exec."""

    path = ROOT / "Tools" / "Setup.py"
    specification = importlib.util.spec_from_file_location("setup_tool", path)
    module = importlib.util.module_from_spec(specification)
    assert specification.loader is not None
    with patch.object(os, "execv"):
        specification.loader.exec_module(module)
    return module


class SetupWorkflowTests(unittest.TestCase):
    def test_failed_package_flow_skips_later_setup_actions(self):
        setup = load_setup_module()
        with patch.object(setup.sys, "argv", ["Setup.py"]), patch.object(setup, "print_system_summary", return_value=("linux", object())), patch.object(
            setup, "run_preflight"
        ), patch.object(setup, "run_package_flow", return_value=False), patch.object(setup, "offer_storage_mount") as storage, patch.object(
            setup, "offer_server_manager"
        ) as server:
            self.assertEqual(setup.main(), 1)
        storage.assert_not_called()
        server.assert_not_called()


if __name__ == "__main__":
    unittest.main()