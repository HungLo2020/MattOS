#!/usr/bin/env bash

set -euo pipefail

DEFAULT_BACKUP_DEST_DIR="/srv/storage/OneDrive/Apps/Games/Storage/MattMC/AutoZipArchives/"
DEFAULT_BACKUP_SOURCE_DIR="/srv/storage/Storage/Sync/MattMC/"
DEFAULT_BACKUP_NAME="mattmc"
DEFAULT_CONFIG_NAME="MattMC"

CONFIG_ROOT="${HOME}/.config/zip-backup-manager"
CONFIGS_DIR="${CONFIG_ROOT}/configs"
HELPERS_DIR="${CONFIG_ROOT}/helpers"
CURRENT_CONFIG_FILE="${CONFIG_ROOT}/current_config"

KEEP_DAILY=3
KEEP_WEEKLY=3
KEEP_MONTHLY=3
KEEP_YEARLY=2

ACTIVE_CONFIG_NAME=""
ACTIVE_CONFIG_SLUG=""
ACTIVE_CONFIG_FILE=""
BACKUP_DEST_DIR=""
BACKUP_SOURCE_DIR=""
BACKUP_NAME=""

declare -a CONFIG_INDEX_SLUGS=()
declare -a CONFIG_INDEX_NAMES=()

log() {
  echo "[$(date '+%Y-%m-%d %H:%M:%S')] $*"
}

ensure_config_dirs() {
  mkdir -p "${CONFIGS_DIR}" "${HELPERS_DIR}"
  chmod 700 "${CONFIG_ROOT}" "${CONFIGS_DIR}" "${HELPERS_DIR}" 2>/dev/null || true
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

sanitize_backup_name() {
  local input="$1"
  local safe

  safe="$(printf '%s' "${input}" | tr '[:upper:]' '[:lower:]' | sed -E 's/[^a-z0-9_-]+/-/g; s/^-+//; s/-+$//')"
  echo "${safe}"
}

config_file_for_slug() {
  local slug="$1"
  echo "${CONFIGS_DIR}/${slug}.env"
}

helper_script_path_for_slug() {
  local slug="$1"
  echo "${HELPERS_DIR}/zip-backup-${slug}.sh"
}

service_name_for_slug() {
  local slug="$1"
  echo "zip-${slug}-backup.service"
}

timer_name_for_slug() {
  local slug="$1"
  echo "zip-${slug}-backup.timer"
}

service_path_for_slug() {
  local slug="$1"
  echo "/etc/systemd/system/$(service_name_for_slug "${slug}")"
}

timer_path_for_slug() {
  local slug="$1"
  echo "/etc/systemd/system/$(timer_name_for_slug "${slug}")"
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

ensure_dependencies() {
  local -a missing=()
  local cmd

  for cmd in zip unzip; do
    if ! command -v "${cmd}" >/dev/null 2>&1; then
      missing+=("${cmd}")
    fi
  done

  if [[ "${#missing[@]}" -eq 0 ]]; then
    return 0
  fi

  log "Missing dependencies: ${missing[*]}"
  if command -v apt-get >/dev/null 2>&1; then
    sudo apt-get update
    sudo apt-get install -y "${missing[@]}"
  else
    log "Error: cannot auto-install dependencies on this distro."
    log "Install manually and rerun."
    exit 1
  fi

  for cmd in "${missing[@]}"; do
    if ! command -v "${cmd}" >/dev/null 2>&1; then
      log "Error: dependency install failed for ${cmd}."
      exit 1
    fi
  done
}

ensure_systemd_available() {
  if ! command -v systemctl >/dev/null 2>&1; then
    log "Error: systemctl is not available on this system."
    exit 1
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

prompt_backup_name() {
  local result_var="$1"
  local entered=""
  local sanitized=""

  while true; do
    read -r -p "Enter backup archive prefix [${DEFAULT_BACKUP_NAME}]: " entered
    entered="${entered:-${DEFAULT_BACKUP_NAME}}"
    sanitized="$(sanitize_backup_name "${entered}")"

    if [[ -z "${sanitized}" ]]; then
      echo "Backup archive prefix must include letters or numbers."
      continue
    fi

    printf -v "${result_var}" '%s' "${sanitized}"
    return 0
  done
}

ensure_destination_writable() {
  local path="$1"
  local probe="${path}/.zip-backup-write-probe"

  if [[ ! -d "${path}" ]]; then
    if mkdir -p "${path}" 2>/dev/null; then
      :
    else
      sudo mkdir -p "${path}"
      sudo chown "${USER}:${USER}" "${path}"
    fi
  fi

  if touch "${probe}" 2>/dev/null; then
    rm -f "${probe}"
    return 0
  fi

  if sudo touch "${probe}" >/dev/null 2>&1; then
    sudo rm -f "${probe}" >/dev/null 2>&1 || true
    return 0
  fi

  log "Error: destination is not writable: ${path}"
  exit 1
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
  local dest_dir="$3"
  local source_dir="$4"
  local backup_name="$5"
  local config_file

  config_file="$(config_file_for_slug "${config_slug}")"

  {
    printf 'CONFIG_NAME=%q\n' "${config_name}"
    printf 'CONFIG_SLUG=%q\n' "${config_slug}"
    printf 'BACKUP_DEST_DIR=%q\n' "${dest_dir}"
    printf 'BACKUP_SOURCE_DIR=%q\n' "${source_dir}"
    printf 'BACKUP_NAME=%q\n' "${backup_name}"
    printf 'KEEP_DAILY=%q\n' "${KEEP_DAILY}"
    printf 'KEEP_WEEKLY=%q\n' "${KEEP_WEEKLY}"
    printf 'KEEP_MONTHLY=%q\n' "${KEEP_MONTHLY}"
    printf 'KEEP_YEARLY=%q\n' "${KEEP_YEARLY}"
  } >"${config_file}"

  chmod 600 "${config_file}"
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

  if [[ -z "${CONFIG_NAME:-}" || -z "${BACKUP_DEST_DIR:-}" || -z "${BACKUP_SOURCE_DIR:-}" || -z "${BACKUP_NAME:-}" ]]; then
    log "Error: configuration file is missing required fields."
    return 1
  fi

  ACTIVE_CONFIG_NAME="${CONFIG_NAME}"
  ACTIVE_CONFIG_SLUG="${slug}"
  ACTIVE_CONFIG_FILE="${config_file}"
  set_current_config_slug "${slug}"

  return 0
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

list_all_configs() {
  local idx=1
  local config_file slug timer_name timer_state

  collect_config_index

  if [[ "${#CONFIG_INDEX_SLUGS[@]}" -eq 0 ]]; then
    echo "No backup configs found."
    return 0
  fi

  echo "=== All Zip Backup Configs ==="
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
    echo "    Source: ${BACKUP_SOURCE_DIR:-unknown}"
    echo "    Dest:   ${BACKUP_DEST_DIR:-unknown}"
    echo "    Prefix: ${BACKUP_NAME:-unknown}"
    echo "    Timer:  ${timer_name} (${timer_state})"
    ((idx++))
  done
}

create_helper_script() {
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
  echo "[\$(date '+%Y-%m-%d %H:%M:%S')] [zip-helper] \$*"
}

set_bucket_latest() {
  local bucket_map_name="\$1"
  local bucket_key="\$2"
  local epoch="\$3"
  local file_path="\$4"
  local -n bucket_map_ref="\${bucket_map_name}"

  if [[ -z "\${bucket_map_ref[\${bucket_key}]:-}" ]]; then
    bucket_map_ref["\${bucket_key}"]="\${epoch}|\${file_path}"
    return 0
  fi

  local current_entry current_epoch
  current_entry="\${bucket_map_ref[\${bucket_key}]}"
  current_epoch="\${current_entry%%|*}"

  if (( epoch > current_epoch )); then
    bucket_map_ref["\${bucket_key}"]="\${epoch}|\${file_path}"
  fi
}

mark_recent_from_bucket() {
  local bucket_map_name="\$1"
  local keep_map_name="\$2"
  local keep_count="\$3"
  local -n bucket_map_ref="\${bucket_map_name}"
  local -n keep_map_ref="\${keep_map_name}"
  local line epoch file

  while IFS='|' read -r epoch file; do
    [[ -n "\${file}" ]] || continue
    keep_map_ref["\${file}"]=1
  done < <(
    for line in "\${bucket_map_ref[@]}"; do
      printf '%s\n' "\${line}"
    done | sort -t'|' -k1,1nr | head -n "\${keep_count}"
  )
}

prune_archives() {
  local dest_dir="\${BACKUP_DEST_DIR%/}"
  local base_name="\${BACKUP_NAME}"
  local file base date_part time_part ts_text epoch
  local day_key week_key month_key year_key
  local -a managed_files=()
  local -a all_files=()
  local -A daily_bucket=()
  local -A weekly_bucket=()
  local -A monthly_bucket=()
  local -A yearly_bucket=()
  local -A keep_files=()

  shopt -s nullglob
  all_files=("\${dest_dir}/\${base_name}"_*.zip)
  shopt -u nullglob

  if [[ "\${#all_files[@]}" -eq 0 ]]; then
    log "No archives found for pruning."
    return 0
  fi

  for file in "\${all_files[@]}"; do
    base="\$(basename "\${file}")"
    if [[ ! "\${base}" =~ ^\${base_name}_([0-9]{4}-[0-9]{2}-[0-9]{2})_([0-9]{2}-[0-9]{2}-[0-9]{2})\.zip$ ]]; then
      continue
    fi

    date_part="\${BASH_REMATCH[1]}"
    time_part="\${BASH_REMATCH[2]}"
    ts_text="\${date_part} \${time_part//-/:}"

    if ! epoch="\$(date -d "\${ts_text}" +%s 2>/dev/null)"; then
      continue
    fi

    managed_files+=("\${file}")
    day_key="\$(date -d "@\${epoch}" +%F)"
    week_key="\$(date -d "@\${epoch}" +%G-%V)"
    month_key="\$(date -d "@\${epoch}" +%Y-%m)"
    year_key="\$(date -d "@\${epoch}" +%Y)"

    set_bucket_latest daily_bucket "\${day_key}" "\${epoch}" "\${file}"
    set_bucket_latest weekly_bucket "\${week_key}" "\${epoch}" "\${file}"
    set_bucket_latest monthly_bucket "\${month_key}" "\${epoch}" "\${file}"
    set_bucket_latest yearly_bucket "\${year_key}" "\${epoch}" "\${file}"
  done

  if [[ "\${#managed_files[@]}" -eq 0 ]]; then
    log "No managed archives matched expected pattern."
    return 0
  fi

  mark_recent_from_bucket daily_bucket keep_files "\${KEEP_DAILY}"
  mark_recent_from_bucket weekly_bucket keep_files "\${KEEP_WEEKLY}"
  mark_recent_from_bucket monthly_bucket keep_files "\${KEEP_MONTHLY}"
  mark_recent_from_bucket yearly_bucket keep_files "\${KEEP_YEARLY}"

  for file in "\${managed_files[@]}"; do
    if [[ -n "\${keep_files[\${file}]:-}" ]]; then
      continue
    fi

    rm -f "\${file}" "\${file}.sha256"
    log "Pruned: \${file}"
  done
}

create_backup_archive() {
  local source_dir="\${BACKUP_SOURCE_DIR%/}"
  local dest_dir="\${BACKUP_DEST_DIR%/}"
  local timestamp archive_path archive_tmp source_parent source_base

  if [[ ! -d "\${source_dir}" ]]; then
    log "Error: source path missing: \${source_dir}"
    exit 1
  fi

  mkdir -p "\${dest_dir}"

  timestamp="\$(date +%Y-%m-%d_%H-%M-%S)"
  archive_path="\${dest_dir}/\${BACKUP_NAME}_\${timestamp}.zip"
  archive_tmp="\${archive_path}.tmp"
  source_parent="\$(dirname "\${source_dir}")"
  source_base="\$(basename "\${source_dir}")"

  rm -f "\${archive_tmp}"

  log "Creating archive: \${archive_path}"
  (
    cd "\${source_parent}"
    zip -r -q "\${archive_tmp}" "\${source_base}"
  )

  mv "\${archive_tmp}" "\${archive_path}"

  if ! unzip -tqq "\${archive_path}" >/dev/null 2>&1; then
    rm -f "\${archive_path}"
    log "Error: archive integrity test failed."
    exit 1
  fi

  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "\${archive_path}" >"\${archive_path}.sha256"
  fi
}

if [[ ! -f "\${CONFIG_FILE}" ]]; then
  log "Error: config file missing: \${CONFIG_FILE}"
  exit 1
fi

# shellcheck disable=SC1090
source "\${CONFIG_FILE}"

for required_var in CONFIG_NAME CONFIG_SLUG BACKUP_DEST_DIR BACKUP_SOURCE_DIR BACKUP_NAME KEEP_DAILY KEEP_WEEKLY KEEP_MONTHLY KEEP_YEARLY; do
  if [[ -z "\${!required_var:-}" ]]; then
    log "Error: missing required setting '\${required_var}' in \${CONFIG_FILE}"
    exit 1
  fi
done

if ! command -v zip >/dev/null 2>&1 || ! command -v unzip >/dev/null 2>&1; then
  log "Error: zip/unzip is not installed."
  exit 1
fi

MODE="\${1:-backup}"
if [[ "\${MODE}" == "--prune-only" ]]; then
  prune_archives
  log "Prune-only run complete."
  exit 0
fi

create_backup_archive
prune_archives
log "Backup + prune run complete."
EOF

  chmod 700 "${helper_script}"
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
  create_helper_script "${ACTIVE_CONFIG_SLUG}"

  sudo tee "${service_path}" >/dev/null <<EOF
[Unit]
Description=Zip backup for ${ACTIVE_CONFIG_NAME}
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
Description=Daily zip backup timer for ${ACTIVE_CONFIG_NAME}

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

execute_helper_for_slug() {
  local slug="$1"
  local mode="${2:-backup}"
  local helper_script

  if ! load_config_by_slug "${slug}"; then
    return 1
  fi

  create_helper_script "${slug}"
  helper_script="$(helper_script_path_for_slug "${slug}")"

  if [[ "${mode}" == "prune" ]]; then
    "${helper_script}" --prune-only
  else
    "${helper_script}"
  fi
}

show_config_by_index() {
  local index="$1"
  local slug service_name timer_name

  if ! resolve_config_slug_by_index "${index}" slug; then
    return 1
  fi
  if ! load_config_by_slug "${slug}"; then
    return 1
  fi

  service_name="$(service_name_for_slug "${slug}")"
  timer_name="$(timer_name_for_slug "${slug}")"

  echo "=== Zip Backup Config ==="
  echo "Name:       ${ACTIVE_CONFIG_NAME}"
  echo "Slug:       ${ACTIVE_CONFIG_SLUG}"
  echo "Source:     ${BACKUP_SOURCE_DIR}"
  echo "Destination:${BACKUP_DEST_DIR}"
  echo "Prefix:     ${BACKUP_NAME}"
  echo "Retention:  daily=${KEEP_DAILY} weekly=${KEEP_WEEKLY} monthly=${KEEP_MONTHLY} yearly=${KEEP_YEARLY}"
  echo "Service:    ${service_name}"
  echo "Timer:      ${timer_name}"
}

list_archives_by_index() {
  local index="$1"
  local slug file date_part time_part created_at size_bytes size_h
  local -a archives=()

  if ! resolve_config_slug_by_index "${index}" slug; then
    return 1
  fi
  if ! load_config_by_slug "${slug}"; then
    return 1
  fi

  shopt -s nullglob
  archives=("${BACKUP_DEST_DIR%/}/${BACKUP_NAME}"_*.zip)
  shopt -u nullglob

  if [[ "${#archives[@]}" -eq 0 ]]; then
    echo "No archives found for config '${ACTIVE_CONFIG_NAME}'."
    return 0
  fi

  mapfile -t archives < <(printf '%s\n' "${archives[@]}" | sort -r)

  printf "%-4s %-20s %-10s %s\n" "No." "Created" "Size" "File"
  printf "%-4s %-20s %-10s %s\n" "----" "-------------------" "----------" "----"

  local i=1
  for file in "${archives[@]}"; do
    created_at="unknown"
    if [[ "$(basename "${file}")" =~ ^${BACKUP_NAME}_([0-9]{4}-[0-9]{2}-[0-9]{2})_([0-9]{2}-[0-9]{2}-[0-9]{2})\.zip$ ]]; then
      date_part="${BASH_REMATCH[1]}"
      time_part="${BASH_REMATCH[2]}"
      created_at="${date_part} ${time_part//-/:}"
    fi

    size_bytes="$(stat -c%s "${file}" 2>/dev/null || echo 0)"
    if command -v numfmt >/dev/null 2>&1; then
      size_h="$(numfmt --to=iec --suffix=B "${size_bytes}")"
    else
      size_h="${size_bytes}B"
    fi

    printf "%-4s %-20s %-10s %s\n" "${i})" "${created_at}" "${size_h}" "$(basename "${file}")"
    ((i++))
  done
}

is_destination_used_by_other_configs() {
  local dest_path="$1"
  local skip_slug="$2"
  local config_file slug candidate_dest

  shopt -s nullglob
  for config_file in "${CONFIGS_DIR}"/*.env; do
    slug="$(basename "${config_file}" .env)"
    if [[ "${slug}" == "${skip_slug}" ]]; then
      continue
    fi

    candidate_dest=""
    # shellcheck disable=SC1090
    source "${config_file}"
    candidate_dest="${BACKUP_DEST_DIR:-}"
    if [[ -n "${candidate_dest}" && "${candidate_dest}" == "${dest_path}" ]]; then
      shopt -u nullglob
      return 0
    fi
  done
  shopt -u nullglob

  return 1
}

delete_config_by_index() {
  local index="$1"
  local slug name config_file helper_script
  local service_name timer_name service_path timer_path
  local dest_path current_slug confirm delete_dest_choice

  collect_config_index
  if [[ "${#CONFIG_INDEX_SLUGS[@]}" -eq 0 ]]; then
    log "No configs to delete."
    return 0
  fi

  if [[ ! "${index}" =~ ^[0-9]+$ ]] || (( index < 1 || index > ${#CONFIG_INDEX_SLUGS[@]} )); then
    log "Invalid config number '${index}'."
    return 1
  fi

  slug="${CONFIG_INDEX_SLUGS[$((index - 1))]}"
  name="${CONFIG_INDEX_NAMES[$((index - 1))]}"

  if ! load_config_by_slug "${slug}"; then
    return 1
  fi

  config_file="$(config_file_for_slug "${slug}")"
  helper_script="$(helper_script_path_for_slug "${slug}")"
  service_name="$(service_name_for_slug "${slug}")"
  timer_name="$(timer_name_for_slug "${slug}")"
  service_path="$(service_path_for_slug "${slug}")"
  timer_path="$(timer_path_for_slug "${slug}")"
  dest_path="${BACKUP_DEST_DIR}"

  read -r -p "Delete config '${name}' [${slug}] and associated service/timer/helper? [y/N]: " confirm
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
  rm -f "${config_file}"

  if command -v systemctl >/dev/null 2>&1; then
    sudo systemctl daemon-reload || true
    sudo systemctl reset-failed "${service_name}" "${timer_name}" >/dev/null 2>&1 || true
  fi

  if is_destination_used_by_other_configs "${dest_path}" "${slug}"; then
    log "Destination is used by another config; preserving ${dest_path}."
  else
    read -r -p "Delete destination directory '${dest_path}' and all zip backups too? [y/N]: " delete_dest_choice
    delete_dest_choice="${delete_dest_choice:-N}"
    if [[ "${delete_dest_choice}" =~ ^[Yy]$ ]]; then
      rm -rf "${dest_path}" >/dev/null 2>&1 || sudo rm -rf "${dest_path}"
      log "Deleted destination directory: ${dest_path}"
    else
      log "Destination preserved: ${dest_path}"
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

run_setup_flow() {
  ensure_dependencies
  ensure_systemd_available
  ensure_config_dirs

  local config_name config_slug backup_name
  local dest_dir source_dir config_file overwrite_choice

  prompt_path "Enter backup destination directory" "${DEFAULT_BACKUP_DEST_DIR}" dest_dir
  prompt_path "Enter source path to zip" "${DEFAULT_BACKUP_SOURCE_DIR}" source_dir
  prompt_backup_name backup_name
  prompt_config_name config_name config_slug

  validate_source_directory "${source_dir}"
  ensure_destination_writable "${dest_dir}"

  config_file="$(config_file_for_slug "${config_slug}")"
  if [[ -f "${config_file}" ]]; then
    read -r -p "Config '${config_name}' already exists. Overwrite settings? [Y/n]: " overwrite_choice
    overwrite_choice="${overwrite_choice:-Y}"
    if [[ ! "${overwrite_choice}" =~ ^[Yy]$ ]]; then
      log "Setup cancelled."
      return 0
    fi
  fi

  write_config_file "${config_name}" "${config_slug}" "${dest_dir}" "${source_dir}" "${backup_name}"
  load_config_by_slug "${config_slug}"
  setup_systemd_timer

  log "Setup complete for '${ACTIVE_CONFIG_NAME}'."
}

run_backup_now_by_index() {
  local index="$1" slug

  ensure_dependencies
  if resolve_config_slug_by_index "${index}" slug; then
    execute_helper_for_slug "${slug}" backup
  fi
}

run_prune_now_by_index() {
  local index="$1" slug

  ensure_dependencies
  if resolve_config_slug_by_index "${index}" slug; then
    execute_helper_for_slug "${slug}" prune
  fi
}

trigger_service_by_index() {
  local index="$1" slug service_name

  ensure_systemd_available
  if ! resolve_config_slug_by_index "${index}" slug; then
    return 1
  fi

  service_name="$(service_name_for_slug "${slug}")"
  sudo systemctl start "${service_name}"
  log "Triggered ${service_name}."
}

print_menu() {
  echo
  list_all_configs
  echo
  echo "=== Zip Backup Manager ==="
  echo "1) Run / rerun setup"
  echo "2) Exit"
  echo
  echo "Special commands:"
  echo "  delete <config-number>   Delete config + service/timer/helper"
  echo "  3 <config-number>        Take immediate zip backup now"
  echo "  4 <config-number>        List zip archives"
  echo "  5 <config-number>        Run prune now"
  echo "  6 <config-number>        Show config details"
  echo "  7 <config-number>        Trigger systemd service now"
  echo "  backup|list|prune|show|service <config-number>"
  echo
}

main_loop() {
  local choice

  while true; do
    print_menu
    read -r -p "Choose an option [1-2 or command]: " choice

    if [[ "${choice}" =~ ^[Dd][Ee][Ll][Ee][Tt][Ee][[:space:]]+([0-9]+)$ ]]; then
      delete_config_by_index "${BASH_REMATCH[1]}"
      continue
    fi

    if [[ "${choice}" =~ ^3[[:space:]]+([0-9]+)$ ]] || [[ "${choice}" =~ ^[Bb][Aa][Cc][Kk][Uu][Pp][[:space:]]+([0-9]+)$ ]]; then
      run_backup_now_by_index "${BASH_REMATCH[1]}"
      continue
    fi

    if [[ "${choice}" =~ ^4[[:space:]]+([0-9]+)$ ]] || [[ "${choice}" =~ ^[Ll][Ii][Ss][Tt][[:space:]]+([0-9]+)$ ]]; then
      list_archives_by_index "${BASH_REMATCH[1]}"
      continue
    fi

    if [[ "${choice}" =~ ^5[[:space:]]+([0-9]+)$ ]] || [[ "${choice}" =~ ^[Pp][Rr][Uu][Nn][Ee][[:space:]]+([0-9]+)$ ]]; then
      run_prune_now_by_index "${BASH_REMATCH[1]}"
      continue
    fi

    if [[ "${choice}" =~ ^6[[:space:]]+([0-9]+)$ ]] || [[ "${choice}" =~ ^[Ss][Hh][Oo][Ww][[:space:]]+([0-9]+)$ ]]; then
      show_config_by_index "${BASH_REMATCH[1]}"
      continue
    fi

    if [[ "${choice}" =~ ^7[[:space:]]+([0-9]+)$ ]] || [[ "${choice}" =~ ^[Ss][Ee][Rr][Vv][Ii][Cc][Ee][[:space:]]+([0-9]+)$ ]]; then
      trigger_service_by_index "${BASH_REMATCH[1]}"
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

ensure_config_dirs
main_loop
