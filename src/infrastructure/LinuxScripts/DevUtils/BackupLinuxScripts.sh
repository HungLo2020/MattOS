#!/usr/bin/env bash

# Archive this repository to the configured personal OneDrive destination.
# Change DEST_DIR before using this script on another machine or account.
set -euo pipefail

DEST_DIR="/mnt/storage/OneDrive/Apps/Programming/LinuxScripts/"
EXCLUDES=("node_modules" "*.tmp" ".git")

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/.." && pwd)"
repo_name="$(basename "$repo_root")"
timestamp="$(date +%Y-%m-%d_%H-%M-%S)"
zip_name="${repo_name}_${timestamp}.zip"
temp_zip_path="/tmp/$zip_name"

exclude_args=()
for pattern in "${EXCLUDES[@]}"; do
  exclude_args+=("-x" "$pattern")
done

cd "$repo_root"
zip -r "$temp_zip_path" . "${exclude_args[@]}"
echo "Created zip: $temp_zip_path"

mkdir -p "$DEST_DIR"
mv "$temp_zip_path" "$DEST_DIR"
echo "Moved to: $DEST_DIR$zip_name"