#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
PROFILES_DIR="$REPO_ROOT/KDEProfiles"

if ! command -v curl >/dev/null 2>&1; then
  echo "Error: curl is required but not installed."
  exit 1
fi

if ! command -v python3 >/dev/null 2>&1; then
  echo "Error: python3 is required but not installed."
  exit 1
fi

if ! mkdir -p "$PROFILES_DIR"; then
  echo "Error: Could not create profiles directory: $PROFILES_DIR"
  exit 1
fi

if [[ ! -w "$PROFILES_DIR" ]]; then
  echo "Error: Profiles directory is not writable: $PROFILES_DIR"
  exit 1
fi

ORIGIN_URL="$(git -C "$REPO_ROOT" remote get-url origin 2>/dev/null || true)"
if [[ -z "$ORIGIN_URL" ]]; then
  echo "Error: Could not determine git origin URL."
  exit 1
fi

if [[ "$ORIGIN_URL" =~ github\.com[:/]([^/]+)/([^/.]+)(\.git)?$ ]]; then
  OWNER="${BASH_REMATCH[1]}"
  REPO="${BASH_REMATCH[2]}"
else
  echo "Error: Origin is not a recognizable GitHub URL: $ORIGIN_URL"
  exit 1
fi

REPO_SLUG="$OWNER/$REPO"

echo "Fetching releases from $REPO_SLUG..."
mapfile -t ASSET_ENTRIES < <(
  REPO_SLUG="$REPO_SLUG" GITHUB_TOKEN="${GITHUB_TOKEN:-}" python3 - <<'PY'
import json
import os
import urllib.request

repo_slug = os.environ["REPO_SLUG"]
token = os.environ.get("GITHUB_TOKEN", "").strip()

headers = {
    "Accept": "application/vnd.github+json",
    "User-Agent": "DownloadKonsaveProfilesScript",
}
if token:
    headers["Authorization"] = f"Bearer {token}"

page = 1
release_count = 0

while True:
    url = f"https://api.github.com/repos/{repo_slug}/releases?per_page=100&page={page}"
    req = urllib.request.Request(url, headers=headers)
    with urllib.request.urlopen(req, timeout=30) as response:
        payload = json.loads(response.read().decode("utf-8"))

    if not payload:
        break

    release_count += len(payload)
    for release in payload:
        tag = release.get("tag_name", "")
        for asset in release.get("assets", []):
            name = asset.get("name", "")
            download_url = asset.get("browser_download_url", "")
            if tag and name and download_url:
                print(f"{tag}|{name}|{download_url}")

    if len(payload) < 100:
        break
    page += 1
PY
)

if [[ ${#ASSET_ENTRIES[@]} -eq 0 ]]; then
  echo "No release assets found in $REPO_SLUG."
  exit 0
fi

echo "Found ${#ASSET_ENTRIES[@]} asset(s). Downloading to $PROFILES_DIR"
for entry in "${ASSET_ENTRIES[@]}"; do
  tag="${entry%%|*}"
  rest="${entry#*|}"
  asset_name="${rest%%|*}"
  download_url="${rest#*|}"
  destination_path="$PROFILES_DIR/$asset_name"

  echo "Downloading [$tag] $asset_name"
  if [[ -n "${GITHUB_TOKEN:-}" ]]; then
    curl -fsSL -H "Authorization: Bearer ${GITHUB_TOKEN}" "$download_url" -o "$destination_path"
  else
    curl -fsSL "$download_url" -o "$destination_path"
  fi
done

echo "Done. Downloaded release assets into $PROFILES_DIR"