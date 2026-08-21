"""Execute an already reviewed package-provider command plan."""

from __future__ import annotations

import hashlib
import os
import re
import sys
import tempfile
from contextlib import contextmanager
from pathlib import Path
from collections.abc import Iterable
from urllib.parse import urlparse
from urllib.request import Request, urlopen

if os.name != "nt":
    import fcntl

from packages.models import CommandSpec, NodejsOperation, ProviderOperation, ScriptOperation, ShellInstallerOperation
from process import find_command, require_command, run_command


_SHELL_INSTALLER_USER_AGENT = "curl/8.5.0"


def _command_with_privileges(command: CommandSpec) -> tuple[str, ...]:
    if not command.elevated or os.name == "nt" or os.geteuid() == 0:
        return command.argv
    return (require_command("sudo"), *command.argv)


def _script_path(repository_root: Path, script: str) -> Path:
    path = repository_root / "src" / "scripts" / script
    if not path.is_file():
        raise RuntimeError(f"Dependency script does not exist: {path}")
    return path


def _refresh_windows_path() -> None:
    """Load persisted Windows PATH entries after an installer updates them."""

    if os.name != "nt":
        return

    import winreg

    entries = os.environ.get("PATH", "").split(os.pathsep)
    for hive, key in ((winreg.HKEY_LOCAL_MACHINE, r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment"), (winreg.HKEY_CURRENT_USER, "Environment")):
        try:
            with winreg.OpenKey(hive, key) as environment_key:
                value, _ = winreg.QueryValueEx(environment_key, "Path")
        except OSError:
            continue
        entries.extend(os.path.expandvars(value).split(os.pathsep))

    unique_entries = list(dict.fromkeys(entry for entry in entries if entry))
    os.environ["PATH"] = os.pathsep.join(unique_entries)


def _prepare_provider(provider: str) -> None:
    if os.name != "nt":
        return

    _refresh_windows_path()
    if provider != "npm" or find_command("npm") is not None:
        return

    for variable, relative_path in (("ProgramFiles", "nodejs"), ("LOCALAPPDATA", "Programs\\nodejs")):
        base_path = os.environ.get(variable)
        if base_path is None:
            continue
        candidate = str(Path(base_path) / relative_path)
        if Path(candidate).is_dir():
            os.environ["PATH"] = os.environ["PATH"] + os.pathsep + candidate

    if find_command("npm") is None:
        raise RuntimeError("npm was not found after installing Node.js. Close and reopen PowerShell, then rerun the package apply command.")


def validate_script_dependencies(operations: Iterable[ProviderOperation | NodejsOperation | ShellInstallerOperation | ScriptOperation], repository_root: Path) -> None:
    """Fail planning before package installation when a declared script is unavailable."""

    for operation in operations:
        if isinstance(operation, ScriptOperation):
            _script_path(repository_root, operation.script)


def _working_command(command: str) -> bool:
    """Return whether a command can run successfully from the current PATH."""

    if find_command(command) is None:
        return False
    try:
        return run_command((command, "--version"), check=False).returncode == 0
    except OSError:
        return False


def _nodesource_nodejs_candidate(policy: str) -> bool:
    """Return whether APT's selected Node.js candidate is published by NodeSource."""

    candidate = re.search(r"^\s*Candidate:\s*(\S+)", policy, re.MULTILINE)
    if candidate is None or candidate.group(1) == "(none)":
        return False
    candidate_version = candidate.group(1)
    sections = re.split(r"^\s{2,}(?=\S+\s+\d+\s*$)", policy, flags=re.MULTILINE)
    return any(
        re.match(rf"{re.escape(candidate_version)}\s+\d+\s*$", section, re.MULTILINE)
        and "nodesource.com" in section
        for section in sections
    )


def ensure_nodejs_npm() -> None:
    """Ensure a compatible Node.js installation supplies both node and npm."""

    if _working_command("node") and _working_command("npm"):
        print("  Node.js and npm are already available.")
        return
    policy = run_command(("apt-cache", "policy", "nodejs"), capture_output=True).stdout
    package = "nodejs" if _nodesource_nodejs_candidate(policy) else "npm"
    source = "NodeSource nodejs" if package == "nodejs" else "distribution npm"
    print(f"  Installing {source} to provide Node.js and npm.")
    run_command(_command_with_privileges(CommandSpec(("apt-get", "update"), "Refresh APT package metadata", elevated=True)))
    run_command(_command_with_privileges(CommandSpec(("apt-get", "install", "-y", package), "Install Node.js and npm", elevated=True)))
    if not (_working_command("node") and _working_command("npm")):
        raise RuntimeError("Node.js installation completed but node and npm are not both available on PATH.")


def _validate_shell_installer_url(url: str) -> None:
    parsed = urlparse(url)
    if parsed.scheme != "https" or not parsed.hostname or parsed.username or parsed.password:
        raise RuntimeError("Shell installer URLs must use HTTPS and cannot include embedded credentials.")


def run_shell_installer(url: str) -> None:
    """Download an HTTPS installer to a private file and execute it as the current user."""

    _validate_shell_installer_url(url)
    installer_path: Path | None = None
    try:
        request = Request(url, headers={"User-Agent": _SHELL_INSTALLER_USER_AGENT})
        with urlopen(request, timeout=60) as response:
            _validate_shell_installer_url(response.geturl())
            with tempfile.NamedTemporaryFile(prefix="linuxscripts-installer-", suffix=".sh", delete=False) as installer_file:
                installer_path = Path(installer_file.name)
                installer_path.chmod(0o600)
                while chunk := response.read(1024 * 1024):
                    installer_file.write(chunk)
        if installer_path.stat().st_size == 0:
            raise RuntimeError(f"Shell installer download was empty: {url}")
        run_command(("sh", str(installer_path)))
    finally:
        if installer_path is not None:
            installer_path.unlink(missing_ok=True)


@contextmanager
def execution_lock(repository_root: Path):
    """Prevent concurrent package applies for the same checkout on POSIX hosts."""

    if os.name == "nt":
        yield
        return
    lock_name = hashlib.sha256(str(repository_root.resolve()).encode("utf-8")).hexdigest()[:16]
    lock_path = Path(tempfile.gettempdir()) / f"linuxscripts-package-apply-{lock_name}.lock"
    with lock_path.open("a+", encoding="utf-8") as lock_file:
        try:
            fcntl.flock(lock_file, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError as error:
            raise RuntimeError(f"Another package apply is already running for this checkout ({lock_path}).") from error
        try:
            yield
        finally:
            fcntl.flock(lock_file, fcntl.LOCK_UN)


def execute_operations(operations: Iterable[ProviderOperation | NodejsOperation | ShellInstallerOperation | ScriptOperation], repository_root: Path) -> None:
    """Execute provider operations in their planned order."""

    with execution_lock(repository_root):
        for operation in operations:
            if isinstance(operation, ScriptOperation):
                script = _script_path(repository_root, operation.script)
                print(operation.description)
                run_command((sys.executable, str(script)), cwd=repository_root)
                continue
            if isinstance(operation, NodejsOperation):
                print(f"Provider: nodejs ({', '.join(operation.packages)})")
                ensure_nodejs_npm()
                continue
            if isinstance(operation, ShellInstallerOperation):
                print(f"Provider: shell_installer ({', '.join(operation.packages)})")
                for package, url in zip(operation.packages, operation.urls, strict=True):
                    print(f"  Run shell installer for '{package}': {url}")
                    run_shell_installer(url)
                continue
            _prepare_provider(operation.provider)
            print(f"Provider: {operation.provider} ({', '.join(operation.packages)})")
            for command in operation.commands:
                print(f"  {command.description}")
                run_command(_command_with_privileges(command))