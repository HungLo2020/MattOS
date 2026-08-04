#!/usr/bin/env bash

set -euo pipefail

game_packages=(
  "steam"
  "kmines"
)

echo "Installing game packages..."
for package in "${game_packages[@]}"; do
  sudo apt install -y "$package"
  echo "Installed $package"
done

echo "Installing Basalt..."
curl -fsSL https://raw.githubusercontent.com/HungLo2020/Basalt/main/Install.sh | bash

echo "Done installing game packages."