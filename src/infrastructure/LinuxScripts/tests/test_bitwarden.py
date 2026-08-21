"""Regression coverage for shared Bitwarden authentication behavior."""

from __future__ import annotations

import json
import subprocess
import sys
import unittest
from pathlib import Path
from unittest.mock import patch


ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))

from bitwarden import BitwardenClient, BitwardenError


class BitwardenClientTests(unittest.TestCase):
    def test_locked_vault_uses_visible_getpass_then_noninteractive_unlock(self):
        responses = iter(
            (
                subprocess.CompletedProcess(("bw", "status", "--raw"), 0, json.dumps({"status": "locked"}), ""),
                subprocess.CompletedProcess(("bw", "unlock"), 0, "session-token\n", ""),
                subprocess.CompletedProcess(("bw", "get", "password", "PCPassword"), 0, "secret\n", ""),
            )
        )
        with patch("bitwarden.shutil.which", return_value="/usr/bin/bw"), patch("bitwarden.subprocess.run", side_effect=lambda *args, **kwargs: next(responses)) as run_command, patch(
            "bitwarden.getpass.getpass", return_value="master-password"
        ) as prompt:
            self.assertEqual(BitwardenClient().password("PCPassword"), "secret")
        prompt.assert_called_once_with("Bitwarden master password: ")
        commands = [call.args[0] for call in run_command.call_args_list]
        self.assertIn(("bw", "unlock", "--passwordenv", "BW_MASTER_PASSWORD", "--nointeraction", "--raw"), commands)
        self.assertNotIn(("bw", "unlock", "--raw"), commands)

    def test_noninteractive_locked_vault_fails_without_prompting(self):
        status = subprocess.CompletedProcess(("bw", "status", "--raw"), 0, json.dumps({"status": "locked"}), "")
        with patch("bitwarden.shutil.which", return_value="/usr/bin/bw"), patch("bitwarden.subprocess.run", return_value=status), patch(
            "bitwarden.getpass.getpass"
        ) as prompt:
            with self.assertRaisesRegex(BitwardenError, "interactive authentication is disabled"):
                BitwardenClient(non_interactive=True).ensure_session()
        prompt.assert_not_called()


if __name__ == "__main__":
    unittest.main()