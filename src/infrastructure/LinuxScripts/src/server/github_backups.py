#!/usr/bin/env python3
"""Public GitHub mirrors and independently restorable, rotating tar.zst archives."""
from __future__ import annotations

# Edit these settings, then rerun --install on the machine running the backups.
GITHUB_USER = "HungLo2020"
from pathlib import Path
BACKUP_DESTINATION = Path("/srv/storage/OneDrive/Apps/Programming")
WORK_DIRECTORY = Path.home() / ".local/state/github-backups"

import argparse
import calendar
import contextlib
from datetime import datetime, timedelta, timezone
import fcntl
import hashlib
import json
import os
import pwd
import re
import shutil
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request

UTC = timezone.utc
ARCHIVE_TIME_FORMAT = "%Y-%m-%d_%H-%M-%S_UTC"
ARCHIVE_PATTERN = re.compile(r"^backup_(\d{4}-\d{2}-\d{2}_\d{2}-\d{2}-\d{2}_UTC|\d{8}T\d{12}Z)\.tar\.zst$")
UNIT = "github-public-backups"

# The GitHub job is an orchestration layer. Archive creation and checksum
# publication are delegated to the generic Zip Backup Manager engine next to
# this script, both in the repository and in the installed runtime directory.
BACKEND_DIRECTORY = Path(__file__).resolve().parent
if str(BACKEND_DIRECTORY) not in sys.path:
    sys.path.insert(0, str(BACKEND_DIRECTORY))
from zip_backups import ArchiveSpec, ZipBackupManager


def run(args, **kwargs):
    return subprocess.run([str(arg) for arg in args], check=True, **kwargs)


def atomic_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.write_text(json.dumps(value, indent=2) + "\n")
    temporary.replace(path)


def shift_months(date, months):
    year, month = divmod(date.year * 12 + date.month - 1 + months, 12)
    month += 1
    return date.replace(year=year, month=month, day=min(date.day, calendar.monthrange(year, month)[1]))


def retention_key(created, now):
    """Calendar-aligned slots, with age controlling the sampling density."""
    if created >= now - timedelta(days=1):
        return ("recent", created.isoformat())
    if created >= now - timedelta(days=7):
        return ("daily", created.date(), created.hour // 12)
    if created >= shift_months(now, -1):
        year, week, _ = created.isocalendar()
        seconds = created.weekday() * 86400 + created.hour * 3600 + created.minute * 60 + created.second
        return ("weekly", year, week, int(seconds // (7 * 86400 / 4)))
    if created >= shift_months(now, -12):
        days = calendar.monthrange(created.year, created.month)[1]
        seconds = (created.day - 1) * 86400 + created.hour * 3600 + created.minute * 60 + created.second
        return ("monthly", created.year, created.month, int(seconds // (days * 86400 / 4)))
    if created >= shift_months(now, -60):
        return ("yearly", created.year, (created.month - 1) // 3)
    # Five-year periods anchored at years divisible by five; two 30-month slots.
    return ("five-year", created.year // 5, ((created.year % 5) * 12 + created.month - 1) // 30)


def retained_archives(archives, now):
    selected = {}
    for created, path in sorted(archives):
        selected[retention_key(created, now)] = path
    keep = set(selected.values())
    if archives:
        keep.add(max(archives)[1])
    return keep


def prune(destination, now):
    archives = []
    for path in destination.glob("backup_*.tar.zst"):
        match = ARCHIVE_PATTERN.fullmatch(path.name)
        if match and path.is_file() and not path.is_symlink():
            try:
                time_format = ARCHIVE_TIME_FORMAT if match[1].endswith("_UTC") else "%Y%m%dT%H%M%S%fZ"
                created = datetime.strptime(match[1], time_format).replace(tzinfo=UTC)
                archives.append((created, path))
            except ValueError:
                continue
    keep = retained_archives(archives, now)
    ZipBackupManager().prune_paths(archives, keep)


class GitHub:
    """Anonymous API access, with pagination and bounded network retries."""
    def request(self, url, *, download=None):
        if urllib.parse.urlparse(url).scheme != "https":
            raise RuntimeError(f"Refusing non-HTTPS GitHub URL: {url}")
        for attempt in range(5):
            try:
                request = urllib.request.Request(url, headers={"User-Agent": "LinuxScripts-public-backups", "Accept": "application/vnd.github+json"})
                with urllib.request.urlopen(request, timeout=60) as response:
                    if download is not None:
                        with download.open("wb") as output:
                            shutil.copyfileobj(response, output, 1024 * 1024)
                        return None, response.headers
                    return json.load(response), response.headers
            except urllib.error.HTTPError as error:
                if error.code in (403, 429) and (error.headers.get("X-RateLimit-Remaining") == "0" or error.headers.get("Retry-After")):
                    delay = max(1, int(error.headers.get("Retry-After", "0")), int(error.headers.get("X-RateLimit-Reset", "0")) - int(time.time()) + 2)
                    print(f"GitHub rate limit: waiting {delay}s; existing backups are untouched.", flush=True)
                    while delay > 0:
                        pause = min(delay, 60)
                        time.sleep(pause)
                        delay -= pause
                    continue
                if error.code < 500 or attempt == 4:
                    raise
            except (OSError, TimeoutError):
                if attempt == 4:
                    raise
            time.sleep(2 ** attempt)
        raise RuntimeError(f"GitHub request could not complete: {url}")

    def pages(self, endpoint):
        url = "https://api.github.com" + endpoint + ("&" if "?" in endpoint else "?") + "per_page=100"
        items = []
        while url:
            page, headers = self.request(url)
            if not isinstance(page, list):
                raise RuntimeError(f"Expected a GitHub list: {url}")
            items.extend(page)
            links = re.findall(r'<([^>]+)>; rel="next"', headers.get("Link", ""))
            url = links[0] if links else None
            if url and urllib.parse.urlparse(url).netloc != "api.github.com":
                raise RuntimeError("Unexpected pagination host")
        return items


def git_environment():
    env = os.environ.copy()
    for name in list(env):
        if name.startswith("GIT_"):
            del env[name]
    env.update(GIT_TERMINAL_PROMPT="0", GIT_CONFIG_NOSYSTEM="1", GIT_CONFIG_GLOBAL=os.devnull, HOME="/nonexistent")
    return env


def update_mirror(url, mirror):
    prefix = ["git", "-c", "credential.helper=", "-c", "core.askPass=", "-c", "protocol.file.allow=never"]
    if mirror.exists():
        run([*prefix, "--git-dir", mirror, "fetch", "--prune", url, "+refs/*:refs/*"], env=git_environment(), timeout=21600)
    else:
        with tempfile.TemporaryDirectory(prefix="clone-", dir=mirror.parent) as directory:
            candidate = Path(directory) / "repository.git"
            run([*prefix, "clone", "--mirror", url, candidate], env=git_environment(), timeout=21600)
            candidate.replace(mirror)
    # A remote default branch can change without changing any branch contents.
    result = run([*prefix, "ls-remote", "--symref", url, "HEAD"], env=git_environment(), capture_output=True, text=True, timeout=120)
    for line in result.stdout.splitlines():
        if line.startswith("ref: ") and line.endswith("\tHEAD"):
            run(["git", "--git-dir", mirror, "symbolic-ref", "HEAD", line[5:].split("\t")[0]], env=git_environment())
    run(["git", "--git-dir", mirror, "fsck", "--full"], env=git_environment(), stdout=subprocess.DEVNULL, timeout=21600)


def update_metadata(api, owner, repo, directory):
    """Stage a complete metadata snapshot before replacing the previous one."""
    endpoint = f"/repos/{owner}/{repo['name']}"
    with tempfile.TemporaryDirectory(prefix="metadata-", dir=directory) as temporary:
        staged = Path(temporary)
        atomic_json(staged / "repository.json", repo)
        issues = api.pages(endpoint + "/issues?state=all") if repo.get("has_issues", True) else []
        comments = api.pages(endpoint + "/issues/comments") if repo.get("has_issues", True) else []
        atomic_json(staged / "issues.json", issues)
        atomic_json(staged / "issue-comments.json", comments)
        releases = api.pages(endpoint + "/releases")
        assets_root = directory / "release-assets"
        assets_root.mkdir(exist_ok=True)
        manifest = []
        for release in releases:
            assets = api.pages(endpoint + f"/releases/{release['id']}/assets")
            release["assets"] = assets
            for asset in assets:
                if Path(asset["name"]).name != asset["name"] or asset["name"] in (".", ".."):
                    raise RuntimeError("Unsafe release asset filename")
                identity = hashlib.sha256(json.dumps([asset["id"], asset["updated_at"], asset["size"], asset.get("digest")]).encode()).hexdigest()
                relative = Path(identity) / asset["name"]
                target = assets_root / relative
                if not target.is_file():
                    target.parent.mkdir(exist_ok=True)
                    partial = target.with_name(target.name + ".partial")
                    try:
                        api.request(asset["browser_download_url"], download=partial)
                        if partial.stat().st_size != asset["size"]:
                            raise RuntimeError(f"Incomplete release asset: {asset['name']}")
                        digest = asset.get("digest")
                        if digest and digest.startswith("sha256:") and file_digest(partial) != digest[7:]:
                            raise RuntimeError(f"Release checksum mismatch: {asset['name']}")
                        partial.replace(target)
                    finally:
                        partial.unlink(missing_ok=True)
                manifest.append({"release_id": release["id"], "asset": asset, "path": str(relative)})
        atomic_json(staged / "releases.json", releases)
        atomic_json(staged / "assets.json", manifest)
        # Serialized under the job lock. A failed collection leaves old metadata intact.
        current = directory / "metadata"
        previous = directory / "metadata.previous"
        if previous.exists():
            shutil.rmtree(previous)
        if current.exists():
            current.rename(previous)
        staged.rename(current)
        if previous.exists():
            shutil.rmtree(previous)


def file_digest(path):
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def create_archive(directory, destination, now):
    name = "backup_" + now.astimezone(UTC).strftime(ARCHIVE_TIME_FORMAT) + ".tar.zst"
    manager = ZipBackupManager()
    return manager.create_tar_zst(
        ArchiveSpec(
            source_root=directory,
            members=("repository.git", "metadata", "release-assets"),
            destination=destination,
            prefix="backup",
            timestamp_format=ARCHIVE_TIME_FORMAT,
            checksum_name=name,
        ),
        now=now,
    )


@contextlib.contextmanager
def job_lock(work):
    work.mkdir(parents=True, exist_ok=True)
    with (work / "job.lock").open("a") as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            raise RuntimeError("Another GitHub backup or installation is running") from None
        yield


def backup():
    validate_settings()
    for executable in ("git", "tar", "zstd"):
        if not shutil.which(executable):
            raise RuntimeError(f"Install required command: {executable}")
    with job_lock(WORK_DIRECTORY):
        api = GitHub()
        # Complete discovery before acting; never delete repos missing from this list.
        repos = api.pages(f"/users/{GITHUB_USER}/repos?type=owner")
        failed = []
        for repo in repos:
            if repo.get("private") is not False or repo["owner"]["login"].lower() != GITHUB_USER.lower():
                continue
            if not re.fullmatch(r"[A-Za-z0-9_.-]+", repo["name"]) or repo["name"] in (".", ".."):
                raise RuntimeError("Unsafe repository name")
            directory = WORK_DIRECTORY / GITHUB_USER / str(int(repo["id"]))
            directory.mkdir(parents=True, exist_ok=True)
            try:
                print(f"Backing up {GITHUB_USER}/{repo['name']}", flush=True)
                update_mirror(f"https://github.com/{GITHUB_USER}/{repo['name']}.git", directory / "repository.git")
                update_metadata(api, GITHUB_USER, repo, directory)
                now = datetime.now(UTC)
                atomic_json(directory / "metadata/backup.json", {"completed_at": now.isoformat(), "scope": "Git mirror, public issues and issue comments, releases and uploaded assets; no LFS payloads or submodule repositories"})
                destination = BACKUP_DESTINATION / GITHUB_USER / repo["name"]
                create_archive(directory, destination, now)
                prune(destination, now)
            except (OSError, ValueError, RuntimeError, subprocess.SubprocessError) as error:
                failed.append(repo["name"])
                print(f"FAILED {repo['name']}: {error}", file=sys.stderr, flush=True)
        if failed:
            raise RuntimeError("Backup incomplete for: " + ", ".join(failed))


def validate_settings():
    if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9-]*", GITHUB_USER):
        raise RuntimeError("Invalid GitHub account name")
    for path in (WORK_DIRECTORY, BACKUP_DESTINATION):
        if not path.is_absolute():
            raise RuntimeError("Backup and working directories must be absolute paths")
    if BACKUP_DESTINATION == WORK_DIRECTORY or WORK_DIRECTORY in BACKUP_DESTINATION.parents or BACKUP_DESTINATION in WORK_DIRECTORY.parents:
        raise RuntimeError("Working mirrors and archive destinations must be separate directories")


def unit_contents(user, script):
    escaped = str(script).replace('\\', '\\\\').replace('"', '\\"').replace('%', '%%')
    service = f'''[Unit]
Description=Public GitHub repository backups
Wants=network-online.target
After=network-online.target

[Service]
Type=oneshot
User={user}
UMask=0077
ExecStart=/usr/bin/python3 "{escaped}" --run
TimeoutStartSec=infinity
'''
    timer = f'''[Unit]
Description=Back up public GitHub repositories every six hours

[Timer]
OnCalendar=*-*-* 00,06,12,18:00:00
Persistent=true
Unit={UNIT}.service

[Install]
WantedBy=timers.target
'''
    return service, timer


def install():
    validate_settings()
    if os.geteuid() == 0:
        raise RuntimeError("Run --install as the backup user, not root; sudo is used only for systemd setup")
    user = pwd.getpwuid(os.getuid()).pw_name
    installed = Path.home() / ".local/lib/github-backups/GitHubBackups.py"
    # Stop the old job before replacing its script or changing its destination.
    for suffix in ("timer", "service"):
        unit = UNIT + "." + suffix
        state = run(["systemctl", "show", unit, "--property=LoadState", "--value"], capture_output=True, text=True).stdout.strip()
        if state != "not-found":
            run(["sudo", "systemctl", "stop", unit])
    with job_lock(WORK_DIRECTORY):
        installed.parent.mkdir(parents=True, exist_ok=True)
        temporary = installed.with_suffix(".tmp")
        temporary.write_bytes(Path(__file__).read_bytes())
        temporary.replace(installed)
        backend = installed.with_name("zip_backups.py")
        backend_temporary = backend.with_suffix(".tmp")
        backend_temporary.write_bytes(BACKEND_DIRECTORY.joinpath("zip_backups.py").read_bytes())
        backend_temporary.replace(backend)
        service, timer = unit_contents(user, installed)
        with tempfile.TemporaryDirectory() as directory:
            for suffix, contents in (("service", service), ("timer", timer)):
                path = Path(directory) / (UNIT + "." + suffix)
                path.write_text(contents)
                run(["sudo", "install", "-m", "0644", path, "/etc/systemd/system/" + path.name])
        run(["sudo", "systemctl", "daemon-reload"])
    run(["sudo", "systemctl", "enable", "--now", UNIT + ".timer"])
    print(f"Installed six-hour timer for {user}; archive destination: {BACKUP_DESTINATION / GITHUB_USER}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    action = parser.add_mutually_exclusive_group(required=True)
    action.add_argument("--install", action="store_true", help="install/update this user's systemd job (uses sudo)")
    action.add_argument("--run", action="store_true", help="perform one backup pass")
    args = parser.parse_args()
    try:
        install() if args.install else backup()
        return 0
    except (OSError, ValueError, RuntimeError, subprocess.SubprocessError) as error:
        print(f"Error: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
