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
    def test_mattpackages_decline_and_unsupported_hosts_do_not_configure(self):
        setup = load_setup_module()
        for platform, manager, problem in (("linux", setup.PackageManager.APT, None), ("linux", setup.PackageManager.APT, "arm64 unsupported"), ("linux", setup.PackageManager.DNF, None), ("windows", None, None)):
            with patch.object(setup, "eligibility_problem", return_value=problem), patch.object(
                setup, "prompt_yes_no", return_value=False
            ) as prompt, patch.object(setup, "configure_repository") as configure:
                setup.offer_mattpackages(platform, manager)
                configure.assert_not_called()
                if problem or platform != "linux" or manager is not setup.PackageManager.APT:
                    prompt.assert_not_called()

    def test_mattpackages_acceptance_configures(self):
        setup = load_setup_module()
        for platform in ("linux", "mattos"):
            with patch.object(setup, "eligibility_problem", return_value=None), patch.object(
                setup, "prompt_yes_no", return_value=True
            ), patch.object(setup, "configure_repository") as configure:
                setup.offer_mattpackages(platform, setup.PackageManager.APT)
                configure.assert_called_once_with()

    def test_repository_runs_between_preflight_and_packages(self):
        setup = load_setup_module()
        calls = []
        with patch.object(setup.sys, "argv", ["Setup.py"]), patch.object(setup, "print_system_summary", return_value=("linux", setup.PackageManager.APT)), patch.object(
            setup, "run_preflight", side_effect=lambda *_: calls.append("preflight")
        ), patch.object(setup, "offer_mattpackages", side_effect=lambda *_: calls.append("repository")), patch.object(
            setup, "run_package_flow", side_effect=lambda: calls.append("packages") or True
        ), patch.object(setup, "offer_storage_mount"), patch.object(setup, "offer_server_manager"):
            self.assertEqual(setup.main(), 0)
        self.assertEqual(calls, ["preflight", "repository", "packages"])

    def test_repository_failure_stops_before_packages(self):
        setup = load_setup_module()
        with patch.object(setup.sys, "argv", ["Setup.py"]), patch.object(setup, "print_system_summary", return_value=("linux", setup.PackageManager.APT)), patch.object(
            setup, "run_preflight"
        ), patch.object(setup, "offer_mattpackages", side_effect=RuntimeError("refresh failed")), patch.object(setup, "run_package_flow") as packages:
            self.assertEqual(setup.main(), 1)
            packages.assert_not_called()

    def test_cli_never_enrolls_repository(self):
        setup = load_setup_module()
        with patch.object(setup.sys, "argv", ["Setup.py", "plan", "desktop"]), patch.object(
            setup, "package_cli_main", return_value=0
        ) as cli, patch.object(setup, "offer_mattpackages") as offer:
            self.assertEqual(setup.main(), 0)
            cli.assert_called_once_with(["plan", "desktop"])
            offer.assert_not_called()

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
