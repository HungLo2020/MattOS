#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CREATE_REPOS_SCRIPT="$SCRIPT_DIR/../notautorun/CreateReposDir.sh"

echo "Ensuring repos directory exists..."
if [[ ! -x "$CREATE_REPOS_SCRIPT" ]]; then
  echo "Error: Required script not found or not executable: $CREATE_REPOS_SCRIPT"
  exit 1
fi
"$CREATE_REPOS_SCRIPT"

# Install Regular APT packages for development work
packages=(
  "cura"
  "virt-manager"
  "gh"
  "ripgrep"
)

echo "Installing apt dev packages..."
for package in "${packages[@]}"; do
  sudo apt install -y "$package"
  echo "Installed $package"
done

echo "Done installing dev packages."

# Check if vscode is installed
if command -v code >/dev/null 2>&1; then
  echo "VS Code is already installed."
else
  echo "Installing VS Code via official Microsoft APT repository..."
  sudo apt install -y wget gpg apt-transport-https
  wget -qO- https://packages.microsoft.com/keys/microsoft.asc | gpg --dearmor > packages.microsoft.gpg
  sudo install -D -o root -g root -m 644 packages.microsoft.gpg /etc/apt/keyrings/packages.microsoft.gpg
  rm -f packages.microsoft.gpg
  echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/packages.microsoft.gpg] https://packages.microsoft.com/repos/code stable main" | sudo tee /etc/apt/sources.list.d/vscode.list >/dev/null
  sudo apt update
  sudo apt install -y code
fi

# Check if intellij-idea is installed via snap
#if snap list intellij-idea >/dev/null 2>&1; then
#  echo "intellij-idea is already installed via snap."
#else
#  echo "Installing intellij-idea via snap..."
#  sudo snap install intellij-idea --classic
#fi
# I am using vs code now

echo "InstallDevUtils complete."