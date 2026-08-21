"""Legacy-compatible local ZIP backup manager for Linux servers.

This preserves the legacy configuration layout, archive names, retention
algorithm, helper/unit names, menu commands, and systemd scheduling behavior.
"""

from __future__ import annotations

import argparse
import getpass
import hashlib
import os
import re
import shlex
import shutil
import subprocess
import sys
from dataclasses import dataclass
from datetime import datetime
from pathlib import Path


DEFAULT_DESTINATION = Path("/srv/storage/OneDrive/Apps/Games/Storage/MattMC/AutoZipArchives")
DEFAULT_SOURCE = Path("/srv/storage/Storage/Sync/MattMC")
DEFAULT_PREFIX = "mattmc"
DEFAULT_NAME = "MattMC"
KEEP_DAILY, KEEP_WEEKLY, KEEP_MONTHLY, KEEP_YEARLY = 3, 3, 3, 2
ARCHIVE_TIME_FORMAT = "%Y-%m-%d_%H-%M-%S"


def log(message: str) -> None:
    """Print an operational timestamp matching the deprecated manager."""

    print(f"[{datetime.now():%Y-%m-%d %H:%M:%S}] {message}")


def slugify(value: str) -> str:
    """Use the legacy config-name normalization rules."""

    return re.sub(r"(^-+|-+$)", "", re.sub(r"[^a-z0-9]+", "-", value.lower()))


def archive_prefix(value: str) -> str:
    """Use the legacy archive-prefix normalization rules."""

    return re.sub(r"(^-+|-+$)", "", re.sub(r"[^a-z0-9_-]+", "-", value.lower()))


@dataclass(frozen=True)
class ZipBackupConfig:
    """One persisted ZIP job using the legacy shell environment format."""

    name: str
    slug: str
    destination: Path
    source: Path
    prefix: str
    keep_daily: int = KEEP_DAILY
    keep_weekly: int = KEEP_WEEKLY
    keep_monthly: int = KEEP_MONTHLY
    keep_yearly: int = KEEP_YEARLY


class ZipBackupManager:
    """Manage verified ZIP backup archives and daily systemd timers."""

    def __init__(self, home: Path | None = None, manager_path: Path | None = None) -> None:
        self.home = home or Path.home()
        self.root = self.home / ".config" / "zip-backup-manager"
        self.configs = self.root / "configs"
        self.helpers = self.root / "helpers"
        self.current_file = self.root / "current_config"
        self.manager_path = manager_path or Path(__file__).resolve()

    def run(self, command: tuple[str, ...], *, check: bool = True, capture: bool = False, cwd: Path | None = None) -> subprocess.CompletedProcess[str]:
        """Run an external command without shell interpolation."""

        return subprocess.run(command, check=check, text=True, capture_output=capture, cwd=cwd)

    def sudo(self, command: tuple[str, ...], *, check: bool = True) -> subprocess.CompletedProcess[str]:
        """Perform privileged setup only when access requires it."""

        return self.run(("sudo", *command), check=check)

    def ensure_config_directories(self) -> None:
        for directory in (self.root, self.configs, self.helpers):
            directory.mkdir(parents=True, exist_ok=True)
            directory.chmod(0o700)

    def config_path(self, slug: str) -> Path:
        return self.configs / f"{slug}.env"

    def helper_path(self, slug: str) -> Path:
        return self.helpers / f"zip-backup-{slug}.sh"

    @staticmethod
    def service_name(slug: str) -> str:
        return f"zip-{slug}-backup.service"

    @staticmethod
    def timer_name(slug: str) -> str:
        return f"zip-{slug}-backup.timer"

    @staticmethod
    def service_path(slug: str) -> Path:
        return Path("/etc/systemd/system") / ZipBackupManager.service_name(slug)

    @staticmethod
    def timer_path(slug: str) -> Path:
        return Path("/etc/systemd/system") / ZipBackupManager.timer_name(slug)

    def require_dependencies(self) -> None:
        """Install the exact legacy zip/unzip dependencies on APT systems."""

        missing = [command for command in ("zip", "unzip") if shutil.which(command) is None]
        if not missing:
            return
        if shutil.which("apt-get") is None:
            raise RuntimeError("zip/unzip are missing and this distro is unsupported for automatic installation.")
        log(f"Missing dependencies: {' '.join(missing)}")
        self.sudo(("apt-get", "update"))
        self.sudo(("apt-get", "install", "-y", *missing))
        if any(shutil.which(command) is None for command in missing):
            raise RuntimeError("dependency installation failed.")

    def require_systemd(self) -> None:
        if shutil.which("systemctl") is None:
            raise RuntimeError("systemctl is not available on this system.")

    @staticmethod
    def read_environment(path: Path) -> dict[str, str]:
        """Safely parse legacy `KEY=shell-quoted value` configuration files."""

        values: dict[str, str] = {}
        for line in path.read_text(encoding="utf-8").splitlines():
            key, separator, raw = line.partition("=")
            if not separator or not key.replace("_", "").isalnum():
                continue
            parsed = shlex.split(raw, posix=True)
            values[key] = parsed[0] if len(parsed) == 1 else raw
        return values

    def config_from_file(self, path: Path) -> ZipBackupConfig:
        values = self.read_environment(path)
        required = ("CONFIG_NAME", "BACKUP_DEST_DIR", "BACKUP_SOURCE_DIR", "BACKUP_NAME")
        missing = [key for key in required if not values.get(key)]
        if missing:
            raise RuntimeError(f"configuration file is missing required fields: {', '.join(missing)}")
        return ZipBackupConfig(
            values["CONFIG_NAME"], values.get("CONFIG_SLUG", path.stem), Path(values["BACKUP_DEST_DIR"]), Path(values["BACKUP_SOURCE_DIR"]), values["BACKUP_NAME"],
            int(values.get("KEEP_DAILY", KEEP_DAILY)), int(values.get("KEEP_WEEKLY", KEEP_WEEKLY)), int(values.get("KEEP_MONTHLY", KEEP_MONTHLY)), int(values.get("KEEP_YEARLY", KEEP_YEARLY)),
        )

    def write_config(self, config: ZipBackupConfig) -> None:
        """Write a legacy-readable config with private permissions."""

        self.ensure_config_directories()
        values = {
            "CONFIG_NAME": config.name, "CONFIG_SLUG": config.slug, "BACKUP_DEST_DIR": str(config.destination),
            "BACKUP_SOURCE_DIR": str(config.source), "BACKUP_NAME": config.prefix, "KEEP_DAILY": str(config.keep_daily),
            "KEEP_WEEKLY": str(config.keep_weekly), "KEEP_MONTHLY": str(config.keep_monthly), "KEEP_YEARLY": str(config.keep_yearly),
        }
        path = self.config_path(config.slug)
        path.write_text("".join(f"{key}={shlex.quote(value)}\n" for key, value in values.items()), encoding="utf-8")
        path.chmod(0o600)

    def all_configs(self) -> list[ZipBackupConfig]:
        self.ensure_config_directories()
        configs: list[ZipBackupConfig] = []
        for path in sorted(self.configs.glob("*.env")):
            try:
                configs.append(self.config_from_file(path))
            except (OSError, RuntimeError, ValueError) as error:
                log(f"Skipping invalid configuration {path.name}: {error}")
        return configs

    def set_current(self, slug: str) -> None:
        self.ensure_config_directories()
        self.current_file.write_text(f"{slug}\n", encoding="utf-8")

    def current_slug(self) -> str:
        return self.current_file.read_text(encoding="utf-8").strip() if self.current_file.is_file() else ""

    def select_config(self, configs: list[ZipBackupConfig]) -> ZipBackupConfig:
        current = self.current_slug()
        default = next((index for index, config in enumerate(configs, 1) if config.slug == current), 1)
        for index, config in enumerate(configs, 1):
            print(f"{index}) {config.name} [{config.slug}]")
        while True:
            entered = self.prompt(f"Select config [{default}]: ") or str(default)
            if entered.isdigit() and 1 <= int(entered) <= len(configs):
                selected = configs[int(entered) - 1]
                self.set_current(selected.slug)
                return selected
            print(f"Please enter a valid number between 1 and {len(configs)}.")

    def resolve_config(self, selector: str | None = None) -> ZipBackupConfig:
        configs = self.all_configs()
        if not configs:
            raise RuntimeError("No backup configs exist yet. Run setup first.")
        if selector:
            normalized = slugify(selector)
            for config in configs:
                if config.slug == normalized or config.name.lower() == selector.lower():
                    self.set_current(config.slug)
                    return config
            raise RuntimeError(f"no config found for selector '{selector}'.")
        for config in configs:
            if config.slug == self.current_slug():
                return config
        return self.select_config(configs)

    def config_by_index(self, index: str) -> ZipBackupConfig:
        configs = self.all_configs()
        if not index.isdigit() or not 1 <= int(index) <= len(configs):
            raise RuntimeError(f"Invalid config number '{index}'. Use the list above for valid numbers.")
        config = configs[int(index) - 1]
        self.set_current(config.slug)
        return config

    @staticmethod
    def validate_source(source: Path) -> None:
        if not source.is_dir():
            raise RuntimeError(f"source path does not exist: {source}")
        if not os.access(source, os.R_OK):
            raise RuntimeError(f"source path is not readable: {source}")

    def ensure_destination_writable(self, destination: Path) -> None:
        """Retain direct-then-sudo destination creation and writable probe behavior."""

        try:
            destination.mkdir(parents=True, exist_ok=True)
        except PermissionError:
            self.sudo(("mkdir", "-p", str(destination)))
            self.sudo(("chown", f"{getpass.getuser()}:{getpass.getuser()}", str(destination)))
        probe = destination / ".zip-backup-write-probe"
        try:
            probe.touch()
            probe.unlink()
        except PermissionError:
            self.sudo(("touch", str(probe)))
            self.sudo(("rm", "-f", str(probe)), check=False)

    @staticmethod
    def archive_pattern(config: ZipBackupConfig) -> re.Pattern[str]:
        return re.compile(rf"^{re.escape(config.prefix)}_(\d{{4}}-\d{{2}}-\d{{2}})_(\d{{2}}-\d{{2}}-\d{{2}})\.zip$")

    def managed_archives(self, config: ZipBackupConfig) -> list[tuple[datetime, Path]]:
        """Return only archives whose names exactly match the legacy managed pattern."""

        pattern = self.archive_pattern(config)
        archives: list[tuple[datetime, Path]] = []
        for archive in config.destination.glob(f"{config.prefix}_*.zip"):
            match = pattern.match(archive.name)
            if match is None:
                continue
            try:
                archives.append((datetime.strptime(f"{match.group(1)}_{match.group(2)}", ARCHIVE_TIME_FORMAT), archive))
            except ValueError:
                continue
        return archives

    def prune(self, config: ZipBackupConfig) -> None:
        """Keep the newest archive in the newest daily/weekly/monthly/yearly buckets."""

        archives = self.managed_archives(config)
        if not archives:
            log("No archives found for pruning.")
            return
        buckets: dict[str, dict[str, tuple[datetime, Path]]] = {"daily": {}, "weekly": {}, "monthly": {}, "yearly": {}}
        for created, archive in archives:
            iso_year, iso_week, _ = created.isocalendar()
            keys = {"daily": created.strftime("%F"), "weekly": f"{iso_year}-{iso_week:02d}", "monthly": created.strftime("%Y-%m"), "yearly": created.strftime("%Y")}
            for bucket, key in keys.items():
                previous = buckets[bucket].get(key)
                if previous is None or created > previous[0]:
                    buckets[bucket][key] = (created, archive)
        keep: set[Path] = set()
        for bucket, count in (("daily", config.keep_daily), ("weekly", config.keep_weekly), ("monthly", config.keep_monthly), ("yearly", config.keep_yearly)):
            keep.update(archive for _, archive in sorted(buckets[bucket].values(), reverse=True)[:count])
        for _, archive in archives:
            if archive not in keep:
                archive.unlink(missing_ok=True)
                archive.with_name(f"{archive.name}.sha256").unlink(missing_ok=True)
                log(f"Pruned: {archive}")

    def write_checksum(self, archive: Path) -> None:
        """Write the optional legacy sha256sum-compatible sidecar file."""

        if shutil.which("sha256sum") is None:
            return
        digest = hashlib.sha256(archive.read_bytes()).hexdigest()
        archive.with_name(f"{archive.name}.sha256").write_text(f"{digest}  {archive}\n", encoding="utf-8")

    def create_archive(self, config: ZipBackupConfig, now: datetime | None = None) -> Path:
        """Create, validate, and checksum a timestamped archive using zip/unzip."""

        self.validate_source(config.source)
        self.ensure_destination_writable(config.destination)
        timestamp = (now or datetime.now()).strftime(ARCHIVE_TIME_FORMAT)
        archive = config.destination / f"{config.prefix}_{timestamp}.zip"
        temporary = archive.with_name(f"{archive.name}.tmp")
        temporary.unlink(missing_ok=True)
        log(f"Creating archive: {archive}")
        self.run(("zip", "-r", "-q", str(temporary), config.source.name), cwd=config.source.parent)
        temporary.replace(archive)
        if self.run(("unzip", "-tqq", str(archive)), check=False).returncode != 0:
            archive.unlink(missing_ok=True)
            raise RuntimeError("archive integrity test failed.")
        self.write_checksum(archive)
        return archive

    def backup_now(self, selector: str | None = None) -> None:
        self.require_dependencies()
        config = self.resolve_config(selector)
        self.create_archive(config)
        self.prune(config)
        log("Backup + prune run complete.")

    def create_helper(self, config: ZipBackupConfig) -> Path:
        """Create the legacy-named helper which re-execs this Python manager."""

        self.ensure_config_directories()
        helper = self.helper_path(config.slug)
        helper.write_text(
            "#!/usr/bin/env python3\nimport os\nimport sys\n"
            f"os.execv(sys.executable, (sys.executable, {str(self.manager_path)!r}, '--run-backup', '--config-name', {config.slug!r}, *sys.argv[1:]))\n",
            encoding="utf-8",
        )
        helper.chmod(0o700)
        return helper

    def setup_timer(self, config: ZipBackupConfig) -> None:
        """Write the legacy daily, persistent, randomized systemd timer."""

        self.require_systemd()
        helper = self.create_helper(config)
        user = getpass.getuser()
        service = "\n".join(("[Unit]", f"Description=Zip backup for {config.name}", "After=network-online.target", "Wants=network-online.target", "", "[Service]", "Type=oneshot", f"User={user}", f"Group={user}", f"ExecStart={helper}", ""))
        timer = "\n".join(("[Unit]", f"Description=Daily zip backup timer for {config.name}", "", "[Timer]", "OnCalendar=daily", "Persistent=true", "RandomizedDelaySec=30m", "", "[Install]", "WantedBy=timers.target", ""))
        subprocess.run(("sudo", "tee", str(self.service_path(config.slug))), input=service, text=True, check=True, stdout=subprocess.DEVNULL)
        subprocess.run(("sudo", "tee", str(self.timer_path(config.slug))), input=timer, text=True, check=True, stdout=subprocess.DEVNULL)
        self.sudo(("systemctl", "daemon-reload"))
        self.sudo(("systemctl", "enable", "--now", self.timer_name(config.slug)))
        log(f"Automatic backups enabled via {self.timer_name(config.slug)}.")

    def prompt(self, question: str) -> str:
        try:
            return input(question).strip()
        except EOFError:
            return ""

    def prompt_path(self, label: str, default: Path) -> Path:
        while True:
            value = self.prompt(f"{label} [{default}]: ") or str(default)
            path = Path(value).expanduser()
            if str(path):
                return Path(str(path).rstrip("/")) if str(path) != "/" else path
            print("Path cannot be empty.")

    def setup(self) -> None:
        """Run the legacy setup/rerun flow with its overwrite defaults."""

        self.require_dependencies()
        self.require_systemd()
        self.ensure_config_directories()
        destination = self.prompt_path("Enter backup destination directory", DEFAULT_DESTINATION)
        source = self.prompt_path("Enter source path to zip", DEFAULT_SOURCE)
        while True:
            prefix = archive_prefix(self.prompt(f"Enter backup archive prefix [{DEFAULT_PREFIX}]: ") or DEFAULT_PREFIX)
            if prefix:
                break
            print("Backup archive prefix must include letters or numbers.")
        while True:
            name = self.prompt(f"Enter backup config name [{DEFAULT_NAME}]: ") or DEFAULT_NAME
            slug = slugify(name)
            if slug:
                break
            print("Config name must include at least one letter or number.")
        self.validate_source(source)
        self.ensure_destination_writable(destination)
        existing = self.config_path(slug)
        if existing.is_file():
            overwrite = self.prompt(f"Config '{name}' already exists. Overwrite settings? [Y/n]: ") or "Y"
            if overwrite.lower() not in {"y", "yes"}:
                log("Setup cancelled.")
                return
        config = ZipBackupConfig(name, slug, destination, source, prefix)
        self.write_config(config)
        self.set_current(config.slug)
        self.setup_timer(config)
        log(f"Setup complete for '{config.name}'.")
        self.show(config)

    def show(self, config: ZipBackupConfig) -> None:
        print("=== Zip Backup Config ===")
        print(f"Name:       {config.name}\nSlug:       {config.slug}\nSource:     {config.source}\nDestination:{config.destination}\nPrefix:     {config.prefix}")
        print(f"Retention:  daily={config.keep_daily} weekly={config.keep_weekly} monthly={config.keep_monthly} yearly={config.keep_yearly}")
        print(f"Service:    {self.service_name(config.slug)}\nTimer:      {self.timer_name(config.slug)}")

    def list_configs(self) -> list[ZipBackupConfig]:
        configs = self.all_configs()
        if not configs:
            print("No backup configs found.")
            return []
        print("=== All Zip Backup Configs ===")
        for index, config in enumerate(configs, 1):
            enabled = self.run(("systemctl", "is-enabled", self.timer_name(config.slug)), check=False, capture=True).returncode == 0 if shutil.which("systemctl") else False
            print(f"{index}) {config.name} [{config.slug}]\n    Source: {config.source}\n    Dest:   {config.destination}\n    Prefix: {config.prefix}\n    Timer:  {self.timer_name(config.slug)} ({'enabled' if enabled else 'not-enabled'})")
        return configs

    def list_archives(self, config: ZipBackupConfig) -> None:
        archives = sorted(self.managed_archives(config), reverse=True)
        if not archives:
            print(f"No archives found for config '{config.name}'.")
            return
        print(f"{'No.':<4} {'Created':<20} {'Size':<10} File")
        for index, (created, archive) in enumerate(archives, 1):
            size = archive.stat().st_size
            print(f"{index})  {created:%Y-%m-%d %H:%M:%S} {self.human_size(size):<10} {archive.name}")

    @staticmethod
    def human_size(size: int) -> str:
        for unit in ("B", "KiB", "MiB", "GiB", "TiB"):
            if size < 1024 or unit == "TiB":
                return f"{size:.0f}{unit}" if unit == "B" else f"{size / 1:.1f}{unit}"
            size /= 1024
        return f"{size:.1f}TiB"

    def delete(self, config: ZipBackupConfig) -> None:
        confirm = self.prompt(f"Delete config '{config.name}' [{config.slug}] and associated service/timer/helper? [y/N]: ") or "N"
        if confirm.lower() not in {"y", "yes"}:
            log("Delete cancelled.")
            return
        for unit in (self.timer_name(config.slug), self.service_name(config.slug)):
            self.sudo(("systemctl", "disable", "--now", unit), check=False)
            self.sudo(("systemctl", "stop", unit), check=False)
        self.sudo(("rm", "-f", str(self.service_path(config.slug)), str(self.timer_path(config.slug))), check=False)
        self.helper_path(config.slug).unlink(missing_ok=True)
        self.config_path(config.slug).unlink(missing_ok=True)
        self.sudo(("systemctl", "daemon-reload"), check=False)
        self.sudo(("systemctl", "reset-failed", self.service_name(config.slug), self.timer_name(config.slug)), check=False)
        if any(other.destination == config.destination for other in self.all_configs()):
            log(f"Destination is used by another config; preserving {config.destination}.")
        else:
            remove = self.prompt(f"Delete destination directory '{config.destination}' and all zip backups too? [y/N]: ") or "N"
            if remove.lower() in {"y", "yes"}:
                try:
                    shutil.rmtree(config.destination)
                except PermissionError:
                    self.sudo(("rm", "-rf", str(config.destination)))
                log(f"Deleted destination directory: {config.destination}")
            else:
                log(f"Destination preserved: {config.destination}")
        if self.current_slug() == config.slug:
            remaining = self.all_configs()
            if remaining:
                self.set_current(remaining[0].slug)
            else:
                self.current_file.unlink(missing_ok=True)
        log(f"Deleted config '{config.name}' [{config.slug}].")

    def menu(self) -> int:
        """Run the legacy interactive menu and all its command aliases."""

        self.ensure_config_directories()
        while True:
            print()
            self.list_configs()
            print("\n=== Zip Backup Manager ===\n1) Run / rerun setup\n2) Exit\n\nSpecial commands:\n  delete <config-number>   Delete config + service/timer/helper\n  3 <config-number>        Take immediate zip backup now\n  4 <config-number>        List zip archives\n  5 <config-number>        Run prune now\n  6 <config-number>        Show config details\n  7 <config-number>        Trigger systemd service now\n  backup|list|prune|show|service <config-number>\n")
            entered = self.prompt("Choose an option [1-2 or command]: ")
            command, _, index = entered.partition(" ")
            try:
                if command.lower() == "delete" and index:
                    self.delete(self.config_by_index(index.strip()))
                elif command in {"3", "backup"} and index:
                    self.backup_now(self.config_by_index(index.strip()).slug)
                elif command in {"4", "list"} and index:
                    self.list_archives(self.config_by_index(index.strip()))
                elif command in {"5", "prune"} and index:
                    self.require_dependencies(); self.prune(self.config_by_index(index.strip()))
                elif command in {"6", "show"} and index:
                    self.show(self.config_by_index(index.strip()))
                elif command in {"7", "service"} and index:
                    config = self.config_by_index(index.strip()); self.sudo(("systemctl", "start", self.service_name(config.slug))); log(f"Triggered {self.service_name(config.slug)}.")
                elif entered == "1":
                    self.setup()
                elif entered == "2" or not entered:
                    print("Goodbye.")
                    return 0
                else:
                    print("Invalid selection.")
            except (OSError, RuntimeError, subprocess.CalledProcessError, ValueError) as error:
                print(f"Error: {error}", file=sys.stderr)


def main(argv: list[str] | None = None) -> int:
    """Run interactively or execute a configured backup helper invocation."""

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--run-backup", action="store_true", help="Run a configured backup without the menu.")
    parser.add_argument("--config-name", help="Select a backup by name or slug with --run-backup.")
    parser.add_argument("--prune-only", action="store_true", help="Apply retention without creating an archive.")
    args = parser.parse_args(argv)
    manager = ZipBackupManager()
    manager.ensure_config_directories()
    if args.run_backup:
        manager.require_dependencies()
        config = manager.resolve_config(args.config_name)
        if args.prune_only:
            manager.prune(config)
            log("Prune-only run complete.")
        else:
            manager.create_archive(config)
            manager.prune(config)
            log("Backup + prune run complete.")
        return 0
    return manager.menu()


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, subprocess.CalledProcessError, ValueError) as error:
        print(f"Error: {error}", file=sys.stderr)
        raise SystemExit(1) from error