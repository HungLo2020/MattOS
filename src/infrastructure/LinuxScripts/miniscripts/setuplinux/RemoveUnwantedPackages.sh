#!/usr/bin/env bash

set -euo pipefail

unwanted_packages=(
  "plasma-vault"
  "krdc"
  "neochat"
  "konversation"
  "skanlite"
  "akregator"
  "dragonplayer"
  "gimp"
  "juk"
  "kdeconnect"
  "kmail"
  "kmouth"
  "konqueror"
  "korganizer"
  "kwrite"
  "kmahjongg"
  "kpat"
  "ksudoku"
  "katawa-shoujo"
  "anydesk"
)

echo "Removing unwanted packages..."
for package in "${unwanted_packages[@]}"; do
  if ! apt-cache show "$package" >/dev/null 2>&1; then
    echo "Skipping $package (package name not found)"
    continue
  fi

  if dpkg -s "$package" >/dev/null 2>&1; then
    sudo apt remove -y "$package"
    echo "Removed $package"
  else
    echo "Skipping $package (not installed)"
  fi
done

echo "Done removing unwanted packages."