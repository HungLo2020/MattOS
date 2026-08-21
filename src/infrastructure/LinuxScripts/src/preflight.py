"""Interactive Linux/MattOS operator and repository preparation."""

from __future__ import annotations

import grp
import os
import pwd
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path


OPERATOR_NAME = "matt"


def prompt_yes_no(question: str) -> bool:
    """Return True only after an explicit affirmative answer."""

    try:
        return input(f"{question} [y/N]: ").strip().lower() in {"y", "yes"}
    except EOFError:
        return False


def run_privileged(*arguments: str) -> None:
    """Run one explicit elevated command without shell interpolation."""

    command = arguments if os.geteuid() == 0 else ("sudo", *arguments)
    subprocess.run(command, check=True)


@dataclass(frozen=True)
class OperatorAccount:
    """Resolved state for the non-root account used by server workloads."""

    name: str
    home: Path
    exists: bool
    has_sudo: bool


def account_state(name: str = OPERATOR_NAME) -> OperatorAccount:
    """Return the account's home path and supplementary sudo-group state."""

    try:
        entry = pwd.getpwnam(name)
    except KeyError:
        return OperatorAccount(name, Path("/home") / name, False, False)
    sudo_group = grp.getgrnam("sudo")
    has_sudo = entry.pw_gid == sudo_group.gr_gid or name in sudo_group.gr_mem
    return OperatorAccount(name, Path(entry.pw_dir), True, has_sudo)


def ensure_operator_account(name: str = OPERATOR_NAME) -> OperatorAccount:
    """Offer the legacy account creation, home repair, and sudo enrollment steps."""

    account = account_state(name)
    if not account.exists:
        if not prompt_yes_no(f"Create the '{name}' operator account with a home directory?"):
            print(f"Skipping creation of '{name}'.")
            return account
        run_privileged("useradd", "-m", "-s", "/bin/bash", name)
        print(f"Set a password for '{name}' now.")
        run_privileged("passwd", name)
        account = account_state(name)

    if account.exists and not account.home.is_dir():
        if prompt_yes_no(f"Create the missing home directory {account.home} for '{name}'?"):
            run_privileged("mkdir", "-p", str(account.home))
            run_privileged("chown", f"{name}:{name}", str(account.home))
        else:
            print(f"Leaving missing home directory unchanged: {account.home}")

    account = account_state(name)
    if account.exists and not account.has_sudo:
        if prompt_yes_no(f"Add '{name}' to the sudo group?"):
            run_privileged("usermod", "-aG", "sudo", name)
        else:
            print(f"Leaving '{name}' without sudo-group membership.")
    return account_state(name)


def canonical_repository_path(account: OperatorAccount, repository_root: Path) -> Path:
    """Return the legacy operator-owned checkout location for this repository."""

    return account.home / "Documents" / "Repos" / repository_root.name


def is_git_repository(path: Path) -> bool:
    """Return whether a path is a Git working tree without running Git."""

    return (path / ".git").exists()


def offer_repository_relocation(repository_root: Path, account: OperatorAccount) -> Path | None:
    """Offer an explicit, guarded copy to the legacy operator repository path."""

    if not account.exists or not account.home.is_dir():
        print(f"Repository relocation requires an existing home directory for '{account.name}'.")
        return None

    source = repository_root.resolve()
    destination = canonical_repository_path(account, source)
    if destination.exists() and source == destination.resolve():
        print(f"Repository is already at the operator location: {destination}")
        return None
    if destination.exists():
        if is_git_repository(destination):
            print(f"Not relocating: another Git repository already exists at {destination}.")
        else:
            print(f"Not relocating: destination exists and is not a Git repository: {destination}")
        return None

    question = f"Copy this repository to {destination} and make the copy owned by '{account.name}'?"
    if not prompt_yes_no(question):
        print("Skipping repository relocation.")
        return None
    run_privileged("mkdir", "-p", str(destination.parent))
    run_privileged("cp", "-a", str(source), str(destination))
    run_privileged("chown", "-R", f"{account.name}:{account.name}", str(destination))
    print(f"Repository copied to {destination} and assigned to '{account.name}'.")
    return destination


def relaunch_setup_as_operator(repository_root: Path, account: OperatorAccount) -> None:
    """Continue from an approved relocated checkout as the operator account."""

    setup_script = repository_root / "Tools" / "Setup.py"
    if not setup_script.is_file():
        raise RuntimeError(f"Relocated Setup entry point is missing: {setup_script}")
    operator_uid = pwd.getpwnam(account.name).pw_uid
    command = (sys.executable, str(setup_script))
    if os.geteuid() == operator_uid:
        os.execv(sys.executable, command)
    os.execvp("sudo", ("sudo", "-H", "-u", account.name, *command))


def run(repository_root: Path) -> None:
    """Run the interactive preflight for the Linux/MattOS setup path."""

    if not repository_root.is_dir():
        raise RuntimeError(f"Repository root does not exist: {repository_root}")
    print("LinuxScripts preflight")
    print("=" * 20)
    account = ensure_operator_account()
    relocated = offer_repository_relocation(repository_root, account)
    if relocated is not None:
        print(f"Continuing Setup from {relocated} as '{account.name}'.")
        relaunch_setup_as_operator(relocated, account)