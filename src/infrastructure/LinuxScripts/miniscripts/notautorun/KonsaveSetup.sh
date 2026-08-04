#!/usr/bin/env bash

set -euo pipefail

echo "Installing dependencies for konsave..."
sudo apt install -y python3 python3-pip python3-venv pipx

echo "Installing/Updating konsave..."
pipx install --force konsave

echo "KonsaveSetup complete."
echo "If needed, add ~/.local/bin to PATH so 'konsave' is directly runnable."