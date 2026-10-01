#!/usr/bin/env python3
"""Install and manage an unattended Cryptomator vault and optional Jellyfin bind."""

from __future__ import annotations

import argparse
import getpass
import hashlib
import json
import os
import platform
import pwd
import shlex
import shutil
import subprocess
import sys
import tempfile
import time
import urllib.request
import zipfile
from dataclasses import asdict, dataclass
from pathlib import Path


CLI_VERSION = "0.6.2"
CLI_SHA256_X64 = "6c2ac174f94a2ff30fdfa00ac43669703f1bca1fa633a762dc336bf9d794b1cb"
CLI_URL_X64 = f"https://github.com/cryptomator/cli/releases/download/{CLI_VERSION}/cryptomator-cli-{CLI_VERSION}-linux-x64.zip"
FUSE_MOUNTER = "org.cryptomator.frontend.fuse.mount.LinuxFuseMountProvider"
DEFAULT_VAULT_PATH = Path("/srv/storage/OneDrive/MattsVault")
DEFAULT_MOUNT_PATH = Path("/mnt/cryptomator/mattsvault")
DEFAULT_JELLYFIN_TARGET = "/vault-media"
CONFIG_PATH = Path("/etc/linuxscripts/cryptomator-mattsvault.json")
HELPER_PATH = Path("/usr/local/lib/linuxscripts/cryptomator.py")
CLI_ROOT = Path("/opt/linuxscripts/cryptomator-cli") / CLI_VERSION
SERVICE_NAME = "cryptomator-mattsvault.service"
INTEGRATION_SERVICE_NAME = "cryptomator-mattsvault-jellyfin.service"
SERVICE_PATH = Path("/etc/systemd/system") / SERVICE_NAME
INTEGRATION_SERVICE_PATH = Path("/etc/systemd/system") / INTEGRATION_SERVICE_NAME
APPARMOR_PROFILE = Path("/etc/apparmor.d/fusermount3")
APPARMOR_LOCAL = Path("/etc/apparmor.d/local/fusermount3")
APPARMOR_MARKER = "LinuxScripts Cryptomator vault: mattsvault"
FUSE_CONFIG = Path("/etc/fuse.conf")


@dataclass(frozen=True)
class VaultConfiguration:
    """Non-secret machine settings persisted in a root-owned JSON file."""

    name: str
    vault_path: str
    mount_path: str
    password_file: str
    jellyfin_stack: str
    jellyfin_target: str
    service_user: str
    uid: int
    gid: int

    @property
    def vault(self) -> Path:
        return Path(self.vault_path)

    @property
    def mount(self) -> Path:
        return Path(self.mount_path)

    @property
    def password(self) -> Path:
        return Path(self.password_file)

    @property
    def stack(self) -> Path:
        return Path(self.jellyfin_stack)


def sudo(command: tuple[str, ...], **kwargs) -> subprocess.CompletedProcess:
    """Run a privileged command directly as root or through sudo."""

    prefix = () if os.geteuid() == 0 else ("sudo",)
    check = kwargs.pop("check", True)
    return subprocess.run((*prefix, *command), check=check, **kwargs)


def run(command: tuple[str, ...], **kwargs) -> subprocess.CompletedProcess:
    return subprocess.run(command, check=kwargs.pop("check", True), **kwargs)


def prompt_yes_no(question: str, *, default: bool = False) -> bool:
    suffix = " [Y/n]: " if default else " [y/N]: "
    try:
        answer = input(question + suffix).strip().lower()
    except EOFError:
        return default
    if not answer:
        return default
    return answer in {"y", "yes"}


def prompt_value(label: str, default: str) -> str:
    value = input(f"{label} [{default}]: ").strip()
    return value or default


def write_root_file(path: Path, contents: str, mode: str) -> None:
    sudo(("install", "-d", "-m", "0755", str(path.parent)))
    sudo(("tee", str(path)), input=contents, text=True, stdout=subprocess.DEVNULL)
    sudo(("chmod", mode, str(path)))


def load_config(path: Path = CONFIG_PATH) -> VaultConfiguration:
    try:
        values = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise RuntimeError(f"cannot read Cryptomator configuration {path}: {error}") from error
    try:
        return VaultConfiguration(**values)
    except TypeError as error:
        raise RuntimeError(f"invalid Cryptomator configuration {path}: {error}") from error


def legacy_values(home: Path) -> dict[str, str]:
    """Read only path values from the previous shell-style configuration."""

    path = home / ".config/cryptomator-vault-manager/configs/mattsvault.env"
    if not path.is_file():
        return {}
    values: dict[str, str] = {}
    for line in path.read_text(encoding="utf-8").splitlines():
        key, separator, raw = line.partition("=")
        if not separator or key not in {"VAULT_PATH", "PASSWORD_FILE", "JELLYFIN_STACK"}:
            continue
        parsed = shlex.split(raw)
        if len(parsed) == 1:
            values[key] = parsed[0]
    return values


def validate_configuration(config: VaultConfiguration) -> None:
    if not config.vault.is_absolute() or not config.mount.is_absolute() or not config.password.is_absolute():
        raise RuntimeError("vault, mount, and password paths must be absolute")
    if config.mount == Path("/") or config.mount == config.vault or config.mount in config.vault.parents or config.vault in config.mount.parents:
        raise RuntimeError("the encrypted vault and decrypted mount must be separate, non-nested paths")
    try:
        config.mount.relative_to(Path("/srv/storage"))
    except ValueError:
        pass
    else:
        raise RuntimeError("the decrypted mount must be outside the /srv/storage Samba tree")
    if not config.vault.is_dir() or not (config.vault / "masterkey.cryptomator").is_file():
        raise RuntimeError(f"not a Cryptomator vault: {config.vault}")
    if not config.password.is_file() or config.password.stat().st_size == 0:
        raise RuntimeError(f"vault password is missing or empty: {config.password}")
    if not config.jellyfin_target.startswith("/"):
        raise RuntimeError("the Jellyfin target must be an absolute container path")
    account = pwd.getpwnam(config.service_user)
    if account.pw_uid != config.uid or account.pw_gid != config.gid:
        raise RuntimeError(f"saved UID/GID no longer match user {config.service_user}")


def cli_binary(root: Path = CLI_ROOT) -> Path:
    matches = sorted(root.rglob("cryptomator-cli")) if root.is_dir() else []
    executable = next((path for path in matches if path.is_file() and os.access(path, os.X_OK)), None)
    if executable is None:
        raise RuntimeError(f"Cryptomator CLI {CLI_VERSION} is not installed below {root}")
    return executable


def install_cli() -> None:
    if platform.machine().lower() not in {"x86_64", "amd64"}:
        raise RuntimeError(f"the pinned Cryptomator CLI package does not support {platform.machine()}")
    try:
        cli_binary()
        return
    except RuntimeError:
        pass
    with tempfile.TemporaryDirectory(prefix="linuxscripts-cryptomator-") as temporary:
        root = Path(temporary)
        archive = root / "cryptomator-cli.zip"
        request = urllib.request.Request(CLI_URL_X64, headers={"User-Agent": "LinuxScripts-Cryptomator"})
        with urllib.request.urlopen(request, timeout=120) as response, archive.open("wb") as output:
            shutil.copyfileobj(response, output, 1024 * 1024)
        actual = hashlib.sha256(archive.read_bytes()).hexdigest()
        if actual != CLI_SHA256_X64:
            raise RuntimeError(f"Cryptomator CLI checksum mismatch: expected {CLI_SHA256_X64}, got {actual}")
        extracted = root / "extracted"
        extracted.mkdir()
        with zipfile.ZipFile(archive) as package:
            for member in package.infolist():
                target = (extracted / member.filename).resolve()
                if extracted.resolve() not in target.parents and target != extracted.resolve():
                    raise RuntimeError(f"unsafe archive member: {member.filename}")
            package.extractall(extracted)
        binary = next((path for path in extracted.rglob("cryptomator-cli") if path.is_file()), None)
        if binary is None:
            raise RuntimeError("Cryptomator archive did not contain cryptomator-cli")
        sudo(("install", "-d", "-m", "0755", str(CLI_ROOT)))
        sudo(("cp", "-a", f"{extracted}/.", str(CLI_ROOT)))
        sudo(("chmod", "0755", str(CLI_ROOT / binary.relative_to(extracted))))
    cli_binary()


def install_prerequisites() -> None:
    if not shutil.which("fusermount3"):
        if not shutil.which("apt-get"):
            raise RuntimeError("fuse3 is required and automatic installation needs APT")
        sudo(("apt-get", "install", "-y", "fuse3"))
    if not shutil.which("docker"):
        print("Warning: Docker is unavailable; Jellyfin integration will remain idle.")


def fuse_config_contents(contents: str) -> str:
    """Enable non-owner FUSE access without disturbing distribution settings."""

    if any(line.strip() == "user_allow_other" for line in contents.splitlines()):
        return contents
    prefix = contents.rstrip()
    block = "# Enabled by LinuxScripts for Docker access to Cryptomator\nuser_allow_other\n"
    return (prefix + "\n\n" if prefix else "") + block


def fuse_allows_other() -> bool:
    try:
        return any(line.strip() == "user_allow_other" for line in FUSE_CONFIG.read_text(encoding="utf-8").splitlines())
    except OSError:
        return False


def configure_fuse() -> None:
    if FUSE_CONFIG.exists() and (FUSE_CONFIG.is_symlink() or not FUSE_CONFIG.is_file()):
        raise RuntimeError(f"refusing to replace non-regular FUSE configuration: {FUSE_CONFIG}")
    existing = FUSE_CONFIG.read_text(encoding="utf-8") if FUSE_CONFIG.is_file() else ""
    updated = fuse_config_contents(existing)
    if updated != existing:
        write_root_file(FUSE_CONFIG, updated, "0644")


def apparmor_block(config: VaultConfiguration) -> str:
    mount = str(config.mount).replace("\\", "\\\\").replace(" ", "\\ ").rstrip("/") + "/"
    parent = str(config.mount.parent).replace("\\", "\\\\").replace(" ", "\\ ").rstrip("/") + "/"
    return f"""# BEGIN {APPARMOR_MARKER}
# Exact permissions used by fuse3 3.18 while mounting and cleaning up this vault.
capability dac_override,
capability setuid,
{parent} r,
{mount} r,
mount options=(rw, rprivate) -> /,
mount options=(rw, rbind) {parent} -> /,
mount fstype=@{{fuse_types}} options=(nosuid,nodev) options in (ro,rw,noatime,dirsync,nodiratime,noexec,sync) -> {mount},
umount {mount},
# END {APPARMOR_MARKER}"""


def replace_marked_block(contents: str, marker: str, replacement: str) -> str:
    start = f"# BEGIN {marker}"
    end = f"# END {marker}"
    kept: list[str] = []
    inside = False
    found_end = True
    for line in contents.splitlines():
        if line == start:
            inside = True
            found_end = False
            continue
        if inside and line == end:
            inside = False
            found_end = True
            continue
        if not inside:
            kept.append(line)
    if not found_end:
        raise RuntimeError(f"incomplete managed block in {APPARMOR_LOCAL}")
    base = "\n".join(kept).rstrip()
    return (base + "\n\n" if base else "") + replacement + "\n"


def configure_apparmor(config: VaultConfiguration) -> None:
    if not APPARMOR_PROFILE.is_file():
        return
    if APPARMOR_LOCAL.exists() and (APPARMOR_LOCAL.is_symlink() or not APPARMOR_LOCAL.is_file()):
        raise RuntimeError(f"refusing to replace non-regular AppArmor file: {APPARMOR_LOCAL}")
    existing = APPARMOR_LOCAL.read_text(encoding="utf-8") if APPARMOR_LOCAL.exists() else ""
    updated = replace_marked_block(existing, APPARMOR_MARKER, apparmor_block(config))
    write_root_file(APPARMOR_LOCAL, updated, "0644")
    parser = shutil.which("apparmor_parser")
    if not parser:
        raise RuntimeError("AppArmor is active but apparmor_parser is unavailable")
    sudo((parser, "-r", str(APPARMOR_PROFILE)))


def vault_service(config: VaultConfiguration) -> str:
    return f"""[Unit]
Description=Unlock and mount Cryptomator vault {config.name}
After=local-fs.target network-online.target
Wants=network-online.target
RequiresMountsFor={config.vault}
StartLimitIntervalSec=0

[Service]
Type=simple
User={config.service_user}
Group={config.service_user}
UMask=0077
ExecStartPre=/usr/bin/test -s {config.password}
ExecStartPre=/usr/bin/test -d {config.vault}
ExecStartPre=/usr/bin/test -d {config.mount}
ExecStart=/usr/bin/python3 {HELPER_PATH} --run-mount --config {CONFIG_PATH}
ExecStop=/usr/bin/python3 {HELPER_PATH} --detach-jellyfin --config {CONFIG_PATH}
ExecStopPost=-/usr/bin/python3 {HELPER_PATH} --cleanup-mount --config {CONFIG_PATH}
Restart=on-failure
RestartSec=15
KillSignal=SIGINT
SuccessExitStatus=130 SIGINT
TimeoutStopSec=60

[Install]
WantedBy=multi-user.target
"""


def integration_service(config: VaultConfiguration) -> str:
    return f"""[Unit]
Description=Attach unlocked Cryptomator media to Jellyfin
After=docker.service {SERVICE_NAME}
Wants=docker.service {SERVICE_NAME}

[Service]
Type=simple
User={config.service_user}
Group={config.service_user}
UMask=0077
ExecStart=/usr/bin/python3 {HELPER_PATH} --watch-jellyfin --config {CONFIG_PATH}
ExecStop=/usr/bin/python3 {HELPER_PATH} --detach-jellyfin --config {CONFIG_PATH}
Restart=always
RestartSec=15

[Install]
WantedBy=multi-user.target
"""


def override_contents(config: VaultConfiguration) -> str:
    return f"""services:
  jellyfin:
    volumes:
      - type: bind
        source: {json.dumps(str(config.mount))}
        target: {json.dumps(config.jellyfin_target)}
        read_only: true
        bind:
          propagation: rslave
          create_host_path: false
"""


def override_path(config: VaultConfiguration) -> Path:
    return config.stack / "cryptomator.compose.yml"


def install_runtime(config: VaultConfiguration, source: Path) -> None:
    sudo(("install", "-d", "-m", "0755", str(HELPER_PATH.parent)))
    sudo(("install", "-m", "0755", str(source), str(HELPER_PATH)))
    write_root_file(CONFIG_PATH, json.dumps(asdict(config), indent=2) + "\n", "0644")
    write_root_file(SERVICE_PATH, vault_service(config), "0644")
    write_root_file(INTEGRATION_SERVICE_PATH, integration_service(config), "0644")
    config.stack.mkdir(parents=True, exist_ok=True)
    write_override(config)
    sudo(("systemctl", "daemon-reload"))


def is_mounted(config: VaultConfiguration) -> bool:
    result = run(("findmnt", "-rn", "--mountpoint", str(config.mount), "-o", "FSTYPE"), check=False, capture_output=True, text=True)
    return result.returncode == 0 and result.stdout.strip().startswith("fuse")


def mount_allows_other(config: VaultConfiguration) -> bool:
    result = run(("findmnt", "-rn", "--mountpoint", str(config.mount), "-o", "OPTIONS"), check=False, capture_output=True, text=True)
    options = {option.strip() for option in result.stdout.strip().split(",")}
    return result.returncode == 0 and "allow_other" in options


def mounted_entry_count(config: VaultConfiguration) -> int:
    if not is_mounted(config):
        return 0
    try:
        return sum(1 for _ in config.mount.iterdir())
    except OSError:
        return 0


def wait_for_nonempty_mount(config: VaultConfiguration, timeout: int = 90) -> int:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        count = mounted_entry_count(config)
        if count:
            return count
        time.sleep(1)
    status_result = run(("systemctl", "status", SERVICE_NAME, "--no-pager", "-l"), check=False, capture_output=True, text=True)
    raise RuntimeError(f"vault did not become a non-empty FUSE mount within {timeout}s\n{status_result.stdout}{status_result.stderr}")


def docker_available() -> bool:
    return shutil.which("docker") is not None and run(("docker", "info"), check=False, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL).returncode == 0


def jellyfin_files_exist(config: VaultConfiguration) -> bool:
    return (config.stack / "docker-compose.yml").is_file() and (config.stack / ".env").is_file()


def jellyfin_running() -> bool:
    result = run(("docker", "ps", "-q", "--filter", "name=^/jellyfin$"), check=False, capture_output=True, text=True)
    return result.returncode == 0 and bool(result.stdout.strip())


def jellyfin_has_mount(config: VaultConfiguration) -> bool:
    result = run(("docker", "inspect", "jellyfin", "--format", "{{json .Mounts}}"), check=False, capture_output=True, text=True)
    if result.returncode != 0:
        return False
    try:
        mounts = json.loads(result.stdout)
    except json.JSONDecodeError:
        return False
    return any(item.get("Source") == str(config.mount) and item.get("Destination") == config.jellyfin_target and item.get("RW") is False for item in mounts)


def jellyfin_entry_count(config: VaultConfiguration) -> int:
    """Count visible top-level entries as Jellyfin's numeric UID/GID."""

    if not jellyfin_has_mount(config):
        return 0
    result = run(
        (
            "docker",
            "exec",
            "--user",
            f"{config.uid}:{config.gid}",
            "jellyfin",
            "find",
            config.jellyfin_target,
            "-mindepth",
            "1",
            "-maxdepth",
            "1",
            "-printf",
            "x",
        ),
        check=False,
        capture_output=True,
    )
    return len(result.stdout) if result.returncode == 0 else 0


def compose_command(config: VaultConfiguration, *, with_override: bool) -> tuple[str, ...]:
    command = ["docker", "compose", "-f", str(config.stack / "docker-compose.yml")]
    if with_override:
        command.extend(("-f", str(override_path(config))))
    command.extend(("--env-file", str(config.stack / ".env")))
    return tuple(command)


def write_override(config: VaultConfiguration) -> Path:
    path = override_path(config)
    desired = override_contents(config)
    if path.is_file() and path.read_text(encoding="utf-8") == desired:
        path.chmod(0o600)
        return path
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.write_text(desired, encoding="utf-8")
    temporary.chmod(0o600)
    temporary.replace(path)
    return path


def attach_jellyfin(config: VaultConfiguration) -> bool:
    if not is_mounted(config) or not mount_allows_other(config) or mounted_entry_count(config) == 0 or not jellyfin_files_exist(config) or not docker_available() or not jellyfin_running():
        return False
    if jellyfin_has_mount(config):
        return True
    write_override(config)
    run((*compose_command(config, with_override=True), "up", "-d", "--no-deps", "--force-recreate", "jellyfin"))
    return jellyfin_has_mount(config)


def detach_jellyfin(config: VaultConfiguration) -> bool:
    if not docker_available() or not jellyfin_files_exist(config) or not jellyfin_running() or not jellyfin_has_mount(config):
        return False
    run((*compose_command(config, with_override=False), "up", "-d", "--no-deps", "--force-recreate", "jellyfin"))
    return True


def watch_jellyfin(config: VaultConfiguration) -> None:
    while True:
        if is_mounted(config) and mounted_entry_count(config):
            attach_jellyfin(config)
        else:
            detach_jellyfin(config)
        time.sleep(15)


def wait_for_jellyfin_mount(config: VaultConfiguration, timeout: int = 120) -> int:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        count = jellyfin_entry_count(config)
        if count:
            return count
        time.sleep(1)
    service_status = run(("systemctl", "status", INTEGRATION_SERVICE_NAME, "--no-pager", "-l"), check=False, capture_output=True, text=True)
    raise RuntimeError(f"Jellyfin did not receive a non-empty {config.jellyfin_target} mount within {timeout}s\n{service_status.stdout}{service_status.stderr}")


def run_mount(config: VaultConfiguration) -> None:
    validate_configuration(config)
    if is_mounted(config):
        raise RuntimeError(f"mount point is already mounted: {config.mount}")
    binary = cli_binary()
    if not fuse_allows_other():
        raise RuntimeError(f"{FUSE_CONFIG} must contain user_allow_other before mounting for Docker")
    command = unlock_command(config, binary)
    with config.password.open("rb") as password:
        os.dup2(password.fileno(), 0)
        os.execv(str(binary), command)


def unlock_command(config: VaultConfiguration, binary: Path) -> list[str]:
    """Build the CLI command with the exact libfuse option syntax."""

    return [
        str(binary),
        "unlock",
        "--password:stdin",
        f"--mounter={FUSE_MOUNTER}",
        f"--mountPoint={config.mount}",
        "--mountOption=-oallow_other",
        str(config.vault),
    ]


def cleanup_mount(config: VaultConfiguration) -> None:
    """Lazily detach a stale FUSE mount after the CLI has exited."""

    if not is_mounted(config):
        return
    fusermount = shutil.which("fusermount3") or shutil.which("fusermount")
    if not fusermount:
        raise RuntimeError("cannot clean up FUSE mount: fusermount is unavailable")
    result = run((fusermount, "-u", "-z", str(config.mount)), check=False, capture_output=True, text=True)
    deadline = time.monotonic() + 10
    while is_mounted(config) and time.monotonic() < deadline:
        time.sleep(0.25)
    if is_mounted(config):
        detail = (result.stderr or result.stdout).strip()
        raise RuntimeError(f"could not detach stale FUSE mount {config.mount}: {detail or 'fusermount failed'}")


def choose_password(path: Path) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    if path.is_file() and path.stat().st_size:
        path.chmod(0o600)
        if not prompt_yes_no(f"A stored vault password exists at {path}. Replace it?"):
            return
    while True:
        first = getpass.getpass("Cryptomator vault password: ")
        second = getpass.getpass("Confirm Cryptomator vault password: ")
        if first and first == second:
            break
        print("Passwords were empty or did not match.")
    temporary = path.with_suffix(path.suffix + ".tmp")
    descriptor = os.open(temporary, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    with os.fdopen(descriptor, "w", encoding="utf-8") as output:
        output.write(first + "\n")
    temporary.replace(path)
    path.chmod(0o600)


def default_account() -> pwd.struct_passwd:
    user = os.environ.get("SUDO_USER") or getpass.getuser()
    if user == "root":
        raise RuntimeError("run setup as the normal vault owner, not as root")
    return pwd.getpwnam(user)


def configuration_artifacts(home: Path) -> list[Path]:
    legacy = home / ".config/cryptomator-vault-manager/configs/mattsvault.env"
    password = home / ".config/cryptomator-vault-manager/credentials/mattsvault.password"
    return [path for path in (CONFIG_PATH, SERVICE_PATH, INTEGRATION_SERVICE_PATH, legacy, password) if path.exists()]


def configure_interactively(source: Path) -> int:
    account = default_account()
    home = Path(account.pw_dir)
    artifacts = configuration_artifacts(home)
    if artifacts:
        print("Existing Cryptomator configuration detected:")
        for path in artifacts:
            print(f"  {path}")
        if not prompt_yes_no("Reconfigure the vault paths, credentials, and services?"):
            print("Existing configuration was left unchanged.")
            return 0
    elif not prompt_yes_no("No Cryptomator service is configured. Create it now?", default=True):
        return 0
    old = legacy_values(home)
    vault = Path(prompt_value("Encrypted vault directory", old.get("VAULT_PATH", str(DEFAULT_VAULT_PATH)))).expanduser().resolve()
    mount = Path(prompt_value("Decrypted mount point outside Samba", str(DEFAULT_MOUNT_PATH))).expanduser().resolve()
    stack = Path(prompt_value("Jellyfin stack directory", old.get("JELLYFIN_STACK", str(home / ".jellyfin-stack")))).expanduser().resolve()
    password = Path(old.get("PASSWORD_FILE", str(home / ".config/cryptomator-vault-manager/credentials/mattsvault.password"))).expanduser().resolve()
    choose_password(password)
    config = VaultConfiguration("MattsVault", str(vault), str(mount), str(password), str(stack), DEFAULT_JELLYFIN_TARGET, account.pw_name, account.pw_uid, account.pw_gid)
    validate_configuration(config)
    reconcile(config, source)
    return 0


def reconcile(config: VaultConfiguration, source: Path) -> None:
    validate_configuration(config)
    install_prerequisites()
    install_cli()
    configure_fuse()
    sudo(("install", "-d", "-m", "0750", "-o", config.service_user, "-g", config.service_user, str(config.mount.parent)))
    # GNU install treats an active FUSE mountpoint as an existing non-directory
    # on some systems. Preserve it during idempotent reconciliation; systemd's
    # restart below performs the controlled detach/remount.
    if not is_mounted(config):
        sudo(("install", "-d", "-m", "0700", "-o", config.service_user, "-g", config.service_user, str(config.mount)))
    configure_apparmor(config)
    install_runtime(config, source)
    # The optional integration is deliberately not enabled here. Jellyfin must
    # remain independent and setup must never recreate its container.
    # A normal CLI shutdown should unmount cleanly. The explicit cleanup also
    # recovers a stale transport from an interrupted or older installation.
    sudo(("systemctl", "stop", SERVICE_NAME), check=False)
    cleanup_mount(config)
    config.mount.mkdir(parents=True, exist_ok=True)
    config.mount.chmod(0o700)
    sudo(("systemctl", "enable", SERVICE_NAME))
    sudo(("systemctl", "restart", SERVICE_NAME))
    entries = wait_for_nonempty_mount(config)
    noun = "entry" if entries == 1 else "entries"
    print(f"Cryptomator is mounted at {config.mount}; verified {entries} top-level {noun}.")
    print(f"{SERVICE_NAME} is enabled and running. Jellyfin integration was installed but not enabled by setup.")


def status(config: VaultConfiguration) -> int:
    enabled = run(("systemctl", "is-enabled", SERVICE_NAME), check=False, capture_output=True, text=True).stdout.strip()
    active = run(("systemctl", "is-active", SERVICE_NAME), check=False, capture_output=True, text=True).stdout.strip()
    integration = run(("systemctl", "is-enabled", INTEGRATION_SERVICE_NAME), check=False, capture_output=True, text=True).stdout.strip()
    count = mounted_entry_count(config)
    docker_ready = mount_allows_other(config)
    attached = jellyfin_has_mount(config) if docker_available() and jellyfin_running() else False
    visible = jellyfin_entry_count(config) if attached else 0
    print(f"Vault:       {config.vault}")
    print(f"Mount:       {config.mount}")
    print(f"Service:     {enabled or 'unknown'}, {active or 'unknown'}")
    print(f"Contents:    {count} top-level entries" if count else "Contents:    unavailable or empty")
    print(f"Docker:      {'mount access ready' if docker_ready else 'run reconcile to enable allow_other'}")
    print(f"Jellyfin:    integration {integration or 'disabled'}; {'attached' if attached else 'not attached'}; {visible} visible entries")
    return 0 if enabled == "enabled" and active == "active" and count and docker_ready else 1


def enable_jellyfin(config: VaultConfiguration) -> None:
    validate_configuration(config)
    if not is_mounted(config) or not mounted_entry_count(config):
        raise RuntimeError("the Cryptomator vault is unavailable or empty; run reconcile first")
    if not fuse_allows_other() or not mount_allows_other(config):
        raise RuntimeError("the live vault mount is not accessible to Docker; choose reconcile first to remount it with allow_other")
    if not jellyfin_files_exist(config) or not docker_available() or not jellyfin_running():
        raise RuntimeError("the Jellyfin Compose stack and running container are required before enabling integration")
    if not prompt_yes_no("Enable integration now? This may recreate only the Jellyfin container."):
        print("Jellyfin integration remains disabled.")
        return
    sudo(("systemctl", "enable", "--now", INTEGRATION_SERVICE_NAME))
    entries = wait_for_jellyfin_mount(config)
    print(f"Jellyfin can read {entries} top-level entries from {config.jellyfin_target}.")


def disable_jellyfin(config: VaultConfiguration) -> None:
    if not prompt_yes_no("Disable integration now? This may recreate only the Jellyfin container."):
        print("Jellyfin integration was left unchanged.")
        return
    sudo(("systemctl", "disable", "--now", INTEGRATION_SERVICE_NAME), check=False)
    if jellyfin_has_mount(config):
        raise RuntimeError(f"Jellyfin still has {config.jellyfin_target} after disabling integration")
    print("Jellyfin integration is disabled and the optional mount is detached.")


def menu(source: Path) -> int:
    while True:
        print("\n=== Cryptomator Vault Manager ===")
        print("1) Run/re-run setup")
        print("2) Show status")
        print("3) Reconcile installed files and restart vault")
        print("4) Enable optional Jellyfin integration")
        print("5) Disable optional Jellyfin integration")
        print("0) Exit")
        try:
            choice = input("Choose an option: ").strip()
        except EOFError:
            return 0
        if choice in {"", "0"}:
            return 0
        try:
            if choice == "1":
                configure_interactively(source)
            else:
                config = load_config()
                if choice == "2":
                    status(config)
                elif choice == "3":
                    reconcile(config, source)
                elif choice == "4":
                    enable_jellyfin(config)
                elif choice == "5":
                    disable_jellyfin(config)
                else:
                    print("Invalid selection.")
        except (OSError, RuntimeError, subprocess.CalledProcessError, ValueError) as error:
            print(f"Error: {error}", file=sys.stderr)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    actions = parser.add_mutually_exclusive_group()
    actions.add_argument("--setup", action="store_true")
    actions.add_argument("--reconcile", action="store_true")
    actions.add_argument("--status", action="store_true")
    actions.add_argument("--enable-jellyfin", action="store_true")
    actions.add_argument("--disable-jellyfin", action="store_true")
    actions.add_argument("--run-mount", action="store_true", help=argparse.SUPPRESS)
    actions.add_argument("--watch-jellyfin", action="store_true", help=argparse.SUPPRESS)
    actions.add_argument("--detach-jellyfin", action="store_true", help=argparse.SUPPRESS)
    actions.add_argument("--cleanup-mount", action="store_true", help=argparse.SUPPRESS)
    parser.add_argument("--config", type=Path, default=CONFIG_PATH)
    args = parser.parse_args(argv)
    source = Path(__file__).resolve()
    try:
        if not any((args.setup, args.reconcile, args.status, args.enable_jellyfin, args.disable_jellyfin, args.run_mount, args.watch_jellyfin, args.detach_jellyfin, args.cleanup_mount)):
            return menu(source)
        if args.setup:
            return configure_interactively(source)
        config = load_config(args.config)
        if args.reconcile:
            reconcile(config, source)
        elif args.status:
            return status(config)
        elif args.enable_jellyfin:
            enable_jellyfin(config)
        elif args.disable_jellyfin:
            disable_jellyfin(config)
        elif args.run_mount:
            run_mount(config)
        elif args.watch_jellyfin:
            watch_jellyfin(config)
        elif args.detach_jellyfin:
            detach_jellyfin(config)
        elif args.cleanup_mount:
            cleanup_mount(config)
        return 0
    except (OSError, RuntimeError, subprocess.CalledProcessError, ValueError) as error:
        print(f"Error: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
