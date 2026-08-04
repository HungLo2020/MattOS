#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$SCRIPT_DIR"
while [[ "$REPO_ROOT" != "/" && ! -d "$REPO_ROOT/miniscripts/droplet" ]]; do
  REPO_ROOT="$(dirname "$REPO_ROOT")"
done

DROPLET_SCRIPTS_DIR="$REPO_ROOT/miniscripts/droplet"
SETUP_MATT_USER_SCRIPT="$REPO_ROOT/miniscripts/notautorun/SetupMattUser.sh"
TARGET_USER="matt"

SELECTED_SCRIPTS=()
DISCOVERED_SCRIPTS=()

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

run_privileged() {
  if [[ "$(id -u)" -eq 0 ]]; then
    "$@"
  else
    sudo "$@"
  fi
}

run_as_target_user() {
  if [[ "$(id -un)" == "$TARGET_USER" ]]; then
    "$@"
  else
    sudo -H -u "$TARGET_USER" "$@"
  fi
}

ensure_repo_location_for_target_user() {
  local target_home
  local target_repo_dir
  local target_parent

  target_home="$(getent passwd "$TARGET_USER" | cut -d: -f6)"
  if [[ -z "$target_home" ]]; then
    echo "Error: could not resolve home directory for user '$TARGET_USER'."
    exit 1
  fi

  target_repo_dir="$target_home/Documents/Repos/LinuxScripts"
  target_parent="$(dirname "$target_repo_dir")"

  run_privileged mkdir -p "$target_parent"

  run_privileged chown -R "$TARGET_USER:$TARGET_USER" "$REPO_ROOT"

  if [[ "$REPO_ROOT" == "$target_repo_dir" ]]; then
    return 0
  fi

  echo "Current repo location is '$REPO_ROOT'."
  echo "Recommended droplet location is '$target_repo_dir'."

  if ! ask_yes_no "Move repo to '$target_repo_dir' and continue from there?"; then
    echo "Continuing from current path."
    return 0
  fi

  if [[ -e "$target_repo_dir" && ! -d "$target_repo_dir/.git" ]]; then
    echo "Error: target path exists and is not this repo: $target_repo_dir"
    exit 1
  fi

  if [[ ! -d "$target_repo_dir/.git" ]]; then
    run_privileged mkdir -p "$target_repo_dir"
    run_privileged cp -a "$REPO_ROOT/." "$target_repo_dir/"
  else
    run_privileged cp -a "$REPO_ROOT/." "$target_repo_dir/"
  fi

  run_privileged chown -R "$TARGET_USER:$TARGET_USER" "$target_repo_dir"

  echo "Re-launching SetupDroplet from '$target_repo_dir' as '$TARGET_USER'..."
  run_as_target_user bash "$target_repo_dir/miniscripts/server/SetupDroplet.sh"
  exit $?
}

if [[ ! -f "$SETUP_MATT_USER_SCRIPT" ]]; then
  echo "Required script not found: $SETUP_MATT_USER_SCRIPT"
  exit 1
fi

echo "Running matt user setup prerequisite..."
if ! bash "$SETUP_MATT_USER_SCRIPT"; then
  echo "SetupMattUser failed. Exiting."
  exit 1
fi

ensure_repo_location_for_target_user

if [[ ! -d "$DROPLET_SCRIPTS_DIR" ]]; then
  echo "No droplet scripts directory found at: $DROPLET_SCRIPTS_DIR"
  echo "Droplet setup complete."
  exit 0
fi

mapfile -t DISCOVERED_SCRIPTS < <(find "$DROPLET_SCRIPTS_DIR" -type f -name "*.sh" | sort)

if [[ ${#DISCOVERED_SCRIPTS[@]} -eq 0 ]]; then
  echo "No droplet scripts found in: $DROPLET_SCRIPTS_DIR"
  echo "Droplet setup complete."
  exit 0
fi

for script_path in "${DISCOVERED_SCRIPTS[@]}"; do
  relative_script="${script_path#"$DROPLET_SCRIPTS_DIR"/}"

  if ask_yes_no "Run $relative_script?"; then
    SELECTED_SCRIPTS+=("$script_path")
  fi
done

if [[ ${#SELECTED_SCRIPTS[@]} -eq 0 ]]; then
  echo "No scripts selected."
  echo "Droplet setup complete."
  exit 0
fi

echo "Running selected droplet scripts..."
for script_path in "${SELECTED_SCRIPTS[@]}"; do
  if [[ ! -f "$script_path" ]]; then
    echo "Skipping missing script: $script_path"
    continue
  fi

  echo "Running: $(basename "$script_path")"
  run_as_target_user bash "$script_path"
done

echo "Droplet setup complete."
