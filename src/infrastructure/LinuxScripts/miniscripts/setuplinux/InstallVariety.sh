#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
TARGET_USER="${SUDO_USER:-$(id -un)}"
TARGET_HOME="$(getent passwd "$TARGET_USER" | cut -d: -f6)"

if [[ -z "$TARGET_HOME" ]]; then
  echo "Error: Could not resolve home directory for user '$TARGET_USER'."
  exit 1
fi

SOURCE_CONF="$REPO_ROOT/resources/variety.conf"
TARGET_DIR="$TARGET_HOME/.config/variety"
TARGET_CONF="$TARGET_DIR/variety.conf"

echo "Installing Variety..."
sudo apt update
sudo apt install -y variety

if [[ ! -f "$SOURCE_CONF" ]]; then
  echo "Error: Could not find source config at $SOURCE_CONF"
  exit 1
fi

echo "Copying variety.conf to $TARGET_CONF..."
mkdir -p "$TARGET_DIR"
cp "$SOURCE_CONF" "$TARGET_CONF"

if [[ "$(id -u)" -eq 0 ]]; then
  chown -R "$TARGET_USER:$(id -gn "$TARGET_USER")" "$TARGET_HOME/.config/variety"
fi

echo "Done. Variety is installed and config copied."