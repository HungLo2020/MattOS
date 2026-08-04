#!/usr/bin/env bash
# =============================================================================
# RunOllamaContainer.sh
#
# Self-contained script to install and run Ollama + Open WebUI via Docker.
#
# Usage:
#   ./RunOllamaContainer.sh          Install (if needed) and start services
#   ./RunOllamaContainer.sh -D       Stop containers and delete all files/images
#   ./RunOllamaContainer.sh --off    Stop containers without deleting files
#   ./RunOllamaContainer.sh --on     Start containers only if already installed
#
# Notes:
#   - Default model is dolphin-mistral:7b (uncensored-oriented, practical for 8GB VRAM)
#   - Open WebUI is exposed on http://localhost:3000
#   - Ollama API is exposed on http://localhost:11434
# =============================================================================

set -euo pipefail

# ── Configuration ─────────────────────────────────────────────────────────────
OLLAMA_CONTAINER_NAME="ollama"
WEBUI_CONTAINER_NAME="open-webui"
OLLAMA_IMAGE="ollama/ollama"
WEBUI_IMAGE="ghcr.io/open-webui/open-webui:main"
DOCKER_NETWORK="ai-stack"
MODEL_NAME="dolphin-mistral:7b"
BASE_DATA_DIR="${HOME}/.ollama-stack"
OLLAMA_DATA_DIR="${BASE_DATA_DIR}/ollama"
WEBUI_DATA_DIR="${BASE_DATA_DIR}/open-webui"
OLLAMA_PORT=11434
WEBUI_PORT=3000

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
    log "=== Shutting down Ollama + Open WebUI and removing files ==="

    if container_running "${WEBUI_CONTAINER_NAME}"; then
        log "Stopping ${WEBUI_CONTAINER_NAME}..."
        docker_exec stop "${WEBUI_CONTAINER_NAME}" >/dev/null
    fi

    if container_running "${OLLAMA_CONTAINER_NAME}"; then
        log "Stopping ${OLLAMA_CONTAINER_NAME}..."
        docker_exec stop "${OLLAMA_CONTAINER_NAME}" >/dev/null
    fi

    if container_exists "${WEBUI_CONTAINER_NAME}"; then
        log "Removing ${WEBUI_CONTAINER_NAME} container..."
        docker_exec rm "${WEBUI_CONTAINER_NAME}" >/dev/null
    fi

    if container_exists "${OLLAMA_CONTAINER_NAME}"; then
        log "Removing ${OLLAMA_CONTAINER_NAME} container..."
        docker_exec rm "${OLLAMA_CONTAINER_NAME}" >/dev/null
    fi

    if docker_exec images -q "${WEBUI_IMAGE}" 2>/dev/null | grep -q .; then
        log "Removing Open WebUI image..."
        docker_exec rmi "${WEBUI_IMAGE}" >/dev/null || true
    fi

    if docker_exec images -q "${OLLAMA_IMAGE}" 2>/dev/null | grep -q .; then
        log "Removing Ollama image..."
        docker_exec rmi "${OLLAMA_IMAGE}" >/dev/null || true
    fi

    if docker_exec network ls --format '{{.Name}}' | grep -qx "${DOCKER_NETWORK}"; then
        log "Removing Docker network ${DOCKER_NETWORK}..."
        docker_exec network rm "${DOCKER_NETWORK}" >/dev/null || true
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
    if container_running "${WEBUI_CONTAINER_NAME}"; then
        log "Stopping ${WEBUI_CONTAINER_NAME}..."
        docker_exec stop "${WEBUI_CONTAINER_NAME}" >/dev/null
    else
        log "${WEBUI_CONTAINER_NAME} is not running."
    fi

    if container_running "${OLLAMA_CONTAINER_NAME}"; then
        log "Stopping ${OLLAMA_CONTAINER_NAME}..."
        docker_exec stop "${OLLAMA_CONTAINER_NAME}" >/dev/null
    else
        log "${OLLAMA_CONTAINER_NAME} is not running."
    fi

    log "Services stopped."
    exit 0
fi

# ── Install/on prechecks ─────────────────────────────────────────────────────
if [[ "${ACTION}" == "on" ]]; then
    if ! docker_exec images -q "${OLLAMA_IMAGE}" 2>/dev/null | grep -q .; then
        log "Error: Ollama image is not installed. Run without flags first."
        exit 1
    fi
    if ! docker_exec images -q "${WEBUI_IMAGE}" 2>/dev/null | grep -q .; then
        log "Error: Open WebUI image is not installed. Run without flags first."
        exit 1
    fi
    if [[ ! -d "${OLLAMA_DATA_DIR}/models" ]]; then
        log "Error: Ollama model data is not installed. Run without flags first."
        exit 1
    fi
fi

mkdir -p "${OLLAMA_DATA_DIR}" "${WEBUI_DATA_DIR}"

if ! docker_exec network ls --format '{{.Name}}' | grep -qx "${DOCKER_NETWORK}"; then
    log "Creating Docker network: ${DOCKER_NETWORK}"
    docker_exec network create "${DOCKER_NETWORK}" >/dev/null
fi

GPU_ARGS=()
if command -v nvidia-smi &>/dev/null && nvidia-smi &>/dev/null 2>&1; then
    if docker_exec info 2>/dev/null | grep -qi "nvidia\|gpu runtime"; then
        GPU_ARGS=(--gpus all)
        log "NVIDIA GPU detected — enabling GPU passthrough for Ollama."
    else
        log "NVIDIA GPU detected but Docker NVIDIA runtime not configured; using CPU mode."
    fi
fi

# ── Ensure Ollama container exists/runs ──────────────────────────────────────
if container_running "${OLLAMA_CONTAINER_NAME}"; then
    log "${OLLAMA_CONTAINER_NAME} is already running."
elif container_exists "${OLLAMA_CONTAINER_NAME}"; then
    log "Starting existing ${OLLAMA_CONTAINER_NAME} container..."
    docker_exec start "${OLLAMA_CONTAINER_NAME}" >/dev/null
else
    log "Creating and starting ${OLLAMA_CONTAINER_NAME} container..."
    docker_exec run -d \
        --name "${OLLAMA_CONTAINER_NAME}" \
        --restart unless-stopped \
        --network "${DOCKER_NETWORK}" \
        -p "${OLLAMA_PORT}:11434" \
        -v "${OLLAMA_DATA_DIR}:/root/.ollama" \
        "${GPU_ARGS[@]}" \
        "${OLLAMA_IMAGE}" >/dev/null
fi

# Wait for Ollama API
log "Waiting for Ollama API..."
for _ in {1..90}; do
    if curl -fsS "http://127.0.0.1:${OLLAMA_PORT}/api/tags" >/dev/null 2>&1; then
        break
    fi
    sleep 1
done

if ! curl -fsS "http://127.0.0.1:${OLLAMA_PORT}/api/tags" >/dev/null 2>&1; then
    log "Error: Ollama API did not become ready."
    exit 1
fi

# ── Ensure model present ──────────────────────────────────────────────────────
if docker_exec exec "${OLLAMA_CONTAINER_NAME}" ollama list | awk 'NR>1 {print $1}' | grep -qx "${MODEL_NAME}"; then
    log "Model already installed: ${MODEL_NAME}"
else
    if [[ "${ACTION}" == "on" ]]; then
        log "Error: required model not installed (${MODEL_NAME}). Run without flags first."
        exit 1
    fi

    log "Pulling model: ${MODEL_NAME}"
    docker_exec exec "${OLLAMA_CONTAINER_NAME}" ollama pull "${MODEL_NAME}"
fi

# ── Ensure Open WebUI container exists/runs ──────────────────────────────────
if container_running "${WEBUI_CONTAINER_NAME}"; then
    log "${WEBUI_CONTAINER_NAME} is already running."
elif container_exists "${WEBUI_CONTAINER_NAME}"; then
    log "Starting existing ${WEBUI_CONTAINER_NAME} container..."
    docker_exec start "${WEBUI_CONTAINER_NAME}" >/dev/null
else
    log "Creating and starting ${WEBUI_CONTAINER_NAME} container..."
    docker_exec run -d \
        --name "${WEBUI_CONTAINER_NAME}" \
        --restart unless-stopped \
        --network "${DOCKER_NETWORK}" \
        -p "${WEBUI_PORT}:8080" \
        -e OLLAMA_BASE_URL="http://ollama:11434" \
        -e OLLAMA_API_BASE_URL="http://ollama:11434" \
        -v "${WEBUI_DATA_DIR}:/app/backend/data" \
        "${WEBUI_IMAGE}" >/dev/null
fi

log "=== Services ready ==="
log "Open WebUI: http://localhost:${WEBUI_PORT}"
log "Ollama API: http://localhost:${OLLAMA_PORT}"
log "Model: ${MODEL_NAME}"
log "Use --off to stop, --on to start existing install, -D to fully remove."
