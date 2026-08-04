#!/usr/bin/env bash

set -euo pipefail

echo "Installing prerequisites for Tailscale..."
sudo apt update
sudo apt install -y curl

echo "Installing Tailscale..."
if [[ "$EUID" -eq 0 ]]; then
  curl -fsSL https://tailscale.com/install.sh | sh
else
  curl -fsSL https://tailscale.com/install.sh | sudo sh
fi

echo "Bringing Tailscale up..."
sudo tailscale up

echo "Installing OpenSSH Server..."
sudo apt update
sudo apt install -y openssh-server

echo "Enabling and starting sshd service..."
sudo systemctl enable ssh
sudo systemctl start ssh

echo "Headless setup complete. Tailscale and OpenSSH Server are installed and running."
