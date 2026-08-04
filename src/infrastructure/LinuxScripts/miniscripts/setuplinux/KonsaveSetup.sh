#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
NOTAUTORUN_DIR="$SCRIPT_DIR/../notautorun"

KONSAVE_SETUP_SCRIPT="$NOTAUTORUN_DIR/KonsaveSetup.sh"
DOWNLOAD_PROFILES_SCRIPT="$NOTAUTORUN_DIR/DownloadKonsaveProfiles.sh"
APPLY_PROFILE_SCRIPT="$NOTAUTORUN_DIR/ApplyKonsaveProfile.sh"

# Install Papirus Icons
sudo apt install -y papirus-icon-theme

for required_script in "$KONSAVE_SETUP_SCRIPT" "$DOWNLOAD_PROFILES_SCRIPT" "$APPLY_PROFILE_SCRIPT"; do
  if [[ ! -f "$required_script" ]]; then
    echo "Error: Required script not found: $required_script"
    exit 1
  fi
done

echo "Running Konsave setup..."
bash "$KONSAVE_SETUP_SCRIPT"

echo "Downloading konsave profiles from GitHub releases..."
bash "$DOWNLOAD_PROFILES_SCRIPT"

echo "Applying konsave profile..."
bash "$APPLY_PROFILE_SCRIPT"

echo "Done."