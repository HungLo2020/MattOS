"""Upstream release discovery (host only).

Discovery only reports whether upstream has a newer release than the one
selected in releases.json; it never affects what a build produces, so it is
not part of any package's build inputs.
"""

from __future__ import annotations

import json
import re
import time
import urllib.error
from urllib.request import Request, urlopen

from .build import RecipeError, command

def fetch_json(url: str, *, headers: dict[str, str] | None = None, attempts: int = 3) -> dict:
    request_headers = {"User-Agent": "MattOS-third-party-packages/1"}
    if headers:
        request_headers.update(headers)
    last: Exception | None = None
    for attempt in range(1, attempts + 1):
        try:
            with urlopen(Request(url, headers=request_headers), timeout=30) as response:
                value = json.load(response)
            if not isinstance(value, dict):
                raise RecipeError(f"upstream API returned a non-object response: {url}")
            return value
        except (OSError, urllib.error.URLError, json.JSONDecodeError, RecipeError) as exc:
            last = exc
            if attempt < attempts:
                time.sleep(attempt)
    raise RecipeError(f"upstream API request failed after {attempts} attempts: {url}: {last}") from last


def github_latest_release(owner: str, repository: str) -> tuple[str, dict[str, str]]:
    url = f"https://api.github.com/repos/{owner}/{repository}/releases/latest"
    release = fetch_json(url, headers={"Accept": "application/vnd.github+json"})
    tag = str(release.get("tag_name", ""))
    if not tag:
        raise RecipeError(f"GitHub release API returned no stable tag: {url}")
    version = tag[1:] if tag.startswith("v") else tag
    return version, {
        "upstream": f"https://github.com/{owner}/{repository}",
        "release_tag": tag,
        "release_api": url,
    }


def version_key(version: str) -> tuple[int, ...]:
    return tuple(int(part) for part in re.findall(r"[0-9]+", version))


def git_latest_tag(url: str, pattern: str) -> tuple[str, str]:
    """The newest release tag of a Git repository, by version order.

    `pattern` is a full-match regular expression whose first group is the
    version (for example ``v([0-9.]+)``).  Listing tags needs no API quota,
    so every recipe can be checked in one run.
    """
    # A transient network failure only delays a check; retry before failing.
    for attempt in range(1, 4):
        try:
            output = command(["git", "ls-remote", "--tags", "--refs", url])
            break
        except RecipeError:
            if attempt == 3:
                raise
            time.sleep(2 * attempt)
    candidates = []
    for line in output.splitlines():
        tag = line.split("refs/tags/", 1)[-1].strip()
        match = re.fullmatch(pattern, tag)
        if match:
            candidates.append((version_key(match.group(1)), match.group(1), tag))
    if not candidates:
        raise RecipeError(f"no release tag of {url} matches {pattern!r}")
    _, version, tag = max(candidates)
    return version, tag

