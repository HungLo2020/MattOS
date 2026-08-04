#!/usr/bin/env bash
# =============================================================================
# RunJellyfinStackContainer.sh
#
# Sets up and manages a Docker media stack:
#   - Jellyfin
#   - Radarr
#   - Sonarr
#   - Seerr
#   - Jackett
#   - qBittorrent (behind ProtonVPN via Gluetun kill-switch)
#
# Usage:
#   ./RunJellyfinStackContainer.sh          Install/update and start stack
#   ./RunJellyfinStackContainer.sh --on     Start stack only if installed
#   ./RunJellyfinStackContainer.sh --off    Stop stack without deleting data
#   ./RunJellyfinStackContainer.sh -D       Stop stack and delete stack files
# =============================================================================

set -euo pipefail

ACTION="run"
if [[ "$#" -gt 1 ]]; then
  echo "Error: too many arguments. Use -D, --on, --off, or no flag."
  exit 1
fi
if [[ "$#" -eq 1 ]]; then
  case "$1" in
    -D) ACTION="delete" ;;
    --on) ACTION="on" ;;
    --off) ACTION="off" ;;
    *)
      echo "Error: unknown argument '$1'. Use -D, --on, --off, or no flag."
      exit 1
      ;;
  esac
fi

log() { echo "[$(date '+%Y-%m-%d %H:%M:%S')] $*"; }

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="${SCRIPT_DIR}"
while [[ "${REPO_ROOT}" != "/" && ! -d "${REPO_ROOT}/resources" ]]; do
  REPO_ROOT="$(dirname "${REPO_ROOT}")"
done

if [[ ! -d "${REPO_ROOT}/resources" ]]; then
  echo "Error: could not locate repository resources directory from ${SCRIPT_DIR}"
  exit 1
fi

BW_MASTER_PASSWORD_FILE="${REPO_ROOT}/.bw_master_password"

RESOURCE_DIR="${REPO_ROOT}/resources/jellyfin"
COMPOSE_TEMPLATE="${RESOURCE_DIR}/docker-compose.yml"
ENV_TEMPLATE="${RESOURCE_DIR}/.env.example"

STACK_ROOT="${HOME}/.jellyfin-stack"
STACK_COMPOSE_FILE="${STACK_ROOT}/docker-compose.yml"
STACK_ENV_FILE="${STACK_ROOT}/.env"
STACK_MEDIA_PATHS_FILE="${STACK_ROOT}/media-paths.txt"

DOCKER_USE_SUDO="false"
docker_exec() {
  if [[ "${DOCKER_USE_SUDO}" == "true" ]]; then
    sudo docker "$@"
  else
    docker "$@"
  fi
}

compose_exec() {
  if docker_exec compose version >/dev/null 2>&1; then
    docker_exec compose "$@"
    return 0
  fi

  if command -v docker-compose >/dev/null 2>&1; then
    if [[ "${DOCKER_USE_SUDO}" == "true" ]]; then
      sudo docker-compose "$@"
    else
      docker-compose "$@"
    fi
    return 0
  fi

  return 1
}

ensure_docker() {
  if ! command -v docker >/dev/null 2>&1; then
    if [[ "${ACTION}" != "run" ]]; then
      echo "Error: Docker is not installed."
      exit 1
    fi
    log "Docker not found. Installing via official script..."
    curl -fsSL https://get.docker.com | sudo sh
    sudo usermod -aG docker "${USER}" || true
    log "Docker installed. You may need to re-login for docker group permissions."
  fi

  if ! docker info >/dev/null 2>&1; then
    if sudo docker info >/dev/null 2>&1; then
      DOCKER_USE_SUDO="true"
      log "Using 'sudo docker' for this session."
    else
      echo "Error: cannot connect to Docker daemon."
      exit 1
    fi
  fi

  if ! compose_exec version >/dev/null 2>&1; then
    if [[ "${ACTION}" != "run" ]]; then
      echo "Error: Docker Compose is not available."
      exit 1
    fi

    log "Docker Compose not found. Installing docker-compose-plugin..."
    if command -v apt-get >/dev/null 2>&1; then
      sudo apt-get update
      sudo apt-get install -y docker-compose-plugin
    else
      echo "Error: couldn't auto-install compose plugin on this distro. Install Docker Compose manually."
      exit 1
    fi

    if ! compose_exec version >/dev/null 2>&1; then
      echo "Error: Docker Compose still not available after install."
      exit 1
    fi
  fi
}

validate_template_files() {
  if [[ ! -f "${COMPOSE_TEMPLATE}" ]]; then
    echo "Error: missing compose template: ${COMPOSE_TEMPLATE}"
    exit 1
  fi

  if [[ ! -f "${ENV_TEMPLATE}" ]]; then
    echo "Error: missing env template: ${ENV_TEMPLATE}"
    exit 1
  fi
}

prompt_non_empty() {
  local prompt="$1"
  local value=""
  while true; do
    read -r -p "$prompt" value
    if [[ -n "${value}" ]]; then
      echo "${value}"
      return 0
    fi
    echo "Value cannot be empty."
  done
}

prompt_absolute_existing_dir() {
  local prompt="$1"
  local path=""
  while true; do
    read -r -p "$prompt" path
    if [[ -z "${path}" ]]; then
      echo "Path cannot be empty."
      continue
    fi
    if [[ "${path}" != /* ]]; then
      echo "Please provide an absolute path starting with '/'."
      continue
    fi
    if [[ ! -d "${path}" ]]; then
      echo "Directory does not exist: ${path}"
      continue
    fi
    echo "${path}"
    return 0
  done
}

load_saved_media_paths() {
  if [[ ! -f "${STACK_MEDIA_PATHS_FILE}" ]]; then
    return 1
  fi

  local saved_media saved_music saved_downloads
  saved_media="$(sed -n 's/^MEDIA_PATH=//p' "${STACK_MEDIA_PATHS_FILE}" | head -n1)"
  saved_music="$(sed -n 's/^MUSIC_PATH=//p' "${STACK_MEDIA_PATHS_FILE}" | head -n1)"
  saved_downloads="$(sed -n 's/^DOWNLOADS_PATH=//p' "${STACK_MEDIA_PATHS_FILE}" | head -n1)"

  if [[ -z "${saved_media}" || -z "${saved_music}" || -z "${saved_downloads}" ]]; then
    return 1
  fi

  if [[ "${saved_media}" != /* || "${saved_music}" != /* || "${saved_downloads}" != /* ]]; then
    return 1
  fi

  SAVED_MEDIA_PATH="${saved_media}"
  SAVED_MUSIC_PATH="${saved_music}"
  SAVED_DOWNLOADS_PATH="${saved_downloads}"
  return 0
}

save_media_paths() {
  local media_path="$1"
  local music_path="$2"
  local downloads_path="$3"

  mkdir -p "${STACK_ROOT}"
  cat > "${STACK_MEDIA_PATHS_FILE}" <<EOF
MEDIA_PATH=${media_path}
MUSIC_PATH=${music_path}
DOWNLOADS_PATH=${downloads_path}
EOF
}

prompt_for_media_paths() {
  local media_path=""
  local music_path=""
  local downloads_path=""
  local choice=""

  if load_saved_media_paths; then
    echo "Saved media path config found: ${STACK_MEDIA_PATHS_FILE}"
    echo "  MEDIA_PATH=${SAVED_MEDIA_PATH}"
    echo "  MUSIC_PATH=${SAVED_MUSIC_PATH}"
    echo "  DOWNLOADS_PATH=${SAVED_DOWNLOADS_PATH}"
    echo
    read -r -p "Use existing saved paths? [Y/n]: " choice
    if [[ ! "${choice}" =~ ^[Nn]$ ]]; then
      media_path="${SAVED_MEDIA_PATH}"
      music_path="${SAVED_MUSIC_PATH}"
      downloads_path="${SAVED_DOWNLOADS_PATH}"

      if [[ ! -d "${media_path}" ]]; then
        echo "Error: saved MEDIA_PATH no longer exists: ${media_path}"
        return 1
      fi

      if [[ ! -d "${music_path}" ]]; then
        echo "Error: saved MUSIC_PATH no longer exists: ${music_path}"
        return 1
      fi

      mkdir -p "${downloads_path}"

      PROMPTED_MEDIA_PATH="${media_path}"
      PROMPTED_MUSIC_PATH="${music_path}"
      PROMPTED_DOWNLOADS_PATH="${downloads_path}"
      return 0
    fi
  fi

  log "Paste the absolute path to your existing media root directory."
  media_path="$(prompt_absolute_existing_dir 'Media path: ')"
  if [[ "${media_path}" != "/" ]]; then
    media_path="${media_path%/}"
  fi

  log "Paste a second absolute library path (tip: use your music directory)."
  music_path="$(prompt_absolute_existing_dir 'Second library path (music): ')"
  if [[ "${music_path}" != "/" ]]; then
    music_path="${music_path%/}"
  fi

  read -r -p "Downloads path (Enter for ${media_path}/downloads): " downloads_path
  if [[ -z "${downloads_path}" ]]; then
    downloads_path="${media_path}/downloads"
  fi

  if [[ "${downloads_path}" != /* ]]; then
    echo "Error: downloads path must be absolute."
    return 1
  fi

  mkdir -p "${downloads_path}"
  save_media_paths "${media_path}" "${music_path}" "${downloads_path}"

  PROMPTED_MEDIA_PATH="${media_path}"
  PROMPTED_MUSIC_PATH="${music_path}"
  PROMPTED_DOWNLOADS_PATH="${downloads_path}"
}

bitwarden_status() {
  local status_json
  local parsed

  status_json="$(bw status 2>/dev/null || true)"
  parsed="$(printf '%s' "$status_json" | sed -n 's/.*"status"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p')"

  if [[ -z "$parsed" ]]; then
    echo "unknown"
  else
    echo "$parsed"
  fi
}

try_bitwarden_protonvpn_credentials() {
  local item_name="${BITWARDEN_PROTONVPN_ITEM:-ProtonVPN}"
  local status
  local session
  local bw_user
  local bw_pass

  if ! command -v bw >/dev/null 2>&1; then
    log "Bitwarden CLI (bw) not found; using manual ProtonVPN credential entry."
    return 1
  fi

  log "Attempting Bitwarden lookup for ProtonVPN credentials (item: ${item_name})..."
  status="$(bitwarden_status)"

  if [[ "$status" == "unauthenticated" || "$status" == "unknown" ]]; then
    log "Bitwarden CLI is not authenticated. Attempting 'bw login'..."
    if ! bw login </dev/tty >/dev/tty 2>&1; then
      log "Bitwarden login failed; falling back to manual ProtonVPN credential entry."
      return 1
    fi
    status="$(bitwarden_status)"
  fi

  if [[ "$status" == "locked" ]]; then
    log "Bitwarden vault is locked. Attempting 'bw unlock'..."
    if [[ -f "$BW_MASTER_PASSWORD_FILE" ]]; then
      IFS= read -r BW_MASTER_PASSWORD < "$BW_MASTER_PASSWORD_FILE"
      export BW_MASTER_PASSWORD
      session="$(bw unlock --passwordenv BW_MASTER_PASSWORD --nointeraction --raw 2>/dev/null || true)"
      unset BW_MASTER_PASSWORD
    else
      session="$(bw unlock --raw </dev/tty 2>/dev/null || true)"
    fi
    if [[ -z "$session" ]]; then
      log "Bitwarden unlock failed; falling back to manual ProtonVPN credential entry."
      return 1
    fi
    export BW_SESSION="$session"
  fi

  bw_user="$(bw get username "$item_name" 2>/dev/null || true)"
  bw_pass="$(bw get password "$item_name" 2>/dev/null || true)"

  if [[ -z "$bw_user" || -z "$bw_pass" ]]; then
    log "Bitwarden item missing username/password or item not found; falling back to manual entry."
    return 1
  fi

  PROTONVPN_USER_FROM_BW="$bw_user"
  PROTONVPN_PASS_FROM_BW="$bw_pass"
  return 0
}

write_env_file() {
  local media_path="$1"
  local music_path="$2"
  local downloads_path="$3"
  local proton_user="$4"
  local proton_pass="$5"
  local proton_country="$6"

  mkdir -p "${STACK_ROOT}"
  cp -f "${ENV_TEMPLATE}" "${STACK_ENV_FILE}"

  local uid gid tz
  uid="$(id -u)"
  gid="$(id -g)"
  tz="${TZ:-America/Los_Angeles}"

  cat > "${STACK_ENV_FILE}" <<EOF
PUID=${uid}
PGID=${gid}
TZ=${tz}

STACK_ROOT=${STACK_ROOT}
MEDIA_PATH=${media_path}
MUSIC_PATH=${music_path}
DOWNLOADS_PATH=${downloads_path}

PROTONVPN_USER=${proton_user}
PROTONVPN_PASSWORD=${proton_pass}
PROTONVPN_COUNTRY=${proton_country}

JELLYFIN_PORT=8096
RADARR_PORT=7878
SONARR_PORT=8989
SEERR_PORT=5055
JACKETT_PORT=9117
FLARESOLVERR_PORT=8191
QBITTORRENT_WEBUI_PORT=8080
QBITTORRENT_TORRENT_PORT=6881
EOF
}

copy_compose_file() {
  mkdir -p "${STACK_ROOT}"
  cp -f "${COMPOSE_TEMPLATE}" "${STACK_COMPOSE_FILE}"
}

env_value() {
  local key="$1"
  local file="$2"
  sed -n "s/^${key}=//p" "$file" | head -n1
}

migrate_installed_stack_to_protonvpn_if_needed() {
  if [[ ! -f "${STACK_COMPOSE_FILE}" || ! -f "${STACK_ENV_FILE}" ]]; then
    return 0
  fi

  local needs_migration="false"
  if grep -q 'VPN_SERVICE_PROVIDER=nordvpn' "${STACK_COMPOSE_FILE}"; then
    needs_migration="true"
  fi
  if grep -q '^NORDVPN_' "${STACK_ENV_FILE}"; then
    needs_migration="true"
  fi

  if [[ "${needs_migration}" != "true" ]]; then
    return 0
  fi

  log "Existing stack appears to use NordVPN; migrating configuration to ProtonVPN..."

  local proton_user=""
  local proton_pass=""
  local proton_country=""

  proton_country="$(env_value "PROTONVPN_COUNTRY" "${STACK_ENV_FILE}")"
  if [[ -z "${proton_country}" ]]; then
    proton_country="$(env_value "NORDVPN_COUNTRY" "${STACK_ENV_FILE}")"
  fi
  if [[ -z "${proton_country}" ]]; then
    proton_country="United States"
  fi

  if try_bitwarden_protonvpn_credentials; then
    proton_user="$PROTONVPN_USER_FROM_BW"
    proton_pass="$PROTONVPN_PASS_FROM_BW"
    log "Using ProtonVPN credentials from Bitwarden for migration."
  else
    echo "Error: migration requires Bitwarden item 'ProtonVPN' with username/password."
    exit 1
  fi

  local tmp_env
  tmp_env="$(mktemp)"
  awk -v user="$proton_user" -v pass="$proton_pass" -v country="$proton_country" '
    BEGIN {
      seen_user=0; seen_pass=0; seen_country=0;
    }
    /^NORDVPN_USER=/ || /^PROTONVPN_USER=/ {
      if (!seen_user) { print "PROTONVPN_USER=" user; seen_user=1; }
      next
    }
    /^NORDVPN_PASSWORD=/ || /^PROTONVPN_PASSWORD=/ {
      if (!seen_pass) { print "PROTONVPN_PASSWORD=" pass; seen_pass=1; }
      next
    }
    /^NORDVPN_COUNTRY=/ || /^PROTONVPN_COUNTRY=/ {
      if (!seen_country) { print "PROTONVPN_COUNTRY=" country; seen_country=1; }
      next
    }
    { print }
    END {
      if (!seen_user) print "PROTONVPN_USER=" user;
      if (!seen_pass) print "PROTONVPN_PASSWORD=" pass;
      if (!seen_country) print "PROTONVPN_COUNTRY=" country;
    }
  ' "${STACK_ENV_FILE}" > "${tmp_env}"

  mv "${tmp_env}" "${STACK_ENV_FILE}"

  sed -i \
    -e 's/VPN_SERVICE_PROVIDER=nordvpn/VPN_SERVICE_PROVIDER=protonvpn/g' \
    -e 's/${NORDVPN_USER}/${PROTONVPN_USER}/g' \
    -e 's/${NORDVPN_PASSWORD}/${PROTONVPN_PASSWORD}/g' \
    -e 's/${NORDVPN_COUNTRY}/${PROTONVPN_COUNTRY}/g' \
    "${STACK_COMPOSE_FILE}"

  log "Migration to ProtonVPN completed."
}

print_qbittorrent_credentials() {
  local username="admin"
  local password_line=""
  local password_value=""
  local i

  for i in {1..15}; do
    password_line="$(docker_exec logs qbittorrent 2>&1 | grep -i 'temporary password' | tail -n1 || true)"
    if [[ -n "${password_line}" ]]; then
      break
    fi
    sleep 1
  done

  if [[ -n "${password_line}" ]]; then
    password_value="$(printf '%s' "${password_line}" | sed -E 's/.*session[: ]+//')"
    log "qBittorrent login username: ${username}"
    log "qBittorrent temporary password: ${password_value}"
    log "Change this in qBittorrent WebUI after first login."
  else
    log "qBittorrent login username: ${username}"
    log "Temporary password not found in logs (it may already be configured)."
    log "To inspect manually: sudo docker logs qbittorrent | grep -i password"
  fi
}

start_stack() {
  mkdir -p \
    "${STACK_ROOT}/config/jellyfin" \
    "${STACK_ROOT}/config/radarr" \
    "${STACK_ROOT}/config/sonarr" \
    "${STACK_ROOT}/config/seerr" \
    "${STACK_ROOT}/config/jackett" \
    "${STACK_ROOT}/config/qbittorrent"

  compose_exec -f "${STACK_COMPOSE_FILE}" --env-file "${STACK_ENV_FILE}" pull
  compose_exec -f "${STACK_COMPOSE_FILE}" --env-file "${STACK_ENV_FILE}" up -d

  log "Stack started."
  log "Jellyfin:     http://localhost:8096"
  log "Radarr:       http://localhost:7878"
  log "Sonarr:       http://localhost:8989"
  log "Seerr:        http://localhost:5055"
  log "Jackett:      http://localhost:9117"
  log "For Radarr/Sonarr indexer URL use: http://jackett:9117"
  log "FlareSolverr: internal to Jackett at http://localhost:8191"
  log "qBittorrent:  http://localhost:8080"
  print_qbittorrent_credentials
}

stop_stack() {
  if [[ ! -f "${STACK_COMPOSE_FILE}" || ! -f "${STACK_ENV_FILE}" ]]; then
    log "Stack is not installed. Nothing to stop."
    return 0
  fi

  compose_exec -f "${STACK_COMPOSE_FILE}" --env-file "${STACK_ENV_FILE}" stop || true
  log "Stack stopped."
}

delete_stack() {
  if [[ -f "${STACK_COMPOSE_FILE}" && -f "${STACK_ENV_FILE}" ]]; then
    compose_exec -f "${STACK_COMPOSE_FILE}" --env-file "${STACK_ENV_FILE}" down --remove-orphans || true
  fi

  if [[ -d "${STACK_ROOT}" ]]; then
    rm -rf "${STACK_ROOT}"
    log "Removed stack files: ${STACK_ROOT}"
  else
    log "Stack directory does not exist."
  fi
}

ensure_installed_for_on_off() {
  if [[ ! -f "${STACK_COMPOSE_FILE}" || ! -f "${STACK_ENV_FILE}" ]]; then
    echo "Error: stack not installed. Run without flags first."
    exit 1
  fi
}

validate_template_files
ensure_docker

case "${ACTION}" in
  delete)
    delete_stack
    ;;

  off)
    ensure_installed_for_on_off
    stop_stack
    ;;

  on)
    ensure_installed_for_on_off
    migrate_installed_stack_to_protonvpn_if_needed
    start_stack
    ;;

  run)
    prompt_for_media_paths
    media_path="${PROMPTED_MEDIA_PATH}"
    music_path="${PROMPTED_MUSIC_PATH}"
    downloads_path="${PROMPTED_DOWNLOADS_PATH}"

    if try_bitwarden_protonvpn_credentials; then
      proton_user="$PROTONVPN_USER_FROM_BW"
      proton_pass="$PROTONVPN_PASS_FROM_BW"
      log "Using ProtonVPN credentials from Bitwarden."
    else
      proton_user="$(prompt_non_empty 'ProtonVPN username (OpenVPN/IKEv2 service credentials): ')"
      proton_pass="$(prompt_non_empty 'ProtonVPN password (OpenVPN/IKEv2 service credentials): ')"
    fi

    read -r -p "ProtonVPN country (Enter for United States): " proton_country
    if [[ -z "${proton_country}" ]]; then
      proton_country="United States"
    fi

    write_env_file "${media_path}" "${music_path}" "${downloads_path}" "${proton_user}" "${proton_pass}" "${proton_country}"
    copy_compose_file
    start_stack
    ;;
esac
