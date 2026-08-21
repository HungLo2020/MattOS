"""Regression coverage for the legacy-compatible ZIP backup manager."""

from __future__ import annotations

import sys
import tempfile
import unittest
from datetime import datetime
from pathlib import Path
from unittest.mock import patch


ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))

from server.zip_backups import ZipBackupConfig, ZipBackupManager, archive_prefix, slugify


class ZipBackupManagerTests(unittest.TestCase):
    def manager(self, directory: str) -> ZipBackupManager:
        return ZipBackupManager(Path(directory), ROOT / "src" / "server" / "zip_backups.py")

    def test_legacy_name_and_prefix_rules_are_preserved(self):
        self.assertEqual(slugify("MattMC Backup!"), "mattmc-backup")
        self.assertEqual(archive_prefix("MattMC_Archives!"), "mattmc_archives")
        self.assertEqual(archive_prefix("---"), "")

    def test_config_round_trip_uses_legacy_environment_format(self):
        with tempfile.TemporaryDirectory() as directory:
            manager = self.manager(directory)
            config = ZipBackupConfig("Archive Job", "archive-job", Path("/srv/destination with spaces"), Path("/srv/source"), "archive_job")
            manager.write_config(config)
            stored = manager.config_path(config.slug).read_text(encoding="utf-8")
            self.assertIn("CONFIG_NAME='Archive Job'", stored)
            self.assertEqual(manager.config_from_file(manager.config_path(config.slug)), config)
            self.assertEqual(manager.config_path(config.slug).stat().st_mode & 0o777, 0o600)

    def test_prune_keeps_newest_distinct_retention_buckets_and_sidecars(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            manager = self.manager(directory)
            config = ZipBackupConfig("MattMC", "mattmc", root, root, "mattmc", keep_daily=1, keep_weekly=1, keep_monthly=1, keep_yearly=1)
            old = root / "mattmc_2024-01-01_12-00-00.zip"
            latest = root / "mattmc_2025-02-03_12-00-00.zip"
            for archive in (old, latest):
                archive.write_bytes(b"archive")
                archive.with_name(f"{archive.name}.sha256").write_text("checksum", encoding="utf-8")
            manager.prune(config)
            self.assertFalse(old.exists())
            self.assertFalse(old.with_name(f"{old.name}.sha256").exists())
            self.assertTrue(latest.exists())
            self.assertTrue(latest.with_name(f"{latest.name}.sha256").exists())

    def test_create_archive_uses_legacy_zip_cwd_and_validates_before_checksum(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "source"
            destination = root / "dest"
            source.mkdir()
            config = ZipBackupConfig("MattMC", "mattmc", destination, source, "mattmc")
            manager = self.manager(directory)
            with patch.object(manager, "ensure_destination_writable"), patch.object(manager, "run") as run_command, patch.object(manager, "write_checksum") as checksum:
                run_command.return_value.returncode = 0
                with patch.object(Path, "replace") as replace:
                    archive = manager.create_archive(config, datetime(2026, 8, 6, 12, 30, 15))
            self.assertEqual(archive.name, "mattmc_2026-08-06_12-30-15.zip")
            self.assertEqual(run_command.call_args_list[0].args[0][:4], ("zip", "-r", "-q", str(destination / "mattmc_2026-08-06_12-30-15.zip.tmp")))
            self.assertEqual(run_command.call_args_list[0].kwargs["cwd"], source.parent)
            self.assertEqual(run_command.call_args_list[1].args[0], ("unzip", "-tqq", str(archive)))
            replace.assert_called_once_with(archive)
            checksum.assert_called_once_with(archive)

    def test_helper_and_timer_keep_legacy_names_and_daily_semantics(self):
        with tempfile.TemporaryDirectory() as directory:
            manager = self.manager(directory)
            manager.ensure_config_directories()
            config = ZipBackupConfig("MattMC", "mattmc", Path("/srv/dest"), Path("/srv/source"), "mattmc")
            helper = manager.create_helper(config)
            self.assertEqual(helper.name, "zip-backup-mattmc.sh")
            self.assertIn("--run-backup", helper.read_text(encoding="utf-8"))
            with patch.object(manager, "require_systemd"), patch("server.zip_backups.getpass.getuser", return_value="matt"), patch("server.zip_backups.subprocess.run") as run_command, patch.object(manager, "sudo") as sudo:
                manager.setup_timer(config)
            service = run_command.call_args_list[0].kwargs["input"]
            timer = run_command.call_args_list[1].kwargs["input"]
            self.assertIn("User=matt", service)
            self.assertIn("zip-backup-mattmc.sh", service)
            self.assertIn("OnCalendar=daily", timer)
            self.assertIn("Persistent=true", timer)
            self.assertIn("RandomizedDelaySec=30m", timer)
            self.assertIn(("systemctl", "enable", "--now", "zip-mattmc-backup.timer"), [call.args[0] for call in sudo.call_args_list])


if __name__ == "__main__":
    unittest.main()