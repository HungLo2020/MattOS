#!/usr/bin/env bash

set -euo pipefail

repair_yarn_repo_if_needed() {
  local yarn_source_file="/etc/apt/sources.list.d/yarn.list"
  local yarn_keyring="/etc/apt/keyrings/yarn-archive-keyring.asc"
  local yarn_source_line="deb [signed-by=${yarn_keyring}] https://dl.yarnpkg.com/debian/ stable main"

  if [[ ! -f "$yarn_source_file" ]]; then
    return 0
  fi

  if ! grep -q "dl.yarnpkg.com/debian" "$yarn_source_file"; then
    return 0
  fi

  echo "Detected Yarn apt repo. Ensuring keyring is configured..."
  sudo mkdir -p /etc/apt/keyrings

  if command -v curl >/dev/null 2>&1; then
    curl -fsSL https://dl.yarnpkg.com/debian/pubkey.gpg | sudo tee "$yarn_keyring" >/dev/null
  elif command -v wget >/dev/null 2>&1; then
    wget -qO- https://dl.yarnpkg.com/debian/pubkey.gpg | sudo tee "$yarn_keyring" >/dev/null
  else
    echo "Error: Neither curl nor wget is available to fetch Yarn repository key." >&2
    exit 1
  fi

  echo "$yarn_source_line" | sudo tee "$yarn_source_file" >/dev/null
}

repair_yarn_repo_if_needed

sudo apt update

sudo apt install -y curl
sudo apt install -y openssh-client

echo "Installing Tailscale..."
if [[ "$EUID" -eq 0 ]]; then
  curl -fsSL https://tailscale.com/install.sh | sh
else
  curl -fsSL https://tailscale.com/install.sh | sudo sh
fi

start_tailscaled() {
  tailscaled_is_ready() {
    pgrep -x tailscaled >/dev/null 2>&1 && sudo test -S /var/run/tailscale/tailscaled.sock
  }

  if tailscaled_is_ready; then
    return 0
  fi

  if command -v systemctl >/dev/null 2>&1; then
    sudo systemctl start tailscaled >/dev/null 2>&1 || true
    sleep 1
    if tailscaled_is_ready; then
      return 0
    fi
  fi

  if command -v service >/dev/null 2>&1; then
    sudo service tailscaled start >/dev/null 2>&1 || true
    sleep 1
    if tailscaled_is_ready; then
      return 0
    fi

    sudo service start tailscaled >/dev/null 2>&1 || true
    sleep 1
    if tailscaled_is_ready; then
      return 0
    fi
  fi

  if ! command -v tailscaled >/dev/null 2>&1; then
    echo "Error: tailscaled binary not found." >&2
    return 1
  fi

  sudo mkdir -p /var/lib/tailscale /var/run/tailscale

  sudo nohup tailscaled \
    --state=/var/lib/tailscale/tailscaled.state \
    --socket=/var/run/tailscale/tailscaled.sock \
    >/tmp/tailscaled.log 2>&1 &
  sleep 2
  if tailscaled_is_ready; then
    return 0
  fi

  sudo nohup tailscaled \
    --state=/var/lib/tailscale/tailscaled.state \
    --socket=/var/run/tailscale/tailscaled.sock \
    --tun=userspace-networking \
    >/tmp/tailscaled.log 2>&1 &
  sleep 2
  tailscaled_is_ready
}

echo "Starting tailscaled service..."
if ! start_tailscaled; then
  echo "Error: Failed to start tailscaled. Check /tmp/tailscaled.log for details." >&2
  exit 1
fi

sudo tailscale up