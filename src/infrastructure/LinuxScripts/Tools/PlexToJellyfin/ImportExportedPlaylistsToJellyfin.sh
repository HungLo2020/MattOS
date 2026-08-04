#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
EXPORT_DIR="${SCRIPT_DIR}/exports"
JELLYFIN_URL="http://100.72.33.98:8096"
JELLYFIN_USERNAME="matt"
OVERWRITE_EXISTING="true"

require_command() {
	if ! command -v "$1" >/dev/null 2>&1; then
		echo "Error: required command '$1' not found in PATH."
		exit 1
	fi
}

prompt_config() {
    echo "Using Jellyfin URL: ${JELLYFIN_URL}"

	read -r -p "Jellyfin API key: " JELLYFIN_API_KEY
	if [[ -z "${JELLYFIN_API_KEY}" ]]; then
		echo "Error: API key cannot be empty."
		exit 1
	fi

    echo "Using Jellyfin username: ${JELLYFIN_USERNAME}"
    echo "Overwrite existing playlists: yes"
}

validate_exports() {
	if [[ ! -d "${EXPORT_DIR}" ]]; then
		echo "Error: export directory not found: ${EXPORT_DIR}"
		exit 1
	fi

	if ! find "${EXPORT_DIR}" -maxdepth 1 -type f -name '*.m3u' | grep -q .; then
		echo "Error: no .m3u files found in ${EXPORT_DIR}"
		exit 1
	fi
}

run_import() {
	JELLYFIN_URL="${JELLYFIN_URL}" \
	JELLYFIN_API_KEY="${JELLYFIN_API_KEY}" \
	JELLYFIN_USERNAME="${JELLYFIN_USERNAME}" \
	OVERWRITE_EXISTING="${OVERWRITE_EXISTING}" \
	EXPORT_DIR="${EXPORT_DIR}" \
	python3 - <<'PY'
import json
import os
import re
import sys
import urllib.parse
import urllib.request
from pathlib import Path

base_url = os.environ["JELLYFIN_URL"].rstrip("/")
api_key = os.environ["JELLYFIN_API_KEY"]
username = os.environ.get("JELLYFIN_USERNAME", "").strip()
overwrite_existing = os.environ.get("OVERWRITE_EXISTING", "true").lower() == "true"
export_dir = Path(os.environ["EXPORT_DIR"])

headers = {
    "X-Emby-Token": api_key,
    "X-MediaBrowser-Token": api_key,
    "Authorization": f'MediaBrowser Token="{api_key}"',
    "Accept": "application/json",
    "Content-Type": "application/json",
}


def request_json(method: str, path: str, query: dict | None = None, body: dict | None = None):
    query = query or {}
    query_string = urllib.parse.urlencode(query, doseq=True)
    url = f"{base_url}{path}"
    if query_string:
        url = f"{url}?{query_string}"

    data = None
    if body is not None:
        data = json.dumps(body).encode("utf-8")

    req = urllib.request.Request(url, method=method, headers=headers, data=data)
    try:
        with urllib.request.urlopen(req, timeout=60) as resp:
            content = resp.read()
            if not content:
                return {}
            return json.loads(content.decode("utf-8"))
    except urllib.error.HTTPError as exc:
        details = exc.read().decode("utf-8", errors="ignore")
        if exc.code == 401:
            raise RuntimeError(
                f"HTTP 401 Unauthorized for {method} {url}\n"
                "Your Jellyfin API key is invalid for this server/user, or has insufficient scope.\n"
                "Generate a fresh API key from Jellyfin Dashboard -> API Keys and try again."
            ) from exc
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
    return value.rstrip("/").replace("\\", "/")


def music_relative_key(path: str) -> str:
    normalized = normalize_path(path)
    lower = normalized.lower()
    marker = "/music/"
    idx = lower.find(marker)
    if idx == -1:
        return ""
    return normalized[idx + len(marker):].lstrip("/").lower()


def suffix_key(path: str, parts: int = 4) -> str:
    normalized = normalize_path(path)
    chunks = [c for c in normalized.split("/") if c]
    if not chunks:
        return ""
    tail = chunks[-parts:] if len(chunks) >= parts else chunks
    return "/".join(tail).lower()


def basename_key(path: str) -> str:
    normalized = normalize_path(path)
    chunks = [c for c in normalized.split("/") if c]
    if not chunks:
        return ""
    return chunks[-1].lower()


def normalize_token(value: str) -> str:
    return re.sub(r"[^a-z0-9]+", "", value.lower())


def artist_filename_key(path: str) -> str:
    normalized = normalize_path(path)
    chunks = [c for c in normalized.split("/") if c]
    if len(chunks) < 2:
        return ""

    artist = ""
    lower_chunks = [c.lower() for c in chunks]

    if "music" in lower_chunks:
        music_idx = lower_chunks.index("music")
        if music_idx + 1 < len(chunks):
            candidate = chunks[music_idx + 1]
            if candidate.lower() == "artists" and music_idx + 2 < len(chunks):
                artist = chunks[music_idx + 2]
            else:
                artist = candidate
    elif "artists" in lower_chunks:
        artists_idx = lower_chunks.index("artists")
        if artists_idx + 1 < len(chunks):
            artist = chunks[artists_idx + 1]

    if not artist:
        return ""

    filename = chunks[-1]
    artist_norm = normalize_token(artist)
    filename_norm = normalize_token(filename)
    if not artist_norm or not filename_norm:
        return ""
    return f"{artist_norm}|{filename_norm}"


def get_me():
    return request_json("GET", "/Users/Me")


def get_users():
    return request_json("GET", "/Users")


def resolve_user():
    try:
        me = get_me()
        if me.get("Id"):
            return me
    except RuntimeError as exc:
        message = str(exc)
        if "HTTP 400" not in message and "HTTP 404" not in message and "HTTP 405" not in message:
            raise

    users = get_users()
    if not users:
        raise RuntimeError("No Jellyfin users returned by API.")

    if username:
        for user in users:
            if user.get("Name", "").lower() == username.lower():
                return user
        raise RuntimeError(f"User '{username}' not found.")

    return users[0]


def paged_items(user_id: str, include_item_types: str, fields: str):
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
                "Fields": fields,
                "StartIndex": str(start),
                "Limit": str(limit),
            },
        )

        items = payload.get("Items", [])
        all_items.extend(items)

        if len(items) < limit:
            break
        start += limit

    return all_items


def build_audio_path_indexes(user_id: str):
    audio_items = paged_items(user_id, "Audio", "Path")
    exact_index = {}
    relative_index = {}
    suffix_index = {}
    suffix_counts = {}
    artist_file_index = {}
    artist_file_counts = {}
    basename_index = {}
    basename_counts = {}
    for item in audio_items:
        item_id = item.get("Id")
        item_path = item.get("Path")
        if not item_id or not item_path:
            continue
        normalized = normalize_path(item_path)
        exact_index[normalized] = item_id
        rel_key = music_relative_key(normalized)
        if rel_key and rel_key not in relative_index:
            relative_index[rel_key] = item_id
        sfx = suffix_key(normalized, parts=4)
        if sfx:
            suffix_counts[sfx] = suffix_counts.get(sfx, 0) + 1
            if sfx not in suffix_index:
                suffix_index[sfx] = item_id
        af_key = artist_filename_key(normalized)
        if af_key:
            artist_file_counts[af_key] = artist_file_counts.get(af_key, 0) + 1
            if af_key not in artist_file_index:
                artist_file_index[af_key] = item_id
        bname = basename_key(normalized)
        if bname:
            basename_counts[bname] = basename_counts.get(bname, 0) + 1
            if bname not in basename_index:
                basename_index[bname] = item_id
    return (
        exact_index,
        relative_index,
        suffix_index,
        suffix_counts,
        artist_file_index,
        artist_file_counts,
        basename_index,
        basename_counts,
    )


def get_existing_playlists_by_name(user_id: str):
    playlists = paged_items(user_id, "Playlist", "")
    by_name = {}
    for item in playlists:
        name = item.get("Name")
        item_id = item.get("Id")
        if name and item_id:
            by_name[name] = item_id
    return by_name


def delete_playlist(item_id: str):
    request_json("DELETE", f"/Items/{item_id}")


def create_playlist(user_id: str, name: str, first_item_id: str):
    return request_json(
        "POST",
        "/Playlists",
        {
            "UserId": user_id,
            "Name": name,
            "MediaType": "Audio",
            "Ids": first_item_id,
        },
    )


def add_items_to_playlist(playlist_id: str, user_id: str, item_ids: list[str]):
    if not item_ids:
        return

    chunk_size = 200
    for i in range(0, len(item_ids), chunk_size):
        chunk = item_ids[i : i + chunk_size]
        request_json(
            "POST",
            f"/Playlists/{playlist_id}/Items",
            {
                "UserId": user_id,
                "Ids": ",".join(chunk),
            },
        )


def parse_m3u_paths(path: Path):
    entries = []
    text = path.read_text(encoding="utf-8", errors="ignore")
    for line in text.splitlines():
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        entries.append(normalize_path(stripped))
    return entries


user = resolve_user()
user_id = user.get("Id")
user_name = user.get("Name", "unknown")

if not user_id:
    raise RuntimeError("Chosen Jellyfin user has no Id.")

if username and user_name.lower() != username.lower():
    raise RuntimeError(
        f"Authenticated API key belongs to '{user_name}', but you entered username '{username}'."
    )

print(f"Using Jellyfin user: {user_name} ({user_id})")
print("Indexing Jellyfin audio items by path...")
audio_index, audio_relative_index, audio_suffix_index, audio_suffix_counts, audio_artist_file_index, audio_artist_file_counts, audio_basename_index, audio_basename_counts = build_audio_path_indexes(user_id)
print(f"Indexed audio items (exact): {len(audio_index)}")
print(f"Indexed audio items (relative /Music/): {len(audio_relative_index)}")
print(f"Indexed audio items (suffix-4): {len(audio_suffix_index)}")
print(f"Indexed audio items (artist+filename): {len(audio_artist_file_index)}")
print(f"Indexed audio items (basename): {len(audio_basename_index)}")

existing_playlists = get_existing_playlists_by_name(user_id)

m3u_files = sorted(export_dir.glob("*.m3u"))
if not m3u_files:
    print(f"No .m3u files found in {export_dir}")
    sys.exit(1)

imported = 0
skipped = 0
missing_total = 0
matched_by_relative_total = 0
matched_by_suffix_total = 0
matched_by_artist_file_total = 0
matched_by_basename_total = 0

for m3u_file in m3u_files:
    playlist_name = m3u_file.stem
    paths = parse_m3u_paths(m3u_file)

    if not paths:
        print(f"[SKIP] {playlist_name}: no track entries in file")
        skipped += 1
        continue

    resolved_ids = []
    missing = []
    matched_by_relative = 0
    matched_by_suffix = 0
    matched_by_artist_file = 0
    matched_by_basename = 0
    for p in paths:
        item_id = audio_index.get(p)
        if not item_id:
            rel_key = music_relative_key(p)
            if rel_key:
                item_id = audio_relative_index.get(rel_key)
                if item_id:
                    matched_by_relative += 1
        if not item_id:
            sfx = suffix_key(p, parts=4)
            if sfx and audio_suffix_counts.get(sfx, 0) == 1:
                item_id = audio_suffix_index.get(sfx)
                if item_id:
                    matched_by_suffix += 1
        if not item_id:
            af_key = artist_filename_key(p)
            if af_key and audio_artist_file_counts.get(af_key, 0) == 1:
                item_id = audio_artist_file_index.get(af_key)
                if item_id:
                    matched_by_artist_file += 1
        if not item_id:
            bname = basename_key(p)
            if bname and audio_basename_counts.get(bname, 0) == 1:
                item_id = audio_basename_index.get(bname)
                if item_id:
                    matched_by_basename += 1
        if item_id:
            resolved_ids.append(item_id)
        else:
            missing.append(p)

    missing_total += len(missing)
    matched_by_relative_total += matched_by_relative
    matched_by_suffix_total += matched_by_suffix
    matched_by_artist_file_total += matched_by_artist_file
    matched_by_basename_total += matched_by_basename

    if not resolved_ids:
        print(f"[SKIP] {playlist_name}: 0 matched tracks, {len(missing)} missing")
        skipped += 1
        continue

    existing_id = existing_playlists.get(playlist_name)
    if existing_id and overwrite_existing:
        delete_playlist(existing_id)
        existing_playlists.pop(playlist_name, None)
        print(f"[INFO] Deleted existing playlist: {playlist_name}")
    elif existing_id and not overwrite_existing:
        print(f"[SKIP] {playlist_name}: already exists (overwrite disabled)")
        skipped += 1
        continue

    create_response = create_playlist(user_id, playlist_name, resolved_ids[0])
    playlist_id = create_response.get("Id")
    if not playlist_id:
        item = create_response.get("Item") or {}
        playlist_id = item.get("Id")
    if not playlist_id:
        raise RuntimeError(f"Failed to create playlist '{playlist_name}': no Id in response")

    add_items_to_playlist(playlist_id, user_id, resolved_ids[1:])
    imported += 1

    print(
        f"[OK]   {playlist_name}: imported {len(resolved_ids)} tracks"
        + (f", missing {len(missing)}" if missing else "")
        + (f", fallback-matched {matched_by_relative}" if matched_by_relative else "")
        + (f", suffix-matched {matched_by_suffix}" if matched_by_suffix else "")
        + (f", artist+filename-matched {matched_by_artist_file}" if matched_by_artist_file else "")
        + (f", basename-matched {matched_by_basename}" if matched_by_basename else "")
    )

    if missing:
        print(f"[MISSING] {playlist_name}:")
        for missing_path in missing:
            print(f"  - {missing_path}")

print()
print("Import complete.")
print(f"  Imported playlists : {imported}")
print(f"  Skipped playlists  : {skipped}")
print(f"  Missing tracks     : {missing_total}")
print(f"  Fallback matched   : {matched_by_relative_total}")
print(f"  Suffix matched     : {matched_by_suffix_total}")
print(f"  Artist+file matched: {matched_by_artist_file_total}")
print(f"  Basename matched   : {matched_by_basename_total}")
PY
}

main() {
	require_command python3
	validate_exports
	prompt_config
	run_import
}

main
