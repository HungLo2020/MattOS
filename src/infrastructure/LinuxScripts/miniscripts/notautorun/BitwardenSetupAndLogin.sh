#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
BW_MASTER_PASSWORD_FILE="$PROJECT_ROOT/.bw_master_password"

BITWARDEN_SNAP_NAME="bitwarden"

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

echo "Installing prerequisites for Bitwarden via Snap..."
sudo apt update
sudo apt install -y snapd

if ! systemctl is-active --quiet snapd; then
  echo "Starting snapd service..."
  sudo systemctl enable --now snapd
fi

if snap list "$BITWARDEN_SNAP_NAME" >/dev/null 2>&1; then
  echo "Bitwarden snap is already installed."
else
  echo "Installing Bitwarden desktop app via snap..."
  if ! sudo snap install "$BITWARDEN_SNAP_NAME"; then
    echo "Error: failed to install Bitwarden via snap."
    exit 1
  fi
fi

# Optional CLI install to allow true login-state checks.
if ! command -v bw >/dev/null 2>&1; then
  if snap info bw >/dev/null 2>&1; then
    sudo snap install bw || true
  fi
fi

if command -v bw >/dev/null 2>&1; then
  echo "Bitwarden CLI detected; enforcing authenticated login..."

  status="$(bw status 2>/dev/null | python3 -c 'import json,sys; print(json.load(sys.stdin).get("status","unknown"))' 2>/dev/null || echo "unknown")"

  while [[ "$status" == "unauthenticated" || "$status" == "unknown" ]]; do
    echo "Launching interactive 'bw login'..."
    bw login </dev/tty >/dev/tty 2>&1 || true
    status="$(bw status 2>/dev/null | python3 -c 'import json,sys; print(json.load(sys.stdin).get("status","unknown"))' 2>/dev/null || echo "unknown")"

    if [[ "$status" == "unauthenticated" || "$status" == "unknown" ]]; then
      echo "Bitwarden login is still not complete."
      if ! ask_yes_no "Try login again now?"; then
        echo "Bitwarden login is required. Exiting setup."
        exit 1
      fi
    fi
  done

  echo "Bitwarden login complete (status: $status)."

  # ── Save master password for non-interactive vault unlock ─────────────────
  echo "The master password is needed so that later scripts can unlock the"
  echo "vault non-interactively. It will be stored at:"
  echo "  $BW_MASTER_PASSWORD_FILE"
  echo "(This file is chmod 600 and is excluded from git.)"
  bw_setup_master_pw=""
  while [[ -z "$bw_setup_master_pw" ]]; do
    read -r -s -p "Enter your Bitwarden master password: " bw_setup_master_pw </dev/tty
    echo
    if [[ -z "$bw_setup_master_pw" ]]; then
      echo "Password cannot be empty."
    fi
  done
  (umask 077; printf '%s' "$bw_setup_master_pw" > "$BW_MASTER_PASSWORD_FILE") \
    || { echo "Error: failed to save master password to $BW_MASTER_PASSWORD_FILE"; exit 1; }
  unset bw_setup_master_pw
  echo "Master password saved to: $BW_MASTER_PASSWORD_FILE"

  exit 0
fi

echo "Bitwarden CLI not available; falling back to desktop login confirmation."

if command -v bitwarden >/dev/null 2>&1; then
  if [[ -n "${DISPLAY:-}" || -n "${WAYLAND_DISPLAY:-}" ]]; then
    nohup bitwarden >/dev/null 2>&1 &
    echo "Opened Bitwarden desktop app."
  fi
fi

while true; do
  if ask_yes_no "Have you signed in to Bitwarden successfully?"; then
    break
  fi
  echo "Bitwarden login is required before continuing setup."
done

echo "Bitwarden desktop login confirmed."
