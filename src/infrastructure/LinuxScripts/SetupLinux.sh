#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SETUPLINUX_SCRIPTS_DIR="$SCRIPT_DIR/miniscripts/setuplinux"
VALIDATION_SCRIPT="$SCRIPT_DIR/miniscripts/notautorun/SetupValidation.sh"
BITWARDEN_LOGIN_SCRIPT="$SCRIPT_DIR/miniscripts/notautorun/BitwardenSetupAndLogin.sh"
SCRIPT_ORDER_FILE="$SCRIPT_DIR/resources/script-order.txt"

# This array stores script paths chosen during the question phase.
# The run phase executes them after all prompts are answered.
SELECTED_SCRIPTS=()
DISCOVERED_SCRIPTS=()

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

contains_exact() {
  local needle="$1"
  shift
  local item

  for item in "$@"; do
    if [[ "$item" == "$needle" ]]; then
      return 0
    fi
  done
  return 1
}

script_matches_order_rule() {
  local script_path="$1"
  local order_rule="$2"
  local relative_script
  local script_name

  relative_script="${script_path#"$SETUPLINUX_SCRIPTS_DIR"/}"
  script_name="$(basename "$script_path")"

  [[
    "$order_rule" == "$script_name" ||
    "$order_rule" == "$relative_script" ||
    "$order_rule" == "miniscripts/setuplinux/$relative_script" ||
    "$order_rule" == "$script_path"
  ]]
}

apply_script_order() {
  if [[ ! -f "$SCRIPT_ORDER_FILE" ]]; then
    return
  fi

  local line
  local cleaned
  local lowered
  local section
  local script_path
  local rule
  local is_in_first
  local is_in_last

  local -a first_rules=()
  local -a last_rules=()
  local -a forced_first_order=()
  local -a middle_order=()
  local -a forced_last_order=()

  section="last"

  while IFS= read -r line; do
    cleaned="$(echo "$line" | sed 's/[[:space:]]*#.*$//;s/^[[:space:]]*//;s/[[:space:]]*$//')"
    if [[ -z "$cleaned" ]]; then
      continue
    fi

    lowered="$(echo "$cleaned" | tr '[:upper:]' '[:lower:]')"
    if [[ "$lowered" == "[first]" ]]; then
      section="first"
      continue
    fi
    if [[ "$lowered" == "[last]" ]]; then
      section="last"
      continue
    fi

    if [[ "$section" == "first" ]]; then
      first_rules+=("$cleaned")
    else
      last_rules+=("$cleaned")
    fi
  done < "$SCRIPT_ORDER_FILE"

  if [[ ${#first_rules[@]} -eq 0 && ${#last_rules[@]} -eq 0 ]]; then
    return
  fi

  for rule in "${first_rules[@]}"; do
    for script_path in "${SELECTED_SCRIPTS[@]}"; do
      if script_matches_order_rule "$script_path" "$rule"; then
        if ! contains_exact "$script_path" "${forced_first_order[@]}"; then
          forced_first_order+=("$script_path")
        fi
      fi
    done
  done

  for script_path in "${SELECTED_SCRIPTS[@]}"; do
    is_in_first=false
    is_in_last=false

    for rule in "${first_rules[@]}"; do
      if script_matches_order_rule "$script_path" "$rule"; then
        is_in_first=true
        break
      fi
    done

    for rule in "${last_rules[@]}"; do
      if script_matches_order_rule "$script_path" "$rule"; then
        is_in_last=true
        break
      fi
    done

    if [[ "$is_in_first" == true || "$is_in_last" == true ]]; then
      continue
    fi

    middle_order+=("$script_path")
  done

  for rule in "${last_rules[@]}"; do
    for script_path in "${SELECTED_SCRIPTS[@]}"; do
      if script_matches_order_rule "$script_path" "$rule"; then
        if contains_exact "$script_path" "${forced_first_order[@]}"; then
          continue
        fi
        if ! contains_exact "$script_path" "${forced_last_order[@]}"; then
          forced_last_order+=("$script_path")
        fi
      fi
    done
  done

  SELECTED_SCRIPTS=("${forced_first_order[@]}" "${middle_order[@]}" "${forced_last_order[@]}")
}

if [[ ! -f "$VALIDATION_SCRIPT" ]]; then
  echo "Validation script not found: $VALIDATION_SCRIPT"
  exit 1
fi

echo "Running setup validation..."
if ! "$VALIDATION_SCRIPT"; then
  echo "Setup validation failed. Exiting."
  exit 1
fi

# ---------------------------
# Question Phase (prompts only)
# ---------------------------
# Place setup scripts under miniscripts/setuplinux/ (including subdirectories).
# Any *.sh file found there will be prompted in sorted order, then run later
# if selected.

if [[ ! -d "$SETUPLINUX_SCRIPTS_DIR" ]]; then
  echo "No setup scripts directory found at: $SETUPLINUX_SCRIPTS_DIR"
  echo "Setup complete."
  exit 0
fi

mapfile -t DISCOVERED_SCRIPTS < <(find "$SETUPLINUX_SCRIPTS_DIR" -type f -name "*.sh" | sort)

if ask_yes_no "Run all scripts?"; then
  SELECTED_SCRIPTS=("${DISCOVERED_SCRIPTS[@]}")
else
  for script_path in "${DISCOVERED_SCRIPTS[@]}"; do
    relative_script="${script_path#"$SETUPLINUX_SCRIPTS_DIR"/}"
    if ask_yes_no "Run $relative_script?"; then
      SELECTED_SCRIPTS+=("$script_path")
    fi
  done
fi

apply_script_order

if [[ ! -f "$BITWARDEN_LOGIN_SCRIPT" ]]; then
  echo "Bitwarden setup/login script not found: $BITWARDEN_LOGIN_SCRIPT"
  exit 1
fi

echo "Running Bitwarden setup/login step..."
if ! bash "$BITWARDEN_LOGIN_SCRIPT"; then
  echo "Bitwarden setup/login failed. Exiting."
  exit 1
fi

# Run apt update once at the start to ensure we have the latest package info after script selection
sudo add-apt-repository multiverse -y
echo "Updating package lists..."
sudo apt update
echo "Upgrading installed packages..."
sudo apt upgrade -y

# ---------------------------
# Run Phase (execute selection)
# ---------------------------
if [[ ${#SELECTED_SCRIPTS[@]} -eq 0 ]]; then
  echo "No scripts selected."
  echo "Setup complete."
  exit 0
fi

echo "Running selected scripts..."
for script_path in "${SELECTED_SCRIPTS[@]}"; do
  if [[ ! -f "$script_path" ]]; then
    echo "Skipping missing script: $script_path"
    continue
  fi

  echo "Running: $(basename "$script_path")"
  "$script_path"
done

echo "Setup complete."