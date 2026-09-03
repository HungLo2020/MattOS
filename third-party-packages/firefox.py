#!/usr/bin/env python3
"""Build and publish Mozilla's official stable Firefox binary as a .deb."""

from __future__ import annotations

import json
import re
import shutil
import sys
from pathlib import Path
from urllib.request import Request, urlopen

sys.path.insert(0, str(Path(__file__).resolve().parent))
from common import BuildResult, PackageRecipe, RecipeError, command, download, extract_archive, require_tools, run_recipe, sha256_file, write_control, write_provenance

MOZILLA_FINGERPRINT = "14F26682D0916CDD81E37B6D61B7B526D98F0353"
VERSION_API = "https://product-details.mozilla.org/1.0/firefox_versions.json"
RELEASE_ROOT = "https://ftp.mozilla.org/pub/firefox/releases/{version}"


class FirefoxRecipe(PackageRecipe):
    name = "firefox"

    def discover_version(self) -> tuple[str, dict[str, str]]:
        with urlopen(Request(VERSION_API, headers={"User-Agent": "MattOS-third-party-packages/1"}), timeout=30) as response:
            data = json.load(response)
        version = str(data.get("LATEST_FIREFOX_VERSION", ""))
        if not re.fullmatch(r"[0-9]+(?:\.[0-9]+)+", version):
            raise RecipeError("Mozilla API did not return a stable Firefox version")
        return version, {"upstream": "https://ftp.mozilla.org/pub/firefox/releases", "release_api": VERSION_API, "channel": "stable", "signing_key_fingerprint": MOZILLA_FINGERPRINT}

    def dependency_names(self) -> tuple[str, ...]:
        # These are runtime ABI names from the official Mozilla binary. They
        # are deliberately declared, not bundled from the build host. The
        # MattOS repository must contain compatible providers before install.
        return ("libc6", "libgcc-s1", "libstdc++6", "libgtk-3-0", "libpango-1.0-0", "libgdk-pixbuf-2.0-0", "libglib2.0-0t64", "libcairo2", "libatk1.0-0", "libfontconfig1", "libfreetype6", "libdbus-1-3", "libasound2", "libx11-6", "libxcomposite1", "libxdamage1", "libxext6", "libxfixes3", "libxrandr2", "libxrender1", "libxcb1", "libxcb-shm0", "libx11-xcb1")

    def build(self, workspace: Path, version: str, provenance: dict[str, str]) -> BuildResult:
        require_tools(["curl", "tar", "gpg", "gpgv", "sha512sum", "dpkg-deb"])
        root = RELEASE_ROOT.format(version=version)
        filename = f"firefox-{version}.tar.xz"
        key = workspace / "KEY"
        sums = workspace / "SHA512SUMS"
        signature = workspace / "SHA512SUMS.asc"
        for name, destination in (("KEY", key), ("SHA512SUMS", sums), ("SHA512SUMS.asc", signature)):
            download(f"{root}/{name}", destination)
        keyring = workspace / "mozilla-release.gpg"
        command(["gpg", "--batch", "--yes", "--dearmor", "--output", str(keyring), str(key)])
        info = command(["gpg", "--batch", "--with-colons", "--import-options", "show-only", "--import", str(key)])
        fingerprints = [line.split(":")[9] for line in info.splitlines() if line.startswith("fpr:") and len(line.split(":")) > 9]
        if MOZILLA_FINGERPRINT not in fingerprints:
            raise RecipeError("Mozilla release KEY fingerprint did not match the pinned trust policy")
        command(["gpgv", "--keyring", str(keyring), str(signature), str(sums)])
        expected = None
        wanted = f"linux-x86_64/en-US/{filename}"
        for line in sums.read_text(encoding="utf-8").splitlines():
            fields = line.split()
            if len(fields) >= 2 and fields[1].lstrip("*") == wanted:
                expected = fields[0]
                break
        if not expected:
            raise RecipeError(f"Mozilla SHA512SUMS omitted {wanted}")
        archive = workspace / filename
        download(f"{root}/linux-x86_64/en-US/{filename}", archive)
        actual = sha256_file(archive)  # retained in provenance; SHA-512 is checked below
        check = command(["sha512sum", str(archive)]).split()[0]
        if check != expected:
            raise RecipeError(f"Firefox archive SHA-512 mismatch: expected {expected}, got {check}")
        source = extract_archive(archive, workspace / "source")
        staging = workspace / "package"
        opt = staging / "opt/firefox"
        opt.mkdir(parents=True)
        for item in source.iterdir():
            destination = opt / item.name
            if item.is_dir():
                shutil.copytree(item, destination, symlinks=True)
            else:
                shutil.copy2(item, destination, follow_symlinks=False)
        (staging / "usr/bin").mkdir(parents=True)
        (staging / "usr/bin/firefox").symlink_to("/opt/firefox/firefox")
        icon = source / "browser/chrome/icons/default/default128.png"
        icon_destination = staging / "usr/share/icons/hicolor/128x128/apps/firefox.png"
        icon_destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(icon, icon_destination)
        desktop = staging / "usr/share/applications/firefox.desktop"
        desktop.parent.mkdir(parents=True, exist_ok=True)
        desktop.write_text("[Desktop Entry]\nName=Firefox\nComment=Web Browser\nExec=firefox %u\nIcon=firefox\nTerminal=false\nType=Application\nCategories=Network;WebBrowser;\nMimeType=x-scheme-handler/http;x-scheme-handler/https;text/html;\n", encoding="utf-8")
        write_control(staging, name=self.name, version=version, description="Official Mozilla Firefox web browser", depends=self.dependency_names(), provides=["www-browser"])
        provenance = {**provenance, "release_url": root, "archive": filename, "archive_sha512": check, "archive_sha256": actual, "architecture": "x86_64"}
        write_provenance(staging, provenance)
        artifact = workspace / f"{self.name}_{version}_amd64.deb"
        command(["dpkg-deb", "--root-owner-group", "--build", str(staging), str(artifact)])
        return BuildResult(self.name, version, self.architecture, artifact, provenance)


if __name__ == "__main__":
    try:
        raise SystemExit(run_recipe(FirefoxRecipe(), sys.argv[1:], Path(__file__).resolve()))
    except RecipeError as exc:
        print(f"error: {exc}", file=sys.stderr)
        raise SystemExit(1)
