#!/usr/bin/env python3
"""Project-agnostic Debian repository manager for Cloudflare R2.

This tool manages a package repository; it does not build packages and makes
no assumptions about the project that produced them. A caller only needs to
provide one or more .deb paths:

    python3 ManageMattOSRepository.py upload /absolute/path/package.deb

Commands
--------
doctor                         Read-only dependency/configuration report.
init                           Initialize and publish an empty repository.
upload PACKAGE [...]           Validate, ingest, publish, and verify packages.
add PACKAGE [...]              Compatibility alias for upload.
remove PACKAGE [--version V]   Remove package(s), publish, and verify.
publish                        Rebuild from remote packages and publish.
list                           List packages currently published.
verify                         Verify the public repository completely.
status                         Show remote/configuration status.
export-key --output FILE       Export the public repository key.
export-private-key --output FILE
                               Explicitly export the private key (0600).

The first ``init`` creates the signing key automatically if the configured
Bitwarden Secure Note does not exist. It generates the key in a temporary GPG
home and stores the armored private key in Bitwarden. Other systems retrieve
that same key from Bitwarden; they do not need a permanent local key.

Cloudflare/R2 configuration
---------------------------
R2 credentials come from a Bitwarden Login item. Defaults:

    item:          MattOS R2 Repository Publisher
    bucket:        matt-apt-repo
    public URL:    https://packages.mattsherfey.com

The Login username is the R2 Access Key ID and password is the R2 Secret Access
Key. Add custom fields named R2_ENDPOINT, R2_BUCKET_NAME, and R2_PUBLIC_URL.

The signing key comes from a Bitwarden Secure Note named MattOS Repository
Signing Key. Its note body or PRIVATE_KEY custom field contains the armored
private key. The first ``init`` creates this item if it is missing.

Repository defaults
-------------------
Suite:        trixie
Component:    main
Architectures: amd64

Architecture ``all`` packages are accepted and are published in Debian's
binary-all index. ``all`` must not be included in reprepro's Architectures
configuration.

Dependency behavior
-------------------
Python dependencies are installed into a tool-owned virtual environment under
MATTOS_REPO_TOOL_HOME (default: ~/.local/share/mattos-repository). The script
never uses pip --user or --break-system-packages. Missing venv support produces
an actionable python3-venv installation message.

reprepro, gpg, and dpkg-deb are checked as system dependencies. Mutating
commands offer to install missing apt packages; ``doctor`` is strictly
read-only. Bitwarden CLI is never installed automatically because its account
and authentication setup require an explicit user decision.

Bitwarden authentication
-------------------------
The script reuses a valid BW_SESSION, logs in interactively when required,
then unlocks using ~/Documents/Repos/LinuxScripts/.bw_master_password when
available. If that file is absent or invalid, it securely prompts. Secrets,
sessions, and private keys are never printed or passed as command-line
arguments.

Remote state and publication
----------------------------
R2 is the persistent source of truth. A temporary workspace is populated from
the remote package set, reprepro rebuilds the repository, and only changed
objects are uploaded. Package objects are uploaded before indexes; indexes are
uploaded before Release.gpg/InRelease; stale objects are deleted last. A
short-lived R2 lock object detects concurrent writers.
"""

from __future__ import annotations

import argparse
import getpass
import gzip
import hashlib
import importlib.util
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Iterable, Sequence


DEFAULT_R2_ITEM = "MattOS R2 Repository Publisher"
DEFAULT_GPG_ITEM = "MattOS Repository Signing Key"
DEFAULT_BUCKET = "matt-apt-repo"
DEFAULT_PUBLIC_URL = "https://packages.mattsherfey.com"
DEFAULT_SUITE = "trixie"
DEFAULT_COMPONENT = "main"
DEFAULT_REPOSITORY_ARCHITECTURES = ("amd64",)
DEFAULT_TOOL_HOME = Path.home() / ".local" / "share" / "mattos-repository"
PASSWORD_FILE = Path.home() / "Documents" / "Repos" / "LinuxScripts" / ".bw_master_password"
REEXEC_MARKER = "MATTOS_REPO_TOOL_REEXEC"
LOCK_KEY = "._mattos_repository_lock.json"


class AppError(Exception):
    def __init__(self, message: str, code: int = 1, category: str = "error") -> None:
        super().__init__(message)
        self.code = code
        self.category = category


class DependencyError(AppError):
    def __init__(self, message: str) -> None:
        super().__init__(message, 10, "dependency")


class ConfigurationError(AppError):
    def __init__(self, message: str) -> None:
        super().__init__(message, 11, "configuration")


class AuthenticationError(AppError):
    def __init__(self, message: str) -> None:
        super().__init__(message, 20, "authentication")


class PackageError(AppError):
    def __init__(self, message: str) -> None:
        super().__init__(message, 30, "package")


class RemoteError(AppError):
    def __init__(self, message: str) -> None:
        super().__init__(message, 40, "remote")


class VerificationError(AppError):
    def __init__(self, message: str) -> None:
        super().__init__(message, 50, "verification")


def command_exists(name: str) -> bool:
    return shutil.which(name) is not None


def run_command(
    args: Sequence[str],
    *,
    env: dict[str, str] | None = None,
    cwd: Path | None = None,
    input_text: str | None = None,
    check: bool = True,
    capture: bool = True,
    error: type[AppError] = AppError,
) -> subprocess.CompletedProcess[str]:
    try:
        result = subprocess.run(
            list(args),
            cwd=str(cwd) if cwd else None,
            env=env,
            input=input_text,
            text=True,
            capture_output=capture,
            check=False,
        )
    except FileNotFoundError as exc:
        raise DependencyError(f"Required command is not installed: {args[0]}") from exc
    if check and result.returncode != 0:
        detail = (result.stderr or result.stdout or "").strip()
        raise error(
            f"Command failed ({result.returncode}): {args[0]}"
            + (f"\n{detail}" if detail else "")
        )
    return result


def tool_home() -> Path:
    return Path(os.environ.get("MATTOS_REPO_TOOL_HOME", str(DEFAULT_TOOL_HOME))).expanduser()


def bootstrap_python(argv: list[str]) -> None:
    """Re-exec with boto3 in a tool-owned venv, without PEP 668 violations."""
    if os.environ.get(REEXEC_MARKER) == "1":
        return

    home = tool_home()
    venv = home / "venv"
    interpreter = venv / "bin" / "python"
    home.mkdir(parents=True, exist_ok=True)

    if not interpreter.exists():
        result = subprocess.run(
            [sys.executable, "-m", "venv", str(venv)],
            text=True,
            capture_output=True,
            check=False,
        )
        if result.returncode != 0:
            detail = (result.stderr or result.stdout or "").strip()
            raise DependencyError(
                "Python venv support is unavailable. Install the system package "
                "python3-venv (for example, 'sudo apt install python3-venv') "
                f"and retry. Details: {detail}"
            )

    result = subprocess.run(
        [str(interpreter), "-c", "import boto3"],
        text=True,
        capture_output=True,
        check=False,
    )
    if result.returncode != 0:
        install = subprocess.run(
            [str(interpreter), "-m", "pip", "install", "--upgrade", "boto3"],
            text=True,
            capture_output=True,
            check=False,
        )
        if install.returncode != 0:
            detail = (install.stderr or install.stdout or "").strip()
            raise DependencyError(f"Could not install boto3 in {venv}: {detail}")

    env = os.environ.copy()
    env[REEXEC_MARKER] = "1"
    os.execve(str(interpreter), [str(interpreter), str(Path(__file__).resolve()), *argv[1:]], env)


def missing_system_dependencies() -> dict[str, str]:
    checks = {
        "reprepro": "reprepro",
        "gpg": "gnupg",
        "dpkg-deb": "dpkg",
    }
    return {command: package for command, package in checks.items() if not command_exists(command)}


def ensure_system_dependencies(*, yes: bool, non_interactive: bool, dry_run: bool) -> None:
    missing = missing_system_dependencies()
    if not missing:
        return
    if not command_exists("apt-get"):
        raise DependencyError(
            "Missing system dependencies: " + ", ".join(missing) + ". "
            "Install them with your distribution package manager."
        )
    packages = sorted(set(missing.values()))
    if dry_run or non_interactive:
        raise DependencyError(
            "Missing system dependencies: " + ", ".join(missing) + ". "
            "Run an interactive command to install: apt-get install " + " ".join(packages)
        )
    if not yes:
        answer = input("Install missing system packages with apt? [y/N] ").strip().lower()
        if answer not in {"y", "yes"}:
            raise DependencyError("System dependency installation declined")
    prefix = [] if os.geteuid() == 0 else ["sudo"]
    if os.geteuid() != 0 and not command_exists("sudo"):
        raise DependencyError("sudo is required to install missing system dependencies")
    run_command(prefix + ["apt-get", "update"], capture=False, error=DependencyError)
    run_command(prefix + ["apt-get", "install", "-y", *packages], capture=False, error=DependencyError)


def validate_architectures(raw: str | Iterable[str]) -> tuple[str, ...]:
    values = tuple(item.strip() for item in (raw.split(",") if isinstance(raw, str) else raw) if item.strip())
    if not values:
        raise ConfigurationError("At least one repository architecture is required")
    if "all" in values:
        raise ConfigurationError(
            "Repository architectures must not contain 'all'; architecture-independent "
            "packages are accepted automatically and published in binary-all."
        )
    if any(not re.fullmatch(r"[a-z0-9][a-z0-9+.-]*", value) for value in values):
        raise ConfigurationError("Repository architectures contain an invalid value")
    return tuple(dict.fromkeys(values))


@dataclass(frozen=True)
class Config:
    r2_item: str = DEFAULT_R2_ITEM
    gpg_item: str = DEFAULT_GPG_ITEM
    bucket: str = DEFAULT_BUCKET
    endpoint: str = ""
    public_url: str = DEFAULT_PUBLIC_URL
    suite: str = DEFAULT_SUITE
    component: str = DEFAULT_COMPONENT
    architectures: tuple[str, ...] = DEFAULT_REPOSITORY_ARCHITECTURES

    @classmethod
    def from_env(cls) -> "Config":
        return cls(
            r2_item=os.environ.get("MATTOS_R2_ITEM", DEFAULT_R2_ITEM),
            gpg_item=os.environ.get("MATTOS_GPG_ITEM", DEFAULT_GPG_ITEM),
            bucket=os.environ.get("MATTOS_R2_BUCKET", DEFAULT_BUCKET),
            endpoint=os.environ.get("MATTOS_R2_ENDPOINT", ""),
            public_url=os.environ.get("MATTOS_REPOSITORY_URL", DEFAULT_PUBLIC_URL).rstrip("/"),
            suite=os.environ.get("MATTOS_REPOSITORY_SUITE", DEFAULT_SUITE),
            component=os.environ.get("MATTOS_REPOSITORY_COMPONENT", DEFAULT_COMPONENT),
            architectures=validate_architectures(
                os.environ.get("MATTOS_REPOSITORY_ARCHITECTURES", "amd64")
            ),
        )


def bitwarden_status_payload() -> tuple[str, str]:
    result = run_command(["bw", "status", "--raw"], check=False)
    if result.returncode != 0:
        return "unknown", "status command failed"
    try:
        payload = json.loads(result.stdout)
    except json.JSONDecodeError:
        return "unknown", "invalid status response"
    return str(payload.get("status", "unknown")), ""


class Bitwarden:
    def __init__(self, *, non_interactive: bool, yes: bool) -> None:
        self.non_interactive = non_interactive
        self.yes = yes
        self.ready = False

    def ensure_cli(self) -> None:
        if not command_exists("bw"):
            raise AuthenticationError(
                "Bitwarden CLI (bw) is not installed. Install the Bitwarden CLI "
                "and ensure 'bw' is in PATH; this tool will not install it."
            )

    def _unlock_with_password(self, password: str) -> bool:
        env = os.environ.copy()
        env["BW_MASTER_PASSWORD"] = password
        result = run_command(
            ["bw", "unlock", "--passwordenv", "BW_MASTER_PASSWORD", "--nointeraction", "--raw"],
            env=env,
            check=False,
        )
        if result.returncode == 0 and result.stdout.strip():
            os.environ["BW_SESSION"] = result.stdout.strip()
            return True
        return False

    def ensure_session(self) -> None:
        if self.ready:
            return
        self.ensure_cli()
        status, _ = bitwarden_status_payload()
        if status == "unlocked":
            self.ready = True
            return
        if status == "unknown" and os.environ.get("BW_SESSION"):
            os.environ.pop("BW_SESSION", None)
            status, _ = bitwarden_status_payload()

        if status == "unauthenticated":
            if self.non_interactive:
                raise AuthenticationError("Bitwarden is not logged in and --non-interactive was supplied")
            print("Bitwarden login is required.")
            login = run_command(["bw", "login"], check=False)
            if login.returncode != 0:
                raise AuthenticationError("Bitwarden login failed")
            status, _ = bitwarden_status_payload()

        if status not in {"locked", "unlocked"}:
            raise AuthenticationError(f"Bitwarden authentication state is {status!r}")
        if status == "unlocked":
            self.ready = True
            return

        if PASSWORD_FILE.is_file():
            password = PASSWORD_FILE.read_text(encoding="utf-8").rstrip("\n")
            if self._unlock_with_password(password):
                self.ready = True
                return
            print(f"Could not unlock Bitwarden using {PASSWORD_FILE}; prompting instead.", file=sys.stderr)
        if self.non_interactive:
            raise AuthenticationError("Bitwarden vault is locked and --non-interactive was supplied")
        try:
            password = getpass.getpass("Bitwarden master password: ")
        except (EOFError, KeyboardInterrupt) as exc:
            raise AuthenticationError("Bitwarden unlock was cancelled") from exc
        if not self._unlock_with_password(password):
            raise AuthenticationError("Bitwarden unlock failed")
        self.ready = True

    def list_items(self, name: str) -> list[dict[str, Any]]:
        self.ensure_session()
        result = run_command(["bw", "list", "items", "--search", name, "--raw"], check=False)
        if result.returncode != 0:
            raise AuthenticationError("Bitwarden item search failed; the session may be stale or inaccessible")
        try:
            items = json.loads(result.stdout)
        except json.JSONDecodeError as exc:
            raise AuthenticationError("Bitwarden returned an invalid item-search response") from exc
        return items if isinstance(items, list) else []

    def item(self, name: str, *, required: bool = True) -> dict[str, Any] | None:
        matches = [item for item in self.list_items(name) if item.get("name") == name]
        if not matches:
            if required:
                raise AuthenticationError(f"Bitwarden item not found: {name}")
            return None
        item_id = matches[0].get("id")
        if not item_id:
            raise AuthenticationError(f"Bitwarden item {name!r} has no readable ID")
        result = run_command(["bw", "get", "item", str(item_id), "--raw"], check=False)
        if result.returncode != 0:
            raise AuthenticationError(f"Bitwarden item {name!r} is inaccessible")
        try:
            item = json.loads(result.stdout)
        except json.JSONDecodeError as exc:
            raise AuthenticationError(f"Bitwarden item {name!r} returned invalid JSON") from exc
        if not isinstance(item, dict):
            raise AuthenticationError(f"Bitwarden item {name!r} has invalid data")
        return item

    def create_secure_note(self, name: str, notes: str) -> None:
        if self.item(name, required=False) is not None:
            raise AuthenticationError(f"Refusing to overwrite existing Bitwarden item: {name}")
        payload = json.dumps({"type": 2, "secureNote": {"type": 0}, "name": name, "notes": notes, "fields": []})
        encoded = run_command(["bw", "encode"], input_text=payload).stdout
        # bw accepts encoded JSON on stdin, so private key material never goes
        # into argv or appears in process listings.
        result = run_command(["bw", "create", "item"], input_text=encoded, check=False)
        if result.returncode != 0:
            raise AuthenticationError("Could not create the Bitwarden signing-key item")
        stored = self.item(name, required=True)
        stored_key = field_map(stored or {}).get("PRIVATE_KEY") or str((stored or {}).get("notes") or "")
        if "BEGIN PGP PRIVATE KEY BLOCK" not in stored_key:
            raise AuthenticationError("Bitwarden signing-key item was created but could not be validated")


def field_map(item: dict[str, Any]) -> dict[str, str]:
    result: dict[str, str] = {}
    for field in item.get("fields") or []:
        if isinstance(field, dict) and field.get("name") and field.get("value") is not None:
            result[str(field["name"])] = str(field["value"])
    return result


def r2_settings(config: Config, bw: Bitwarden) -> tuple[str, str, str, str, str]:
    item = bw.item(config.r2_item)
    assert item is not None
    login = item.get("login") or {}
    access_key = str(login.get("username") or "")
    secret_key = str(login.get("password") or "")
    fields = field_map(item)
    endpoint = config.endpoint or fields.get("R2_ENDPOINT", "")
    bucket = fields.get("R2_BUCKET_NAME", config.bucket)
    public_url = fields.get("R2_PUBLIC_URL", config.public_url).rstrip("/")
    if not access_key or not secret_key or not endpoint or not bucket:
        raise ConfigurationError("R2 Bitwarden item must contain credentials and R2_ENDPOINT/R2_BUCKET_NAME fields")
    return access_key, secret_key, endpoint, bucket, public_url


def gpg_material(config: Config, bw: Bitwarden, *, bootstrap: bool) -> tuple[str, str]:
    local = os.environ.get("MATTOS_GPG_PRIVATE_KEY_FILE")
    if local:
        path = Path(local).expanduser()
        if not path.is_file():
            raise ConfigurationError(f"GPG key file does not exist: {path}")
        return path.read_text(encoding="utf-8"), "local-file"
    item = bw.item(config.gpg_item, required=False)
    if item is None:
        if not bootstrap:
            raise ConfigurationError(f"Signing-key item {config.gpg_item!r} is missing; run init")
        if not command_exists("gpg"):
            raise DependencyError("gpg is required to bootstrap the repository signing key")
        with tempfile.TemporaryDirectory(prefix="mattos-key-") as temporary:
            home = Path(temporary) / "gnupg"
            home.mkdir(mode=0o700)
            env = os.environ.copy()
            env["GNUPGHOME"] = str(home)
            identity = os.environ.get(
                "MATTOS_GPG_IDENTITY", "MattOS Repository Signing Key <packages@mattsherfey.com>"
            )
            algorithm = os.environ.get("MATTOS_GPG_ALGORITHM", "rsa4096")
            expiry = os.environ.get("MATTOS_GPG_EXPIRY", "3y")
            run_command(
                ["gpg", "--batch", "--pinentry-mode", "loopback", "--passphrase", "", "--quick-gen-key", identity, algorithm, "sign", expiry],
                env=env,
                error=DependencyError,
            )
            keys = run_command(["gpg", "--batch", "--armor", "--export-secret-keys"], env=env).stdout
        if not bw.yes:
            if bw.non_interactive:
                raise AuthenticationError("Signing-key bootstrap requires --yes in non-interactive mode")
            answer = input(
                f"Create the Bitwarden Secure Note {config.gpg_item!r} with a new repository signing key? [y/N] "
            ).strip().lower()
            if answer not in {"y", "yes"}:
                raise AuthenticationError("Signing-key bootstrap declined")
        bw.create_secure_note(config.gpg_item, keys)
        return keys, "generated"
    fields = field_map(item)
    key = fields.get("PRIVATE_KEY") or str(item.get("notes") or "")
    if "BEGIN PGP PRIVATE KEY BLOCK" not in key:
        raise ConfigurationError(f"Bitwarden signing-key item {config.gpg_item!r} has no armored private key")
    return key, "bitwarden"


@dataclass(frozen=True)
class PackageInfo:
    path: Path
    name: str
    version: str
    architecture: str


def package_info(path: Path, repository_architectures: Sequence[str]) -> PackageInfo:
    path = path.expanduser().resolve()
    if not path.is_file():
        raise PackageError(f"Package is not a regular file: {path}")
    if path.suffix != ".deb":
        raise PackageError(f"Package must have a .deb suffix: {path}")
    format_string = "${Package}\\n${Version}\\n${Architecture}\\n"
    result = run_command(
        ["dpkg-deb", "--show", "--showformat=" + format_string, "--", str(path)],
        error=PackageError,
    )
    values = result.stdout.splitlines()
    if len(values) != 3 or not all(values):
        raise PackageError(f"Package metadata is missing or malformed: {path}")
    if any(value.startswith(prefix) for value, prefix in zip(values, ("Package:", "Version:", "Architecture:"))):
        raise PackageError(f"dpkg-deb returned labeled metadata unexpectedly: {path}")
    name, version, architecture = (value.strip() for value in values)
    if architecture != "all" and architecture not in repository_architectures:
        raise PackageError(
            f"Package {name} targets {architecture}; repository architectures are "
            f"{', '.join(repository_architectures)} (architecture 'all' is accepted)"
        )
    return PackageInfo(path, name, version, architecture)


def validate_packages(paths: Sequence[Path], architectures: Sequence[str]) -> list[PackageInfo]:
    if not paths:
        raise PackageError("At least one package path is required")
    infos = [package_info(path, architectures) for path in paths]
    identities = {(item.name, item.version, item.architecture) for item in infos}
    if len(identities) != len(infos):
        raise PackageError("The upload contains duplicate package name/version/architecture entries")
    return infos


def write_reprepro_config(root: Path, config: Config, fingerprint: str) -> None:
    conf = root / "conf"
    conf.mkdir(parents=True, exist_ok=True)
    text = (
        f"Origin: MattOS\nLabel: MattOS\nCodename: {config.suite}\nSuite: {config.suite}\n"
        f"Architectures: {' '.join(config.architectures)}\nComponents: {config.component}\n"
        f"Description: MattOS packages compatible with Debian 13 Trixie\n"
        f"SignWith: {fingerprint}\nDebIndices: Packages Release . .gz\n"
    )
    (conf / "distributions").write_text(text, encoding="utf-8")


def import_gpg(root: Path, armored_key: str) -> tuple[Path, str]:
    home = root / "gnupg"
    home.mkdir(mode=0o700)
    env = os.environ.copy()
    env["GNUPGHOME"] = str(home)
    run_command(["gpg", "--batch", "--import"], env=env, input_text=armored_key, error=DependencyError)
    result = run_command(["gpg", "--batch", "--with-colons", "--list-secret-keys"], env=env, error=DependencyError)
    for line in result.stdout.splitlines():
        parts = line.split(":")
        if len(parts) > 9 and parts[0] == "fpr":
            return home, parts[9]
    raise ConfigurationError("The signing material contains no secret GPG key")


def build_repository(root: Path, config: Config, armored_key: str, packages: Sequence[Path]) -> None:
    gpg_home, fingerprint = import_gpg(root, armored_key)
    write_reprepro_config(root, config, fingerprint)
    env = os.environ.copy()
    env["GNUPGHOME"] = str(gpg_home)
    for package in packages:
        run_command(["reprepro", "--basedir", str(root), "includedeb", config.suite, str(package)], env=env, error=AppError)
    run_command(["reprepro", "--basedir", str(root), "export"], env=env, error=AppError)


def safe_key(key: str) -> str:
    if not key or key.startswith("/") or "\\" in key:
        raise RemoteError(f"Unsafe remote object key: {key!r}")
    parts = Path(key).parts
    if ".." in parts or any(part in {"", "."} for part in parts):
        raise RemoteError(f"Unsafe remote object key: {key!r}")
    if not (key.startswith("dists/") or key.startswith("pool/")):
        raise RemoteError(f"Unmanaged remote object key: {key!r}")
    return key


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def public_files(root: Path) -> dict[str, Path]:
    result: dict[str, Path] = {}
    for prefix in ("dists", "pool"):
        base = root / prefix
        if base.exists():
            for path in base.rglob("*"):
                if path.is_file():
                    result[safe_key(path.relative_to(root).as_posix())] = path
    return result


def boto3_module() -> Any:
    try:
        import boto3  # type: ignore

        return boto3
    except ImportError as exc:
        raise DependencyError("boto3 is unavailable even after Python environment bootstrap") from exc


class R2:
    def __init__(self, config: Config, bw: Bitwarden) -> None:
        access, secret, endpoint, bucket, public_url = r2_settings(config, bw)
        self.bucket = bucket
        self.public_url = public_url
        self.client = boto3_module().client(
            "s3", endpoint_url=endpoint, aws_access_key_id=access, aws_secret_access_key=secret, region_name="auto"
        )

    def call(self, method: str, *args: Any, **kwargs: Any) -> Any:
        last: Exception | None = None
        for attempt in range(4):
            try:
                body = kwargs.get("Body")
                if hasattr(body, "seek"):
                    body.seek(0)
                return getattr(self.client, method)(*args, **kwargs)
            except Exception as exc:  # R2/botocore exception types vary.
                last = exc
                if attempt == 3:
                    break
                time.sleep(0.5 * (2**attempt))
        raise RemoteError(f"R2 operation {method} failed after retries") from last

    def keys(self) -> set[str]:
        keys: set[str] = set()
        token: str | None = None
        while True:
            kwargs: dict[str, Any] = {"Bucket": self.bucket}
            if token:
                kwargs["ContinuationToken"] = token
            page = self.call("list_objects_v2", **kwargs)
            for entry in page.get("Contents", []):
                key = entry.get("Key")
                if isinstance(key, str) and (key.startswith("dists/") or key.startswith("pool/")):
                    keys.add(safe_key(key))
            if not page.get("IsTruncated"):
                break
            token = page.get("NextContinuationToken")
            if not token:
                raise RemoteError("R2 returned a truncated listing without a continuation token")
        return keys

    def download(self, key: str, path: Path) -> None:
        safe_key(key)
        path.parent.mkdir(parents=True, exist_ok=True)
        self.call("download_file", self.bucket, key, str(path))

    def lock(self, *, dry_run: bool) -> str | None:
        if dry_run:
            return None
        owner = hashlib.sha256(f"{os.getpid()}:{time.time_ns()}".encode()).hexdigest()
        body = json.dumps({"owner": owner, "created": time.time()}).encode()
        try:
            self.call("put_object", Bucket=self.bucket, Key=LOCK_KEY, Body=body, ContentType="application/json", IfNoneMatch="*")
        except RemoteError as exc:
            raise RemoteError("Repository lock is already held or cannot be acquired") from exc
        return owner

    def unlock(self, owner: str | None) -> None:
        if owner:
            try:
                self.call("delete_object", Bucket=self.bucket, Key=LOCK_KEY)
            except RemoteError:
                print("Warning: repository lock could not be removed; inspect R2 before retrying.", file=sys.stderr)

    def publish(self, root: Path, old_keys: set[str], *, dry_run: bool) -> None:
        local = public_files(root)
        new_keys = set(local)
        changed = []
        for key, path in local.items():
            if key not in old_keys:
                changed.append(key)
                continue
            try:
                head = self.call("head_object", Bucket=self.bucket, Key=key)
                if head.get("Metadata", {}).get("sha256") == sha256_file(path):
                    continue
                changed.append(key)
            except RemoteError:
                changed.append(key)
        stale = old_keys - new_keys
        if dry_run:
            print(f"Dry run: upload {len(changed)} object(s), delete {len(stale)} object(s)")
            return

        def upload(key: str) -> None:
            path = local[key]
            args = {"Bucket": self.bucket, "Key": key, "Body": path.open("rb"), "Metadata": {"sha256": sha256_file(path)}}
            if key.startswith("dists/"):
                args.update(ContentType="text/plain; charset=utf-8", CacheControl="no-cache, max-age=0, must-revalidate")
            elif key.endswith(".deb"):
                args.update(ContentType="application/vnd.debian.binary-package", CacheControl="public, max-age=31536000, immutable")
            elif key.endswith(".gz"):
                args.update(ContentType="application/gzip", CacheControl="no-cache, max-age=0, must-revalidate")
            else:
                args.update(ContentType="application/octet-stream", CacheControl="no-cache, max-age=0, must-revalidate")
            try:
                self.call("put_object", **args)
            finally:
                args["Body"].close()

        pool_changes = [key for key in changed if key.startswith("pool/")]
        dist_changes = [key for key in changed if key.startswith("dists/")]
        for key in sorted(pool_changes):
            print(f"Uploading {key}")
            upload(key)
        for key in sorted(dist_changes, key=lambda value: (value.endswith("InRelease"), value.endswith("Release.gpg"), value)):
            print(f"Uploading {key}")
            upload(key)
        for key in sorted(stale, key=lambda value: (not value.startswith("dists/"), value)):
            print(f"Deleting {key}")
            self.call("delete_object", Bucket=self.bucket, Key=key)


def download_workspace(r2: R2, root: Path) -> set[str]:
    keys = r2.keys()
    for key in sorted(keys):
        r2.download(key, root / key)
    return keys


def package_files(root: Path) -> list[Path]:
    return sorted((root / "pool").rglob("*.deb")) if (root / "pool").exists() else []


def fetch_public_bytes(url: str, description: str) -> bytes:
    last_error: Exception | None = None
    request = urllib.request.Request(
        url,
        headers={
            "User-Agent": "MattOSRepositoryManager/1.0",
            "Accept": "text/plain, application/octet-stream, */*",
        },
    )
    for attempt in range(4):
        try:
            with urllib.request.urlopen(request, timeout=20) as response:
                return response.read()
        except (urllib.error.URLError, urllib.error.HTTPError) as exc:
            last_error = exc
            if attempt < 3:
                time.sleep(2**attempt)
    detail = f" ({last_error})" if last_error else ""
    raise VerificationError(f"{description}: {url}{detail}") from last_error


def complete_public_verification(r2: R2, config: Config, armored_key: str) -> None:
    url = f"{r2.public_url}/dists/{config.suite}/InRelease"
    signed = fetch_public_bytes(url, "Public repository endpoint is unreachable")
    with tempfile.TemporaryDirectory(prefix="mattos-verify-") as temporary:
        root = Path(temporary)
        release_path = root / "InRelease"
        release_path.write_bytes(signed)
        gpg_home, _ = import_gpg(root, armored_key)
        env = os.environ.copy()
        env["GNUPGHOME"] = str(gpg_home)
        run_command(["gpg", "--batch", "--verify", str(release_path)], env=env, error=VerificationError)
        plain = root / "Release"
        run_command(["gpg", "--batch", "--output", str(plain), "--decrypt", str(release_path)], env=env, error=VerificationError)
        release = plain.read_text(encoding="utf-8", errors="strict")
        if f"Suite: {config.suite}" not in release or f"Codename: {config.suite}" not in release or f"Components: {config.component}" not in release:
            raise VerificationError("Release metadata does not match configured suite/component")
        if any(f"binary-{arch}/Packages" not in release for arch in config.architectures):
            raise VerificationError("Release metadata is missing a configured architecture index")
        hashes = parse_sha256_release(release)
        for arch in (*config.architectures, "all"):
            if arch == "all" and f"binary-{arch}/Packages" not in release:
                continue
            release_relative = f"{config.component}/binary-{arch}/Packages.gz"
            relative = f"dists/{config.suite}/{release_relative}"
            digest = hashes.get(release_relative) or hashes.get(release_relative.removesuffix(".gz"))
            if not digest:
                raise VerificationError(f"Release metadata does not reference {relative}")
            content = fetch_public_bytes(
                f"{r2.public_url}/{relative}",
                f"Package index is missing: {relative}",
            )
            if hashlib.sha256(content).hexdigest() != digest:
                raise VerificationError(f"Package index checksum mismatch: {relative}")
            try:
                index = gzip.decompress(content).decode("utf-8") if relative.endswith(".gz") else content.decode("utf-8")
            except (OSError, UnicodeDecodeError) as exc:
                raise VerificationError(f"Package index is invalid: {relative}") from exc
            for filename in re.findall(r"^Filename: (.+)$", index, flags=re.MULTILINE):
                safe_key(filename)
                try:
                    r2.call("head_object", Bucket=r2.bucket, Key=filename)
                except RemoteError as exc:
                    raise VerificationError(f"Repository references missing package object: {filename}") from exc


def parse_sha256_release(text: str) -> dict[str, str]:
    result: dict[str, str] = {}
    in_section = False
    for line in text.splitlines():
        if line == "SHA256:":
            in_section = True
            continue
        if in_section and line and not line.startswith(" "):
            break
        if in_section:
            parts = line.split()
            if len(parts) == 3 and re.fullmatch(r"[0-9a-fA-F]{64}", parts[0]):
                result[parts[2]] = parts[0]
    return result


def mutate(config: Config, bw: Bitwarden, command: str, *, yes: bool, non_interactive: bool, dry_run: bool, packages: Sequence[Path] = (), remove_name: str | None = None, remove_version: str | None = None) -> None:
    r2 = R2(config, bw)
    owner = r2.lock(dry_run=dry_run)
    try:
        with tempfile.TemporaryDirectory(prefix="mattos-repository-") as temporary:
            root = Path(temporary) / "repo"
            root.mkdir()
            old_keys = download_workspace(r2, root)
            if command == "init" and old_keys:
                armored_key, _ = gpg_material(config, bw, bootstrap=False)
                complete_public_verification(r2, config, armored_key)
                print("Repository is already initialized; no changes were made.")
                return
            armored_key, _ = gpg_material(config, bw, bootstrap=command in {"init", "upload"} and not old_keys)
            existing = package_files(root)
            staging = Path(temporary) / "packages"
            staging.mkdir()
            staged: list[Path] = []
            for item in existing:
                target = staging / item.name
                shutil.copy2(item, target)
                staged.append(target)
            if remove_name:
                kept = []
                for item in staged:
                    info = package_info(item, config.architectures)
                    if info.name == remove_name and (remove_version is None or info.version == remove_version):
                        continue
                    kept.append(item)
                staged = kept
            for item in packages:
                target = staging / item.name
                shutil.copy2(item, target)
                staged.append(target)
            shutil.rmtree(root / "dists", ignore_errors=True)
            shutil.rmtree(root / "pool", ignore_errors=True)
            build_repository(root, config, armored_key, staged)
            r2.publish(root, old_keys, dry_run=dry_run)
            if not dry_run:
                complete_public_verification(r2, config, armored_key)
    finally:
        r2.unlock(owner)


def list_packages(config: Config, bw: Bitwarden) -> list[PackageInfo]:
    r2 = R2(config, bw)
    with tempfile.TemporaryDirectory(prefix="mattos-list-") as temporary:
        root = Path(temporary)
        download_workspace(r2, root)
        return [package_info(path, config.architectures) for path in package_files(root)]


def export_key(config: Config, bw: Bitwarden, output: Path, private: bool) -> None:
    armored, _ = gpg_material(config, bw, bootstrap=False)
    with tempfile.TemporaryDirectory(prefix="mattos-export-") as temporary:
        root = Path(temporary)
        home, fingerprint = import_gpg(root, armored)
        env = os.environ.copy()
        env["GNUPGHOME"] = str(home)
        args = ["gpg", "--batch", "--armor", "--export-secret-keys" if private else "--export", fingerprint]
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_text(run_command(args, env=env).stdout, encoding="utf-8")
        output.chmod(0o600 if private else 0o644)


def doctor(config: Config, *, as_json: bool) -> int:
    missing = missing_system_dependencies()
    result = {
        "python": sys.executable,
        "python_version": sys.version.split()[0],
        "venv": sys.prefix != sys.base_prefix,
        "boto3_in_current_interpreter": importlib.util.find_spec("boto3") is not None,
        "system_dependencies": {name: name not in missing for name in ("reprepro", "gpg", "dpkg-deb")},
        "bitwarden_cli": command_exists("bw"),
        "r2_item": config.r2_item,
        "gpg_item": config.gpg_item,
        "suite": config.suite,
        "component": config.component,
        "architectures": list(config.architectures),
    }
    if as_json:
        print(json.dumps(result, indent=2, sort_keys=True))
    else:
        for key, value in result.items():
            print(f"{key}: {value}")
        if missing:
            print("Missing system dependencies (doctor is read-only): " + ", ".join(missing))
        if not result["bitwarden_cli"]:
            print("Bitwarden CLI missing; install it separately before publishing.")
    return 0 if not missing and result["bitwarden_cli"] else 10


def public_url_status(config: Config, bw: Bitwarden) -> dict[str, Any]:
    r2 = R2(config, bw)
    url = f"{r2.public_url}/dists/{config.suite}/InRelease"
    try:
        with urllib.request.urlopen(urllib.request.Request(url, method="HEAD"), timeout=20) as response:
            code = response.status
    except urllib.error.URLError:
        code = None
    return {"bucket": r2.bucket, "public_url": r2.public_url, "inrelease_status": code, "objects": len(r2.keys())}


def parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(description="Manage a project-agnostic Debian package repository on Cloudflare R2")
    p.add_argument("--yes", action="store_true", help="Do not prompt before safe apt/credential operations")
    p.add_argument("--non-interactive", action="store_true", help="Never prompt; fail when input is required")
    p.add_argument("--dry-run", action="store_true", help="Validate and show mutations without publishing")
    sub = p.add_subparsers(dest="command", required=True)
    sub.add_parser("doctor").add_argument("--json", action="store_true")
    sub.add_parser("init")
    upload = sub.add_parser("upload")
    upload.add_argument("packages", nargs="+", type=Path)
    add = sub.add_parser("add")
    add.add_argument("packages", nargs="+", type=Path)
    remove = sub.add_parser("remove")
    remove.add_argument("package")
    remove.add_argument("--version")
    sub.add_parser("publish")
    sub.add_parser("list")
    sub.add_parser("verify")
    sub.add_parser("status")
    key = sub.add_parser("export-key")
    key.add_argument("--output", required=True, type=Path)
    private = sub.add_parser("export-private-key")
    private.add_argument("--output", required=True, type=Path)
    return p


def main(argv: list[str] | None = None) -> int:
    argv = argv or sys.argv
    # Doctor is intentionally read-only, including with respect to the tool's
    # Python environment. Other commands bootstrap/re-exec into the venv.
    if not any(value in {"doctor", "--help", "-h"} for value in argv[1:]):
        bootstrap_python(argv)
    args = parser().parse_args(argv[1:])
    if args.non_interactive and args.yes is False:
        args.yes = True
    try:
        config = Config.from_env()
        if args.command == "doctor":
            return doctor(config, as_json=args.json)
        ensure_system_dependencies(yes=args.yes, non_interactive=args.non_interactive, dry_run=args.dry_run)
        bw = Bitwarden(non_interactive=args.non_interactive, yes=args.yes)
        if args.command == "upload" or args.command == "add":
            infos = validate_packages(args.packages, config.architectures)
            if args.dry_run:
                print(f"Dry run: upload {len(infos)} validated package(s)")
                return 0
            mutate(config, bw, "upload", yes=args.yes, non_interactive=args.non_interactive, dry_run=args.dry_run, packages=[item.path for item in infos])
            return 0
        if args.command == "init":
            if args.dry_run:
                print("Dry run: initialize an empty signed repository if the remote bucket is empty")
                return 0
            mutate(config, bw, "init", yes=args.yes, non_interactive=args.non_interactive, dry_run=args.dry_run)
            return 0
        if args.command == "remove":
            if args.dry_run:
                print(f"Dry run: remove {args.package}" + (f" version {args.version}" if args.version else ""))
                return 0
            mutate(config, bw, "remove", yes=args.yes, non_interactive=args.non_interactive, dry_run=args.dry_run, remove_name=args.package, remove_version=args.version)
            return 0
        if args.command == "publish":
            if args.dry_run:
                print("Dry run: rebuild and publish the current remote package set")
                return 0
            mutate(config, bw, "publish", yes=args.yes, non_interactive=args.non_interactive, dry_run=args.dry_run)
            return 0
        if args.command == "list":
            for item in list_packages(config, bw):
                print(f"{item.name}\t{item.version}\t{item.architecture}")
            return 0
        if args.command == "verify":
            r2 = R2(config, bw)
            armored, _ = gpg_material(config, bw, bootstrap=False)
            complete_public_verification(r2, config, armored)
            print("Repository verification passed.")
            return 0
        if args.command == "status":
            print(json.dumps(public_url_status(config, bw), indent=2, sort_keys=True))
            return 0
        if args.command == "export-key":
            export_key(config, bw, args.output.expanduser().resolve(), False)
            return 0
        if args.command == "export-private-key":
            print("Warning: writing a private signing key locally.", file=sys.stderr)
            export_key(config, bw, args.output.expanduser().resolve(), True)
            return 0
        raise AppError(f"Unknown command: {args.command}")
    except AppError as exc:
        print(f"Error [{exc.category}]: {exc}", file=sys.stderr)
        return exc.code
    except KeyboardInterrupt:
        print("Cancelled.", file=sys.stderr)
        return 130


if __name__ == "__main__":
    raise SystemExit(main())
