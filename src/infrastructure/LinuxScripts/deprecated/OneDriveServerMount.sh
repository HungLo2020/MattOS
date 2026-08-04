#!/usr/bin/env bash

set -euo pipefail

TARGET_USER="${SUDO_USER:-$(id -un)}"
TARGET_HOME="$(getent passwd "$TARGET_USER" | cut -d: -f6)"
if [[ -z "$TARGET_HOME" || ! -d "$TARGET_HOME" ]]; then
  echo "Error: Could not resolve home directory for user '$TARGET_USER'."
  exit 1
fi
TARGET_GROUP="$(id -gn "$TARGET_USER")"

run_as_target_user() {
  if [[ "$(id -un)" == "$TARGET_USER" ]]; then
    "$@"
  else
    sudo -u "$TARGET_USER" "$@"
  fi
}

STORAGE_ROOT="/srv/storage"
ONEDRIVE_DIR="$STORAGE_ROOT/OneDrive"
RCLONE_CONFIG="$TARGET_HOME/.config/rclone/rclone.conf"
RCLONE_REMOTE="OneDrive:"

LOCK_DIR="$TARGET_HOME/.cache/rclone"
LOCK_FILE="$LOCK_DIR/onedrive-server-bisync.lock"
LOG_DIR="$TARGET_HOME/.local/state/rclone"
BISYNC_LOG="$LOG_DIR/onedrive-server-bisync.log"
RESYNC_LOG="$LOG_DIR/onedrive-server-bisync-resync.log"

CRON_BLOCK_BEGIN="# >>> LinuxScripts OneDriveServer >>>"
CRON_BLOCK_END="# <<< LinuxScripts OneDriveServer <<<"

BISYNC_CMD="/usr/bin/flock -n \"$LOCK_FILE\" /usr/bin/rclone bisync \"$RCLONE_REMOTE\" \"$ONEDRIVE_DIR\" --config \"$RCLONE_CONFIG\" --exclude \"Personal Vault/**\" --exclude \"Personal Vault\" --check-access --verbose >> \"$BISYNC_LOG\" 2>&1"
RESYNC_CMD="/usr/bin/flock -n \"$LOCK_FILE\" /usr/bin/rclone bisync \"$RCLONE_REMOTE\" \"$ONEDRIVE_DIR\" --config \"$RCLONE_CONFIG\" --exclude \"Personal Vault/**\" --exclude \"Personal Vault\" --resync --check-access --verbose >> \"$RESYNC_LOG\" 2>&1"

DESIRED_CRON_ENTRIES=(
  "*/5 * * * * $BISYNC_CMD"
  "17 2 * * 1 $RESYNC_CMD"
)

normalize_crontab() {
  sed 's/[[:space:]]\+$//' | sed '/^[[:space:]]*$/d'
}

strip_managed_cron_entries() {
  awk -v begin="$CRON_BLOCK_BEGIN" -v end="$CRON_BLOCK_END" '
    $0 == begin { in_block = 1; next }
    $0 == end { in_block = 0; next }
    in_block { next }
    { print }
  '
}

if [[ ! -d "$STORAGE_ROOT" ]]; then
  echo "Error: '$STORAGE_ROOT' does not exist. Hard-failing because storage is not present."
  exit 1
fi

if [[ ! -d "$ONEDRIVE_DIR" ]]; then
  echo "'$ONEDRIVE_DIR' does not exist; creating it..."
  sudo mkdir -p "$ONEDRIVE_DIR"
  sudo chown "$TARGET_USER:$TARGET_GROUP" "$ONEDRIVE_DIR"
fi

if ! command -v rclone >/dev/null 2>&1; then
  echo "Error: rclone is not installed."
  exit 1
fi

if ! command -v crontab >/dev/null 2>&1; then
  echo "Error: crontab command is not available. Install cron before running this script."
  exit 1
fi

echo "Ensuring rclone remote '$RCLONE_REMOTE' exists for user '$TARGET_USER'..."
while true; do
  if run_as_target_user rclone listremotes --config "$RCLONE_CONFIG" 2>/dev/null | grep -qE '^OneDrive:$'; then
    echo "Found remote 'OneDrive:' in $RCLONE_CONFIG"
    break
  fi

  echo "Remote 'OneDrive:' was not found."
  echo "Starting rclone config for user '$TARGET_USER'..."
  run_as_target_user rclone config --config "$RCLONE_CONFIG" </dev/tty >/dev/tty 2>&1
done

run_as_target_user mkdir -p "$LOCK_DIR" "$LOG_DIR"

if [[ "$(id -u)" -eq 0 ]]; then
  current_crontab="$(crontab -u "$TARGET_USER" -l 2>/dev/null || true)"
else
  current_crontab="$(crontab -l 2>/dev/null || true)"
fi

cleaned_crontab="$(printf "%s\n" "$current_crontab" | strip_managed_cron_entries | normalize_crontab || true)"

managed_block="$CRON_BLOCK_BEGIN"
for entry in "${DESIRED_CRON_ENTRIES[@]}"; do
  managed_block+=$'\n'"$entry"
done
managed_block+=$'\n'"$CRON_BLOCK_END"

if [[ -n "$cleaned_crontab" ]]; then
  new_crontab="$cleaned_crontab"$'\n'"$managed_block"
else
  new_crontab="$managed_block"
fi

if [[ "$(id -u)" -eq 0 ]]; then
  printf "%s\n" "$new_crontab" | crontab -u "$TARGET_USER" -
else
  printf "%s\n" "$new_crontab" | crontab -
fi

echo "Updated managed OneDriveServer cron block with ${#DESIRED_CRON_ENTRIES[@]} entries."

if [[ "$(id -u)" -eq 0 ]]; then
  installed_crontab="$(crontab -u "$TARGET_USER" -l 2>/dev/null || true)"
else
  installed_crontab="$(crontab -l 2>/dev/null || true)"
fi

if ! grep -qF "$CRON_BLOCK_BEGIN" <<<"$installed_crontab" || ! grep -qF "$CRON_BLOCK_END" <<<"$installed_crontab"; then
  echo "Error: OneDriveServer cron block verification failed after write."
  exit 1
fi

echo "Verified OneDriveServer cron block is present in crontab."

echo "Running initial one-time bisync resync for '$RCLONE_REMOTE' <-> '$ONEDRIVE_DIR'..."
if ! run_as_target_user rclone bisync "$RCLONE_REMOTE" "$ONEDRIVE_DIR" --config "$RCLONE_CONFIG" --exclude "Personal Vault/**" --exclude "Personal Vault" --resync --check-access --verbose; then
  echo "Warning: initial bisync resync failed. Cron sync schedule is still installed; check logs and rerun once issues are resolved."
fi

echo "OneDrive server sync setup complete."
