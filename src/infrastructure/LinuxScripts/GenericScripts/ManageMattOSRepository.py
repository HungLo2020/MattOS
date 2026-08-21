#!/usr/bin/env python3
"""Project-agnostic client for the locally hosted MattOS Debian repository.

The command-line interface intentionally retains the historical repository
manager commands. Repository state, signing, reprepro, and publication now
live on the home server; this program uploads only packages being changed.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import urllib.error
import urllib.request
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Iterable, Sequence

SOURCE_DIRECTORY = Path(__file__).resolve().parents[1] / "src"
if str(SOURCE_DIRECTORY) not in sys.path:
    sys.path.insert(0, str(SOURCE_DIRECTORY))

DEFAULT_SERVER_URL = "http://hunglosvr:8790"
DEFAULT_PUBLIC_URL = "https://packages.mattsherfey.com"
DEFAULT_R2_ITEM = "MattOS R2 Repository Publisher"
DEFAULT_GPG_ITEM = "MattOS Repository Signing Key"
DEFAULT_BUCKET = "matt-apt-repo"
DEFAULT_SUITE = "trixie"
DEFAULT_COMPONENT = "main"
DEFAULT_REPOSITORY_ARCHITECTURES = ("amd64",)
DEFAULT_TOKEN_FILE = Path.home() / ".config" / "mattos-repository" / "token"
SYSTEM_TOKEN_FILE = Path("/etc/mattos-repository/client-token")
SERVER_CONFIG_FILE = Path("/etc/mattos-repository/client.conf")


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


def run_command(args: Sequence[str], *, env: dict[str, str] | None = None, cwd: Path | None = None,
                input_text: str | None = None, check: bool = True, capture: bool = True,
                error: type[AppError] = AppError) -> subprocess.CompletedProcess[str]:
    try:
        result = subprocess.run(list(args), cwd=str(cwd) if cwd else None, env=env, input=input_text,
                                text=True, capture_output=capture, check=False)
    except FileNotFoundError as exc:
        raise DependencyError(f"Required command is not installed: {args[0]}") from exc
    if check and result.returncode:
        detail = (result.stderr or result.stdout or "").strip()
        raise error(f"Command failed ({result.returncode}): {args[0]}" + (f"\n{detail}" if detail else ""))
    return result


def validate_architectures(raw: str | Iterable[str]) -> tuple[str, ...]:
    values = tuple(item.strip() for item in (raw.split(",") if isinstance(raw, str) else raw) if item.strip())
    if not values or "all" in values or any(not re.fullmatch(r"[a-z0-9][a-z0-9+.-]*", value) for value in values):
        raise ConfigurationError("Repository architectures are invalid")
    return tuple(dict.fromkeys(values))


@dataclass(frozen=True)
class Config:
    """Client configuration; old repository metadata fields remain available."""

    # Deprecated fields are retained so callers that constructed Config using
    # the former positional/keyword shape do not fail during migration.
    r2_item: str = ""
    gpg_item: str = ""
    bucket: str = ""
    endpoint: str = ""
    public_url: str = DEFAULT_PUBLIC_URL
    suite: str = DEFAULT_SUITE
    component: str = DEFAULT_COMPONENT
    architectures: tuple[str, ...] = DEFAULT_REPOSITORY_ARCHITECTURES
    server_url: str = DEFAULT_SERVER_URL
    token_file: Path = DEFAULT_TOKEN_FILE

    @classmethod
    def from_env(cls) -> "Config":
        file_values: dict[str, str] = {}
        for candidate in (SERVER_CONFIG_FILE, Path.home() / ".config" / "mattos-repository" / "client.conf"):
            if candidate.is_file():
                for line in candidate.read_text(encoding="utf-8").splitlines():
                    key, separator, value = line.partition("=")
                    if separator and key.strip() and value.strip():
                        file_values[key.strip()] = value.strip()
                break
        server_url = os.environ.get("MATTOS_REPOSITORY_SERVER_URL", file_values.get("SERVER_URL", DEFAULT_SERVER_URL)).rstrip("/")
        token_value = os.environ.get("MATTOS_REPOSITORY_TOKEN_FILE") or file_values.get("TOKEN_FILE")
        token_path = Path(token_value).expanduser() if token_value else DEFAULT_TOKEN_FILE
        return cls(
            r2_item=os.environ.get("MATTOS_R2_ITEM", DEFAULT_R2_ITEM),
            gpg_item=os.environ.get("MATTOS_GPG_ITEM", DEFAULT_GPG_ITEM),
            bucket=os.environ.get("MATTOS_R2_BUCKET", DEFAULT_BUCKET),
            endpoint=os.environ.get("MATTOS_R2_ENDPOINT", ""),
            public_url=os.environ.get("MATTOS_REPOSITORY_URL", DEFAULT_PUBLIC_URL).rstrip("/"),
            suite=os.environ.get("MATTOS_REPOSITORY_SUITE", DEFAULT_SUITE),
            component=os.environ.get("MATTOS_REPOSITORY_COMPONENT", DEFAULT_COMPONENT),
            architectures=validate_architectures(os.environ.get("MATTOS_REPOSITORY_ARCHITECTURES", "amd64")),
            server_url=server_url,
            token_file=token_path,
        )


def tool_home() -> Path:
    """Retain the historical helper for callers that imported it."""
    return Path(os.environ.get("MATTOS_REPO_TOOL_HOME", str(Path.home() / ".local" / "share" / "mattos-repository"))).expanduser()


def bootstrap_python(argv: list[str]) -> None:
    """Compatibility no-op: the local backend has no third-party dependency."""


@dataclass(frozen=True)
class PackageInfo:
    path: Path
    name: str
    version: str
    architecture: str


def package_info(path: Path, repository_architectures: Sequence[str]) -> PackageInfo:
    path = path.expanduser().resolve()
    if not path.is_file() or path.suffix != ".deb":
        raise PackageError(f"Package must be a regular .deb file: {path}")
    result = run_command(["dpkg-deb", "--show", "--showformat=${Package}\n${Version}\n${Architecture}\n", "--", str(path)], error=PackageError)
    values = result.stdout.splitlines()
    if len(values) != 3 or not all(values):
        raise PackageError(f"Package metadata is missing or malformed: {path}")
    name, version, architecture = (value.strip() for value in values)
    if architecture != "all" and architecture not in repository_architectures:
        raise PackageError(f"Package {name} targets unsupported architecture {architecture}")
    return PackageInfo(path, name, version, architecture)


def validate_packages(paths: Sequence[Path], architectures: Sequence[str]) -> list[PackageInfo]:
    if not paths:
        raise PackageError("At least one package path is required")
    infos = [package_info(path, architectures) for path in paths]
    if len({(item.name, item.version, item.architecture) for item in infos}) != len(infos):
        raise PackageError("The upload contains duplicate package entries")
    return infos


def safe_key(key: str) -> str:
    if not key or key.startswith("/") or "\\" in key or ".." in Path(key).parts:
        raise RemoteError(f"Unsafe repository object key: {key!r}")
    if not (key.startswith("dists/") or key.startswith("pool/")):
        raise RemoteError(f"Unmanaged repository object key: {key!r}")
    return key


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def parse_sha256_release(text: str) -> dict[str, str]:
    result: dict[str, str] = {}
    active = False
    for line in text.splitlines():
        if line == "SHA256:":
            active = True
            continue
        if active and line and not line.startswith(" "):
            break
        if active:
            parts = line.split()
            if len(parts) == 3 and re.fullmatch(r"[0-9a-fA-F]{64}", parts[0]):
                result[parts[2]] = parts[0]
    return result


class ServerRepository:
    """HTTP client for the home-server repository API."""

    def __init__(self, config: Config) -> None:
        self.config = config
        token = os.environ.get("MATTOS_REPOSITORY_TOKEN")
        token_candidates = [config.token_file, SYSTEM_TOKEN_FILE]
        if token is None:
            for candidate in token_candidates:
                if candidate.is_file():
                    token = candidate.read_text(encoding="utf-8").strip()
                    if token:
                        break
        self.token = token or ""

    def request(self, method: str, endpoint: str, *, body: bytes | None = None,
                content_type: str = "application/json", headers: dict[str, str] | None = None) -> Any:
        request_headers = {"User-Agent": "MattOSRepositoryManager/2.0", "Accept": "application/json"}
        if self.token:
            request_headers["Authorization"] = f"Bearer {self.token}"
        if headers:
            request_headers.update(headers)
        if body is not None:
            request_headers["Content-Length"] = str(len(body))
            request_headers["Content-Type"] = content_type
        request = urllib.request.Request(f"{self.config.server_url}{endpoint}", data=body, headers=request_headers, method=method)
        try:
            with urllib.request.urlopen(request, timeout=120) as response:
                payload = response.read()
                response_type = response.headers.get_content_type()
        except urllib.error.HTTPError as exc:
            detail = exc.read().decode("utf-8", errors="replace")
            try:
                detail = str(json.loads(detail).get("error", detail))
            except json.JSONDecodeError:
                pass
            raise RemoteError(f"Repository server returned HTTP {exc.code}: {detail}") from exc
        except urllib.error.URLError as exc:
            raise RemoteError(f"Repository server is unreachable: {self.config.server_url}") from exc
        if response_type.startswith("text/"):
            return payload.decode("utf-8")
        try:
            return json.loads(payload.decode("utf-8"))
        except json.JSONDecodeError as exc:
            raise RemoteError("Repository server returned invalid JSON") from exc

    def upload(self, path: Path) -> dict[str, Any]:
        return self.request("POST", "/v1/upload", body=path.read_bytes(), content_type="application/vnd.debian.binary-package", headers={"X-Package-Filename": path.name})

    def remove(self, name: str, version: str | None = None) -> dict[str, Any]:
        return self.request("POST", "/v1/remove", body=json.dumps({"name": name, "version": version}).encode())


def fetch_public_bytes(url: str, description: str) -> bytes:
    request = urllib.request.Request(url, headers={"User-Agent": "MattOSRepositoryManager/2.0", "Accept": "*/*"})
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            return response.read()
    except (urllib.error.URLError, urllib.error.HTTPError) as exc:
        raise VerificationError(f"{description}: {url}") from exc


def doctor(config: Config, *, as_json: bool) -> int:
    result = {
        "python": sys.executable, "python_version": sys.version.split()[0],
        "server_url": config.server_url, "public_url": config.public_url,
        "token_configured": bool(os.environ.get("MATTOS_REPOSITORY_TOKEN") or config.token_file.is_file()),
        "system_dependencies": {"dpkg-deb": command_exists("dpkg-deb")},
    }
    if as_json:
        print(json.dumps(result, indent=2, sort_keys=True))
    else:
        for key, value in result.items():
            print(f"{key}: {value}")
    return 0 if all(result["system_dependencies"].values()) else 10


def parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(description="Manage the locally hosted MattOS Debian repository")
    p.add_argument("--yes", action="store_true", help="Retained for compatibility")
    p.add_argument("--non-interactive", action="store_true", help="Never prompt")
    p.add_argument("--dry-run", action="store_true", help="Validate without changing the server")
    sub = p.add_subparsers(dest="command", required=True)
    sub.add_parser("doctor").add_argument("--json", action="store_true")
    for command in ("init", "publish", "list", "verify", "status"):
        sub.add_parser(command)
    for command in ("upload", "add"):
        item = sub.add_parser(command)
        item.add_argument("packages", nargs="+", type=Path)
    remove = sub.add_parser("remove")
    remove.add_argument("package")
    remove.add_argument("--version")
    key = sub.add_parser("export-key")
    key.add_argument("--output", required=True, type=Path)
    private = sub.add_parser("export-private-key")
    private.add_argument("--output", required=True, type=Path)
    return p


def main(argv: list[str] | None = None) -> int:
    args = parser().parse_args(argv[1:] if argv is not None else None)
    try:
        config = Config.from_env()
        if args.command == "doctor":
            return doctor(config, as_json=args.json)
        repository = ServerRepository(config)
        if args.command in {"upload", "add"}:
            infos = validate_packages(args.packages, config.architectures)
            if args.dry_run:
                print(f"Dry run: upload {len(infos)} validated package(s)")
                return 0
            for info in infos:
                result = repository.upload(info.path)
                print(f"Uploaded {result['name']} {result['version']} ({result['architecture']})")
            return 0
        if args.command == "init":
            if args.dry_run:
                print("Dry run: initialize the server repository if needed")
                return 0
            print(json.dumps(repository.request("POST", "/v1/init"), indent=2, sort_keys=True))
            return 0
        if args.command == "remove":
            if args.dry_run:
                print(f"Dry run: remove {args.package}" + (f" version {args.version}" if args.version else ""))
                return 0
            repository.remove(args.package, args.version)
            print("Package removed.")
            return 0
        if args.command == "publish":
            if args.dry_run:
                print("Dry run: verify the server repository")
                return 0
            print(json.dumps(repository.request("POST", "/v1/publish"), indent=2, sort_keys=True))
            return 0
        if args.command == "list":
            for item in repository.request("GET", "/v1/packages")["packages"]:
                print(f"{item['name']}\t{item['version']}\t{item['architecture']}")
            return 0
        if args.command == "status":
            print(json.dumps(repository.request("GET", "/v1/status"), indent=2, sort_keys=True))
            return 0
        if args.command == "verify":
            repository.request("GET", "/v1/verify")
            print("Repository verification passed.")
            return 0
        if args.command == "export-key":
            output = args.output.expanduser().resolve()
            output.parent.mkdir(parents=True, exist_ok=True)
            output.write_text(repository.request("GET", "/v1/public-key"), encoding="utf-8")
            output.chmod(0o644)
            return 0
        if args.command == "export-private-key":
            print("Warning: writing a private signing key locally.", file=sys.stderr)
            output = args.output.expanduser().resolve()
            output.parent.mkdir(parents=True, exist_ok=True)
            output.write_text(repository.request("GET", "/v1/private-key"), encoding="utf-8")
            output.chmod(0o600)
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
