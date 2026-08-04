#!/usr/bin/env bash

set -euo pipefail

packages=(
  "btop"
)

echo "Installing default packages..."
for package in "${packages[@]}"; do
  sudo apt install -y "$package"
  echo "Installed $package"
done

echo "Done installing default packages."
