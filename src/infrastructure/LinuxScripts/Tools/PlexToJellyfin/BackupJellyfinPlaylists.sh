#!/usr/bin/env bash

set -euo pipefail

JELLYFIN_URL="http://100.72.33.98:8096"
BACKUP_ROOT="/srv/storage/OneDrive/Media/Music/Playlists"

require_command() {
	if ! command -v "$1" >/dev/null 2>&1; then
		echo "Error: required command '$1' not found in PATH."
		exit 1
	fi
}

prompt_config() {
	echo "Using Jellyfin URL: ${JELLYFIN_URL}"
	echo "Backup root: ${BACKUP_ROOT}"

	read -r -p "Jellyfin API key: " JELLYFIN_API_KEY
	if [[ -z "${JELLYFIN_API_KEY}" ]]; then
		echo "Error: API key cannot be empty."
		exit 1
	fi
}

run_backup() {
	JELLYFIN_URL="${JELLYFIN_URL}" \
	JELLYFIN_API_KEY="${JELLYFIN_API_KEY}" \
	BACKUP_ROOT="${BACKUP_ROOT}" \
	python3 - <<'PY'
import json
import os
import re
import sys
import urllib.parse
import urllib.request
from datetime import datetime
from pathlib import Path

base_url = os.environ["JELLYFIN_URL"].rstrip("/")
api_key = os.environ["JELLYFIN_API_KEY"]
backup_root = Path(os.environ["BACKUP_ROOT"])

headers = {
    "X-Emby-Token": api_key,
    "X-MediaBrowser-Token": api_key,
    "Authorization": f'MediaBrowser Token="{api_key}"',
    "Accept": "application/json",
}


def request_json(method: str, path: str, query: dict | None = None):
    query = dict(query or {})
    query_string = urllib.parse.urlencode(query, doseq=True)
    url = f"{base_url}{path}"
    if query_string:
        url = f"{url}?{query_string}"

    req = urllib.request.Request(url, method=method, headers=headers)
    try:
        with urllib.request.urlopen(req, timeout=60) as resp:
            content = resp.read()
            if not content:
                return {}
            return json.loads(content.decode("utf-8"))
    except urllib.error.HTTPError as exc:
        details = exc.read().decode("utf-8", errors="ignore")
        if exc.code in (400, 401):
            # Retry with api_key query style for stricter builds
            query_with_key = dict(query or {})
            query_with_key["api_key"] = api_key
            query_string = urllib.parse.urlencode(query_with_key, doseq=True)
            retry_url = f"{base_url}{path}?{query_string}" if query_string else f"{base_url}{path}"
            retry_req = urllib.request.Request(retry_url, method=method, headers=headers)
            try:
                with urllib.request.urlopen(retry_req, timeout=60) as resp:
                    content = resp.read()
                    if not content:
                        return {}
                    return json.loads(content.decode("utf-8"))
            except urllib.error.HTTPError as retry_exc:
                retry_details = retry_exc.read().decode("utf-8", errors="ignore")
                raise RuntimeError(f"HTTP {retry_exc.code} for {method} {retry_url}\n{retry_details}") from retry_exc

        raise RuntimeError(f"HTTP {exc.code} for {method} {url}\n{details}") from exc


def normalize_path(path: str) -> str:
    value = path.strip()
    if value.lower().startswith("file://localhost/"):
        value = "/" + value[len("file://localhost/"):]
    elif value.lower().startswith("file:///"):
        value = "/" + value[len("file:///"):]
    elif value.lower().startswith("file://"):
        value = value[len("file://"):]
    value = urllib.parse.unquote(value)
    return value.replace("\\", "/")


def sanitize_filename(name: str) -> str:
    cleaned = re.sub(r"[\\/:*?\"<>|]", "_", name).strip()
    cleaned = re.sub(r"\s+", " ", cleaned)
    return cleaned or "untitled_playlist"


def get_me_user():
    try:
        me = request_json("GET", "/Users/Me")
        if me.get("Id"):
            return me
    except Exception:
        pass

    users = request_json("GET", "/Users")
    if not users:
        raise RuntimeError("Unable to resolve Jellyfin user from API key.")
    return users[0]


def paged_items(user_id: str, include_item_types: str):
    start = 0
    limit = 500
    all_items = []

    while True:
        payload = request_json(
            "GET",
            f"/Users/{user_id}/Items",
            {
                "Recursive": "true",
                "IncludeItemTypes": include_item_types,
                "StartIndex": str(start),
                "Limit": str(limit),
                "Fields": "Path",
            },
        )

        items = payload.get("Items", [])
        all_items.extend(items)

        if len(items) < limit:
            break
        start += limit

    return all_items


def playlist_items(user_id: str, playlist_id: str):
    payload = request_json(
        "GET",
        f"/Playlists/{playlist_id}/Items",
        {
            "UserId": user_id,
            "Fields": "Path",
            "Limit": "10000",
        },
    )
    return payload.get("Items", [])


user = get_me_user()
user_id = user.get("Id")
user_name = user.get("Name", "unknown")
if not user_id:
    raise RuntimeError("Resolved Jellyfin user has no Id.")

stamp = datetime.now().strftime("%Y-%m-%d-%H%M%S")
backup_dir = backup_root / f"jellyfin-playlists-backup-{stamp}"
backup_dir.mkdir(parents=True, exist_ok=False)

playlists = paged_items(user_id, "Playlist")
if not playlists:
    print(f"No playlists found for user {user_name}.")
    print(f"Created empty backup directory: {backup_dir}")
    sys.exit(0)

written = 0
skipped = 0

for playlist in playlists:
    playlist_id = playlist.get("Id")
    name = playlist.get("Name") or "Untitled Playlist"
    if not playlist_id:
        skipped += 1
        continue

    items = playlist_items(user_id, playlist_id)
    paths = []
    for item in items:
        path = item.get("Path")
        if not path:
            continue
        paths.append(normalize_path(path))

    file_name = sanitize_filename(name) + ".m3u"
    out_file = backup_dir / file_name

    with out_file.open("w", encoding="utf-8") as handle:
        handle.write("#EXTM3U\n")
        for entry in paths:
            handle.write(entry + "\n")

    written += 1
    print(f"[OK] {name} -> {out_file.name} ({len(paths)} entries)")

print()
print("Backup complete.")
print(f"  User              : {user_name} ({user_id})")
print(f"  Backup directory  : {backup_dir}")
print(f"  Playlists written : {written}")
print(f"  Playlists skipped : {skipped}")
PY
}

main() {
	require_command python3
	prompt_config
	run_backup
}

main
