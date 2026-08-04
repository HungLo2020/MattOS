#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
KONSAVE_SETUP_SCRIPT="$REPO_ROOT/miniscripts/notautorun/KonsaveSetup.sh"
UPLOAD_SCRIPT="$REPO_ROOT/miniscripts/notautorun/UploadKonsaveProfiles.sh"
KDE_PROFILES_DIR="$REPO_ROOT/KDEProfiles"
DEFAULT_PROFILE_NAME="HungLoStandard"
PROFILE_NAME=""

if [[ ! -f "$KONSAVE_SETUP_SCRIPT" ]]; then
  echo "Error: Konsave setup script not found: $KONSAVE_SETUP_SCRIPT"
  exit 1
fi

echo "Running Konsave setup first..."
bash "$KONSAVE_SETUP_SCRIPT"

export PATH="$HOME/.local/bin:$PATH"
KONSAVE_CMD=""
if command -v konsave >/dev/null 2>&1; then
  KONSAVE_CMD="konsave"
elif command -v pipx >/dev/null 2>&1; then
  KONSAVE_CMD="pipx run konsave"
else
  echo "Error: konsave not found in PATH and pipx is unavailable."
  exit 1
fi

if ! mkdir -p "$KDE_PROFILES_DIR"; then
  echo "Error: Could not create KDE profiles directory: $KDE_PROFILES_DIR"
  exit 1
fi

if [[ ! -w "$KDE_PROFILES_DIR" ]]; then
  echo "Error: KDE profiles directory is not writable: $KDE_PROFILES_DIR"
  exit 1
fi

read -r -p "Enter konsave profile name [${DEFAULT_PROFILE_NAME}]: " PROFILE_NAME
if [[ -z "$PROFILE_NAME" ]]; then
  PROFILE_NAME="$DEFAULT_PROFILE_NAME"
fi

echo "Saving current KDE configuration as profile: $PROFILE_NAME"
$KONSAVE_CMD -s "$PROFILE_NAME"

echo "Exporting profile to $KDE_PROFILES_DIR"
pushd "$KDE_PROFILES_DIR" >/dev/null
if $KONSAVE_CMD -e "$PROFILE_NAME"; then
  if [[ -f "$PROFILE_NAME.knsv" ]]; then
    echo "Saved profile export: $KDE_PROFILES_DIR/$PROFILE_NAME.knsv"
  else
    latest_export="$(ls -1t *.knsv 2>/dev/null | head -n1 || true)"
    if [[ -n "$latest_export" ]]; then
      echo "Saved profile export: $KDE_PROFILES_DIR/$latest_export"
    else
      echo "Profile exported, but .knsv output filename could not be confirmed."
    fi
  fi
else
  popd >/dev/null
  echo "Error: Failed to export profile with konsave -e."
  exit 1
fi
popd >/dev/null

UPLOAD_CHOICE="n"
read -r -p "Upload profiles to GitHub Releases now? [y/N]: " UPLOAD_CHOICE

case "$UPLOAD_CHOICE" in
  [Yy]|[Yy][Ee][Ss])
    if [[ ! -f "$UPLOAD_SCRIPT" ]]; then
      echo "Error: Upload script not found: $UPLOAD_SCRIPT"
      exit 1
    fi

    echo "Uploading profiles to GitHub Releases..."
    bash "$UPLOAD_SCRIPT"
    ;;
  *)
    echo "Skipping upload."
    ;;
esac

echo "Done."
