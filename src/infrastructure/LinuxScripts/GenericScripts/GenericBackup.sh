#!/usr/bin/env bash

set -euo pipefail

DIR_TO_BACKUP=""
DIR_TO_BACKUP_TO=""
EXCLUDES=("node_modules" "*.tmp" ".git")

if [[ -z "$DIR_TO_BACKUP" ]]; then
  echo "Error: DIR_TO_BACKUP is not set."
  exit 1
fi

if [[ -z "$DIR_TO_BACKUP_TO" ]]; then
  echo "Error: DIR_TO_BACKUP_TO is not set."
  exit 1
fi

if [[ ! -d "$DIR_TO_BACKUP" ]]; then
  echo "Error: DIR_TO_BACKUP does not exist or is not a directory: $DIR_TO_BACKUP"
  exit 1
fi

backup_dir="${DIR_TO_BACKUP%/}"
backup_name="$(basename "$backup_dir")"
timestamp="$(date +%Y-%m-%d_%H-%M-%S)"
zip_name="${backup_name}_${timestamp}.zip"
temp_zip_path="/tmp/$zip_name"

exclude_args=()
for pattern in "${EXCLUDES[@]}"; do
  exclude_args+=("-x" "$pattern")
done

cd "$DIR_TO_BACKUP"
zip -r "$temp_zip_path" . "${exclude_args[@]}"
echo "Created zip: $temp_zip_path"

mkdir -p "$DIR_TO_BACKUP_TO"
mv "$temp_zip_path" "$DIR_TO_BACKUP_TO"
echo "Moved to: $DIR_TO_BACKUP_TO/$zip_name"
