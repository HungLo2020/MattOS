#!/usr/bin/env bash

set -euo pipefail

TARGET_USER="${SUDO_USER:-$(id -un)}"
TARGET_HOME="$(getent passwd "$TARGET_USER" | cut -d: -f6)"
if [[ -z "$TARGET_HOME" ]]; then
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

HOME_DIR="$TARGET_HOME"
RCLONE_CONFIG="$HOME_DIR/.config/rclone/rclone.conf"
ONEDRIVE_DIR="$HOME_DIR/OneDrive"
LOCAL_WALLPAPER_DIR="$HOME_DIR/OneDrive-Local/Media/Wallpapers"
SERVICE_PATH="/etc/systemd/system/rclone-mount.service"
LOCK_DIR="$HOME_DIR/.cache/rclone"
LOCK_FILE="$LOCK_DIR/onedrive-wallpapers-bisync.lock"
LOG_DIR="$HOME_DIR/.local/state/rclone"
HOURLY_LOG="$LOG_DIR/onedrive-wallpapers-bisync.log"
RESYNC_LOG="$LOG_DIR/onedrive-wallpapers-bisync-resync.log"
CRON_BLOCK_BEGIN="# >>> LinuxScripts OneDriveRcloneSetup >>>"
CRON_BLOCK_END="# <<< LinuxScripts OneDriveRcloneSetup <<<"

HOURLY_BISYNC_CMD="/usr/bin/flock -n \"$LOCK_FILE\" /usr/bin/rclone bisync \"OneDrive:Media/Wallpapers\" \"$LOCAL_WALLPAPER_DIR\" --config \"$RCLONE_CONFIG\" --check-access --verbose >> \"$HOURLY_LOG\" 2>&1"
WEEKLY_RESYNC_CMD="/usr/bin/flock -n \"$LOCK_FILE\" /usr/bin/rclone bisync \"OneDrive:Media/Wallpapers\" \"$LOCAL_WALLPAPER_DIR\" --config \"$RCLONE_CONFIG\" --resync --check-access --verbose >> \"$RESYNC_LOG\" 2>&1"

DESIRED_CRON_ENTRIES=(
  "1 * * * * $HOURLY_BISYNC_CMD"
  "1 1 * * 1 $WEEKLY_RESYNC_CMD"
)

normalize_crontab() {
  sed 's/[[:space:]]\+$//' | sed '/^[[:space:]]*$/d'
}

strip_managed_cron_entries() {
  awk -v begin="$CRON_BLOCK_BEGIN" -v end="$CRON_BLOCK_END" '
    $0 == begin { in_block = 1; next }
    $0 == end { in_block = 0; next }
    in_block { next }
    /rclone bisync OneDrive:Media\/Wallpapers/ { next }
    { print }
  '
}

echo "Installing rclone..."
sudo apt install -y rclone

echo "Starting rclone config for user '$TARGET_USER'..."
while true; do
  run_as_target_user rclone config --config "$RCLONE_CONFIG" </dev/tty >/dev/tty 2>&1
  if run_as_target_user rclone listremotes --config "$RCLONE_CONFIG" 2>/dev/null | grep -qE '^OneDrive:$'; then
    echo "Found remote 'OneDrive:' in $RCLONE_CONFIG"
    break
  fi

  echo "Remote 'OneDrive:' was not found."
  echo "Press Enter to run rclone config again, or Ctrl+C to exit."
  read -r </dev/tty
done

echo "Ensuring mountpoint exists at $ONEDRIVE_DIR..."
mkdir -p "$ONEDRIVE_DIR"
chown "$TARGET_USER:$TARGET_GROUP" "$ONEDRIVE_DIR"

echo "Creating or updating systemd service at $SERVICE_PATH..."
sudo tee "$SERVICE_PATH" > /dev/null <<EOF
[Unit]
Description=Rclone mount for OneDrive
After=network-online.target

[Service]
Type=simple
ExecStart=/usr/bin/rclone mount OneDrive: $ONEDRIVE_DIR --vfs-cache-mode writes
ExecStop=/bin/fusermount -uz $ONEDRIVE_DIR
Restart=on-failure
User=$TARGET_USER
Group=$TARGET_GROUP

[Install]
WantedBy=default.target
EOF

echo "Enabling and starting rclone-mount.service..."
sudo systemctl daemon-reload
sudo systemctl enable rclone-mount.service
sudo systemctl start rclone-mount.service

echo "Ensuring local wallpaper sync directory exists..."
mkdir -p "$LOCAL_WALLPAPER_DIR"
chown -R "$TARGET_USER:$TARGET_GROUP" "$HOME_DIR/OneDrive-Local"
run_as_target_user mkdir -p "$LOCK_DIR" "$LOG_DIR"

echo "Running initial one-time bisync resync..."
run_as_target_user rclone bisync OneDrive:Media/Wallpapers "$LOCAL_WALLPAPER_DIR" --config "$RCLONE_CONFIG" --resync --check-access --verbose

if [[ "$(id -u)" -eq 0 ]]; then
  current_crontab="$(crontab -u "$TARGET_USER" -l 2>/dev/null || true)"
else
  current_crontab="$(crontab -l 2>/dev/null || true)"
fi

missing_entries=()
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

echo "Updated managed rclone cron block with ${#DESIRED_CRON_ENTRIES[@]} entries."

echo "OneDrive rclone setup complete."