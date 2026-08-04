#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
KONSAVE_SETUP_SCRIPT="$SCRIPT_DIR/KonsaveSetup.sh"
KDE_PROFILES_DIR="$REPO_ROOT/KDEProfiles"
DEFAULT_PROFILE_NAME="HungLoStandard"

run_konsave() {
  if command -v konsave >/dev/null 2>&1; then
    konsave "$@"
  elif command -v pipx >/dev/null 2>&1; then
    pipx run konsave "$@"
  else
    echo "Error: konsave not found in PATH and pipx is unavailable."
    exit 1
  fi
}

if [[ ! -f "$KONSAVE_SETUP_SCRIPT" ]]; then
  echo "Error: Konsave setup script not found: $KONSAVE_SETUP_SCRIPT"
  exit 1
fi

echo "Running Konsave setup first..."
bash "$KONSAVE_SETUP_SCRIPT"

if ! mkdir -p "$KDE_PROFILES_DIR"; then
  echo "Error: Could not create KDE profiles directory: $KDE_PROFILES_DIR"
  exit 1
fi

shopt -s nullglob
profile_files=("$KDE_PROFILES_DIR"/*.knsv)
shopt -u nullglob

declare -a profile_names=()
for profile_path in "${profile_files[@]}"; do
  profile_names+=("$(basename "${profile_path%.knsv}")")
done

hung_present=false
for profile_name in "${profile_names[@]}"; do
  if [[ "$profile_name" == "$DEFAULT_PROFILE_NAME" ]]; then
    hung_present=true
    break
  fi
done

declare -a other_profiles=()
for profile_name in "${profile_names[@]}"; do
  if [[ "$profile_name" == "$DEFAULT_PROFILE_NAME" ]]; then
    continue
  fi
  other_profiles+=("$profile_name")
done

if [[ ${#other_profiles[@]} -gt 0 ]]; then
  mapfile -t other_profiles < <(printf "%s\n" "${other_profiles[@]}" | sort -f)
fi

declare -a menu_profiles=()
menu_profiles+=("__DO_NOT_APPLY__")
if [[ "$hung_present" == true ]]; then
  menu_profiles+=("$DEFAULT_PROFILE_NAME")
fi
for profile_name in "${other_profiles[@]}"; do
  menu_profiles+=("$profile_name")
done

if [[ "$hung_present" == true ]]; then
  default_selection=2
else
  default_selection=1
fi

echo "Available KDE profiles:"
echo "1) Do not apply any profile"

menu_index=2
if [[ "$hung_present" == true ]]; then
  echo "$menu_index) $DEFAULT_PROFILE_NAME"
  ((menu_index++))
fi

for profile_name in "${other_profiles[@]}"; do
  echo "$menu_index) $profile_name"
  ((menu_index++))
done

selected_option=""
while true; do
  read -r -p "Select profile number [${default_selection}]: " selected_option

  if [[ -z "$selected_option" ]]; then
    selected_option="$default_selection"
  fi

  if [[ "$selected_option" =~ ^[0-9]+$ ]] && (( selected_option >= 1 )) && (( selected_option <= ${#menu_profiles[@]} )); then
    break
  fi

  echo "Please enter a valid number between 1 and ${#menu_profiles[@]}."
done

PROFILE_NAME="${menu_profiles[$((selected_option - 1))]}"

if [[ "$PROFILE_NAME" == "__DO_NOT_APPLY__" ]]; then
  echo "Skipping profile apply by user selection."
  exit 0
fi

PROFILE_FILE="$KDE_PROFILES_DIR/$PROFILE_NAME.knsv"

if [[ -f "$PROFILE_FILE" ]]; then
  echo "Importing profile from $PROFILE_FILE"
  run_konsave -i "$PROFILE_FILE"
else
  echo "No export file found at $PROFILE_FILE, attempting to apply existing installed profile by name."
fi

echo "Applying profile: $PROFILE_NAME"
run_konsave -a "$PROFILE_NAME"

echo "Done applying profile '$PROFILE_NAME'."