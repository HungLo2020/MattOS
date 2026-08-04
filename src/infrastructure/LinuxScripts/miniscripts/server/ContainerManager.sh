#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$SCRIPT_DIR"
while [[ "$REPO_ROOT" != "/" && ! -d "$REPO_ROOT/miniscripts/containers" ]]; do
  REPO_ROOT="$(dirname "$REPO_ROOT")"
done
CONTAINER_SCRIPTS_DIR="$REPO_ROOT/miniscripts/containers"

BLUE='\033[34m'
GREEN='\033[32m'
RED='\033[31m'
GRAY='\033[90m'
RESET='\033[0m'

status_color() {
  local status_lc
  status_lc="$(echo "$1" | tr '[:upper:]' '[:lower:]')"

  if [[ "$status_lc" == running* || "$status_lc" == up* ]]; then
    printf '%s' "$GREEN"
  elif [[ "$status_lc" == exited* || "$status_lc" == created* || "$status_lc" == paused* ]]; then
    printf '%s' "$GRAY"
  elif [[ "$status_lc" == dead* || "$status_lc" == restarting* || "$status_lc" == *"error"* || "$status_lc" == *"fail"* ]]; then
    printf '%s' "$RED"
  else
    printf '%s' "$GRAY"
  fi
}

print_container_table() {
  echo "=== System Containers ==="

  if ! command -v docker >/dev/null 2>&1; then
    echo "docker is not installed; cannot list containers."
    echo
    return 0
  fi

  local docker_cmd=(docker)
  if ! docker info >/dev/null 2>&1; then
    if sudo docker info >/dev/null 2>&1; then
      docker_cmd=(sudo docker)
    else
      echo "Cannot connect to Docker daemon; cannot list containers."
      echo
      return 0
    fi
  fi

  mapfile -t container_rows < <("${docker_cmd[@]}" ps -a --format '{{.Names}}|{{.Status}}')

  if [[ ${#container_rows[@]} -eq 0 ]]; then
    echo "No containers found."
    echo
    return 0
  fi

  printf "%-40s %s\n" "NAME" "STATUS"
  for row in "${container_rows[@]}"; do
    local name status color
    name="${row%%|*}"
    status="${row#*|}"
    color="$(status_color "$status")"
    printf "%b%-40s%b %b%s%b\n" "$BLUE" "$name" "$RESET" "$color" "$status" "$RESET"
  done
  echo
}

if [[ ! -d "$CONTAINER_SCRIPTS_DIR" ]]; then
  echo "No container scripts directory found at: $CONTAINER_SCRIPTS_DIR"
  exit 0
fi

mapfile -t scripts < <(find "$CONTAINER_SCRIPTS_DIR" -maxdepth 1 -type f -name '*.sh' | sort)
selected_scripts=()
selected_flags=()

if [[ ${#scripts[@]} -eq 0 ]]; then
  print_container_table
  echo "No container scripts found in: $CONTAINER_SCRIPTS_DIR"
  exit 0
fi

print_container_table

echo "=== Container Scripts ==="
echo "Note: flags entered here are delegated to each individual container script."
echo "Use -I to run the script with no flags (install/default behavior)."
echo "No container script will run until all scripts have been prompted."
for script_path in "${scripts[@]}"; do
  echo "- ${script_path#"$CONTAINER_SCRIPTS_DIR"/}"
done
echo

end_prompting="false"

for script_path in "${scripts[@]}"; do
  script_name="${script_path#"$CONTAINER_SCRIPTS_DIR"/}"

  if [[ "$end_prompting" == "true" ]]; then
    echo "Skipping ${script_name} (--end requested)."
    continue
  fi

  while true; do
    read -r -p "${script_name}: enter one of [--on/--off/--delete/-I/--skip/--end]: " action

    case "$action" in
      --end)
        echo "Stopping prompts. Remaining scripts will be skipped."
        end_prompting="true"
        break
        ;;
      --skip)
        echo "Skipping ${script_name}."
        break
        ;;
      --on)
        selected_scripts+=("$script_path")
        selected_flags+=("--on")
        echo "Queued ${script_name} --on"
        break
        ;;
      --off)
        selected_scripts+=("$script_path")
        selected_flags+=("--off")
        echo "Queued ${script_name} --off"
        break
        ;;
      --delete)
        selected_scripts+=("$script_path")
        selected_flags+=("-D")
        echo "Queued ${script_name} --delete"
        break
        ;;
      -I)
        selected_scripts+=("$script_path")
        selected_flags+=("")
        echo "Queued ${script_name} -I (run with no flags)"
        break
        ;;
      *)
        echo "Invalid input. Use: --on, --off, --delete, -I, --skip, or --end."
        ;;
    esac
  done
done

echo
echo "=== Execution Phase ==="
if [[ ${#selected_scripts[@]} -eq 0 ]]; then
  echo "No actions queued. Nothing to run."
  echo "Container setup complete."
  exit 0
fi

for i in "${!selected_scripts[@]}"; do
  script_path="${selected_scripts[$i]}"
  run_flag="${selected_flags[$i]}"
  script_name="${script_path#"$CONTAINER_SCRIPTS_DIR"/}"

  if [[ -z "$run_flag" ]]; then
    echo "Running ${script_name} (no flags / install mode)"
    bash "$script_path"
  else
    echo "Running ${script_name} ${run_flag}"
    bash "$script_path" "$run_flag"
  fi
done

echo "Container setup complete."
