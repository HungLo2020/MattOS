#!/usr/bin/env bash

set -euo pipefail

DEFAULT_REPO_PATH="/srv/storage/OneDrive/Apps/Games/Storage/MattMC/Restic/"
DEFAULT_SOURCE_PATH="/srv/storage/Storage/Sync/MattMC/"
DEFAULT_CONFIG_NAME="MattMC"

CONFIG_ROOT="${HOME}/.config/restic-mattmc"
CONFIGS_DIR="${CONFIG_ROOT}/configs"
HELPERS_DIR="${CONFIG_ROOT}/helpers"
CURRENT_CONFIG_FILE="${CONFIG_ROOT}/current_config"
LEGACY_CONFIG_FILE="${CONFIG_ROOT}/backup.env"
LEGACY_PASSWORD_FILE="${CONFIG_ROOT}/password"

KEEP_DAILY=7
KEEP_WEEKLY=4
KEEP_MONTHLY=12
KEEP_YEARLY=2

SCRIPT_PATH="$(readlink -f "${BASH_SOURCE[0]}")"

ACTIVE_CONFIG_NAME=""
ACTIVE_CONFIG_SLUG=""
ACTIVE_CONFIG_FILE=""
RESTIC_REPOSITORY=""
RESTIC_SOURCE=""
RESTIC_PASSWORD_FILE=""
declare -a CONFIG_INDEX_SLUGS=()
declare -a CONFIG_INDEX_NAMES=()

log() {
  echo "[$(date '+%Y-%m-%d %H:%M:%S')] $*"
}

ensure_config_dirs() {
  mkdir -p "${CONFIGS_DIR}" "${HELPERS_DIR}"
  chmod 700 "${CONFIG_ROOT}" "${CONFIGS_DIR}" "${HELPERS_DIR}" 2>/dev/null || true
}

collect_config_index() {
  local config_file slug name

  CONFIG_INDEX_SLUGS=()
  CONFIG_INDEX_NAMES=()

  ensure_config_dirs
  shopt -s nullglob
  for config_file in "${CONFIGS_DIR}"/*.env; do
    slug="$(basename "${config_file}" .env)"
    name=""
    # shellcheck disable=SC1090
    source "${config_file}"
    name="${CONFIG_NAME:-${slug}}"
    CONFIG_INDEX_SLUGS+=("${slug}")
    CONFIG_INDEX_NAMES+=("${name}")
  done
  shopt -u nullglob
}

normalize_path() {
  local input="$1"
  if [[ "${input}" == "/" ]]; then
    echo "/"
    return
  fi
  input="${input%/}"
  echo "${input}"
}

sanitize_config_name() {
  local input="$1"
  local slug

  slug="$(printf '%s' "${input}" | tr '[:upper:]' '[:lower:]' | sed -E 's/[^a-z0-9]+/-/g; s/^-+//; s/-+$//')"
  echo "${slug}"
}

config_file_for_slug() {
  local slug="$1"
  echo "${CONFIGS_DIR}/${slug}.env"
}

password_file_for_slug() {
  local slug="$1"
  echo "${CONFIGS_DIR}/password-${slug}.txt"
}

service_name_for_slug() {
  local slug="$1"
  echo "restic-${slug}-backup.service"
}

timer_name_for_slug() {
  local slug="$1"
  echo "restic-${slug}-backup.timer"
}

service_path_for_slug() {
  local slug="$1"
  echo "/etc/systemd/system/$(service_name_for_slug "${slug}")"
}

timer_path_for_slug() {
  local slug="$1"
  echo "/etc/systemd/system/$(timer_name_for_slug "${slug}")"
}

helper_script_path_for_slug() {
  local slug="$1"
  echo "${HELPERS_DIR}/restic-backup-${slug}.sh"
}

ensure_restic_installed() {
  if command -v restic >/dev/null 2>&1; then
    return 0
  fi

  log "restic is not installed. Attempting install..."
  if command -v apt-get >/dev/null 2>&1; then
    sudo apt-get update
    sudo apt-get install -y restic
  else
    log "Error: restic is missing and this distro is unsupported for auto-install."
    log "Install restic manually, then rerun this script."
    exit 1
  fi

  if ! command -v restic >/dev/null 2>&1; then
    log "Error: restic installation failed."
    exit 1
  fi
}

ensure_systemd_available() {
  if ! command -v systemctl >/dev/null 2>&1; then
    log "Error: systemctl is not available on this system."
    exit 1
  fi
}

create_backup_helper_script() {
  local slug="$1"
  local config_file helper_script

  config_file="$(config_file_for_slug "${slug}")"
  helper_script="$(helper_script_path_for_slug "${slug}")"

  if [[ ! -f "${config_file}" ]]; then
    log "Error: missing config file for helper generation: ${config_file}"
    exit 1
  fi

  cat >"${helper_script}" <<EOF
#!/usr/bin/env bash

set -euo pipefail

CONFIG_FILE="${config_file}"

log() {
  echo "[\$(date '+%Y-%m-%d %H:%M:%S')] [restic-helper] \$*"
}

if [[ ! -f "\${CONFIG_FILE}" ]]; then
  log "Error: config file missing: \${CONFIG_FILE}"
  exit 1
fi

# shellcheck disable=SC1090
source "\${CONFIG_FILE}"

for required_var in CONFIG_NAME CONFIG_SLUG RESTIC_REPOSITORY RESTIC_SOURCE RESTIC_PASSWORD_FILE KEEP_DAILY KEEP_WEEKLY KEEP_MONTHLY KEEP_YEARLY; do
  if [[ -z "\${!required_var:-}" ]]; then
    log "Error: missing required setting '\${required_var}' in \${CONFIG_FILE}"
    exit 1
  fi
done

if ! command -v restic >/dev/null 2>&1; then
  log "Error: restic is not installed."
  exit 1
fi

if [[ ! -f "\${RESTIC_PASSWORD_FILE}" ]]; then
  log "Error: password file missing: \${RESTIC_PASSWORD_FILE}"
  exit 1
fi

if [[ ! -d "\${RESTIC_SOURCE}" ]]; then
  log "Error: source path missing: \${RESTIC_SOURCE}"
  exit 1
fi

mkdir -p "\${RESTIC_REPOSITORY}"

if [[ ! -f "\${RESTIC_REPOSITORY}/config" ]]; then
  log "Initializing repository: \${RESTIC_REPOSITORY}"
  RESTIC_PASSWORD_FILE="\${RESTIC_PASSWORD_FILE}" restic -r "\${RESTIC_REPOSITORY}" init
else
  if ! RESTIC_PASSWORD_FILE="\${RESTIC_PASSWORD_FILE}" restic -r "\${RESTIC_REPOSITORY}" snapshots >/dev/null 2>&1; then
    log "Error: existing repository could not be opened with current password."
    exit 1
  fi
fi

log "Running backup for config '\${CONFIG_NAME}' from \${RESTIC_SOURCE}"
RESTIC_PASSWORD_FILE="\${RESTIC_PASSWORD_FILE}" restic -r "\${RESTIC_REPOSITORY}" backup "\${RESTIC_SOURCE}"

log "Applying retention policy"
RESTIC_PASSWORD_FILE="\${RESTIC_PASSWORD_FILE}" restic -r "\${RESTIC_REPOSITORY}" forget --prune \
  --keep-daily "\${KEEP_DAILY}" \
  --keep-weekly "\${KEEP_WEEKLY}" \
  --keep-monthly "\${KEEP_MONTHLY}" \
  --keep-yearly "\${KEEP_YEARLY}"

log "Backup helper completed successfully."
EOF

chmod 700 "${helper_script}"
}

random_token() {
  if command -v openssl >/dev/null 2>&1; then
    openssl rand -hex 32
  else
    tr -dc 'A-Za-z0-9' </dev/urandom | head -c 64
    echo
  fi
}

set_current_config_slug() {
  local slug="$1"
  ensure_config_dirs
  printf '%s\n' "${slug}" >"${CURRENT_CONFIG_FILE}"
}

get_current_config_slug() {
  if [[ -f "${CURRENT_CONFIG_FILE}" ]]; then
    tr -d '\n' <"${CURRENT_CONFIG_FILE}"
  fi
}

prompt_path() {
  local prompt_label="$1"
  local default_value="$2"
  local result_var="$3"
  local entered=""

  while true; do
    read -r -p "${prompt_label} [${default_value}]: " entered
    entered="${entered:-${default_value}}"

    if [[ -z "${entered}" ]]; then
      echo "Path cannot be empty."
      continue
    fi

    printf -v "${result_var}" '%s' "$(normalize_path "${entered}")"
    return 0
  done
}

prompt_config_name() {
  local result_name_var="$1"
  local result_slug_var="$2"
  local entered_name=""
  local entered_slug=""

  while true; do
    read -r -p "Enter backup config name [${DEFAULT_CONFIG_NAME}]: " entered_name
    entered_name="${entered_name:-${DEFAULT_CONFIG_NAME}}"

    if [[ -z "${entered_name}" ]]; then
      echo "Config name cannot be empty."
      continue
    fi

    entered_slug="$(sanitize_config_name "${entered_name}")"
    if [[ -z "${entered_slug}" ]]; then
      echo "Config name must include at least one letter or number."
      continue
    fi

    printf -v "${result_name_var}" '%s' "${entered_name}"
    printf -v "${result_slug_var}" '%s' "${entered_slug}"
    return 0
  done
}

ensure_repo_directory_writable() {
  local repo_path="$1"
  local probe_file="${repo_path}/.restic-write-probe"

  if [[ ! -d "${repo_path}" ]]; then
    if mkdir -p "${repo_path}" 2>/dev/null; then
      :
    else
      sudo mkdir -p "${repo_path}"
      sudo chown "${USER}:${USER}" "${repo_path}"
    fi
  fi

  if touch "${probe_file}" 2>/dev/null; then
    rm -f "${probe_file}"
    return 0
  fi

  if sudo touch "${probe_file}" >/dev/null 2>&1; then
    sudo chown "${USER}:${USER}" "${probe_file}" >/dev/null 2>&1 || true
    rm -f "${probe_file}" >/dev/null 2>&1 || sudo rm -f "${probe_file}" >/dev/null 2>&1 || true
    return 0
  fi

  log "Error: cannot write to repository path: ${repo_path}"
  exit 1
}

ensure_password_file_exists() {
  local password_path="$1"

  if [[ -f "${password_path}" ]]; then
    chmod 600 "${password_path}" 2>/dev/null || true
    return 0
  fi

  random_token >"${password_path}"
  chmod 600 "${password_path}"
}

write_password_file() {
  local password_path="$1"
  local password_value="$2"

  mkdir -p "$(dirname "${password_path}")"
  printf '%s' "${password_value}" >"${password_path}"
  chmod 600 "${password_path}"
}

prompt_restic_password() {
  local allow_keep_existing="$1"
  local result_var="$2"
  local password_one=""
  local password_two=""

  while true; do
    read -r -s -p "Enter restic password (hint: standard pc password): " password_one
    echo

    if [[ -z "${password_one}" && "${allow_keep_existing}" == "true" ]]; then
      printf -v "${result_var}" '%s' ""
      return 0
    fi

    if [[ -z "${password_one}" ]]; then
      echo "Password cannot be empty."
      continue
    fi

    read -r -s -p "Confirm restic password: " password_two
    echo

    if [[ "${password_one}" != "${password_two}" ]]; then
      echo "Passwords do not match. Try again."
      continue
    fi

    printf -v "${result_var}" '%s' "${password_one}"
    return 0
  done
}

validate_source_directory() {
  local source_path="$1"

  if [[ ! -d "${source_path}" ]]; then
    log "Error: source path does not exist: ${source_path}"
    exit 1
  fi

  if [[ ! -r "${source_path}" ]]; then
    log "Error: source path is not readable: ${source_path}"
    exit 1
  fi
}

write_config_file() {
  local config_name="$1"
  local config_slug="$2"
  local repo_path="$3"
  local source_path="$4"
  local password_file="$5"
  local config_file

  config_file="$(config_file_for_slug "${config_slug}")"

  {
    printf 'CONFIG_NAME=%q\n' "${config_name}"
    printf 'CONFIG_SLUG=%q\n' "${config_slug}"
    printf 'RESTIC_REPOSITORY=%q\n' "${repo_path}"
    printf 'RESTIC_SOURCE=%q\n' "${source_path}"
    printf 'RESTIC_PASSWORD_FILE=%q\n' "${password_file}"
    printf 'KEEP_DAILY=%q\n' "${KEEP_DAILY}"
    printf 'KEEP_WEEKLY=%q\n' "${KEEP_WEEKLY}"
    printf 'KEEP_MONTHLY=%q\n' "${KEEP_MONTHLY}"
    printf 'KEEP_YEARLY=%q\n' "${KEEP_YEARLY}"
  } >"${config_file}"

  chmod 600 "${config_file}"
}

migrate_legacy_config_if_needed() {
  local target_slug="mattmc"
  local target_file legacy_repo legacy_source legacy_password

  ensure_config_dirs
  target_file="$(config_file_for_slug "${target_slug}")"

  if [[ -f "${target_file}" || ! -f "${LEGACY_CONFIG_FILE}" ]]; then
    return 0
  fi

  # shellcheck disable=SC1090
  source "${LEGACY_CONFIG_FILE}"

  legacy_repo="${RESTIC_REPOSITORY:-}"
  legacy_source="${RESTIC_SOURCE:-}"
  legacy_password="${RESTIC_PASSWORD_FILE:-${LEGACY_PASSWORD_FILE}}"

  if [[ -z "${legacy_repo}" || -z "${legacy_source}" ]]; then
    return 0
  fi

  if [[ ! -f "${legacy_password}" ]]; then
    legacy_password="$(password_file_for_slug "${target_slug}")"
    ensure_password_file_exists "${legacy_password}"
  fi

  write_config_file "${DEFAULT_CONFIG_NAME}" "${target_slug}" "${legacy_repo}" "${legacy_source}" "${legacy_password}"
  set_current_config_slug "${target_slug}"
  log "Imported legacy backup config as '${DEFAULT_CONFIG_NAME}'."
}

resolve_config_slug() {
  local selector="$1"
  local slug normalized config_file
  local config_path config_name

  normalized="$(sanitize_config_name "${selector}")"
  if [[ -n "${normalized}" ]]; then
    config_file="$(config_file_for_slug "${normalized}")"
    if [[ -f "${config_file}" ]]; then
      echo "${normalized}"
      return 0
    fi
  fi

  shopt -s nullglob
  for config_path in "${CONFIGS_DIR}"/*.env; do
    config_name=""
    # shellcheck disable=SC1090
    source "${config_path}"
    config_name="${CONFIG_NAME:-}"
    if [[ -n "${config_name}" && "${config_name,,}" == "${selector,,}" ]]; then
      slug="$(basename "${config_path}" .env)"
      echo "${slug}"
      shopt -u nullglob
      return 0
    fi
  done
  shopt -u nullglob

  return 1
}

load_config_by_slug() {
  local slug="$1"
  local config_file

  config_file="$(config_file_for_slug "${slug}")"
  if [[ ! -f "${config_file}" ]]; then
    log "Configuration '${slug}' not found."
    return 1
  fi

  # shellcheck disable=SC1090
  source "${config_file}"

  if [[ -z "${CONFIG_NAME:-}" || -z "${RESTIC_REPOSITORY:-}" || -z "${RESTIC_SOURCE:-}" || -z "${RESTIC_PASSWORD_FILE:-}" ]]; then
    log "Error: configuration file is missing required fields."
    return 1
  fi

  ACTIVE_CONFIG_NAME="${CONFIG_NAME}"
  ACTIVE_CONFIG_SLUG="${slug}"
  ACTIVE_CONFIG_FILE="${config_file}"
  set_current_config_slug "${slug}"

  if [[ ! -f "${RESTIC_PASSWORD_FILE}" ]]; then
    log "Error: password file is missing for config '${ACTIVE_CONFIG_NAME}': ${RESTIC_PASSWORD_FILE}"
    return 1
  fi

  return 0
}

load_active_config() {
  local selector="${1:-}"
  local slug

  if [[ -n "${selector}" ]]; then
    if ! slug="$(resolve_config_slug "${selector}")"; then
      log "Error: no config found for selector '${selector}'."
      return 1
    fi
    load_config_by_slug "${slug}"
    return $?
  fi

  slug="$(get_current_config_slug || true)"
  if [[ -n "${slug}" && -f "$(config_file_for_slug "${slug}")" ]]; then
    load_config_by_slug "${slug}"
    return $?
  fi

  if ! select_config_interactively slug; then
    return 1
  fi

  load_config_by_slug "${slug}"
}

select_config_interactively() {
  local result_var="$1"
  local current_slug default_index choice selected_slug
  local idx=1
  local config_file slug name
  local -a slugs=()

  ensure_config_dirs
  shopt -s nullglob
  for config_file in "${CONFIGS_DIR}"/*.env; do
    slug="$(basename "${config_file}" .env)"
    name=""
    # shellcheck disable=SC1090
    source "${config_file}"
    name="${CONFIG_NAME:-${slug}}"
    slugs+=("${slug}")
    echo "${idx}) ${name} [${slug}]"
    ((idx++))
  done
  shopt -u nullglob

  if [[ "${#slugs[@]}" -eq 0 ]]; then
    log "No backup configs exist yet. Run setup first."
    return 1
  fi

  current_slug="$(get_current_config_slug || true)"
  default_index=1

  for idx in "${!slugs[@]}"; do
    if [[ "${slugs[$idx]}" == "${current_slug}" ]]; then
      default_index=$((idx + 1))
      break
    fi
  done

  while true; do
    read -r -p "Select config [${default_index}]: " choice
    choice="${choice:-${default_index}}"
    if [[ "${choice}" =~ ^[0-9]+$ ]] && (( choice >= 1 && choice <= ${#slugs[@]} )); then
      selected_slug="${slugs[$((choice - 1))]}"
      set_current_config_slug "${selected_slug}"
      printf -v "${result_var}" '%s' "${selected_slug}"
      return 0
    fi
    echo "Please enter a valid number between 1 and ${#slugs[@]}."
  done
}

restic_cmd() {
  RESTIC_PASSWORD_FILE="${RESTIC_PASSWORD_FILE}" restic -r "${RESTIC_REPOSITORY}" "$@"
}

ensure_repo_initialized() {
  if [[ -f "${RESTIC_REPOSITORY}/config" ]]; then
    if ! restic_cmd snapshots >/dev/null 2>&1; then
      log "Error: repository exists but could not be opened with current password."
      exit 1
    fi
    return 0
  fi

  log "Initializing restic repository at: ${RESTIC_REPOSITORY}"
  restic_cmd init
}

run_backup_now() {
  ensure_restic_installed
  load_active_config "${1:-}" || return 1
  validate_source_directory "${RESTIC_SOURCE}"
  ensure_repo_directory_writable "${RESTIC_REPOSITORY}"
  ensure_repo_initialized

  log "Starting backup for config '${ACTIVE_CONFIG_NAME}': ${RESTIC_SOURCE}"
  restic_cmd backup "${RESTIC_SOURCE}"

  log "Applying retention policy (daily=${KEEP_DAILY}, weekly=${KEEP_WEEKLY}, monthly=${KEEP_MONTHLY}, yearly=${KEEP_YEARLY})"
  restic_cmd forget --prune \
    --keep-daily "${KEEP_DAILY}" \
    --keep-weekly "${KEEP_WEEKLY}" \
    --keep-monthly "${KEEP_MONTHLY}" \
    --keep-yearly "${KEEP_YEARLY}"

  log "Backup + prune completed."
}

setup_systemd_timer() {
  local run_user service_name timer_name service_path timer_path helper_script
  run_user="${USER}"
  service_name="$(service_name_for_slug "${ACTIVE_CONFIG_SLUG}")"
  timer_name="$(timer_name_for_slug "${ACTIVE_CONFIG_SLUG}")"
  service_path="$(service_path_for_slug "${ACTIVE_CONFIG_SLUG}")"
  timer_path="$(timer_path_for_slug "${ACTIVE_CONFIG_SLUG}")"
  helper_script="$(helper_script_path_for_slug "${ACTIVE_CONFIG_SLUG}")"

  ensure_systemd_available
  create_backup_helper_script "${ACTIVE_CONFIG_SLUG}"

  sudo tee "${service_path}" >/dev/null <<EOF
[Unit]
Description=Restic backup for ${ACTIVE_CONFIG_NAME}
After=network-online.target
Wants=network-online.target

[Service]
Type=oneshot
User=${run_user}
Group=${run_user}
ExecStart=${helper_script}
EOF

  sudo tee "${timer_path}" >/dev/null <<EOF
[Unit]
Description=Daily Restic backup timer for ${ACTIVE_CONFIG_NAME}

[Timer]
OnCalendar=daily
Persistent=true
RandomizedDelaySec=30m

[Install]
WantedBy=timers.target
EOF

  sudo systemctl daemon-reload
  sudo systemctl enable --now "${timer_name}" >/dev/null

  log "Automatic backups enabled via ${timer_name}."
}

show_current_config() {
  if ! load_active_config "${1:-}"; then
    return 1
  fi

  local service_name timer_name
  service_name="$(service_name_for_slug "${ACTIVE_CONFIG_SLUG}")"
  timer_name="$(timer_name_for_slug "${ACTIVE_CONFIG_SLUG}")"

  echo "=== Current Restic Backup Config ==="
  echo "Name:       ${ACTIVE_CONFIG_NAME}"
  echo "Slug:       ${ACTIVE_CONFIG_SLUG}"
  echo "Repository: ${RESTIC_REPOSITORY}"
  echo "Source:     ${RESTIC_SOURCE}"
  echo "Policy:     daily=${KEEP_DAILY} weekly=${KEEP_WEEKLY} monthly=${KEEP_MONTHLY} yearly=${KEEP_YEARLY}"
  echo "Service:    ${service_name}"
  echo "Timer:      ${timer_name}"
}

list_all_configs() {
  local idx=1
  local config_file slug timer_name timer_state

  collect_config_index

  if [[ "${#CONFIG_INDEX_SLUGS[@]}" -eq 0 ]]; then
    echo "No backup configs found."
    return 0
  fi

  echo "=== All Backup Configs ==="
  for slug in "${CONFIG_INDEX_SLUGS[@]}"; do
    config_file="$(config_file_for_slug "${slug}")"
    # shellcheck disable=SC1090
    source "${config_file}"

    timer_name="$(timer_name_for_slug "${slug}")"
    timer_state="not-enabled"
    if command -v systemctl >/dev/null 2>&1 && systemctl is-enabled "${timer_name}" >/dev/null 2>&1; then
      timer_state="enabled"
    fi

    echo "${idx}) ${CONFIG_NAME:-${slug}} [${slug}]"
    echo "    Source: ${RESTIC_SOURCE:-unknown}"
    echo "    Repo:   ${RESTIC_REPOSITORY:-unknown}"
    echo "    Timer:  ${timer_name} (${timer_state})"
    ((idx++))
  done
}

is_password_file_used_by_other_configs() {
  local password_path="$1"
  local skip_slug="$2"
  local config_file slug candidate_password

  shopt -s nullglob
  for config_file in "${CONFIGS_DIR}"/*.env; do
    slug="$(basename "${config_file}" .env)"
    if [[ "${slug}" == "${skip_slug}" ]]; then
      continue
    fi

    candidate_password=""
    # shellcheck disable=SC1090
    source "${config_file}"
    candidate_password="${RESTIC_PASSWORD_FILE:-}"
    if [[ -n "${candidate_password}" && "${candidate_password}" == "${password_path}" ]]; then
      shopt -u nullglob
      return 0
    fi
  done
  shopt -u nullglob

  return 1
}

is_repo_used_by_other_configs() {
  local repo_path="$1"
  local skip_slug="$2"
  local config_file slug candidate_repo

  shopt -s nullglob
  for config_file in "${CONFIGS_DIR}"/*.env; do
    slug="$(basename "${config_file}" .env)"
    if [[ "${slug}" == "${skip_slug}" ]]; then
      continue
    fi

    candidate_repo=""
    # shellcheck disable=SC1090
    source "${config_file}"
    candidate_repo="${RESTIC_REPOSITORY:-}"
    if [[ -n "${candidate_repo}" && "${candidate_repo}" == "${repo_path}" ]]; then
      shopt -u nullglob
      return 0
    fi
  done
  shopt -u nullglob

  return 1
}

delete_config_by_index() {
  local index="$1"
  local slug name config_file password_file repo_path
  local timer_name service_name timer_path service_path helper_script
  local confirm delete_repo_choice current_slug

  collect_config_index

  if [[ "${#CONFIG_INDEX_SLUGS[@]}" -eq 0 ]]; then
    log "No configs to delete."
    return 0
  fi

  if [[ ! "${index}" =~ ^[0-9]+$ ]] || (( index < 1 || index > ${#CONFIG_INDEX_SLUGS[@]} )); then
    log "Invalid config number '${index}'. Use option 2 to see config numbers."
    return 1
  fi

  slug="${CONFIG_INDEX_SLUGS[$((index - 1))]}"
  name="${CONFIG_INDEX_NAMES[$((index - 1))]}"

  if ! load_config_by_slug "${slug}"; then
    return 1
  fi

  config_file="$(config_file_for_slug "${slug}")"
  password_file="${RESTIC_PASSWORD_FILE}"
  repo_path="${RESTIC_REPOSITORY}"
  service_name="$(service_name_for_slug "${slug}")"
  timer_name="$(timer_name_for_slug "${slug}")"
  service_path="$(service_path_for_slug "${slug}")"
  timer_path="$(timer_path_for_slug "${slug}")"
  helper_script="$(helper_script_path_for_slug "${slug}")"

  read -r -p "Delete config '${name}' [${slug}] and associated service/timer? [y/N]: " confirm
  confirm="${confirm:-N}"
  if [[ ! "${confirm}" =~ ^[Yy]$ ]]; then
    log "Delete cancelled."
    return 0
  fi

  if command -v systemctl >/dev/null 2>&1; then
    sudo systemctl disable --now "${timer_name}" >/dev/null 2>&1 || true
    sudo systemctl stop "${service_name}" >/dev/null 2>&1 || true
  fi

  sudo rm -f "${service_path}" "${timer_path}" >/dev/null 2>&1 || rm -f "${service_path}" "${timer_path}" || true
  rm -f "${helper_script}" >/dev/null 2>&1 || sudo rm -f "${helper_script}" >/dev/null 2>&1 || true

  if command -v systemctl >/dev/null 2>&1; then
    sudo systemctl daemon-reload || true
    sudo systemctl reset-failed "${service_name}" "${timer_name}" >/dev/null 2>&1 || true
  fi

  rm -f "${config_file}"

  if [[ -f "${password_file}" ]]; then
    if is_password_file_used_by_other_configs "${password_file}" "${slug}"; then
      log "Password file is shared by another config; preserving ${password_file}."
    else
      rm -f "${password_file}"
    fi
  fi

  if is_repo_used_by_other_configs "${repo_path}" "${slug}"; then
    log "Repository is used by another config; preserving ${repo_path}."
  else
    read -r -p "Delete repository directory and backup data at '${repo_path}' too? [Y/n]: " delete_repo_choice
    delete_repo_choice="${delete_repo_choice:-Y}"
    if [[ "${delete_repo_choice}" =~ ^[Yy]$ ]]; then
      rm -rf "${repo_path}" >/dev/null 2>&1 || sudo rm -rf "${repo_path}"
      log "Deleted repository directory: ${repo_path}"
    else
      log "Repository preserved: ${repo_path}"
    fi
  fi

  current_slug="$(get_current_config_slug || true)"
  if [[ "${current_slug}" == "${slug}" ]]; then
    collect_config_index
    if [[ "${#CONFIG_INDEX_SLUGS[@]}" -gt 0 ]]; then
      set_current_config_slug "${CONFIG_INDEX_SLUGS[0]}"
    else
      rm -f "${CURRENT_CONFIG_FILE}"
    fi
  fi

  log "Deleted config '${name}' [${slug}]."
}

resolve_config_slug_by_index() {
  local index="$1"
  local result_var="$2"

  collect_config_index

  if [[ "${#CONFIG_INDEX_SLUGS[@]}" -eq 0 ]]; then
    log "No configs found. Run setup first."
    return 1
  fi

  if [[ ! "${index}" =~ ^[0-9]+$ ]] || (( index < 1 || index > ${#CONFIG_INDEX_SLUGS[@]} )); then
    log "Invalid config number '${index}'. Use the list above for valid numbers."
    return 1
  fi

  printf -v "${result_var}" '%s' "${CONFIG_INDEX_SLUGS[$((index - 1))]}"
  return 0
}

collect_snapshots() {
  local snapshot_output
  snapshot_output="$(restic_cmd snapshots --compact 2>/dev/null || true)"

  mapfile -t SNAPSHOT_LINES < <(printf '%s\n' "${snapshot_output}" | awk 'NR>2 && NF>0 {print $1"|"$2" "$3}')
}

list_snapshots_with_sizes() {
  ensure_restic_installed
  load_active_config "${1:-}" || return 1
  ensure_repo_initialized

  collect_snapshots

  if [[ "${#SNAPSHOT_LINES[@]}" -eq 0 ]]; then
    echo "No snapshots found."
    return 0
  fi

  printf "%-4s %-12s %-20s %s\n" "No." "Snapshot ID" "Date" "Approx Size"
  printf "%-4s %-12s %-20s %s\n" "----" "------------" "-------------------" "-----------"

  local index=1
  local line id when size
  for line in "${SNAPSHOT_LINES[@]}"; do
    id="${line%%|*}"
    when="${line#*|}"
    size="$(restic_cmd stats "${id}" --mode raw-data 2>/dev/null | sed -n 's/^[[:space:]]*Total Size:[[:space:]]*//p' | head -n1)"
    size="${size:-unknown}"
    printf "%-4s %-12s %-20s %s\n" "${index})" "${id}" "${when}" "${size}"
    ((index++))
  done
}

restore_snapshot_to_downloads() {
  ensure_restic_installed
  load_active_config "${1:-}" || return 1
  ensure_repo_initialized

  collect_snapshots

  if [[ "${#SNAPSHOT_LINES[@]}" -eq 0 ]]; then
    echo "No snapshots available to restore."
    return 0
  fi

  echo "Available snapshots:"
  local i=1
  local line id when
  for line in "${SNAPSHOT_LINES[@]}"; do
    id="${line%%|*}"
    when="${line#*|}"
    echo "${i}) ${id} (${when})"
    ((i++))
  done

  local choice=""
  while true; do
    read -r -p "Select snapshot number to restore to Downloads: " choice
    if [[ "${choice}" =~ ^[0-9]+$ ]] && (( choice >= 1 && choice <= ${#SNAPSHOT_LINES[@]} )); then
      break
    fi
    echo "Please enter a valid number between 1 and ${#SNAPSHOT_LINES[@]}."
  done

  line="${SNAPSHOT_LINES[$((choice - 1))]}"
  id="${line%%|*}"

  local restore_target="${HOME}/Downloads/restic-restore-${id}-$(date +%Y%m%d-%H%M%S)"
  mkdir -p "${restore_target}"

  log "Restoring snapshot ${id} to ${restore_target}"
  restic_cmd restore "${id}" --target "${restore_target}"
  log "Restore complete."
}

run_forget_prune_now() {
  ensure_restic_installed
  load_active_config "${1:-}" || return 1
  ensure_repo_initialized

  log "Running forget + prune policy now..."
  restic_cmd forget --prune \
    --keep-daily "${KEEP_DAILY}" \
    --keep-weekly "${KEEP_WEEKLY}" \
    --keep-monthly "${KEEP_MONTHLY}" \
    --keep-yearly "${KEEP_YEARLY}"

  log "Policy run complete."
}

run_setup_flow() {
  ensure_restic_installed
  ensure_config_dirs

  local repo_path source_path config_name config_slug
  local config_file password_file overwrite_choice=""
  local entered_password="" allow_keep_existing_password="false"

  prompt_path "Enter restic repository path" "${DEFAULT_REPO_PATH}" repo_path
  prompt_path "Enter source path to back up" "${DEFAULT_SOURCE_PATH}" source_path
  prompt_config_name config_name config_slug

  ensure_repo_directory_writable "${repo_path}"
  validate_source_directory "${source_path}"

  config_file="$(config_file_for_slug "${config_slug}")"
  password_file="$(password_file_for_slug "${config_slug}")"

  if [[ -f "${config_file}" ]]; then
    # shellcheck disable=SC1090
    source "${config_file}"
    if [[ -n "${RESTIC_PASSWORD_FILE:-}" ]]; then
      password_file="${RESTIC_PASSWORD_FILE}"
    fi

    read -r -p "Config '${config_name}' already exists. Overwrite paths/settings? [Y/n]: " overwrite_choice
    overwrite_choice="${overwrite_choice:-Y}"
    if [[ ! "${overwrite_choice}" =~ ^[Yy]$ ]]; then
      log "Setup cancelled."
      return 0
    fi
  fi

  if [[ -f "${password_file}" ]]; then
    allow_keep_existing_password="true"
    echo "Press Enter at password prompt to keep existing password for this config."
  fi

  prompt_restic_password "${allow_keep_existing_password}" entered_password
  if [[ -n "${entered_password}" ]]; then
    write_password_file "${password_file}" "${entered_password}"
  elif [[ ! -f "${password_file}" ]]; then
    log "Error: password file does not exist and no password was entered."
    return 1
  fi

  write_config_file "${config_name}" "${config_slug}" "${repo_path}" "${source_path}" "${password_file}"
  load_config_by_slug "${config_slug}"
  ensure_repo_initialized
  setup_systemd_timer

  log "Setup complete."
  show_current_config "${config_slug}"
}

print_menu() {
  echo
  list_all_configs
  echo
  echo "=== Restic Backup Manager ==="
  echo "1) Run / rerun setup"
  echo "2) Exit"
  echo
  echo "Special commands:"
  echo "  delete <config-number>   Delete config + service/timer (example: delete 1)"
  echo "  3 <config-number>        Take immediate backup now"
  echo "  4 <config-number>        List backups (dates + sizes)"
  echo "  5 <config-number>        Restore snapshot to Downloads"
  echo "  6 <config-number>        Run forget + prune now"
  echo "  7 <config-number>        Show current configuration"
  echo "  backup|snapshots|restore|forget|show <config-number>"
  echo
}

main_loop() {
  local choice config_slug
  while true; do
    print_menu
    read -r -p "Choose an option [1-2 or command]: " choice

    if [[ "${choice}" =~ ^[Dd][Ee][Ll][Ee][Tt][Ee][[:space:]]+([0-9]+)$ ]]; then
      delete_config_by_index "${BASH_REMATCH[1]}"
      continue
    fi

    if [[ "${choice}" =~ ^3[[:space:]]+([0-9]+)$ ]] || [[ "${choice}" =~ ^[Bb][Aa][Cc][Kk][Uu][Pp][[:space:]]+([0-9]+)$ ]]; then
      if resolve_config_slug_by_index "${BASH_REMATCH[1]}" config_slug; then
        run_backup_now "${config_slug}"
      fi
      continue
    fi

    if [[ "${choice}" =~ ^4[[:space:]]+([0-9]+)$ ]] || [[ "${choice}" =~ ^[Ss][Nn][Aa][Pp][Ss][Hh][Oo][Tt][Ss][[:space:]]+([0-9]+)$ ]]; then
      if resolve_config_slug_by_index "${BASH_REMATCH[1]}" config_slug; then
        list_snapshots_with_sizes "${config_slug}"
      fi
      continue
    fi

    if [[ "${choice}" =~ ^5[[:space:]]+([0-9]+)$ ]] || [[ "${choice}" =~ ^[Rr][Ee][Ss][Tt][Oo][Rr][Ee][[:space:]]+([0-9]+)$ ]]; then
      if resolve_config_slug_by_index "${BASH_REMATCH[1]}" config_slug; then
        restore_snapshot_to_downloads "${config_slug}"
      fi
      continue
    fi

    if [[ "${choice}" =~ ^6[[:space:]]+([0-9]+)$ ]] || [[ "${choice}" =~ ^[Ff][Oo][Rr][Gg][Ee][Tt][[:space:]]+([0-9]+)$ ]]; then
      if resolve_config_slug_by_index "${BASH_REMATCH[1]}" config_slug; then
        run_forget_prune_now "${config_slug}"
      fi
      continue
    fi

    if [[ "${choice}" =~ ^7[[:space:]]+([0-9]+)$ ]] || [[ "${choice}" =~ ^[Ss][Hh][Oo][Ww][[:space:]]+([0-9]+)$ ]]; then
      if resolve_config_slug_by_index "${BASH_REMATCH[1]}" config_slug; then
        show_current_config "${config_slug}"
      fi
      continue
    fi

    case "${choice}" in
      1)
        run_setup_flow
        ;;
      2)
        echo "Goodbye."
        exit 0
        ;;
      *)
        echo "Invalid selection."
        ;;
    esac
  done
}

MODE="menu"
CONFIG_SELECTOR=""

while [[ "$#" -gt 0 ]]; do
  case "$1" in
    --run-backup)
      MODE="run-backup"
      shift
      ;;
    --config-name)
      if [[ "$#" -lt 2 ]]; then
        log "Error: --config-name requires a value."
        exit 1
      fi
      CONFIG_SELECTOR="$2"
      shift 2
      ;;
    --help|-h)
      echo "Usage:"
      echo "  ${0}                          # interactive menu"
      echo "  ${0} --run-backup [--config-name NAME_OR_SLUG]"
      exit 0
      ;;
    *)
      log "Error: unknown argument '$1'."
      exit 1
      ;;
  esac
done

ensure_config_dirs
migrate_legacy_config_if_needed

if [[ "${MODE}" == "run-backup" ]]; then
  run_backup_now "${CONFIG_SELECTOR}"
  exit 0
fi

main_loop
