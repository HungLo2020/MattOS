"""Focused tests for the local MattOS repository backend and CLI contract."""

from __future__ import annotations

import sys
import tempfile
import unittest
from unittest.mock import patch
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))
sys.path.insert(0, str(ROOT / "GenericScripts"))

from server.mattos_repository import RepositoryManager, ServerConfig  # noqa: E402
import ManageMattOSRepository as client  # noqa: E402


class MattOSRepositoryTests(unittest.TestCase):
    def test_server_initializes_signed_atomic_repository(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / "repository"
            token = Path(temporary) / "token"
            config = ServerConfig(root=root, token_file=token, r2_enabled=False)
            manager = RepositoryManager(config)

            manager.init()

            status = manager.status()
            self.assertTrue(status["initialized"])
            self.assertTrue((root / "current").is_symlink())
            self.assertTrue((root / "current" / "dists" / "trixie" / "InRelease").is_file())
            self.assertIn("BEGIN PGP PUBLIC KEY BLOCK", manager.public_key())
            self.assertEqual(len(token.read_text(encoding="utf-8").strip()), 43)

    def test_client_config_retains_legacy_metadata_fields(self):
        config = client.Config("r2", "gpg", "bucket", "endpoint", "url", "trixie", "main", ("amd64",))
        self.assertEqual(config.r2_item, "r2")
        self.assertEqual(config.gpg_item, "gpg")
        self.assertEqual(config.architectures, ("amd64",))

    def test_cli_parser_retains_commands_and_alias(self):
        parser = client.parser()
        self.assertEqual(parser.parse_args(["add", "package.deb"]).command, "add")
        self.assertEqual(parser.parse_args(["upload", "package.deb"]).command, "upload")
        self.assertEqual(parser.parse_args(["export-key", "--output", "key.asc"]).command, "export-key")

    def test_default_client_uses_tailscale_server_without_token(self):
        with patch.dict("os.environ", {}, clear=True):
            config = client.Config.from_env()
        self.assertEqual(config.server_url, "http://hunglosvr:8790")


if __name__ == "__main__":
    unittest.main()
