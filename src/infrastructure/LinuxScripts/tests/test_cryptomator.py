"""Tests for deterministic Cryptomator service and Jellyfin integration setup."""

from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import sys


ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))

from server.cryptomator import (
    APPARMOR_MARKER,
    DEFAULT_MOUNT_PATH,
    INTEGRATION_SERVICE_NAME,
    SERVICE_NAME,
    VaultConfiguration,
    attach_jellyfin,
    apparmor_block,
    detach_jellyfin,
    enable_jellyfin,
    fuse_config_contents,
    integration_service,
    load_config,
    override_contents,
    reconcile,
    replace_marked_block,
    validate_configuration,
    vault_service,
    unlock_command,
    write_override,
)


class CryptomatorTests(unittest.TestCase):
    def config(self, root: Path) -> VaultConfiguration:
        vault = root / "encrypted"
        vault.mkdir()
        (vault / "masterkey.cryptomator").write_text("key", encoding="utf-8")
        password = root / "credentials" / "vault.password"
        password.parent.mkdir()
        password.write_text("secret\n", encoding="utf-8")
        return VaultConfiguration(
            "MattsVault",
            str(vault),
            "/mnt/cryptomator/mattsvault",
            str(password),
            str(root / "jellyfin"),
            "/vault-media",
            "matt",
            1000,
            1000,
        )

    def test_default_decrypted_mount_is_outside_samba(self):
        self.assertEqual(DEFAULT_MOUNT_PATH, Path("/mnt/cryptomator/mattsvault"))
        self.assertNotEqual(DEFAULT_MOUNT_PATH.parts[:3], ("/", "srv", "storage"))

    def test_configuration_round_trip(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "config.json"
            config = self.config(Path(directory))
            path.write_text(json.dumps(config.__dict__), encoding="utf-8")
            self.assertEqual(load_config(path), config)

    def test_configuration_rejects_mount_below_samba(self):
        with tempfile.TemporaryDirectory() as directory:
            config = self.config(Path(directory))
            invalid = VaultConfiguration(**{**config.__dict__, "mount_path": "/srv/storage/MattsVault"})
            with patch("server.cryptomator.pwd.getpwnam") as account:
                account.return_value.pw_uid = 1000
                account.return_value.pw_gid = 1000
                with self.assertRaisesRegex(RuntimeError, "outside the /srv/storage Samba tree"):
                    validate_configuration(invalid)

    def test_vault_service_is_enabled_separately_and_restarts(self):
        with tempfile.TemporaryDirectory() as directory:
            service = vault_service(self.config(Path(directory)))
        self.assertIn("WantedBy=multi-user.target", service)
        self.assertIn("Restart=on-failure", service)
        self.assertIn("RestartSec=15", service)
        self.assertIn("KillSignal=SIGINT", service)
        self.assertIn("SuccessExitStatus=130 SIGINT", service)
        self.assertIn("ExecStop=", service)
        self.assertIn("--detach-jellyfin", service)
        self.assertNotIn(INTEGRATION_SERVICE_NAME, service)

    def test_jellyfin_integration_cannot_block_base_container(self):
        with tempfile.TemporaryDirectory() as directory:
            config = self.config(Path(directory))
            unit = integration_service(config)
            override = override_contents(config)
        self.assertIn(f"Wants=docker.service {SERVICE_NAME}", unit)
        self.assertNotIn("Requires=", unit)
        self.assertIn("read_only: true", override)
        self.assertIn("create_host_path: false", override)
        self.assertIn('target: "/vault-media"', override)

    def test_fuse_allow_other_configuration_is_idempotent(self):
        initial = "#user_allow_other\n#mount_max = 1000\n"
        configured = fuse_config_contents(initial)
        self.assertIn("\nuser_allow_other\n", configured)
        self.assertEqual(fuse_config_contents(configured), configured)

    def test_unlock_command_uses_cryptomator_supported_allow_other_syntax(self):
        with tempfile.TemporaryDirectory() as directory:
            config = self.config(Path(directory))
        command = unlock_command(config, Path("/opt/cryptomator-cli"))
        self.assertIn("--mountOption=-oallow_other", command)
        self.assertNotIn("--mountOption=allow_other", command)

    def test_override_is_repaired_without_changing_its_target(self):
        with tempfile.TemporaryDirectory() as directory:
            config = self.config(Path(directory))
            config.stack.mkdir()
            path = config.stack / "cryptomator.compose.yml"
            path.write_text("stale\n", encoding="utf-8")
            self.assertEqual(write_override(config), path)
            self.assertEqual(path.read_text(encoding="utf-8"), override_contents(config))
            self.assertEqual(path.stat().st_mode & 0o777, 0o600)

    def test_enable_refuses_to_recreate_jellyfin_without_allow_other(self):
        with tempfile.TemporaryDirectory() as directory:
            config = self.config(Path(directory))
            with (
                patch("server.cryptomator.validate_configuration"),
                patch("server.cryptomator.is_mounted", return_value=True),
                patch("server.cryptomator.mounted_entry_count", return_value=2),
                patch("server.cryptomator.fuse_allows_other", return_value=True),
                patch("server.cryptomator.mount_allows_other", return_value=False),
                patch("server.cryptomator.sudo") as sudo,
            ):
                with self.assertRaisesRegex(RuntimeError, "choose reconcile first"):
                    enable_jellyfin(config)
        sudo.assert_not_called()

    def test_enable_waits_until_jellyfin_can_read_nonempty_mount(self):
        with tempfile.TemporaryDirectory() as directory:
            config = self.config(Path(directory))
            config.stack.mkdir()
            (config.stack / "docker-compose.yml").touch()
            (config.stack / ".env").touch()
            with (
                patch("server.cryptomator.validate_configuration"),
                patch("server.cryptomator.is_mounted", return_value=True),
                patch("server.cryptomator.mounted_entry_count", return_value=2),
                patch("server.cryptomator.fuse_allows_other", return_value=True),
                patch("server.cryptomator.mount_allows_other", return_value=True),
                patch("server.cryptomator.docker_available", return_value=True),
                patch("server.cryptomator.jellyfin_running", return_value=True),
                patch("server.cryptomator.prompt_yes_no", return_value=True),
                patch("server.cryptomator.wait_for_jellyfin_mount", return_value=2),
                patch("server.cryptomator.sudo") as sudo,
            ):
                enable_jellyfin(config)
        sudo.assert_called_once_with(("systemctl", "enable", "--now", INTEGRATION_SERVICE_NAME))

    def test_attach_recreates_only_jellyfin_with_override(self):
        with tempfile.TemporaryDirectory() as directory:
            config = self.config(Path(directory))
            with (
                patch("server.cryptomator.is_mounted", return_value=True),
                patch("server.cryptomator.mount_allows_other", return_value=True),
                patch("server.cryptomator.mounted_entry_count", return_value=2),
                patch("server.cryptomator.jellyfin_files_exist", return_value=True),
                patch("server.cryptomator.docker_available", return_value=True),
                patch("server.cryptomator.jellyfin_running", return_value=True),
                patch("server.cryptomator.jellyfin_has_mount", side_effect=[False, True]),
                patch("server.cryptomator.write_override"),
                patch("server.cryptomator.run") as run,
            ):
                self.assertTrue(attach_jellyfin(config))
        command = run.call_args.args[0]
        self.assertIn(str(config.stack / "cryptomator.compose.yml"), command)
        self.assertEqual(command[-5:], ("up", "-d", "--no-deps", "--force-recreate", "jellyfin"))

    def test_detach_recreates_only_jellyfin_from_base_compose(self):
        with tempfile.TemporaryDirectory() as directory:
            config = self.config(Path(directory))
            with (
                patch("server.cryptomator.docker_available", return_value=True),
                patch("server.cryptomator.jellyfin_files_exist", return_value=True),
                patch("server.cryptomator.jellyfin_running", return_value=True),
                patch("server.cryptomator.jellyfin_has_mount", return_value=True),
                patch("server.cryptomator.run") as run,
            ):
                self.assertTrue(detach_jellyfin(config))
        command = run.call_args.args[0]
        self.assertNotIn(str(config.stack / "cryptomator.compose.yml"), command)
        self.assertEqual(command[-5:], ("up", "-d", "--no-deps", "--force-recreate", "jellyfin"))

    def test_apparmor_rule_covers_mount_and_fuse_cleanup(self):
        with tempfile.TemporaryDirectory() as directory:
            block = apparmor_block(self.config(Path(directory)))
        self.assertIn("/mnt/cryptomator/mattsvault/", block)
        self.assertIn("mount options=(rw, rprivate) -> /", block)
        self.assertIn("mount options=(rw, rbind) /mnt/cryptomator/ -> /", block)
        self.assertIn("/mnt/cryptomator/mattsvault/ r,", block)

    def test_managed_apparmor_block_is_reproducibly_replaced(self):
        old = f"unmanaged\n# BEGIN {APPARMOR_MARKER}\nold\n# END {APPARMOR_MARKER}\ntail\n"
        updated = replace_marked_block(old, APPARMOR_MARKER, f"# BEGIN {APPARMOR_MARKER}\nnew\n# END {APPARMOR_MARKER}")
        self.assertEqual(updated.count(f"# BEGIN {APPARMOR_MARKER}"), 1)
        self.assertIn("unmanaged", updated)
        self.assertIn("tail", updated)
        self.assertIn("new", updated)
        self.assertNotIn("\nold\n", updated)

    def test_reconcile_preserves_an_active_mount_and_restarts_service(self):
        with tempfile.TemporaryDirectory() as directory:
            base = self.config(Path(directory))
            config = VaultConfiguration(**{**base.__dict__, "mount_path": str(Path(directory) / "mount")})
            with (
                patch("server.cryptomator.validate_configuration"),
                patch("server.cryptomator.install_prerequisites"),
                patch("server.cryptomator.install_cli"),
                patch("server.cryptomator.configure_apparmor"),
                patch("server.cryptomator.install_runtime"),
                patch("server.cryptomator.is_mounted", return_value=True),
                patch("server.cryptomator.cleanup_mount"),
                patch("server.cryptomator.wait_for_nonempty_mount", return_value=2),
                patch("server.cryptomator.sudo") as sudo,
            ):
                reconcile(config, ROOT / "src/server/cryptomator.py")
        commands = [call.args[0] for call in sudo.call_args_list]
        self.assertNotIn(("install", "-d", "-m", "0700", "-o", "matt", "-g", "matt", config.mount_path), commands)
        self.assertIn(("systemctl", "stop", SERVICE_NAME), commands)
        self.assertIn(("systemctl", "enable", SERVICE_NAME), commands)
        self.assertIn(("systemctl", "restart", SERVICE_NAME), commands)


if __name__ == "__main__":
    unittest.main()
