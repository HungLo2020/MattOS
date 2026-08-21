"""Focused tests for interactive Linux/MattOS setup preflight safeguards."""

from __future__ import annotations

import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch


ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))

from preflight import OperatorAccount, canonical_repository_path, offer_repository_relocation


class PreflightTests(unittest.TestCase):
    def test_canonical_repository_path_uses_operator_home_and_repository_name(self):
        account = OperatorAccount("matt", Path("/home/matt"), True, True)
        self.assertEqual(
            canonical_repository_path(account, Path("/work/LinuxScripts")),
            Path("/home/matt/Documents/Repos/LinuxScripts"),
        )

    def test_existing_canonical_repository_does_not_prompt_or_mutate(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "LinuxScripts"
            source.mkdir()
            account = OperatorAccount("matt", root, True, True)
            destination = canonical_repository_path(account, source)
            destination.parent.mkdir(parents=True)
            source.rename(destination)
            with patch("preflight.prompt_yes_no") as prompt, patch("preflight.run_privileged") as privileged:
                offer_repository_relocation(destination, account)
            prompt.assert_not_called()
            privileged.assert_not_called()

    def test_relocation_requires_confirmation_before_privileged_copy(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "LinuxScripts"
            source.mkdir()
            account = OperatorAccount("matt", root / "matt", True, True)
            account.home.mkdir()
            with patch("preflight.prompt_yes_no", return_value=False), patch("preflight.run_privileged") as privileged:
                offer_repository_relocation(source, account)
            privileged.assert_not_called()

    def test_confirmed_relocation_returns_the_new_operator_checkout(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "LinuxScripts"
            source.mkdir()
            account = OperatorAccount("matt", root / "matt", True, True)
            account.home.mkdir()
            with patch("preflight.prompt_yes_no", return_value=True), patch("preflight.run_privileged"):
                destination = offer_repository_relocation(source, account)
            self.assertEqual(destination, account.home / "Documents" / "Repos" / "LinuxScripts")


if __name__ == "__main__":
    unittest.main()