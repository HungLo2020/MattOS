#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SERVER_SCRIPTS_DIR="$SCRIPT_DIR/miniscripts/server"

DISCOVERED_SCRIPTS=()
SELECTED_SCRIPTS=()

ask_yes_no() {
  local prompt="$1"
  local answer

  while true; do
    read -r -p "$prompt (y/n): " answer
    case "$answer" in
      [Yy]) return 0 ;;
      [Nn]) return 1 ;;
      *) echo "Please enter y or n." ;;
    esac
  done
}

if [[ ! -d "$SERVER_SCRIPTS_DIR" ]]; then
  echo "No server scripts directory found at: $SERVER_SCRIPTS_DIR"
  exit 0
fi

mapfile -t DISCOVERED_SCRIPTS < <(find "$SERVER_SCRIPTS_DIR" -maxdepth 1 -type f -name "*.sh" | sort)

if [[ ${#DISCOVERED_SCRIPTS[@]} -eq 0 ]]; then
  echo "No server scripts found in: $SERVER_SCRIPTS_DIR"
  exit 0
fi

echo "=== Server Scripts ==="
for script_path in "${DISCOVERED_SCRIPTS[@]}"; do
  echo "- ${script_path#"$SERVER_SCRIPTS_DIR"/}"
done
echo

for script_path in "${DISCOVERED_SCRIPTS[@]}"; do
  script_name="${script_path#"$SERVER_SCRIPTS_DIR"/}"
  if ask_yes_no "Run ${script_name}?"; then
    SELECTED_SCRIPTS+=("$script_path")
  fi
done

echo
if [[ ${#SELECTED_SCRIPTS[@]} -eq 0 ]]; then
  echo "No scripts selected."
  echo "Server manager complete."
  exit 0
fi

echo "=== Execution Phase ==="
for script_path in "${SELECTED_SCRIPTS[@]}"; do
  script_name="${script_path#"$SERVER_SCRIPTS_DIR"/}"

  if [[ ! -f "$script_path" ]]; then
    echo "Skipping missing script: $script_name"
    continue
  fi

  echo "Running: $script_name"
  bash "$script_path"
done

echo "Server manager complete."
