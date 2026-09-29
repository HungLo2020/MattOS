#!/usr/bin/env python3
"""Report whether pinned MattOS source components have upstream updates.

This is intentionally read-only with respect to the MattOS checkout. Git
history needed for commit-distance calculation is fetched into temporary bare
repositories, never into a component source directory or the repository's
Git directory.
"""

from __future__ import annotations

import argparse
from concurrent.futures import ThreadPoolExecutor, as_completed
from dataclasses import dataclass, replace
from collections import Counter
from datetime import datetime, timezone
from functools import lru_cache
import json
import os
import re
import signal
import subprocess
import tempfile
import sys
import time
from pathlib import Path
from urllib.parse import quote, urlsplit
from urllib.request import Request, urlopen

import tomllib


ROOT = Path(__file__).resolve().parents[1]
SOURCES = ROOT / "upstream" / "sources.toml"


@dataclass(frozen=True)
class Component:
    name: str
    repository: str
    ref: str
    revision: str
    source_path: str


@dataclass(frozen=True)
class UpdateResult:
    component: Component
    status: str
    behind: int | None
    current: str
    latest: str | None
    detail: str | None = None


def load_components(path: Path = SOURCES) -> list[Component]:
    with path.open("rb") as stream:
        document = tomllib.load(stream)

    components: list[Component] = []
    for item in document.get("component", []):
        if item.get("sync") != "copy":
            continue
        fields = ("name", "repo", "branch", "revision", "path")
        missing = [field for field in fields if not item.get(field)]
        if missing:
            raise ValueError(
                f"component entry is missing {', '.join(missing)}: {item!r}"
            )
        revision = str(item["revision"])
        if len(revision) != 40 or any(char not in "0123456789abcdef" for char in revision.lower()):
            raise ValueError(f"{item['name']}: revision is not a 40-hex commit: {revision}")
        components.append(
            Component(
                name=str(item["name"]),
                repository=str(item["repo"]),
                ref=str(item["branch"]),
                revision=revision,
                source_path=str(item["path"]),
            )
        )
    return sorted(components, key=lambda component: component.name)


def git(
    command: list[str], *, cwd: Path | None = None, timeout: float
) -> subprocess.CompletedProcess[str]:
    process = subprocess.Popen(
        ["git", *command],
        cwd=cwd,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        env={**os.environ, "GIT_OPTIONAL_LOCKS": "0", "GIT_TERMINAL_PROMPT": "0"},
        start_new_session=os.name == "posix",
    )
    try:
        stdout, stderr = process.communicate(timeout=timeout)
    except subprocess.TimeoutExpired:
        # Kill transport helpers too; otherwise their inherited pipes can keep
        # communicate() blocked even after the top-level git process is killed.
        if os.name == "posix":
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
        else:
            process.kill()
        process.communicate()
        raise
    return subprocess.CompletedProcess(process.args, process.returncode, stdout, stderr)



def list_remote_refs(repository: str, patterns: list[str], timeout: float) -> subprocess.CompletedProcess[str]:
    """Retry one transient transport failure; never reuse a stale remote result."""
    for attempt in range(2):
        try:
            result = git(["ls-remote", transport_url(repository), *patterns], timeout=timeout)
            if result.returncode == 0 or attempt:
                return result
        except subprocess.TimeoutExpired:
            if attempt:
                raise
    raise RuntimeError("upstream ref listing failed")

def transport_url(repository: str) -> str:
    """Same upstream identity, using HTTPS instead of unauthenticated git://."""
    return "https://" + repository[6:] if repository.startswith("git://") else repository


def remote_tip(component: Component, timeout: float) -> str:
    completed = git(
        ["ls-remote", "--exit-code", transport_url(component.repository),
         *ref_patterns(component.ref)],
        timeout=timeout,
    )
    if completed.returncode != 0:
        detail = completed.stderr.strip() or completed.stdout.strip() or "ref not found"
        raise RuntimeError(f"cannot resolve {component.ref!r}: {detail}")
    return resolve_ref(parse_refs(completed.stdout), component.ref)


def ref_patterns(ref: str) -> list[str]:
    if ref.startswith("refs/"):
        return [ref, ref + "^{}"]
    return [f"refs/heads/{ref}", f"refs/tags/{ref}", f"refs/tags/{ref}^{{}}"]


def parse_refs(output: str) -> dict[str, str]:
    refs = {}
    for line in output.splitlines():
        fields = line.split()
        if len(fields) == 2 and re.fullmatch(r"[0-9a-fA-F]{40}", fields[0]):
            refs[fields[1]] = fields[0].lower()
    return refs


def resolve_ref(refs: dict[str, str], ref: str) -> str:
    names = [ref] if ref.startswith("refs/") else [f"refs/heads/{ref}", f"refs/tags/{ref}"]
    matches = [refs.get(name + "^{}", refs[name]) for name in names if name in refs]
    if len(matches) != 1:
        raise RuntimeError(f"ref {ref!r} is missing or ambiguous; use an explicit refs/heads/ or refs/tags/ name")
    return matches[0]


def commit_distance(
    component: Component, latest: str, timeout: float
) -> tuple[str, int | None, str | None]:
    with tempfile.TemporaryDirectory(prefix="mattos-update-check-") as temporary:
        repository = Path(temporary)
        initialized = git(["init", "--bare", "-q"], cwd=repository, timeout=timeout)
        if initialized.returncode != 0:
            raise RuntimeError(initialized.stderr.strip() or "git init failed")

        # Exact mode is deliberately opt-in. Start shallow and deepen only as
        # needed; most recently-pinned components are close to their ref tip.
        fetch = git(
            [
                "fetch",
                "--filter=blob:none",
                "--depth=64",
                "--no-tags",
                "--quiet",
                transport_url(component.repository),
                component.ref,
            ],
            cwd=repository,
            timeout=timeout,
        )
        if fetch.returncode != 0:
            fetch = git(
                ["fetch", "--depth=64", "--no-tags", "--quiet", transport_url(component.repository), component.ref],
                cwd=repository,
                timeout=timeout,
            )
        if fetch.returncode != 0:
            raise RuntimeError(fetch.stderr.strip() or "unable to fetch upstream ref")

        pinned_exists = git(
            ["cat-file", "-e", f"{component.revision}^{{commit}}"],
            cwd=repository,
            timeout=timeout,
        )
        if pinned_exists.returncode != 0:
            depth = 128
            while pinned_exists.returncode != 0 and depth <= 8192:
                deepened = git(
                    [
                        "fetch",
                        "--deepen",
                        str(depth),
                        "--filter=blob:none",
                        "--no-tags",
                        "--quiet",
                        transport_url(component.repository),
                        component.ref,
                    ],
                    cwd=repository,
                    timeout=timeout,
                )
                if deepened.returncode != 0:
                    break
                pinned_exists = git(
                    ["cat-file", "-e", f"{component.revision}^{{commit}}"],
                    cwd=repository,
                    timeout=timeout,
                )
                depth *= 2
            if pinned_exists.returncode != 0:
                unshallow = git(
                    ["fetch", "--unshallow", "--no-tags", "--quiet", transport_url(component.repository), component.ref],
                    cwd=repository,
                    timeout=timeout,
                )
                if unshallow.returncode != 0:
                    raise RuntimeError(unshallow.stderr.strip() or "pinned commit is unavailable upstream")
                pinned_exists = git(
                    ["cat-file", "-e", f"{component.revision}^{{commit}}"],
                    cwd=repository,
                    timeout=timeout,
                )
                if pinned_exists.returncode != 0:
                    raise RuntimeError("pinned commit is unavailable in upstream history")

        if component.revision == latest:
            return "up-to-date", 0, None

        pinned_ancestor = git(
            ["merge-base", "--is-ancestor", component.revision, latest],
            cwd=repository,
            timeout=timeout,
        )
        if pinned_ancestor.returncode not in (0, 1):
            raise RuntimeError(pinned_ancestor.stderr.strip() or "unable to determine ancestry")
        if pinned_ancestor.returncode == 0:
            count = git(
                ["rev-list", "--count", f"{component.revision}..{latest}"],
                cwd=repository,
                timeout=timeout,
            )
            if count.returncode != 0:
                raise RuntimeError(count.stderr.strip() or "unable to count commits")
            return "behind", int(count.stdout.strip()), None

        latest_ancestor = git(
            ["merge-base", "--is-ancestor", latest, component.revision],
            cwd=repository,
            timeout=timeout,
        )
        if latest_ancestor.returncode == 0:
            count = git(
                ["rev-list", "--count", f"{latest}..{component.revision}"],
                cwd=repository,
                timeout=timeout,
            )
            if count.returncode != 0:
                raise RuntimeError(count.stderr.strip() or "unable to count commits")
            return "local-ahead", 0, f"local revision is {count.stdout.strip()} commit(s) ahead"

        return "diverged", None, "pinned revision is not an ancestor of the upstream ref"


def inspect(component: Component, *, exact: bool, timeout: float) -> UpdateResult:
    current = component.revision
    try:
        latest = remote_tip(component, timeout)
        if latest == current:
            return UpdateResult(component, "up-to-date", 0, current, latest)
        if not exact:
            return UpdateResult(
                component,
                "update-available",
                None,
                current,
                latest,
                "exact distance omitted; rerun with --exact",
            )
        status, behind, detail = commit_distance(component, latest, timeout)
        return UpdateResult(component, status, behind, current, latest, detail)
    except Exception as error:  # report one unavailable remote without hiding other components
        return UpdateResult(component, "error", None, current, None, str(error))


def format_result(result: UpdateResult) -> str:
    component = result.component
    current = result.current[:12]
    latest = result.latest[:12] if result.latest else "unknown"
    if result.status == "behind":
        distance = f"behind {result.behind} commit(s)"
    elif result.status == "up-to-date":
        distance = "up to date"
    elif result.status == "update-available":
        distance = "update available"
    elif result.status == "local-ahead":
        distance = "local revision is ahead"
    else:
        distance = result.status
    suffix = f" ({result.detail})" if result.detail else ""
    return f"package {component.name}: {distance}, selected commit {current}, configured ref {component.ref} resolves to {latest}{suffix}"


@dataclass(frozen=True)
class Release:
    tag: str
    commit: str
    family: str
    numbers: tuple[int, ...]
    phase: int  # dev < alpha < beta < pre < rc < final
    serial: int

    @property
    def key(self) -> tuple[tuple[int, ...], int, int]:
        return (self.numbers + (0,) * max(0, 6 - len(self.numbers)), self.phase, self.serial)


def parse_release(tag: str, commit: str = "") -> Release | None:
    """Conservative tag parsing; reject unknown suffixes rather than guess stability.

    Keep the original tag for display, and a family for excluding unrelated tags
    in monorepos. A tag is evidence of a version, not of support or security status.
    """
    match = re.fullmatch(r"([^0-9]*)([0-9]+(?:[._-][0-9]+)*)(.*)", tag)
    if not match:
        return None
    prefix, numeric, suffix = match.groups()
    numbers = tuple(int(part) for part in re.split(r"[._-]", numeric))
    phase, serial = 5, 0
    if suffix.lower() in ("", "-release", "_release", "-final"):
        pass
    elif re.fullmatch(r"[_-][pP][0-9]+", suffix):  # OpenSSH portable patches
        numbers += (int(suffix[2:]),)
    elif re.fullmatch(r"[a-z]", suffix) and len(numeric) == 4 and numeric.startswith("20"):
        numbers += (ord(suffix) - ord("a") + 1,)  # tzdata: 2026a, 2026b
    else:
        pre = re.fullmatch(r"[-._]?(dev|alpha|beta|pre|rc|a|b)(?:[-._]?([0-9]+))?", suffix, re.I)
        if not pre:
            return None
        phase = {"dev": 0, "alpha": 1, "a": 1, "beta": 2, "b": 2, "pre": 3, "rc": 4}[pre[1].lower()]
        serial = int(pre[2] or 0)
    return Release(tag, commit, prefix.lower(), numbers, phase, serial)


def release_tags(refs: dict[str, str]) -> list[Release]:
    releases = []
    for ref, sha in refs.items():
        if not ref.startswith("refs/tags/") or ref.endswith("^{}"):
            continue
        release = parse_release(ref.removeprefix("refs/tags/"), refs.get(ref + "^{}", sha))
        if release:
            releases.append(release)
    return releases


def version_scheme(component: Component, release: Release) -> str:
    # Repositories can retain older calendar tags after switching to semantic
    # versions (e.g. SELinux/libva), or KDE Gear tags after joining Plasma/KF.
    if release.numbers[0] >= 1900:
        return "calendar"
    if component.source_path.startswith("src/desktop/kde/") and release.numbers[0] >= 15:
        return "kde-gear"
    return "version"


def apply_release_policy(component: Component, release: Release) -> Release:
    """Recognize documented numeric prereleases as well as textual suffixes.

    https://develop.kde.org/docs/getting-started/add-project/release/
    https://gstreamer.freedesktop.org/documentation/frequently-asked-questions/developing.html
    https://github.com/flatpak/flatpak-builder#versioning-policy
    https://lists.x.org/archives/xorg-announce/2026-September/003742.html
    https://sourceware.org/glibc/manual/2.40/html_node/Dynamic-Linker-Diagnostics-Values.html
    Other upstream schemes still require the skill's announcement verification.
    """
    numbers = release.numbers
    numeric_pre = (
        component.source_path.startswith("src/desktop/kde/") and len(numbers) >= 3 and numbers[2] >= 70
        or component.name in {"flatpak", "gstreamer"} and len(numbers) >= 2 and numbers[1] % 2 == 1
        or component.name == "glibc" and len(numbers) >= 3 and numbers[2] == 9000
        or component.name == "xwayland" and len(numbers) >= 3 and numbers[2] >= 99
    )
    return replace(release, phase=3) if numeric_pre and release.phase == 5 else release


def release_url(repository: str, tag: str) -> str:
    base = repository.removesuffix(".git").rstrip("/")
    host = urlsplit(base).hostname or ""
    if host == "github.com":
        return f"{base}/releases/tag/{quote(tag, safe='')}"
    if "gitlab" in host or host == "invent.kde.org":
        return f"{base}/-/tags/{quote(tag, safe='')}"
    # The declared upstream repository is always evidence; do not invent a
    # forge-specific tag URL for cgit, gitweb, or other hosting systems.
    return repository


def release_record(release: Release | None, repository: str) -> dict | None:
    if release is None:
        return None
    return {"tag": release.tag, "commit": release.commit,
            "prerelease": release.phase < 5, "url": release_url(repository, release.tag)}


def metadata_state(root: Path, component: Component) -> dict:
    """Check recorded metadata only. This is NOT a physical-tree fidelity audit."""
    try:
        state = tomllib.loads((root / "upstream/state" / f"{component.name}.toml").read_text())
        commit = state.get("imported_commit")
        if not isinstance(commit, str) or not re.fullmatch(r"[0-9a-f]{40}", commit):
            raise ValueError("missing valid imported_commit")
        expected = {"component": component.name, "repo": component.repository,
                    "destination_path": component.source_path, "imported_commit": component.revision}
        mismatches = [key for key, value in expected.items() if state.get(key) != value]
        return {"status": "mismatch" if mismatches else "metadata-match",
                "imported_commit": commit, "detail": ", ".join(mismatches) or None}
    except (OSError, ValueError) as exc:
        return {"status": "error", "imported_commit": None, "detail": str(exc)}


def declared_source_version(root: Path, component: Component) -> str | None:
    """Read version declarations without running any vendored build code.

    These are labels only: an untagged development commit can already declare
    the next release number. Missing files in sparse checkouts stay unknown.
    """
    source = root / component.source_path
    for name in ("Makefile", "Cargo.toml", "meson.build", "configure.ac", "CMakeLists.txt", "VERSION"):
        try:
            content = (source / name).read_text()
            version = version_label(content, name, component.name)
            if version:
                return version
        except (OSError, ValueError, KeyError):
            pass
    return None


def version_label(content: str, filename: str, component: str) -> str | None:
    if filename == "Makefile" and component == "linux":
        values = dict(re.findall(r"^(VERSION|PATCHLEVEL|SUBLEVEL|EXTRAVERSION)[ \t]*=[ \t]*([^\n]*)", content, re.M))
        return ".".join(values[k].strip() for k in ("VERSION", "PATCHLEVEL", "SUBLEVEL")) + values.get("EXTRAVERSION", "").strip()
    if filename == "Cargo.toml":
        document = tomllib.loads(content)
        value = document.get("package", {}).get("version")
        if value is None or isinstance(value, dict) and value.get("workspace"):
            value = document.get("workspace", {}).get("package", {}).get("version")
        return value if isinstance(value, str) else None
    patterns = {
        "meson.build": r"(?s)project\s*\([^)]*?\bversion\s*:\s*['\"]([0-9][^'\"]*)['\"]",
        "configure.ac": r"AC_INIT\s*\(\s*\[?[^,]+,\s*\[?([0-9][0-9a-zA-Z.+_-]*)",
        "CMakeLists.txt": r"(?is)project\s*\([^)]*?\bVERSION\s+([0-9][0-9a-zA-Z.+_-]*)",
        "VERSION": r"^\s*([0-9][0-9a-zA-Z.+_-]*)\s*$",
    }
    match = re.search(patterns[filename], content) if filename in patterns else None
    return match[1] if match else None


def snapshot_label(component: Component, timeout: float) -> dict | None:
    """Sparse-checkout fallback: read labels at the immutable upstream pin."""
    parsed = urlsplit(component.repository)
    if parsed.hostname != "github.com":
        return None
    project = parsed.path.strip("/").removesuffix(".git")
    files = ("Makefile",) if component.name == "linux" else ("Cargo.toml", "meson.build", "configure.ac")
    for filename in files:
        url = f"https://raw.githubusercontent.com/{project}/{component.revision}/{filename}"
        try:
            with urlopen(Request(url, headers={"User-Agent": "MattOS-UpdateChecker"}), timeout=min(timeout, 8)) as response:
                content = response.read(262145)
            if len(content) > 262144:
                continue
            label = version_label(content.decode(), filename, component.name)
            if label:
                return {"label": label, "url": url, "basis": "upstream source declaration at imported commit; not proof of release"}
        except (OSError, ValueError, KeyError):
            pass
    return None


def kernel_releases(timeout: float) -> dict:
    """Torvalds tags omit stable patch releases; consult the official feed."""
    url = "https://www.kernel.org/releases.json"
    try:
        with urlopen(Request(url, headers={"User-Agent": "MattOS-UpdateChecker"}), timeout=timeout) as response:
            document = json.load(response)
        return {"url": url, "releases": [{"version": item["version"], "channel": item["moniker"],
                                         "source": item.get("source"), "eol": item.get("iseol", False)}
                                        for item in document["releases"]],
                "basis": "official release channels; availability only, not ancestry or upgrade compatibility"}
    except (OSError, ValueError, KeyError, TypeError) as exc:
        return {"url": url, "error": str(exc)}


# Upstream namespaces observed in these repositories. These select the product,
# not a frozen latest version; every run still discovers tags from upstream.
TAG_FAMILIES = {
    "brush": {"brush-shell-v", "brush-v"}, "curl": {"curl-"},
    "expat": {"r_"}, "gmp": {"gmp-"}, "gnulib": {"v"},
    "grub-gnulib": {"v"}, "iproute2": {"v"}, "iputils": {"", "s"},
    "libassuan": {"libassuan-"}, "libcap": {"libcap-", "libcap-korg-"},
    "libgcrypt": {"libgcrypt-"}, "libgpg-error": {"libgpg-error-", "gpgrt-"},
    "libksba": {"libksba-"}, "linux-pam": {"linux-pam-", "v"},
    "networkmanager": {""}, "polkit": {""}, "shadow": {"", "v"},
    "systemd": {"v"}, "glibc": {"glibc-"}, "vulkan-headers": {"v"}, "vulkan-loader": {"v"},
    "xcb-util": {"", "xcb-util-"}, "xkbcomp": {"xkbcomp-"},
}


@lru_cache(maxsize=256)
def compare_commits(repository: str, base: str, target: str, timeout: float) -> dict:
    """Compare immutable commits, with bounded isolated history as a fallback.

    'ahead' means target descends from base. A shallow negative ancestry check
    cannot prove divergence. Never use it to manufacture an update or current
    result. The timeout bounds the entire fallback, not every fetch separately.
    """
    if base == target:
        return {"relation": "identical", "basis": "commit equality", "ahead": 0, "behind": 0}
    repository = transport_url(repository)
    parsed = urlsplit(repository)
    api_error = None
    if parsed.hostname == "github.com":
        project = parsed.path.strip("/").removesuffix(".git")
        url = f"https://api.github.com/repos/{project}/compare/{base}...{target}?per_page=1"
        try:
            request = Request(url, headers={"Accept": "application/vnd.github+json",
                                           "User-Agent": "MattOS-UpdateChecker"})
            with urlopen(request, timeout=min(timeout, 20)) as response:
                data = json.load(response)
            relation = data.get("status")
            if relation not in {"ahead", "behind", "identical", "diverged"}:
                raise ValueError("unexpected compare response")
            return {"relation": relation, "ahead": data.get("ahead_by"),
                    "behind": data.get("behind_by"), "basis": "upstream compare API", "url": url}
        except (OSError, ValueError) as exc:
            api_error = str(exc)
    deadline = time.monotonic() + timeout
    try:
        with tempfile.TemporaryDirectory(prefix="mattos-compare-") as directory:
            work = Path(directory)
            def run(args: list[str]) -> subprocess.CompletedProcess[str]:
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    raise RuntimeError("bounded history time budget exhausted")
                result = git(args, cwd=work, timeout=remaining)
                if result.returncode not in (0, 1):
                    raise RuntimeError(result.stderr.strip() or "Git comparison failed")
                return result
            run(["init", "--bare", "--quiet"])
            for depth in (64, 256, 1024):
                fetched = run(["-c", "fetch.writeCommitGraph=false", "fetch", "--quiet", "--no-tags",
                               "--filter=blob:none", f"--depth={depth}", repository, base, target])
                if fetched.returncode:
                    raise RuntimeError(fetched.stderr.strip() or "upstream commits unavailable")
                for first, second, relation in ((base, target, "ahead"), (target, base, "behind")):
                    check = run(["merge-base", "--is-ancestor", first, second])
                    if check.returncode == 0:
                        return {"relation": relation, "basis": "isolated Git ancestry",
                                "detail": "ancestry proven; exact distance omitted for shallow history"}
                if not (work / "shallow").exists():
                    return {"relation": "diverged", "basis": "complete isolated Git history"}
            detail = "ancestry unresolved within 1024 generations; branches may differ or history may be insufficient"
    except (OSError, RuntimeError, subprocess.TimeoutExpired) as exc:
        detail = str(exc)
    if api_error:
        detail = f"compare API: {api_error}; history: {detail}"
    return {"relation": "unknown", "basis": "bounded history", "detail": detail}


def resolve_snapshot(component: Component, row: dict, *, timeout: float = 45.0) -> None:
    """Resolve untagged imports without treating a source version as a release."""
    imported = row["metadata"]["imported_commit"]
    if row["metadata"]["status"] != "metadata-match" or not imported:
        row["release_status"] = "unknown"
        row["release_detail"] = "repair import metadata before comparing the vendored revision"
        return
    latest = row["latest_release"]
    if row["release_status"] == "unknown" and latest:
        comparison = compare_commits(component.repository, imported, latest["commit"], timeout)
        row["release_comparison"] = comparison
        relation = comparison["relation"]
        row["release_status"] = {"ahead": "newer-release", "behind": "ahead-of-release",
                                  "identical": "current-release", "diverged": "diverged-release"}.get(relation, "unknown")
        row["release_detail"] = (
            f"untagged import versus {latest['tag']}: {relation}; {comparison['basis']}"
            + (f"; {comparison['detail']}" if comparison.get("detail") else ""))


def select_releases(component: Component, refs: dict[str, str], imported: str | None) -> dict:
    releases = [apply_release_policy(component, release) for release in release_tags(refs)]
    # Exact imported commit is authoritative, not the descriptive manifest ref.
    exact = [release for release in releases if release.commit == imported]
    declared = component.ref.removeprefix("refs/tags/")
    current = next((release for release in exact if release.tag == declared), None)
    families = {release.family for release in exact}
    if current is None and len(families) == 1:
        current = max(exact, key=lambda release: release.key)
    policy = TAG_FAMILIES.get(component.name)
    hint = current or parse_release(declared)
    if policy:
        releases = [release for release in releases if release.family in policy
                    and (component.name == "iputils" or version_scheme(component, release) == "version")]
        # Exact tag aliases outside the canonical product family must not hide
        # newer product releases (for example Vulkan SDK versus header tags).
    elif hint:
        releases = [release for release in releases if release.family == hint.family
                    and version_scheme(component, release) == version_scheme(component, hint)]
    elif len({release.family for release in releases}) > 1:
        return {"status": "unknown", "detail": "multiple tag families; upstream release policy needs review",
                "current_release": None, "latest_release": None, "latest_prerelease": None,
                "same_series_release": None}
    stable = [release for release in releases if release.phase == 5]
    prereleases = [release for release in releases if release.phase < 5]
    latest = max(stable, key=lambda release: release.key, default=None)
    prerelease = max(prereleases, key=lambda release: release.key, default=None)
    # Historical prereleases are not useful in a current-availability report.
    if prerelease and latest and prerelease.key <= latest.key:
        prerelease = None
    same_series = max((release for release in stable if current and
                       release.numbers[:2] == current.numbers[:2]),
                      key=lambda release: release.key, default=None)
    detail = None
    if latest is None:
        status, detail = "unknown", "no comparable final-version tags; check official release announcements"
    elif current is None:
        status, detail = "unknown", "imported commit has no unambiguous version tag; do not assume it is older than the latest release"
    elif latest.key > current.key:
        status = "newer-release"
    elif latest.key == current.key:
        status = "current-release"
    else:
        status = "ahead-of-release"
    return {"status": status, "detail": detail,
            "current_release": release_record(current, component.repository),
            "latest_release": release_record(latest, component.repository),
            "latest_prerelease": release_record(prerelease, component.repository),
            "same_series_release": release_record(same_series, component.repository)}


def audit_component(component: Component, *, root: Path, exact: bool, timeout: float) -> dict:
    state = metadata_state(root, component)
    row = {"component": component.name, "repository": component.repository, "ref": component.ref,
           "source_path": component.source_path, "selected_commit": component.revision,
           "metadata": state, "source_version_label": declared_source_version(root, component),
           "ref_status": "error", "ref_tip": None, "ref_detail": None,
           "release_status": "error", "release_detail": None,
           "current_release": None, "latest_release": None, "latest_prerelease": None,
           "same_series_release": None}
    try:
        remote = list_remote_refs(component.repository, [*ref_patterns(component.ref), "refs/tags/*"], timeout)
        if remote.returncode:
            raise RuntimeError(remote.stderr.strip() or "cannot list upstream refs")
        refs = parse_refs(remote.stdout)
        try:
            tip = resolve_ref(refs, component.ref)
            row["ref_tip"] = tip
            if tip == component.revision:
                row["ref_status"] = "up-to-date"
            elif exact:
                row["ref_status"], row["behind"], row["ref_detail"] = commit_distance(component, tip, timeout)
            else:
                row["ref_status"] = "ref-differs"
                row["ref_detail"] = "hash difference only; ancestry not checked"
        except (RuntimeError, subprocess.TimeoutExpired) as exc:
            row["ref_detail"] = str(exc)
        selected = select_releases(component, refs, state["imported_commit"])
        if component.name == "noto-sans-mono":
            selected.update(status="unknown", detail="Google Fonts tags do not identify this font release",
                            current_release=None, latest_release=None, latest_prerelease=None,
                            same_series_release=None)
            row["comparison_scope"] = "Google Fonts repository revision; changes may not affect retained ofl/notosansmono subtree"
        row["release_status"] = selected.pop("status")
        row["release_detail"] = selected.pop("detail")
        row.update(selected)
        if not row["current_release"] and not row["source_version_label"]:
            source = snapshot_label(component, timeout)
            if source:
                row["source_version_label"] = source["label"]
                row["source_version_evidence"] = source
        label = parse_release(row["source_version_label"] or "")
        latest_tag = parse_release((row["latest_release"] or {}).get("tag", ""))
        if label and latest_tag and not row["same_series_release"]:
            families = TAG_FAMILIES.get(component.name, {latest_tag.family})
            candidates = [apply_release_policy(component, item) for item in release_tags(refs)
                          if item.family in families and item.numbers[:2] == label.numbers[:2]]
            same = max((item for item in candidates if item.phase == 5), key=lambda item: item.key, default=None)
            row["same_series_release"] = release_record(same, component.repository)
            if same:
                row["same_series_basis"] = "source version declaration; does not prove exact release identity"
        if component.name == "linux":
            row["official_releases"] = kernel_releases(timeout)
        row["transport_url"] = transport_url(component.repository)
        resolve_snapshot(component, row, timeout=timeout)
        # No comparable release tags means revision coverage, not a claim that
        # the installed package is at the latest official stable release.
        revision_only = not release_tags(refs) or component.name == "noto-sans-mono"
        if (revision_only or row["release_status"] in {"unknown", "ahead-of-release", "diverged-release"}) and row["ref_tip"]:
            comparison = compare_commits(component.repository, component.revision, row["ref_tip"], timeout)
            row["revision_comparison"] = comparison
            row["revision_status"] = {"identical": "current-revision", "ahead": "newer-commits",
                                      "behind": "ahead-of-ref", "diverged": "diverged"}.get(comparison["relation"], "unknown")
            if revision_only and state["status"] == "metadata-match":
                row["release_status"] = "revision-only"
                row["release_detail"] = row.get("comparison_scope", "no comparable version tags; tracked-ref comparison only")
        row["status"] = row.get("revision_status", "unknown") if row["release_status"] == "revision-only" else row["release_status"]
    except (OSError, RuntimeError, subprocess.TimeoutExpired) as exc:
        row["release_detail"] = row["ref_detail"] = str(exc)
    row.setdefault("status", row["release_status"])
    return row


def audit_report(components: list[Component], *, root: Path, exact: bool, timeout: float,
                 jobs: int, progress: bool = False) -> dict:
    started = datetime.now(timezone.utc).isoformat()
    rows = []
    with ThreadPoolExecutor(max_workers=min(jobs, len(components) or 1)) as executor:
        futures = [executor.submit(audit_component, component, root=root, exact=exact, timeout=timeout)
                   for component in components]
        for done, future in enumerate(as_completed(futures), 1):
            row = future.result()
            rows.append(row)
            if progress:
                print(f"[{done}/{len(components)}] {row['component']}: {row['status']}", file=sys.stderr, flush=True)
    rows.sort(key=lambda row: row["component"])
    identity = git(["rev-parse", "HEAD"], cwd=root, timeout=timeout)
    dirty = git(["status", "--porcelain", "--untracked-files=normal"], cwd=root, timeout=timeout)
    counts = Counter(row["release_status"] for row in rows)
    statuses = Counter(row["status"] for row in rows)
    incomplete = sum(row["status"] in {"unknown", "error", "diverged", "diverged-release"} or row["ref_status"] == "error"
                     or row["metadata"]["status"] != "metadata-match" for row in rows)
    return {"format": 3, "started_at_utc": started, "finished_at_utc": datetime.now(timezone.utc).isoformat(),
            "repository_commit": identity.stdout.strip() if identity.returncode == 0 else None,
            "working_tree_dirty": bool(dirty.stdout.strip()) if dirty.returncode == 0 else None,
            "scope": "vendored source components from upstream/sources.toml; not installed or published packages",
            "release_evidence": "upstream Git tags and immutable-commit ancestry; final-version tag naming is not proof of upstream stable/support policy",
            "source_verification": "manifest/import metadata comparison only; source contents and patch applicability not verified",
            "summary": {"total": len(rows), "release_statuses": dict(sorted(counts.items())),
                        "statuses": dict(sorted(statuses.items())),
                        "incomplete": incomplete, "complete": len(rows) - incomplete},
            "components": rows}


def render_report(report: dict) -> str:
    def tag(row: dict, field: str) -> str:
        return (row.get(field) or {}).get("tag", "unknown")

    lines = ["MattOS upstream release audit", f"Checked: {report['finished_at_utc']}",
             f"MattOS commit: {report['repository_commit']}; dirty checkout: {report['working_tree_dirty']}",
             f"Coverage: {report['summary']['complete']}/{report['summary']['total']} complete; "
             f"{report['summary']['incomplete']} need review (includes revision-only checks)", report["release_evidence"], report["source_verification"], ""]
    for row in report["components"]:
        current = tag(row, "current_release")
        if current == "unknown":
            current = f"untagged/unknown {str(row['metadata']['imported_commit'] or 'unknown')[:12]}"
            if row["source_version_label"]:
                current += f" (source label {row['source_version_label']})"
        lines.append(f"{row['component']}: {current} -> {tag(row, 'latest_release')} [{row['status']}]")
        lines.append(f"  Ref {row['ref']}: {row['ref_status']}; import metadata: {row['metadata']['status']}")
        if row.get("revision_comparison"):
            lines.append(f"  Revision: {row['revision_status']}; {row['revision_comparison']}")
        for field, label in (("same_series_release", "Same series"), ("latest_prerelease", "Prerelease")):
            if row[field]:
                lines.append(f"  {label}: {tag(row, field)}")
        if row.get("official_releases"):
            lines.append(f"  Official release channels: {row['official_releases']}")
        lines.append(f"  Upstream: {(row['latest_release'] or {}).get('url', row['repository'])}")
        for detail in (row["release_detail"], row["ref_detail"], row["metadata"]["detail"]):
            if detail:
                lines.append(f"  Note: {detail}")
    return "\n".join(lines)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--component", action="append", help="check only this component; repeatable")
    parser.add_argument("--jobs", type=int, default=8, help="parallel upstream checks (default: 8)")
    parser.add_argument(
        "--exact",
        action="store_true",
        help="fetch isolated shallow history and calculate exact commit distance",
    )
    parser.add_argument(
        "--timeout",
        type=float,
        default=45.0,
        help="timeout in seconds for each Git operation (default: 45)",
    )
    parser.add_argument("--json", action="store_true", help="emit machine-readable JSON")
    parser.add_argument("--releases", action="store_true", help="audit version tags and import metadata, independently of the configured ref")
    parser.add_argument("--progress", action="store_true", help="emit release-audit progress on stderr, keeping JSON valid")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if args.jobs < 1:
        raise SystemExit("--jobs must be at least 1")
    if args.timeout <= 0:
        raise SystemExit("--timeout must be greater than zero")
    components = load_components()
    requested = set(args.component or [])
    if requested:
        known = {component.name for component in components}
        unknown = sorted(requested - known)
        if unknown:
            raise SystemExit(f"unknown component(s): {', '.join(unknown)}")
        components = [component for component in components if component.name in requested]

    if args.releases:
        report = audit_report(components, root=ROOT, exact=args.exact, timeout=args.timeout,
                              jobs=args.jobs, progress=args.progress)
        print(json.dumps(report, indent=2, sort_keys=True) if args.json else render_report(report))
        return 1 if report["summary"]["incomplete"] else 0

    results: list[UpdateResult] = []
    with ThreadPoolExecutor(max_workers=min(args.jobs, len(components) or 1)) as executor:
        futures = [executor.submit(inspect, component, exact=args.exact, timeout=args.timeout) for component in components]
        for completed, future in enumerate(as_completed(futures), start=1):
            result = future.result()
            results.append(result)
            if not args.json:
                print(f"[{completed}/{len(components)}] {format_result(result)}", flush=True)
    results.sort(key=lambda result: result.component.name)

    if args.json:
        print(json.dumps([
            {
                "component": result.component.name,
                "repository": result.component.repository,
                "ref": result.component.ref,
                "source_path": result.component.source_path,
                "status": result.status,
                "behind": result.behind,
                "current": result.current,
                "latest": result.latest,
                "detail": result.detail,
            }
            for result in results
        ], indent=2, sort_keys=True))
    # Normal-mode results are printed as each worker completes. JSON remains
    # valid by suppressing progress lines and printing one final document.

    return 1 if any(result.status == "error" for result in results) else 0


if __name__ == "__main__":
    raise SystemExit(main())
