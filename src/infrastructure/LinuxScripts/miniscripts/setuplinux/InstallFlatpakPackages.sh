#!/usr/bin/env bash

set -euo pipefail

flatpak_packages=(
  "com.github.tchx84.Flatseal"
  "io.missioncenter.MissionCenter"
  "com.discordapp.Discord"
)

echo "Installing flatpak..."
sudo apt install -y flatpak

echo "Adding Flathub remote..."
sudo flatpak remote-add --if-not-exists flathub https://flathub.org/repo/flathub.flatpakrepo

echo "Installing Flatpak packages..."
for package in "${flatpak_packages[@]}"; do
  sudo flatpak install -y flathub "$package"
  echo "Installed $package"
done

echo "Done installing Flatpak packages."
