#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
CREATE_REPOS_SCRIPT="$SCRIPT_DIR/../notautorun/CreateReposDir.sh"
REPOS_DIR="$HOME/Documents/Repos"

ask_yes_no() {
  local prompt="$1"
  local answer

  while true; do
    read -r -p "$prompt (y/n): " answer
    case "$answer" in
      [Yy]) return 0 ;;
      [Nn]) return 1 ;;
      *) echo "Please enter y or n." ;;
    esac
  done
}

if [[ ! -x "$CREATE_REPOS_SCRIPT" ]]; then
  echo "Error: Required script not found or not executable: $CREATE_REPOS_SCRIPT"
  exit 1
fi

"$CREATE_REPOS_SCRIPT"

if ! command -v git >/dev/null 2>&1; then
  echo "Error: git is required but was not found."
  exit 1
fi

if ! command -v python3 >/dev/null 2>&1; then
  echo "Error: python3 is required but was not found."
  exit 1
fi

detect_github_username() {
  local origin_url
  origin_url="$(git -C "$REPO_ROOT" config --get remote.origin.url 2>/dev/null || true)"

  if [[ -n "${GITHUB_USERNAME:-}" ]]; then
    echo "$GITHUB_USERNAME"
    return
  fi

  if [[ -n "$origin_url" ]]; then
    if [[ "$origin_url" =~ github\.com[:/]([^/]+)/ ]]; then
      echo "${BASH_REMATCH[1]}"
      return
    fi
  fi

  if git config --global --get github.user >/dev/null 2>&1; then
    git config --global --get github.user
    return
  fi

  read -r -p "Enter your GitHub username: " entered_username
  echo "$entered_username"
}

GITHUB_USER="$(detect_github_username)"

if [[ -z "$GITHUB_USER" ]]; then
  echo "Error: GitHub username is required."
  exit 1
fi

echo "Fetching repositories for GitHub user: $GITHUB_USER"

mapfile -t REPO_ENTRIES < <(
  GITHUB_USER="$GITHUB_USER" GITHUB_TOKEN="${GITHUB_TOKEN:-}" python3 - <<'PY'
import json
import os
import sys
import urllib.request

username = os.environ.get("GITHUB_USER", "").strip()
token = os.environ.get("GITHUB_TOKEN", "").strip()

if not username:
    sys.exit(0)

headers = {"Accept": "application/vnd.github+json", "User-Agent": "DownloadGitReposScript"}
if token:
    headers["Authorization"] = f"Bearer {token}"

repos = []
page = 1

while True:
    url = f"https://api.github.com/users/{username}/repos?per_page=100&page={page}&sort=full_name"
    req = urllib.request.Request(url, headers=headers)
    with urllib.request.urlopen(req, timeout=20) as resp:
        data = json.loads(resp.read().decode("utf-8"))

    if not data:
        break

    for repo in data:
        name = repo.get("name", "")
        clone_url = repo.get("clone_url", "")
        if name and clone_url:
            repos.append((name, clone_url))

    if len(data) < 100:
        break

    page += 1

for name, clone_url in sorted(repos, key=lambda item: item[0].lower()):
    print(f"{name}|{clone_url}")
PY
)

if [[ ${#REPO_ENTRIES[@]} -eq 0 ]]; then
  echo "No repositories found for user '$GITHUB_USER'."
  exit 0
fi

declare -a SELECTED_REPOS=()

echo "Answer prompts to select repos. Cloning starts after all prompts."
for entry in "${REPO_ENTRIES[@]}"; do
  repo_name="${entry%%|*}"
  clone_url="${entry#*|}"

  if ask_yes_no "Clone $repo_name?"; then
    SELECTED_REPOS+=("$repo_name|$clone_url")
  fi
done

if [[ ${#SELECTED_REPOS[@]} -eq 0 ]]; then
  echo "No repositories selected."
  exit 0
fi

for entry in "${SELECTED_REPOS[@]}"; do
  repo_name="${entry%%|*}"
  clone_url="${entry#*|}"
  target_dir="$REPOS_DIR/$repo_name"

  if [[ -d "$target_dir" ]]; then
    echo "Skipping $repo_name (already exists at $target_dir)"
    continue
  fi

  echo "Cloning $repo_name..."
  git clone "$clone_url" "$target_dir"
done

echo "DownloadGitRepos complete."
