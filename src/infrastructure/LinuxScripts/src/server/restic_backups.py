"""Legacy-compatible local Restic backup manager for Linux servers.

The manager intentionally retains the legacy config paths, config-file format,
helper names, systemd unit names, defaults, retention policy, menu commands,
and --run-backup/--config-name command-line interface.
"""

from __future__ import annotations

import argparse
import getpass
import os
import secrets
import shlex
import shutil
import subprocess
import sys
from dataclasses import dataclass
from datetime import datetime
from pathlib import Path


DEFAULT_REPOSITORY = Path("/srv/storage/OneDrive/Apps/Games/Storage/MattMC/Restic")
DEFAULT_SOURCE = Path("/srv/storage/Storage/Sync/MattMC")
DEFAULT_NAME = "MattMC"
KEEP_DAILY, KEEP_WEEKLY, KEEP_MONTHLY, KEEP_YEARLY = 7, 4, 12, 2


def log(message: str) -> None:
    """Print an operational timestamp matching the legacy manager."""

    print(f"[{datetime.now():%Y-%m-%d %H:%M:%S}] {message}")


def slugify(value: str) -> str:
    """Use the legacy config-name slug rules."""

    import re

    return re.sub(r"(^-+|-+$)", "", re.sub(r"[^a-z0-9]+", "-", value.lower()))


@dataclass(frozen=True)
class BackupConfig:
    """One persisted Restic backup job, stored in the legacy shell env format."""

    name: str
    slug: str
    repository: Path
    source: Path
    password_file: Path
    keep_daily: int = KEEP_DAILY
    keep_weekly: int = KEEP_WEEKLY
    keep_monthly: int = KEEP_MONTHLY
    keep_yearly: int = KEEP_YEARLY


class ResticBackupManager:
    """Manage local Restic backups and their daily systemd timers."""

    def __init__(self, home: Path | None = None, manager_path: Path | None = None) -> None:
        self.home = home or Path.home()
        self.root = self.home / ".config" / "restic-mattmc"
        self.configs = self.root / "configs"
        self.helpers = self.root / "helpers"
        self.current_file = self.root / "current_config"
        self.legacy_config = self.root / "backup.env"
        self.legacy_password = self.root / "password"
        self.manager_path = manager_path or Path(__file__).resolve()

    def ensure_config_directories(self) -> None:
        """Create the legacy private configuration locations."""

        for directory in (self.root, self.configs, self.helpers):
            directory.mkdir(parents=True, exist_ok=True)
            directory.chmod(0o700)

    def config_path(self, slug: str) -> Path:
        return self.configs / f"{slug}.env"

    def password_path(self, slug: str) -> Path:
        return self.configs / f"password-{slug}.txt"

    def helper_path(self, slug: str) -> Path:
        return self.helpers / f"restic-backup-{slug}.sh"

    @staticmethod
    def service_name(slug: str) -> str:
        return f"restic-{slug}-backup.service"

    @staticmethod
    def timer_name(slug: str) -> str:
        return f"restic-{slug}-backup.timer"

    @staticmethod
    def service_path(slug: str) -> Path:
        return Path("/etc/systemd/system") / ResticBackupManager.service_name(slug)

    @staticmethod
    def timer_path(slug: str) -> Path:
        return Path("/etc/systemd/system") / ResticBackupManager.timer_name(slug)

    def run(self, command: tuple[str, ...], *, check: bool = True, capture: bool = False, env: dict[str, str] | None = None) -> subprocess.CompletedProcess[str]:
        """Run a system command without shell interpolation."""

        return subprocess.run(command, check=check, text=True, capture_output=capture, env=env)

    def sudo(self, command: tuple[str, ...], *, check: bool = True, capture: bool = False) -> subprocess.CompletedProcess[str]:
        """Run one privileged operation while retaining ordinary user ownership."""

        return self.run(("sudo", *command), check=check, capture=capture)

    def require_restic(self) -> None:
        """Install Restic on APT systems exactly as the legacy workflow did."""

        if shutil.which("restic"):
            return
        if not shutil.which("apt-get"):
            raise RuntimeError("restic is missing and this distro is unsupported for automatic installation.")
        log("restic is not installed. Attempting install...")
        self.sudo(("apt-get", "update"))
        self.sudo(("apt-get", "install", "-y", "restic"))
        if not shutil.which("restic"):
            raise RuntimeError("restic installation failed.")

    def require_systemd(self) -> None:
        if not shutil.which("systemctl"):
            raise RuntimeError("systemctl is not available on this system.")

    @staticmethod
    def read_environment(path: Path) -> dict[str, str]:
        """Read legacy `KEY=shell-quoted value` config files without sourcing them."""

        values: dict[str, str] = {}
        for line in path.read_text(encoding="utf-8").splitlines():
            key, separator, raw_value = line.partition("=")
            if not separator or not key.replace("_", "").isalnum():
                continue
            parsed = shlex.split(raw_value, posix=True)
            values[key] = parsed[0] if len(parsed) == 1 else raw_value
        return values

    def config_from_file(self, path: Path) -> BackupConfig:
        values = self.read_environment(path)
        required = ("CONFIG_NAME", "RESTIC_REPOSITORY", "RESTIC_SOURCE", "RESTIC_PASSWORD_FILE")
        missing = [name for name in required if not values.get(name)]
        if missing:
            raise RuntimeError(f"configuration file is missing required fields: {', '.join(missing)}")
        slug = values.get("CONFIG_SLUG") or path.stem
        return BackupConfig(
            values["CONFIG_NAME"], slug, Path(values["RESTIC_REPOSITORY"]), Path(values["RESTIC_SOURCE"]), Path(values["RESTIC_PASSWORD_FILE"]),
            int(values.get("KEEP_DAILY", KEEP_DAILY)), int(values.get("KEEP_WEEKLY", KEEP_WEEKLY)),
            int(values.get("KEEP_MONTHLY", KEEP_MONTHLY)), int(values.get("KEEP_YEARLY", KEEP_YEARLY)),
        )

    def write_config(self, config: BackupConfig) -> None:
        """Write a shell-compatible config so existing legacy helpers still understand it."""

        self.ensure_config_directories()
        values = {
            "CONFIG_NAME": config.name,
            "CONFIG_SLUG": config.slug,
            "RESTIC_REPOSITORY": str(config.repository),
            "RESTIC_SOURCE": str(config.source),
            "RESTIC_PASSWORD_FILE": str(config.password_file),
            "KEEP_DAILY": str(config.keep_daily),
            "KEEP_WEEKLY": str(config.keep_weekly),
            "KEEP_MONTHLY": str(config.keep_monthly),
            "KEEP_YEARLY": str(config.keep_yearly),
        }
        content = "".join(f"{key}={shlex.quote(value)}\n" for key, value in values.items())
        path = self.config_path(config.slug)
        path.write_text(content, encoding="utf-8")
        path.chmod(0o600)

    def all_configs(self) -> list[BackupConfig]:
        """Return valid named configurations in the legacy alphabetical order."""

        self.ensure_config_directories()
        configs: list[BackupConfig] = []
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

    def migrate_legacy_config(self) -> None:
        """Import the single pre-manager legacy config if no MattMC job exists."""

        target = self.config_path("mattmc")
        if target.is_file() or not self.legacy_config.is_file():
            return
        values = self.read_environment(self.legacy_config)
        if not values.get("RESTIC_REPOSITORY") or not values.get("RESTIC_SOURCE"):
            return
        password = Path(values.get("RESTIC_PASSWORD_FILE", str(self.legacy_password)))
        if not password.is_file():
            password = self.password_path("mattmc")
            self.ensure_password(password)
        self.write_config(BackupConfig(DEFAULT_NAME, "mattmc", Path(values["RESTIC_REPOSITORY"]), Path(values["RESTIC_SOURCE"]), password))
        self.set_current("mattmc")
        log("Imported legacy backup config as 'MattMC'.")

    def resolve_config(self, selector: str | None = None) -> BackupConfig:
        """Select by legacy slug/name, current selection, or an interactive menu."""

        configs = self.all_configs()
        if not configs:
            raise RuntimeError("No backup configs exist yet. Run setup first.")
        if selector:
            selected_slug = slugify(selector)
            for config in configs:
                if config.slug == selected_slug or config.name.lower() == selector.lower():
                    self.set_current(config.slug)
                    return config
            raise RuntimeError(f"no config found for selector '{selector}'.")
        current = self.current_slug()
        for config in configs:
            if config.slug == current:
                return config
        return self.select_config(configs)

    def select_config(self, configs: list[BackupConfig]) -> BackupConfig:
        """Use the legacy current-config aware selection prompt."""

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

    def ensure_password(self, path: Path) -> None:
        path.parent.mkdir(parents=True, exist_ok=True)
        try:
            descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
        except FileExistsError:
            if not path.is_file():
                raise RuntimeError(f"password path is not a regular file: {path}")
            path.chmod(0o600)
            return
        with os.fdopen(descriptor, "w", encoding="utf-8") as password_file:
            password_file.write(secrets.token_hex(32))

    def write_password(self, path: Path, password: str) -> None:
        path.parent.mkdir(parents=True, exist_ok=True)
        temporary = path.with_name(f".{path.name}.tmp-{secrets.token_hex(8)}")
        try:
            descriptor = os.open(temporary, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
            with os.fdopen(descriptor, "w", encoding="utf-8") as password_file:
                password_file.write(password)
            temporary.replace(path)
        finally:
            temporary.unlink(missing_ok=True)

    def ensure_repository_writable(self, repository: Path) -> None:
        """Match the legacy direct-then-sudo repository ownership strategy."""

        try:
            repository.mkdir(parents=True, exist_ok=True)
        except PermissionError:
            self.sudo(("mkdir", "-p", str(repository)))
            self.sudo(("chown", f"{getpass.getuser()}:{getpass.getuser()}", str(repository)))
        probe = repository / ".restic-write-probe"
        try:
            probe.touch()
            probe.unlink()
        except PermissionError:
            self.sudo(("touch", str(probe)))
            self.sudo(("chown", f"{getpass.getuser()}:{getpass.getuser()}", str(probe)), check=False)
            self.sudo(("rm", "-f", str(probe)), check=False)

    @staticmethod
    def validate_source(source: Path) -> None:
        if not source.is_dir():
            raise RuntimeError(f"source path does not exist: {source}")
        if not os.access(source, os.R_OK):
            raise RuntimeError(f"source path is not readable: {source}")

    def restic(self, config: BackupConfig, *arguments: str, capture: bool = False, check: bool = True) -> subprocess.CompletedProcess[str]:
        """Run Restic with the config-specific password environment only."""

        environment = os.environ.copy()
        environment["RESTIC_PASSWORD_FILE"] = str(config.password_file)
        return self.run(("restic", "-r", str(config.repository), *arguments), capture=capture, check=check, env=environment)

    def ensure_repository_initialized(self, config: BackupConfig) -> None:
        if (config.repository / "config").is_file():
            if self.restic(config, "snapshots", check=False, capture=True).returncode != 0:
                raise RuntimeError("repository exists but could not be opened with current password.")
            return
        log(f"Initializing restic repository at: {config.repository}")
        self.restic(config, "init")

    def backup_now(self, selector: str | None = None) -> None:
        self.require_restic()
        config = self.resolve_config(selector)
        self.validate_source(config.source)
        self.ensure_repository_writable(config.repository)
        self.ensure_repository_initialized(config)
        log(f"Starting backup for config '{config.name}': {config.source}")
        self.restic(config, "backup", str(config.source))
        self.prune(config)
        log("Backup + prune completed.")

    def prune(self, config: BackupConfig) -> None:
        log(f"Applying retention policy (daily={config.keep_daily}, weekly={config.keep_weekly}, monthly={config.keep_monthly}, yearly={config.keep_yearly})")
        self.restic(config, "forget", "--prune", "--keep-daily", str(config.keep_daily), "--keep-weekly", str(config.keep_weekly), "--keep-monthly", str(config.keep_monthly), "--keep-yearly", str(config.keep_yearly))

    def create_helper(self, config: BackupConfig) -> Path:
        """Create the legacy-named private helper used by the systemd service."""

        helper = self.helper_path(config.slug)
        helper.write_text(
            "#!/usr/bin/env python3\n"
            "import os\nimport sys\n"
            f"os.execv(sys.executable, (sys.executable, {str(self.manager_path)!r}, '--run-backup', '--config-name', {config.slug!r}))\n",
            encoding="utf-8",
        )
        helper.chmod(0o700)
        return helper

    def setup_timer(self, config: BackupConfig) -> None:
        """Create the same daily persistent timer and user-owned helper contract."""

        self.require_systemd()
        helper = self.create_helper(config)
        user = getpass.getuser()
        service = "\n".join(("[Unit]", f"Description=Restic backup for {config.name}", "After=network-online.target", "Wants=network-online.target", "", "[Service]", "Type=oneshot", f"User={user}", f"Group={user}", f"ExecStart={helper}", ""))
        timer = "\n".join(("[Unit]", f"Description=Daily Restic backup timer for {config.name}", "", "[Timer]", "OnCalendar=daily", "Persistent=true", "RandomizedDelaySec=30m", "", "[Install]", "WantedBy=timers.target", ""))
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
            entered = self.prompt(f"{label} [{default}]: ") or str(default)
            path = Path(entered).expanduser()
            if str(path):
                return Path(str(path).rstrip("/")) if str(path) != "/" else path
            print("Path cannot be empty.")

    def prompt_password(self, allow_existing: bool) -> str | None:
        while True:
            first = getpass.getpass("Enter restic password (hint: standard pc password): ")
            if not first and allow_existing:
                return None
            if not first:
                print("Password cannot be empty.")
                continue
            if first == getpass.getpass("Confirm restic password: "):
                return first
            print("Passwords do not match. Try again.")

    def setup(self) -> None:
        """Run the legacy setup flow, including overwrite and password behavior."""

        self.require_restic()
        self.ensure_config_directories()
        repository = self.prompt_path("Enter restic repository path", DEFAULT_REPOSITORY)
        source = self.prompt_path("Enter source path to back up", DEFAULT_SOURCE)
        while True:
            name = self.prompt(f"Enter backup config name [{DEFAULT_NAME}]: ") or DEFAULT_NAME
            slug = slugify(name)
            if slug:
                break
            print("Config name must include at least one letter or number.")
        self.ensure_repository_writable(repository)
        self.validate_source(source)
        existing = self.config_path(slug)
        password = self.password_path(slug)
        if existing.is_file():
            old = self.config_from_file(existing)
            password = old.password_file
            overwrite = self.prompt(f"Config '{name}' already exists. Overwrite paths/settings? [Y/n]: ") or "Y"
            if overwrite.lower() not in {"y", "yes"}:
                log("Setup cancelled.")
                return
        supplied = self.prompt_password(password.is_file())
        if supplied is not None:
            self.write_password(password, supplied)
        elif not password.is_file():
            raise RuntimeError("password file does not exist and no password was entered.")
        config = BackupConfig(name, slug, repository, source, password)
        self.write_config(config)
        self.set_current(slug)
        self.ensure_repository_initialized(config)
        self.setup_timer(config)
        log("Setup complete.")
        self.show(config)

    def show(self, config: BackupConfig) -> None:
        print("=== Current Restic Backup Config ===")
        print(f"Name:       {config.name}\nSlug:       {config.slug}\nRepository: {config.repository}\nSource:     {config.source}")
        print(f"Policy:     daily={config.keep_daily} weekly={config.keep_weekly} monthly={config.keep_monthly} yearly={config.keep_yearly}")
        print(f"Service:    {self.service_name(config.slug)}\nTimer:      {self.timer_name(config.slug)}")

    def list_configs(self) -> list[BackupConfig]:
        configs = self.all_configs()
        if not configs:
            print("No backup configs found.")
            return []
        print("=== All Backup Configs ===")
        for index, config in enumerate(configs, 1):
            enabled = self.run(("systemctl", "is-enabled", self.timer_name(config.slug)), check=False, capture=True).returncode == 0 if shutil.which("systemctl") else False
            print(f"{index}) {config.name} [{config.slug}]\n    Source: {config.source}\n    Repo:   {config.repository}\n    Timer:  {self.timer_name(config.slug)} ({'enabled' if enabled else 'not-enabled'})")
        return configs

    def config_by_index(self, index: str) -> BackupConfig:
        configs = self.all_configs()
        if not index.isdigit() or not 1 <= int(index) <= len(configs):
            raise RuntimeError(f"Invalid config number '{index}'. Use option 2 to see config numbers.")
        config = configs[int(index) - 1]
        self.set_current(config.slug)
        return config

    def list_snapshots(self, config: BackupConfig) -> list[tuple[str, str]]:
        self.require_restic()
        self.ensure_repository_initialized(config)
        output = self.restic(config, "snapshots", "--compact", capture=True).stdout
        snapshots = [(line.split()[0], " ".join(line.split()[1:3])) for line in output.splitlines()[2:] if len(line.split()) >= 3]
        if not snapshots:
            print("No snapshots found.")
            return []
        print(f"{'No.':<4} {'Snapshot ID':<12} {'Date':<20} Approx Size")
        for index, (snapshot, when) in enumerate(snapshots, 1):
            stats = self.restic(config, "stats", snapshot, "--mode", "raw-data", capture=True, check=False).stdout
            size = next((line.split(":", 1)[1].strip() for line in stats.splitlines() if line.strip().startswith("Total Size:")), "unknown")
            print(f"{index})  {snapshot:<12} {when:<20} {size}")
        return snapshots

    def restore(self, config: BackupConfig) -> None:
        snapshots = self.list_snapshots(config)
        if not snapshots:
            return
        while True:
            selected = self.prompt("Select snapshot number to restore to Downloads: ")
            if selected.isdigit() and 1 <= int(selected) <= len(snapshots):
                break
            print(f"Please enter a valid number between 1 and {len(snapshots)}.")
        snapshot = snapshots[int(selected) - 1][0]
        target = self.home / "Downloads" / f"restic-restore-{snapshot}-{datetime.now():%Y%m%d-%H%M%S}"
        target.mkdir(parents=True, exist_ok=True)
        log(f"Restoring snapshot {snapshot} to {target}")
        self.restic(config, "restore", snapshot, "--target", str(target))
        log("Restore complete.")

    def delete(self, config: BackupConfig) -> None:
        confirm = self.prompt(f"Delete config '{config.name}' [{config.slug}] and associated service/timer? [y/N]: ") or "N"
        if confirm.lower() not in {"y", "yes"}:
            log("Delete cancelled.")
            return
        for unit in (self.timer_name(config.slug), self.service_name(config.slug)):
            self.sudo(("systemctl", "disable", "--now", unit), check=False)
            self.sudo(("systemctl", "stop", unit), check=False)
        self.sudo(("rm", "-f", str(self.service_path(config.slug)), str(self.timer_path(config.slug))), check=False)
        self.helper_path(config.slug).unlink(missing_ok=True)
        self.sudo(("systemctl", "daemon-reload"), check=False)
        self.sudo(("systemctl", "reset-failed", self.service_name(config.slug), self.timer_name(config.slug)), check=False)
        self.config_path(config.slug).unlink(missing_ok=True)
        others = self.all_configs()
        if not any(other.password_file == config.password_file for other in others):
            config.password_file.unlink(missing_ok=True)
        else:
            log(f"Password file is shared by another config; preserving {config.password_file}.")
        if any(other.repository == config.repository for other in others):
            log(f"Repository is used by another config; preserving {config.repository}.")
        else:
            remove = self.prompt(f"Delete repository directory and backup data at '{config.repository}' too? [Y/n]: ") or "Y"
            if remove.lower() in {"y", "yes"}:
                try:
                    shutil.rmtree(config.repository)
                except PermissionError:
                    self.sudo(("rm", "-rf", str(config.repository)))
                log(f"Deleted repository directory: {config.repository}")
            else:
                log(f"Repository preserved: {config.repository}")
        if self.current_slug() == config.slug:
            remaining = self.all_configs()
            if remaining:
                self.set_current(remaining[0].slug)
            else:
                self.current_file.unlink(missing_ok=True)
        log(f"Deleted config '{config.name}' [{config.slug}].")

    def menu(self) -> int:
        """Run the exact legacy menu and its command aliases."""

        self.ensure_config_directories()
        self.migrate_legacy_config()
        while True:
            print()
            self.list_configs()
            print("\n=== Restic Backup Manager ===\n1) Run / rerun setup\n2) Exit\n\nSpecial commands:\n  delete <config-number>   Delete config + service/timer\n  3 <config-number>        Take immediate backup now\n  4 <config-number>        List backups (dates + sizes)\n  5 <config-number>        Restore snapshot to Downloads\n  6 <config-number>        Run forget + prune now\n  7 <config-number>        Show current configuration\n  backup|snapshots|restore|forget|show <config-number>\n")
            entered = self.prompt("Choose an option [1-2 or command]: ")
            command, _, index = entered.partition(" ")
            try:
                if command.lower() == "delete" and index:
                    self.delete(self.config_by_index(index.strip()))
                elif command in {"3", "backup"} and index:
                    self.backup_now(self.config_by_index(index.strip()).slug)
                elif command in {"4", "snapshots"} and index:
                    self.list_snapshots(self.config_by_index(index.strip()))
                elif command in {"5", "restore"} and index:
                    self.restore(self.config_by_index(index.strip()))
                elif command in {"6", "forget"} and index:
                    self.require_restic(); self.prune(self.config_by_index(index.strip()))
                elif command in {"7", "show"} and index:
                    self.show(self.config_by_index(index.strip()))
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
    """Run the manager interactively or execute its legacy timer CLI command."""

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--run-backup", action="store_true", help="Run one configured backup without the menu.")
    parser.add_argument("--config-name", help="Select a backup by name or slug with --run-backup.")
    args = parser.parse_args(argv)
    manager = ResticBackupManager()
    manager.ensure_config_directories()
    manager.migrate_legacy_config()
    if args.run_backup:
        manager.backup_now(args.config_name)
        return 0
    return manager.menu()


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, subprocess.CalledProcessError, ValueError) as error:
        print(f"Error: {error}", file=sys.stderr)
        raise SystemExit(1) from error