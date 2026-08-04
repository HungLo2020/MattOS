#!/usr/bin/env bash
# =============================================================================
# RunPortainerContainer.sh
#
# Self-contained script to install and run Portainer in Docker.
#
# Usage:
#   ./RunPortainerContainer.sh          Install (if needed) and start Portainer
#   ./RunPortainerContainer.sh -D       Stop container and delete all files/image
#   ./RunPortainerContainer.sh --off    Stop container without deleting files
#   ./RunPortainerContainer.sh --on     Start container only if already installed
#
# Notes:
#   - Portainer UI is exposed on https://localhost:9443
#   - Portainer edge port is exposed on http://localhost:8000
#   - Persistent data: ~/.portainer/data
# =============================================================================

set -euo pipefail

# ── Configuration ─────────────────────────────────────────────────────────────
CONTAINER_NAME="portainer"
IMAGE_NAME="portainer/portainer-ce:latest"
BASE_DATA_DIR="${HOME}/.portainer"
DATA_DIR="${BASE_DATA_DIR}/data"
UI_PORT=9443
EDGE_PORT=8000

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
    log "=== Shutting down Portainer and removing files ==="

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
        log "Removing Portainer image..."
        docker_exec rmi "${IMAGE_NAME}" >/dev/null || true
    else
        log "Portainer image does not exist."
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
        log "Error: Portainer image is not installed. Run without flags first."
        exit 1
    fi

    if [[ ! -d "${DATA_DIR}" ]]; then
        log "Error: Portainer data directory is not installed. Run without flags first."
        exit 1
    fi
fi

mkdir -p "${DATA_DIR}"

if [[ "${ACTION}" == "run" ]]; then
    log "Pulling latest Portainer image..."
    docker_exec pull "${IMAGE_NAME}" >/dev/null
fi

if container_running "${CONTAINER_NAME}"; then
    log "${CONTAINER_NAME} is already running at https://localhost:${UI_PORT}"
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
        -p "${UI_PORT}:9443" \
        -p "${EDGE_PORT}:8000" \
        -v /var/run/docker.sock:/var/run/docker.sock \
        -v "${DATA_DIR}:/data" \
        "${IMAGE_NAME}" >/dev/null
fi

for _ in {1..60}; do
    if curl -kfsS "https://127.0.0.1:${UI_PORT}" >/dev/null 2>&1; then
        log "Portainer is ready at: https://localhost:${UI_PORT}"
        log "First setup creates the admin account in browser."
        exit 0
    fi
    sleep 1
done

log "Portainer container started, but readiness check timed out."
log "Check logs with: sudo docker logs -f ${CONTAINER_NAME}"
exit 0
