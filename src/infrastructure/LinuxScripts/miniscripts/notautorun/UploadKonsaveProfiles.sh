#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
BW_MASTER_PASSWORD_FILE="$REPO_ROOT/.bw_master_password"
PROFILES_DIR="$REPO_ROOT/KDEProfiles"

contains_exact() {
  local needle="$1"
  shift
  local item
  for item in "$@"; do
    if [[ "$item" == "$needle" ]]; then
      return 0
    fi
  done
  return 1
}

bitwarden_status() {
  local status_json
  local parsed

  status_json="$(bw status 2>/dev/null || true)"
  parsed="$(printf '%s' "$status_json" | sed -n 's/.*"status"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p')"

  if [[ -z "$parsed" ]]; then
    echo "unknown"
  else
    echo "$parsed"
  fi
}

ensure_gh_authenticated() {
  local bw_item_name="${BITWARDEN_GITHUB_ITEM:-github.com}"
  local bw_state
  local bw_session
  local github_token

  if gh auth status >/dev/null 2>&1; then
    return 0
  fi

  echo "gh is not authenticated. Attempting Bitwarden token login (item: ${bw_item_name})..."

  if command -v bw >/dev/null 2>&1; then
    bw_state="$(bitwarden_status)"

    if [[ "$bw_state" == "unauthenticated" || "$bw_state" == "unknown" ]]; then
      if ! bw login </dev/tty >/dev/tty 2>&1; then
        echo "Bitwarden login failed. Falling back to manual gh login."
      fi
      bw_state="$(bitwarden_status)"
    fi

    if [[ "$bw_state" == "locked" ]]; then
      if [[ -f "$BW_MASTER_PASSWORD_FILE" ]]; then
        IFS= read -r BW_MASTER_PASSWORD < "$BW_MASTER_PASSWORD_FILE"
        export BW_MASTER_PASSWORD
        bw_session="$(bw unlock --passwordenv BW_MASTER_PASSWORD --nointeraction --raw 2>/dev/null || true)"
        unset BW_MASTER_PASSWORD
      else
        bw_session="$(bw unlock --raw </dev/tty 2>/dev/null || true)"
      fi
      if [[ -n "$bw_session" ]]; then
        export BW_SESSION="$bw_session"
        bw_state="$(bitwarden_status)"
      fi
    fi

    if [[ "$bw_state" == "unlocked" ]]; then
      github_token="$(bw get password "$bw_item_name" 2>/dev/null || true)"
      if [[ -n "$github_token" ]]; then
        if printf '%s\n' "$github_token" | gh auth login --hostname github.com --with-token >/dev/null 2>&1; then
          if gh auth status >/dev/null 2>&1; then
            echo "Authenticated gh using Bitwarden token."
            return 0
          fi
        fi
        echo "Bitwarden token login to gh failed. Falling back to manual gh login."
      else
        echo "Bitwarden item '${bw_item_name}' missing password/token. Falling back to manual gh login."
      fi
    else
      echo "Bitwarden is unavailable/locked/unauthenticated. Falling back to manual gh login."
    fi
  else
    echo "Bitwarden CLI not installed. Falling back to manual gh login."
  fi

  gh auth login
  if ! gh auth status >/dev/null 2>&1; then
    echo "Error: gh authentication failed."
    exit 1
  fi
}

if ! command -v gh >/dev/null 2>&1; then
  echo "Error: GitHub CLI (gh) is not installed."
  exit 1
fi

ensure_gh_authenticated

if [[ ! -d "$PROFILES_DIR" ]]; then
  echo "Error: KDE profiles directory not found: $PROFILES_DIR"
  exit 1
fi

shopt -s nullglob
PROFILE_FILES=("$PROFILES_DIR"/*.knsv)
shopt -u nullglob

if [[ ${#PROFILE_FILES[@]} -eq 0 ]]; then
  echo "Error: No .knsv profiles found in $PROFILES_DIR"
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

declare -a LOCAL_TAGS=()
for profile_path in "${PROFILE_FILES[@]}"; do
  profile_file="$(basename "$profile_path")"
  profile_name="${profile_file%.knsv}"
  LOCAL_TAGS+=("$profile_name")
done

echo "Sync target repo: $REPO_SLUG"
echo "Local profiles found: ${#LOCAL_TAGS[@]}"

mapfile -t EXISTING_TAGS < <(gh api --paginate "/repos/$REPO_SLUG/releases?per_page=100" --jq '.[].tag_name' 2>/dev/null || true)

# Delete releases not represented by current KDEProfiles/*.knsv files.
for tag in "${EXISTING_TAGS[@]}"; do
  if ! contains_exact "$tag" "${LOCAL_TAGS[@]}"; then
    echo "Deleting stale release: $tag"
    gh release delete "$tag" --repo "$REPO_SLUG" --yes --cleanup-tag
  fi
done

# Recreate each profile release with the same name as the profile.
for profile_path in "${PROFILE_FILES[@]}"; do
  profile_file="$(basename "$profile_path")"
  profile_name="${profile_file%.knsv}"

  if gh release view "$profile_name" --repo "$REPO_SLUG" >/dev/null 2>&1; then
    echo "Deleting existing release for overwrite: $profile_name"
    gh release delete "$profile_name" --repo "$REPO_SLUG" --yes --cleanup-tag
  fi

  echo "Creating release $profile_name with asset $profile_file"
  gh release create "$profile_name" "$profile_path" \
    --repo "$REPO_SLUG" \
    --title "$profile_name" \
    --notes ""
done

echo "Done. GitHub Releases now match local KDEProfiles/*.knsv files."