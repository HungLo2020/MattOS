#!/usr/bin/env bash
# =============================================================================
# RunUptimeKumaContainer.sh
#
# Self-contained script to install and run Uptime Kuma via Docker.
#
# Usage:
#   ./RunUptimeKumaContainer.sh          Install (if needed) and start Kuma
#   ./RunUptimeKumaContainer.sh -D       Stop container and delete all files/image
#   ./RunUptimeKumaContainer.sh --off    Stop container without deleting files
#   ./RunUptimeKumaContainer.sh --on     Start container only if already installed
#
# Notes:
#   - Uptime Kuma UI is exposed on http://localhost:3002 by default.
#   - Persistent data path: ~/.uptime-kuma/data
# =============================================================================

set -euo pipefail

# ── Configuration ─────────────────────────────────────────────────────────────
CONTAINER_NAME="uptime-kuma"
IMAGE_NAME="louislam/uptime-kuma:latest"
BASE_DATA_DIR="${HOME}/.uptime-kuma"
DATA_DIR="${BASE_DATA_DIR}/data"
WEB_PORT="${UPTIME_KUMA_PORT:-3002}"

# ── Helpers ───────────────────────────────────────────────────────────────────
log() { echo "[$(date '+%Y-%m-%d %H:%M:%S')] $*"; }

DOCKER_USE_SUDO="false"
docker_exec() {
  if [[ "${DOCKER_USE_SUDO}" == "true" ]]; then
    sudo docker "$@"
  else
    docker "$@"
  fi
}

container_exists() {
  docker_exec ps -aq --filter "name=^/$1$" 2>/dev/null | grep -q .
}

container_running() {
  docker_exec ps -q --filter "name=^/$1$" 2>/dev/null | grep -q .
}

is_port_in_use() {
  local port="$1"

  if command -v ss >/dev/null 2>&1; then
    ss -ltn "( sport = :${port} )" 2>/dev/null | grep -q ":${port}"
    return
  fi

  if command -v lsof >/dev/null 2>&1; then
    lsof -iTCP:"${port}" -sTCP:LISTEN >/dev/null 2>&1
    return
  fi

  return 1
}

# ── Parse action ──────────────────────────────────────────────────────────────
ACTION="run"
if [[ "$#" -gt 1 ]]; then
  log "Error: too many arguments."
  exit 1
fi

if [[ "$#" -eq 1 ]]; then
  case "$1" in
    -D)
      ACTION="delete"
      ;;
    --off)
      ACTION="off"
      ;;
    --on)
      ACTION="on"
      ;;
    *)
      log "Error: unknown argument '$1'. Use -D, --off, or --on."
      exit 1
      ;;
  esac
fi

# ── Docker availability ───────────────────────────────────────────────────────
if [[ "${ACTION}" == "run" ]] && ! command -v docker &>/dev/null; then
  log "Docker not found. Installing via the official get.docker.com script..."
  curl -fsSL https://get.docker.com | sudo sh
  sudo usermod -aG docker "${USER}" || true
  log "Docker installed."
fi

if ! command -v docker &>/dev/null; then
  if [[ "${ACTION}" == "delete" ]]; then
    log "Warning: Docker not found; skipping container/image removal."
    if [[ -d "${BASE_DATA_DIR}" ]]; then
      log "Removing data directory: ${BASE_DATA_DIR}"
      rm -rf "${BASE_DATA_DIR}"
    else
      log "Data directory does not exist."
    fi
    log "=== Cleanup complete ==="
    exit 0
  fi

  log "Error: Docker is not installed."
  exit 1
fi

if ! docker info &>/dev/null; then
  if sudo docker info &>/dev/null; then
    DOCKER_USE_SUDO="true"
    log "Using 'sudo docker' for this session (user not yet in docker group)."
  else
    log "Error: cannot connect to the Docker daemon. Is Docker running?"
    exit 1
  fi
fi

# ── -D delete mode ────────────────────────────────────────────────────────────
if [[ "${ACTION}" == "delete" ]]; then
  log "=== Shutting down Uptime Kuma and removing files ==="

  if container_running "${CONTAINER_NAME}"; then
    log "Stopping ${CONTAINER_NAME}..."
    docker_exec stop "${CONTAINER_NAME}" >/dev/null
  else
    log "${CONTAINER_NAME} is not running."
  fi

  if container_exists "${CONTAINER_NAME}"; then
    log "Removing ${CONTAINER_NAME} container..."
    docker_exec rm "${CONTAINER_NAME}" >/dev/null
  else
    log "${CONTAINER_NAME} container does not exist."
  fi

  if docker_exec images -q "${IMAGE_NAME}" 2>/dev/null | grep -q .; then
    log "Removing Uptime Kuma image..."
    docker_exec rmi "${IMAGE_NAME}" >/dev/null || true
  else
    log "Uptime Kuma image does not exist."
  fi

  if [[ -d "${BASE_DATA_DIR}" ]]; then
    log "Removing data directory: ${BASE_DATA_DIR}"
    rm -rf "${BASE_DATA_DIR}"
  else
    log "Data directory does not exist."
  fi

  log "=== Cleanup complete ==="
  exit 0
fi

# ── --off mode ────────────────────────────────────────────────────────────────
if [[ "${ACTION}" == "off" ]]; then
  if container_running "${CONTAINER_NAME}"; then
    log "Stopping ${CONTAINER_NAME}..."
    docker_exec stop "${CONTAINER_NAME}" >/dev/null
    log "Container stopped."
  else
    log "${CONTAINER_NAME} is not running."
  fi
  exit 0
fi

# ── --on checks ───────────────────────────────────────────────────────────────
if [[ "${ACTION}" == "on" ]]; then
  if ! docker_exec images -q "${IMAGE_NAME}" 2>/dev/null | grep -q .; then
    log "Error: Uptime Kuma image is not installed. Run without flags first."
    exit 1
  fi

  if [[ ! -d "${DATA_DIR}" ]]; then
    log "Error: Uptime Kuma data directory is not installed. Run without flags first."
    exit 1
  fi
fi

mkdir -p "${DATA_DIR}"

if [[ "${ACTION}" == "run" ]]; then
  if ! container_exists "${CONTAINER_NAME}" && is_port_in_use "${WEB_PORT}"; then
    log "Error: port ${WEB_PORT} appears to be in use."
    log "Set a different port via UPTIME_KUMA_PORT, e.g. UPTIME_KUMA_PORT=3003 bash $(basename "$0")"
    exit 1
  fi

  log "Pulling latest Uptime Kuma image..."
  docker_exec pull "${IMAGE_NAME}" >/dev/null
fi

if container_running "${CONTAINER_NAME}"; then
  log "${CONTAINER_NAME} is already running at http://localhost:${WEB_PORT}"
  exit 0
fi

if container_exists "${CONTAINER_NAME}"; then
  log "Starting existing ${CONTAINER_NAME} container..."
  docker_exec start "${CONTAINER_NAME}" >/dev/null
else
  log "Creating and starting ${CONTAINER_NAME} container..."
  docker_exec run -d \
    --name "${CONTAINER_NAME}" \
    --restart unless-stopped \
    -p "${WEB_PORT}:3001" \
    -v "${DATA_DIR}:/app/data" \
    "${IMAGE_NAME}" >/dev/null
fi

for _ in {1..90}; do
  if curl -fsS "http://127.0.0.1:${WEB_PORT}" >/dev/null 2>&1; then
    log "Uptime Kuma is ready at: http://localhost:${WEB_PORT}"
    log "Use --off to stop, --on to start existing install, -D to fully remove."
    exit 0
  fi
  sleep 1
done

log "Uptime Kuma container started, but readiness check timed out."
log "Check logs with: sudo docker logs -f ${CONTAINER_NAME}"
exit 0
