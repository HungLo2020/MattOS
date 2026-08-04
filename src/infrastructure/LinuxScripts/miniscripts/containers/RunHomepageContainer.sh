#!/usr/bin/env bash
# =============================================================================
# RunHomepageContainer.sh
#
# Self-contained script to install and run Homepage dashboard in Docker.
#
# Usage:
#   ./RunHomepageContainer.sh          Install (if needed) and start Homepage
#   ./RunHomepageContainer.sh -D       Stop container and delete all files/image
#   ./RunHomepageContainer.sh --off    Stop container without deleting files
#   ./RunHomepageContainer.sh --on     Start container only if already installed
#
# Notes:
#   - Homepage is exposed on http://localhost:3001
#   - Uses default config folder: ~/.homepage-dashboard/config
#   - On run/--on, localhost service URLs are rewritten to the active Tailscale IP
# =============================================================================

set -euo pipefail

# ── Configuration ─────────────────────────────────────────────────────────────
CONTAINER_NAME="homepage"
IMAGE_NAME="ghcr.io/gethomepage/homepage:latest"
BASE_DATA_DIR="${HOME}/.homepage-dashboard"
CONFIG_DIR="${BASE_DATA_DIR}/config"
ICONS_DIR="${BASE_DATA_DIR}/icons"
PORT=3001
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="${SCRIPT_DIR}"
while [[ "${REPO_ROOT}" != "/" && ! -d "${REPO_ROOT}/resources" ]]; do
    REPO_ROOT="$(dirname "${REPO_ROOT}")"
done

if [[ ! -d "${REPO_ROOT}/resources" ]]; then
    log "Error: could not locate repository resources directory from ${SCRIPT_DIR}"
    exit 1
fi

SETTINGS_TEMPLATE="${REPO_ROOT}/resources/homepage/settings.yaml"
SERVICES_TEMPLATE="${REPO_ROOT}/resources/homepage/services.yaml"
WIDGETS_TEMPLATE="${REPO_ROOT}/resources/homepage/widgets.yaml"
BOOKMARKS_TEMPLATE="${REPO_ROOT}/resources/homepage/bookmarks.yaml"
DOCKER_TEMPLATE="${REPO_ROOT}/resources/homepage/docker.yaml"
TAILSCALE_IP=""
ALLOWED_HOSTS=""

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

build_allowed_hosts() {
    local tailscale_ip="${1:-}"
    local entries=()
    local short_host
    local full_host
    local ip

    entries+=("localhost:${PORT}" "127.0.0.1:${PORT}" "[::1]:${PORT}")

    short_host="$(hostname -s 2>/dev/null || true)"
    full_host="$(hostname -f 2>/dev/null || true)"

    if [[ -n "${short_host}" ]]; then
        entries+=("${short_host}:${PORT}" "${short_host}.local:${PORT}")
    fi

    if [[ -n "${full_host}" ]]; then
        entries+=("${full_host}:${PORT}")
    fi

    while IFS= read -r ip; do
        [[ -n "${ip}" ]] && entries+=("${ip}:${PORT}")
    done < <(hostname -I 2>/dev/null | tr ' ' '\n' | sed '/^$/d')

    if [[ -n "${tailscale_ip}" ]]; then
        entries+=("${tailscale_ip}:${PORT}")
    fi

    printf '%s\n' "${entries[@]}" | awk '!seen[$0]++' | paste -sd, -
}

require_tailscale() {
    if ! command -v tailscale >/dev/null 2>&1; then
        log "Error: tailscale is not installed. Install and connect Tailscale first."
        exit 1
    fi

    if ! tailscale status >/dev/null 2>&1; then
        log "Error: tailscale is not running or not connected."
        exit 1
    fi

    TAILSCALE_IP="$(tailscale ip -4 2>/dev/null | head -n1 || true)"
    if [[ -z "${TAILSCALE_IP}" ]]; then
        log "Error: could not determine Tailscale IPv4 address."
        exit 1
    fi

    ALLOWED_HOSTS="$(build_allowed_hosts "${TAILSCALE_IP}")"
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
    log "=== Shutting down Homepage and removing files ==="

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
        log "Removing Homepage image..."
        docker_exec rmi "${IMAGE_NAME}" >/dev/null || true
    else
        log "Homepage image does not exist."
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

if [[ "${ACTION}" == "run" || "${ACTION}" == "on" ]]; then
    require_tailscale
fi

# ── --on checks ───────────────────────────────────────────────────────────────
if [[ "${ACTION}" == "on" ]]; then
    if ! docker_exec images -q "${IMAGE_NAME}" 2>/dev/null | grep -q .; then
        log "Error: Homepage image is not installed. Run without flags first."
        exit 1
    fi

    if [[ ! -f "${CONFIG_DIR}/settings.yaml" ]]; then
        log "Error: Homepage config is not installed. Run without flags first."
        exit 1
    fi
fi

# ── Ensure folders and config ─────────────────────────────────────────────────
mkdir -p "${CONFIG_DIR}" "${ICONS_DIR}"

if [[ ! -f "${SETTINGS_TEMPLATE}" ]]; then
    log "Error: settings template not found at ${SETTINGS_TEMPLATE}"
    exit 1
fi

if [[ ! -f "${SERVICES_TEMPLATE}" ]]; then
    log "Error: services template not found at ${SERVICES_TEMPLATE}"
    exit 1
fi

if [[ ! -f "${WIDGETS_TEMPLATE}" ]]; then
    log "Error: widgets template not found at ${WIDGETS_TEMPLATE}"
    exit 1
fi

if [[ ! -f "${BOOKMARKS_TEMPLATE}" ]]; then
    log "Error: bookmarks template not found at ${BOOKMARKS_TEMPLATE}"
    exit 1
fi

if [[ ! -f "${DOCKER_TEMPLATE}" ]]; then
    log "Error: docker template not found at ${DOCKER_TEMPLATE}"
    exit 1
fi

cp -f "${SETTINGS_TEMPLATE}" "${CONFIG_DIR}/settings.yaml"
cp -f "${SERVICES_TEMPLATE}" "${CONFIG_DIR}/services.yaml"
cp -f "${WIDGETS_TEMPLATE}" "${CONFIG_DIR}/widgets.yaml"
cp -f "${BOOKMARKS_TEMPLATE}" "${CONFIG_DIR}/bookmarks.yaml"
cp -f "${DOCKER_TEMPLATE}" "${CONFIG_DIR}/docker.yaml"

if [[ -n "${TAILSCALE_IP}" ]]; then
    for cfg in "${CONFIG_DIR}/services.yaml" "${CONFIG_DIR}/widgets.yaml" "${CONFIG_DIR}/bookmarks.yaml"; do
        [[ -f "${cfg}" ]] || continue
        sed -E -i "s#(https?://)(localhost|127\\.0\\.0\\.1|\\[::1\\])#\\1${TAILSCALE_IP}#g" "${cfg}"
    done
fi

# ── Pull image on normal install/run ─────────────────────────────────────────
if [[ "${ACTION}" == "run" ]]; then
    log "Pulling latest Homepage image..."
    docker_exec pull "${IMAGE_NAME}" >/dev/null
fi

# ── Start container ───────────────────────────────────────────────────────────
if container_exists "${CONTAINER_NAME}"; then
    current_allowed_hosts="$(docker_exec inspect --format '{{range .Config.Env}}{{println .}}{{end}}' "${CONTAINER_NAME}" | grep '^HOMEPAGE_ALLOWED_HOSTS=' || true)"
    expected_allowed_hosts="HOMEPAGE_ALLOWED_HOSTS=${ALLOWED_HOSTS}"

    if [[ "${current_allowed_hosts}" != "${expected_allowed_hosts}" ]]; then
        if container_running "${CONTAINER_NAME}"; then
            log "Stopping ${CONTAINER_NAME} to apply updated host validation settings..."
            docker_exec stop "${CONTAINER_NAME}" >/dev/null
        fi
        log "Recreating ${CONTAINER_NAME} to apply HOMEPAGE_ALLOWED_HOSTS..."
        docker_exec rm "${CONTAINER_NAME}" >/dev/null
    fi
fi

if container_running "${CONTAINER_NAME}"; then
    log "${CONTAINER_NAME} is already running at http://localhost:${PORT}"
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
        -p "${PORT}:3000" \
        -e "HOMEPAGE_ALLOWED_HOSTS=${ALLOWED_HOSTS}" \
        -v "${CONFIG_DIR}:/app/config" \
        -v "${ICONS_DIR}:/app/public/icons" \
        -v /var/run/docker.sock:/var/run/docker.sock:ro \
        "${IMAGE_NAME}" >/dev/null
fi

# ── Readiness check ───────────────────────────────────────────────────────────
for _ in {1..60}; do
    if curl -fsS "http://127.0.0.1:${PORT}" >/dev/null 2>&1; then
        log "Homepage is ready at: http://localhost:${PORT}"
        exit 0
    fi
    sleep 1
done

log "Homepage container started, but readiness check timed out."
log "Check logs with: sudo docker logs -f ${CONTAINER_NAME}"
exit 0
