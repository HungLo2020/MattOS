#!/usr/bin/env bash

set -euo pipefail

TARGET_USER="matt"

if [[ "$(id -u)" -ne 0 ]] && ! command -v sudo >/dev/null 2>&1; then
  echo "Error: sudo is required to manage users when not running as root."
  exit 1
fi

run_privileged() {
  if [[ "$(id -u)" -eq 0 ]]; then
    "$@"
  else
    sudo "$@"
  fi
}

if id "$TARGET_USER" >/dev/null 2>&1; then
  echo "User '$TARGET_USER' already exists."
else
  echo "Creating user '$TARGET_USER'..."
  run_privileged useradd -m -s /bin/bash "$TARGET_USER"

  echo "Set password for '$TARGET_USER':"
  run_privileged passwd "$TARGET_USER"
fi

TARGET_HOME="$(getent passwd "$TARGET_USER" | cut -d: -f6)"
if [[ -n "$TARGET_HOME" ]]; then
  run_privileged mkdir -p "$TARGET_HOME"
  run_privileged chown -R "$TARGET_USER:$TARGET_USER" "$TARGET_HOME"
fi

if id -nG "$TARGET_USER" | tr ' ' '\n' | grep -qx "sudo"; then
  echo "User '$TARGET_USER' already has sudo privileges."
else
  echo "Granting sudo privileges to '$TARGET_USER'..."
  run_privileged usermod -aG sudo "$TARGET_USER"
fi

echo "SetupMattUser complete."
