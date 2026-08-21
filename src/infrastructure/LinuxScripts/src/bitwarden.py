"""Safe shared authentication and item access for the Bitwarden CLI."""

from __future__ import annotations

import getpass
import json
import os
import shutil
import stat
import subprocess
import sys
from pathlib import Path
from typing import Any


class BitwardenError(RuntimeError):
    """Raised when Bitwarden authentication or item access fails."""


class BitwardenClient:
    """Authenticate without ever hiding an interactive Bitwarden password prompt."""

    def __init__(
        self,
        *,
        password_file: Path | None = None,
        non_interactive: bool = False,
        error_type: type[Exception] = BitwardenError,
    ) -> None:
        self.password_file = password_file
        self.non_interactive = non_interactive
        self.error_type = error_type
        self.ready = False

    def fail(self, message: str) -> None:
        raise self.error_type(message)

    def run(
        self,
        command: tuple[str, ...],
        *,
        input_text: str | None = None,
        environment: dict[str, str] | None = None,
        capture_output: bool = True,
        check: bool = True,
    ) -> subprocess.CompletedProcess[str]:
        try:
            result = subprocess.run(
                command,
                input=input_text,
                env=environment,
                text=True,
                capture_output=capture_output,
                check=False,
            )
        except OSError as error:
            self.fail(f"Could not run Bitwarden CLI: {error}")
        if check and result.returncode != 0:
            detail = (result.stderr or result.stdout).strip()
            self.fail(f"Bitwarden command failed: {' '.join(command)}" + (f"\n{detail}" if detail else ""))
        return result

    def status(self) -> str:
        result = self.run(("bw", "status", "--raw"), check=False)
        if result.returncode != 0:
            return "unknown"
        try:
            payload = json.loads(result.stdout)
        except json.JSONDecodeError:
            return "unknown"
        return str(payload.get("status", "unknown")) if isinstance(payload, dict) else "unknown"

    def password_from_file(self) -> str | None:
        if self.password_file is None or not self.password_file.is_file():
            return None
        details = self.password_file.stat()
        if details.st_uid != os.getuid() or details.st_mode & (stat.S_IRWXG | stat.S_IRWXO):
            print(f"Ignoring insecure Bitwarden password file: {self.password_file}", file=sys.stderr)
            return None
        password = self.password_file.read_text(encoding="utf-8").splitlines()
        return password[0] if password else None

    def unlock(self, password: str) -> None:
        environment = os.environ | {"BW_MASTER_PASSWORD": password}
        result = self.run(
            ("bw", "unlock", "--passwordenv", "BW_MASTER_PASSWORD", "--nointeraction", "--raw"),
            environment=environment,
            check=False,
        )
        session = result.stdout.strip()
        if result.returncode != 0 or not session:
            self.fail("Bitwarden unlock failed.")
        os.environ["BW_SESSION"] = session

    def ensure_session(self) -> None:
        if self.ready:
            return
        if shutil.which("bw") is None:
            self.fail("Bitwarden CLI (bw) is not installed or is not on PATH.")
        status = self.status()
        if status == "unlocked":
            self.ready = True
            return
        if status == "unknown" and os.environ.get("BW_SESSION"):
            os.environ.pop("BW_SESSION", None)
            status = self.status()
        if status == "unauthenticated":
            if self.non_interactive:
                self.fail("Bitwarden is not logged in and interactive authentication is disabled.")
            print("Bitwarden login is required.")
            self.run(("bw", "login"), capture_output=False)
            status = self.status()
        if status == "unlocked":
            self.ready = True
            return
        if status != "locked":
            self.fail(f"Bitwarden authentication state is {status!r}.")

        password = self.password_from_file()
        if password is None:
            if self.non_interactive:
                self.fail("Bitwarden is locked and interactive authentication is disabled.")
            try:
                password = getpass.getpass("Bitwarden master password: ")
            except (EOFError, KeyboardInterrupt) as error:
                self.fail("Bitwarden unlock was cancelled.")
        self.unlock(password)
        self.ready = True

    def password(self, item_name: str) -> str:
        self.ensure_session()
        value = self.run(("bw", "get", "password", item_name)).stdout.strip()
        if not value:
            self.fail(f"Bitwarden item {item_name!r} has no password.")
        return value

    def username(self, item_name: str) -> str:
        self.ensure_session()
        value = self.run(("bw", "get", "username", item_name)).stdout.strip()
        if not value:
            self.fail(f"Bitwarden item {item_name!r} has no username.")
        return value

    def list_items(self, name: str) -> list[dict[str, Any]]:
        self.ensure_session()
        result = self.run(("bw", "list", "items", "--search", name, "--raw"))
        try:
            items = json.loads(result.stdout)
        except json.JSONDecodeError:
            self.fail("Bitwarden returned invalid item-search data.")
        return items if isinstance(items, list) else []

    def item(self, name: str, *, required: bool = True) -> dict[str, Any] | None:
        matches = [item for item in self.list_items(name) if item.get("name") == name]
        if not matches:
            if required:
                self.fail(f"Bitwarden item not found: {name}")
            return None
        item_id = matches[0].get("id")
        if not item_id:
            self.fail(f"Bitwarden item {name!r} has no readable ID.")
        result = self.run(("bw", "get", "item", str(item_id), "--raw"))
        try:
            item = json.loads(result.stdout)
        except json.JSONDecodeError:
            self.fail(f"Bitwarden item {name!r} returned invalid data.")
        if not isinstance(item, dict):
            self.fail(f"Bitwarden item {name!r} has invalid data.")
        return item

    def create_secure_note(self, name: str, notes: str) -> None:
        if self.item(name, required=False) is not None:
            self.fail(f"Refusing to overwrite existing Bitwarden item: {name}")
        payload = json.dumps({"type": 2, "secureNote": {"type": 0}, "name": name, "notes": notes, "fields": []})
        encoded = self.run(("bw", "encode"), input_text=payload).stdout
        self.run(("bw", "create", "item"), input_text=encoded)
        if self.item(name, required=True) is None:
            self.fail(f"Bitwarden signing-key item {name!r} could not be validated.")