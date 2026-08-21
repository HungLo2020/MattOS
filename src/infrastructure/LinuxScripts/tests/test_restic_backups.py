"""Regression coverage for the legacy-compatible Restic manager."""

from __future__ import annotations

import os
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch


ROOT = Path(__file__).resolve().parents[1]
import sys

sys.path.insert(0, str(ROOT / "src"))

from server.restic_backups import BackupConfig, ResticBackupManager, slugify


class ResticBackupManagerTests(unittest.TestCase):
    def manager(self, directory: str) -> ResticBackupManager:
        return ResticBackupManager(Path(directory), ROOT / "src" / "server" / "restic_backups.py")

    def test_legacy_slug_rules_are_preserved(self):
        self.assertEqual(slugify("MattMC Backup!"), "mattmc-backup")
        self.assertEqual(slugify("---"), "")

    def test_config_round_trip_uses_legacy_env_format(self):
        with tempfile.TemporaryDirectory() as directory:
            manager = self.manager(directory)
            config = BackupConfig("Server Backup", "server-backup", Path("/srv/repo with spaces"), Path("/srv/source"), Path(directory) / "password.txt")
            manager.write_config(config)
            stored = manager.config_path(config.slug).read_text(encoding="utf-8")
            self.assertIn("CONFIG_NAME='Server Backup'", stored)
            self.assertEqual(manager.config_from_file(manager.config_path(config.slug)), config)
            self.assertEqual(manager.config_path(config.slug).stat().st_mode & 0o777, 0o600)

    def test_legacy_single_config_is_imported(self):
        with tempfile.TemporaryDirectory() as directory:
            manager = self.manager(directory)
            manager.ensure_config_directories()
            password = manager.legacy_password
            password.write_text("secret", encoding="utf-8")
            password.chmod(0o600)
            manager.legacy_config.write_text(
                f"RESTIC_REPOSITORY=/srv/legacy-repo\nRESTIC_SOURCE=/srv/legacy-source\nRESTIC_PASSWORD_FILE={password}\n",
                encoding="utf-8",
            )
            manager.migrate_legacy_config()
            config = manager.config_from_file(manager.config_path("mattmc"))
            self.assertEqual(config.repository, Path("/srv/legacy-repo"))
            self.assertEqual(config.source, Path("/srv/legacy-source"))
            self.assertEqual(manager.current_slug(), "mattmc")

    def test_password_files_are_created_with_private_permissions(self):
        with tempfile.TemporaryDirectory() as directory:
            manager = self.manager(directory)
            password = manager.password_path("mattmc")
            with patch("server.restic_backups.os.open", wraps=os.open) as private_open:
                manager.ensure_password(password)
            self.assertEqual(password.stat().st_mode & 0o777, 0o600)
            self.assertTrue(private_open.call_args.args[1] & os.O_EXCL)
            self.assertEqual(private_open.call_args.args[2], 0o600)

            manager.write_password(password, "replacement-secret")
            self.assertEqual(password.read_text(encoding="utf-8"), "replacement-secret")
            self.assertEqual(password.stat().st_mode & 0o777, 0o600)

    def test_generated_helper_reuses_the_legacy_name_and_noninteractive_cli(self):
        with tempfile.TemporaryDirectory() as directory:
            manager = self.manager(directory)
            manager.ensure_config_directories()
            config = BackupConfig("MattMC", "mattmc", Path("/srv/repo"), Path("/srv/source"), Path(directory) / "password")
            helper = manager.create_helper(config)
            contents = helper.read_text(encoding="utf-8")
            self.assertEqual(helper.name, "restic-backup-mattmc.sh")
            self.assertIn("--run-backup", contents)
            self.assertIn("--config-name", contents)
            self.assertEqual(helper.stat().st_mode & 0o777, 0o700)

    def test_timer_setup_writes_legacy_unit_semantics(self):
        with tempfile.TemporaryDirectory() as directory:
            manager = self.manager(directory)
            manager.ensure_config_directories()
            config = BackupConfig("MattMC", "mattmc", Path("/srv/repo"), Path("/srv/source"), Path(directory) / "password")
            with patch.object(manager, "require_systemd"), patch("server.restic_backups.getpass.getuser", return_value="matt"), patch("server.restic_backups.subprocess.run") as run_command, patch.object(manager, "sudo") as sudo:
                manager.setup_timer(config)
            service_write = run_command.call_args_list[0].kwargs["input"]
            timer_write = run_command.call_args_list[1].kwargs["input"]
            self.assertIn("User=matt", service_write)
            self.assertIn("restic-backup-mattmc.sh", service_write)
            self.assertIn("OnCalendar=daily", timer_write)
            self.assertIn("Persistent=true", timer_write)
            self.assertIn("RandomizedDelaySec=30m", timer_write)
            self.assertIn(("systemctl", "enable", "--now", "restic-mattmc-backup.timer"), [call.args[0] for call in sudo.call_args_list])


if __name__ == "__main__":
    unittest.main()