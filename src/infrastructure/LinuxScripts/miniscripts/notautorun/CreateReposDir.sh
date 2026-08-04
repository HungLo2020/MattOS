#!/usr/bin/env bash

set -euo pipefail

TARGET_DIR="$HOME/Documents/Repos"

if [[ -d "$TARGET_DIR" ]]; then
  echo "Directory already exists: $TARGET_DIR"
  exit 0
fi

mkdir -p "$TARGET_DIR"
echo "Created directory: $TARGET_DIR"