"""Opt-in enrollment in the public MattPackages APT repository."""

from __future__ import annotations

import hashlib
import os
from pathlib import Path
import subprocess
import tempfile
import uuid

from host import detect_host
from system import PackageManager, detect_package_manager


REPOSITORY_URL = "https://mattpackages.mattsherfey.com"
KEY_SHA256 = "b2c58c61e3b2e9c83b83a2b3da331aeb70a54d1bce44e84cba735b80671079b2"
BUNDLED_KEY = Path(__file__).resolve().parents[1] / "resources/keys/mattpackages-archive-keyring.asc"
APT_DIRECTORY = Path("/etc/apt")
KEY_PATH = APT_DIRECTORY / "keyrings/mattpackages-archive-keyring.asc"
SOURCE_PATH = APT_DIRECTORY / "sources.list.d/mattpackages.sources"


def eligibility_problem() -> str | None:
    """Return a reason to skip, without changing the host."""

    if detect_host().system != "linux" or detect_package_manager() is not PackageManager.APT:
        return "MattPackages requires Linux with APT."
    architecture = subprocess.run(
        ("dpkg", "--print-architecture"), check=True, capture_output=True, text=True
    ).stdout.strip()
    if architecture != "amd64":
        return f"MattPackages currently supports amd64; this machine uses {architecture}."
    return None


def source_contents() -> bytes:
    return (
        "# Managed by LinuxScripts interactive Setup.\n"
        "Types: deb\n"
        f"URIs: {REPOSITORY_URL}\n"
        "Suites: stable\n"
        "Components: main\n"
        "Architectures: amd64\n"
        f"Signed-By: {KEY_PATH}\n"
        "Enabled: yes\n"
    ).encode("utf-8")


def check_existing_sources() -> None:
    """Do not overwrite foreign configuration or create duplicate entries."""

    for path in (KEY_PATH, SOURCE_PATH):
        if path.is_symlink():
            raise RuntimeError(f"{path} is a symbolic link; resolve it before enabling MattPackages.")
    candidates = [APT_DIRECTORY / "sources.list"]
    directory = APT_DIRECTORY / "sources.list.d"
    candidates.extend(sorted(directory.glob("*.list")))
    candidates.extend(sorted(directory.glob("*.sources")))
    for path in candidates:
        if not path.is_file():
            continue
        contents = path.read_text(encoding="utf-8")
        if path == SOURCE_PATH:
            if contents and not contents.startswith("# Managed by LinuxScripts interactive Setup.\n"):
                raise RuntimeError(f"{path} already exists and is not managed by LinuxScripts; reconcile it first.")
            continue
        active_lines = "\n".join(line.split("#", 1)[0] for line in contents.splitlines())
        if "mattpackages.mattsherfey.com" in active_lines.lower():
            raise RuntimeError(
                f"MattPackages is already mentioned in {path}. Reconcile that entry before "
                f"letting Setup manage {SOURCE_PATH}; no repository files were changed."
            )


def install_file(destination: Path, contents: bytes) -> bool:
    """Atomically replace each managed file, leaving correct files untouched."""

    if destination.is_file() and destination.read_bytes() == contents:
        status = destination.stat()
        if status.st_mode & 0o777 == 0o644 and status.st_uid == 0 and status.st_gid == 0:
            return False
    prefix = () if os.geteuid() == 0 else ("sudo",)
    staged = destination.with_name(f".{destination.name}.{uuid.uuid4().hex}.tmp")
    with tempfile.TemporaryDirectory(prefix="mattpackages-") as temporary:
        local = Path(temporary) / "contents"
        local.write_bytes(contents)
        subprocess.run((*prefix, "install", "-d", "-m", "0755", str(destination.parent)), check=True)
        try:
            subprocess.run((*prefix, "install", "-o", "root", "-g", "root", "-m", "0644", str(local), str(staged)), check=True)
            subprocess.run((*prefix, "mv", "-fT", str(staged), str(destination)), check=True)
        finally:
            subprocess.run((*prefix, "rm", "-f", str(staged)), check=True)
    return True


def configure_repository() -> None:
    """Ensure the source and key are installed, then verify through APT."""

    problem = eligibility_problem()
    if problem:
        raise RuntimeError(problem)
    check_existing_sources()
    key = BUNDLED_KEY.read_bytes()
    if hashlib.sha256(key).hexdigest() != KEY_SHA256:
        raise RuntimeError("The bundled MattPackages public key is missing or modified; restore it from LinuxScripts.")
    install_file(KEY_PATH, key)
    install_file(SOURCE_PATH, source_contents())
    prefix = () if os.geteuid() == 0 else ("sudo",)
    try:
        subprocess.run((
            *prefix, "apt-get", "update",
            "-o", f"Dir::Etc::sourcelist={SOURCE_PATH}",
            "-o", "Dir::Etc::sourceparts=-",
            "-o", "APT::Get::List-Cleanup=0",
            "-o", "APT::Update::Error-Mode=any",
            "-o", "Acquire::http::Timeout=30",
            "-o", "Acquire::https::Timeout=30",
        ), check=True)
    except subprocess.CalledProcessError as error:
        raise RuntimeError(
            "MattPackages source and key were installed, but APT could not refresh the repository. "
            "Review the APT error above and rerun Setup; package installation has not started."
        ) from error
    print("MattPackages is enabled and its package index was refreshed.")
