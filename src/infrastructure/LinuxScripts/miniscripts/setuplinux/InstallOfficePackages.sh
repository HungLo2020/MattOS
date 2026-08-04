#!/usr/bin/env bash

set -euo pipefail

office_packages=(
  "libreoffice"
)

echo "Installing office packages..."
for package in "${office_packages[@]}"; do
  sudo apt install -y "$package"
  echo "Installed $package"
done

echo "Done installing office packages."