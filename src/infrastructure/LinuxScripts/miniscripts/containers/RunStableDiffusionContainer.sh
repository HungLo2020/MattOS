#!/usr/bin/env bash
# =============================================================================
# RunStableDiffusionContainer.sh
#
# Self-contained script to install and run AUTOMATIC1111 Stable Diffusion
# WebUI as a Docker container with NO content filter.
#
# Usage:
#   ./RunStableDiffusionContainer.sh          Install (if needed) and start the UI
#   ./RunStableDiffusionContainer.sh -D       Stop container and delete all files
#   ./RunStableDiffusionContainer.sh --off    Stop container without deleting files
#   ./RunStableDiffusionContainer.sh --on     Start container only if already installed
#
# What this script does:
#   - Installs Docker if it is not already present
#   - Downloads DreamShaper 8 (public model, no login required)
#   - Builds a Docker image containing AUTOMATIC1111 from the official source
#   - Runs the container with the safety / NSFW checker disabled
#   - Passes GPU through to the container when an NVIDIA GPU is detected
#   - Caches the Python venv on the host so subsequent starts are fast
#   - Is idempotent: safe to run any number of times
#
# SECURITY NOTICE:
#   --disable-safe-unpickle  Disables pickle safety checks so that all model
#                            formats can be loaded.  Only load model files from
#                            sources you trust.
#   --allow-code             Permits arbitrary Python execution inside the
#                            prompt pipeline.  Do NOT expose port 7861 to an
#                            untrusted network when this flag is active.
#   --enable-insecure-extension-access
#                            Allows extensions broader access. Keep the WebUI
#                            on trusted networks only when this flag is active.
# =============================================================================

set -euo pipefail

# ── Configuration ─────────────────────────────────────────────────────────────
CONTAINER_NAME="automatic1111"
IMAGE_NAME="automatic1111-webui"
# Increment IMAGE_VERSION whenever the Dockerfile changes so stale cached
# images are automatically detected and rebuilt.
IMAGE_VERSION="3"
DATA_DIR="${HOME}/.automatic1111"
PORT=7861
SD_REPO_MIRROR="https://github.com/Jonel865/stable-diffusion-stability-ai.git"
SD_REPO_COMMIT="cf1d67a6fd5ea1aa600c4df58e5b47da45f6bdbf"

# DreamShaper 8 — publicly accessible on Hugging Face, no account required.
MODEL_FILENAME="Dreamshaper_8.safetensors"
MODEL_URL_PRIMARY="https://huggingface.co/Lykon/dreamshaper-8/resolve/main/${MODEL_FILENAME}"
# Mirror: digiplay hosts the same checkpoint under a lowercase filename
MODEL_URL_FALLBACK="https://huggingface.co/digiplay/DreamShaper_8/resolve/main/dreamshaper_8.safetensors"

# ── Helpers ───────────────────────────────────────────────────────────────────
log() { echo "[$(date '+%Y-%m-%d %H:%M:%S')] $*"; }

# Wrapper so we can transparently switch to "sudo docker" when the current
# user is not yet in the docker group (e.g. right after installation).
DOCKER_USE_SUDO="false"
docker_exec() {
    if [[ "${DOCKER_USE_SUDO}" == "true" ]]; then
        sudo docker "$@"
    else
        docker "$@"
    fi
}

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

# ── Install Docker if not present ─────────────────────────────────────────────
if [[ "${ACTION}" == "run" ]] && ! command -v docker &>/dev/null; then
    log "Docker not found. Installing via the official get.docker.com script..."
    curl -fsSL https://get.docker.com | sudo sh
    sudo usermod -aG docker "${USER}" || true
    log "Docker installed."
fi

if ! command -v docker &>/dev/null; then
    if [[ "${ACTION}" == "delete" ]]; then
        log "Warning: Docker not found; skipping container/image removal."
        if [[ -d "${DATA_DIR}" ]]; then
            log "Removing data directory: ${DATA_DIR}"
            rm -rf "${DATA_DIR}"
        else
            log "Data directory does not exist."
        fi
        log "=== Cleanup complete ==="
        exit 0
    fi

    log "Error: Docker is not installed."
    exit 1
fi

# Resolve whether we need sudo to run docker commands.
if ! docker info &>/dev/null; then
    if sudo docker info &>/dev/null; then
        DOCKER_USE_SUDO="true"
        log "Using 'sudo docker' for this session (user not yet in docker group)."
    else
        log "Error: cannot connect to the Docker daemon. Is Docker running?"
        exit 1
    fi
fi

if [[ "${ACTION}" == "delete" ]]; then
    log "=== Shutting down AUTOMATIC1111 and removing its files ==="

    if docker_exec ps -q --filter "name=^/${CONTAINER_NAME}$" 2>/dev/null | grep -q .; then
        log "Stopping container..."
        docker_exec stop "${CONTAINER_NAME}"
    else
        log "Container is not running."
    fi

    if docker_exec ps -aq --filter "name=^/${CONTAINER_NAME}$" 2>/dev/null | grep -q .; then
        log "Removing container..."
        docker_exec rm "${CONTAINER_NAME}"
    else
        log "Container does not exist."
    fi

    if docker_exec images -q "${IMAGE_NAME}" 2>/dev/null | grep -q .; then
        log "Removing Docker image..."
        docker_exec rmi "${IMAGE_NAME}"
    else
        log "Docker image does not exist."
    fi

    if [[ -d "${DATA_DIR}" ]]; then
        log "Removing data directory: ${DATA_DIR}"
        rm -rf "${DATA_DIR}"
    else
        log "Data directory does not exist."
    fi

    log "=== Cleanup complete ==="
    exit 0
fi

if [[ "${ACTION}" == "off" ]]; then
    if docker_exec ps -q --filter "name=^/${CONTAINER_NAME}$" 2>/dev/null | grep -q .; then
        log "Stopping container..."
        docker_exec stop "${CONTAINER_NAME}" >/dev/null
        log "Container stopped."
    else
        log "Container is not running."
    fi
    exit 0
fi

if [[ "${ACTION}" == "on" ]]; then
    if ! docker_exec images -q "${IMAGE_NAME}" 2>/dev/null | grep -q .; then
        log "Error: AUTOMATIC1111 is not installed (image not found). Run without flags first."
        exit 1
    fi

    shopt -s nullglob
    installed_models=(
        "${DATA_DIR}/models/Stable-diffusion"/*.safetensors
        "${DATA_DIR}/models/Stable-diffusion"/*.ckpt
    )
    shopt -u nullglob

    if [[ "${#installed_models[@]}" -eq 0 ]]; then
        log "Error: no installed model found in ${DATA_DIR}/models/Stable-diffusion."
        exit 1
    fi

    if docker_exec ps -q --filter "name=^/${CONTAINER_NAME}$" 2>/dev/null | grep -q .; then
        log "Container is already running."
        exit 0
    fi
fi

# ── Create persistent data directories ───────────────────────────────────────
if [[ "${ACTION}" == "run" ]]; then
    mkdir -p \
        "${DATA_DIR}/models/Stable-diffusion" \
        "${DATA_DIR}/outputs" \
        "${DATA_DIR}/extensions"
fi

# ── Download model (no login required) ───────────────────────────────────────
MODEL_PATH="${DATA_DIR}/models/Stable-diffusion/${MODEL_FILENAME}"

if [[ "${ACTION}" == "run" && ! -f "${MODEL_PATH}" ]]; then
    log "Downloading model: ${MODEL_FILENAME}"
    log "This will take several minutes depending on your connection (file is ~2 GB)."
    PARTIAL="${MODEL_PATH}.partial"

    download_model() {
        local url="$1"
        log "Trying URL: ${url}"
        if command -v wget &>/dev/null; then
            wget -c --progress=bar:force:noscroll -O "${PARTIAL}" "${url}" && return 0
        else
            curl -L --progress-bar -C - -o "${PARTIAL}" "${url}" && return 0
        fi
        return 1
    }

    if ! download_model "${MODEL_URL_PRIMARY}"; then
        log "Primary URL failed; trying fallback mirror..."
        MODEL_FILENAME="dreamshaper_8.safetensors"
        MODEL_PATH="${DATA_DIR}/models/Stable-diffusion/${MODEL_FILENAME}"
        PARTIAL="${MODEL_PATH}.partial"
        if ! download_model "${MODEL_URL_FALLBACK}"; then
            log "Error: could not download the model from any source."
            rm -f "${PARTIAL}"
            exit 1
        fi
        log "Fallback download succeeded; model saved as: ${MODEL_FILENAME}"
    fi

    mv "${PARTIAL}" "${MODEL_PATH}"
    log "Model downloaded: ${MODEL_FILENAME}"
elif [[ "${ACTION}" == "run" ]]; then
    log "Model already present: ${MODEL_FILENAME}"
fi

# ── Build Docker image (rebuild when Dockerfile version changes) ──────────────
NEEDS_BUILD="false"
if [[ "${ACTION}" == "run" ]]; then
    if ! docker_exec images -q "${IMAGE_NAME}" 2>/dev/null | grep -q .; then
        NEEDS_BUILD="true"
    else
        existing_version=$(docker_exec inspect --format '{{index .Config.Labels "version"}}' \
            "${IMAGE_NAME}" 2>/dev/null || echo "")
        if [[ "${existing_version}" != "${IMAGE_VERSION}" ]]; then
            log "Docker image is outdated (version ${existing_version:-unknown} → ${IMAGE_VERSION}); rebuilding..."
            docker_exec rmi "${IMAGE_NAME}" 2>/dev/null || true
            NEEDS_BUILD="true"
        else
            log "Docker image already exists and is up to date: ${IMAGE_NAME}"
        fi
    fi
fi

if [[ "${ACTION}" == "run" && "${NEEDS_BUILD}" == "true" ]]; then
    log "Building Docker image — this is a one-time step that may take 10-20 minutes."

    BUILD_CTX=$(mktemp -d)
    # shellcheck disable=SC2064
    trap 'rm -rf "${BUILD_CTX}"' EXIT

    cat > "${BUILD_CTX}/Dockerfile" << 'DOCKERFILE'
FROM python:3.10-slim-bookworm

LABEL version="3"

ENV DEBIAN_FRONTEND=noninteractive \
    PYTHONDONTWRITEBYTECODE=1 \
    PYTHONUNBUFFERED=1

RUN set -eux; \
    for i in 1 2 3 4 5; do \
        apt-get update -o Acquire::Retries=5 -o Acquire::http::Timeout=30 -o Acquire::https::Timeout=30 && \
        apt-cache show git >/dev/null 2>&1 && \
        apt-cache show curl >/dev/null 2>&1 && break; \
        echo "apt index fetch incomplete (attempt ${i}/5), retrying..."; \
        sleep 5; \
    done; \
    apt-cache show git >/dev/null 2>&1; \
    apt-cache show curl >/dev/null 2>&1; \
    apt-get install -y --no-install-recommends --fix-missing \
        git \
        wget \
        curl \
        libgl1 \
        libglib2.0-0 \
        libsm6 \
        libxrender1 \
        libxext6 \
        libgomp1 \
        ffmpeg; \
    rm -rf /var/lib/apt/lists/*

RUN git clone --depth=1 https://github.com/AUTOMATIC1111/stable-diffusion-webui /app

# Create a non-root user so webui.sh's root check passes.
# webui.sh refuses to start when run as UID 0.
RUN groupadd -g 1000 webui \
    && useradd -m -u 1000 -g webui -s /bin/bash webui \
    && chown -R webui:webui /app

WORKDIR /app

RUN mkdir -p \
    /app/models/Stable-diffusion \
    /app/models/VAE \
    /app/outputs \
    /app/extensions \
    /app/venv \
    && chown -R webui:webui \
        /app/models \
        /app/outputs \
        /app/extensions \
        /app/venv

USER webui
ENV HOME=/home/webui

EXPOSE 7861
DOCKERFILE

    docker_exec build --network=host -t "${IMAGE_NAME}" "${BUILD_CTX}"
    log "Docker image built successfully."
fi

# ── GPU detection ─────────────────────────────────────────────────────────────
USE_GPU="false"
if command -v nvidia-smi &>/dev/null && nvidia-smi &>/dev/null 2>&1; then
    if docker_exec info 2>/dev/null | grep -qi "nvidia\|gpu runtime"; then
        USE_GPU="true"
        log "NVIDIA GPU detected — enabling GPU passthrough."
    else
        log "NVIDIA GPU found but nvidia-container-toolkit is not configured; running on CPU."
    fi
fi

# ── Build the argument list for AUTOMATIC1111 ─────────────────────────────────
# --listen              bind to 0.0.0.0 so the host can reach the UI
# --port 7860           explicit port
# --disable-safe-unpickle   skip the pickle safety check (allows all models)
# --no-download-sd-model    do not auto-download the default SD model (we provide ours)
# --api                 enable the REST API
# --allow-code          allow arbitrary Python in the prompt processing pipeline
# --enable-insecure-extension-access  allow insecure extension access
#
# CPU-only flags (omitted when a GPU is present):
# --skip-torch-cuda-test   skip the CUDA smoke test on startup
# --no-half                run in FP32 (required on CPU; FP16 is GPU-only)
# --precision full         equivalent to --no-half

WEBUI_ARGS="--listen --port 7860 --disable-safe-unpickle --no-download-sd-model --api --allow-code --enable-insecure-extension-access"
if [[ "${USE_GPU}" == "false" ]]; then
    WEBUI_ARGS="${WEBUI_ARGS} --skip-torch-cuda-test --no-half --precision full"
fi

# ── Build docker run arguments ────────────────────────────────────────────────
# Run as the calling user's UID:GID so that files written to the mounted
# volumes (models, outputs, venv) are owned by the host user, not by UID 1000.
# This also satisfies webui.sh's check that the process is not running as root.
HOST_UID=$(id -u)
HOST_GID=$(id -g)

if [[ "${HOST_UID}" == "0" ]]; then
    log "Error: do not run this script as root. Log in as a regular user and re-run."
    exit 1
fi

# Ensure the mounted venv cache is valid. webui.sh only creates the venv when
# the directory is absent; an empty/broken directory causes activation failure.
if [[ -d "${DATA_DIR}/venv" && ! -f "${DATA_DIR}/venv/bin/activate" ]]; then
    log "Detected invalid cached venv; removing it so it can be recreated."
    rm -rf "${DATA_DIR}/venv"
fi

if [[ ! -f "${DATA_DIR}/venv/bin/activate" ]]; then
    log "Bootstrapping Python venv cache..."
    mkdir -p "${DATA_DIR}/venv"
    docker_exec run --rm \
        --user "${HOST_UID}:${HOST_GID}" \
        -v "${DATA_DIR}/venv:/app/venv" \
        "${IMAGE_NAME}" \
        python -m venv /app/venv
fi

# Use /tmp as HOME so that any UID works without needing /home/<user> inside
# the container (the Dockerfile only created /home/webui for UID 1000).
DOCKER_RUN_ARGS=(
    --name "${CONTAINER_NAME}"
    --user "${HOST_UID}:${HOST_GID}"
    -p "${PORT}:7860"
    -v "${DATA_DIR}/models/Stable-diffusion:/app/models/Stable-diffusion"
    -v "${DATA_DIR}/outputs:/app/outputs"
    -v "${DATA_DIR}/venv:/app/venv"
    -v "${DATA_DIR}/extensions:/app/extensions"
    -e "STABLE_DIFFUSION_REPO=${SD_REPO_MIRROR}"
    -e "STABLE_DIFFUSION_COMMIT_HASH=${SD_REPO_COMMIT}"
    -e "COMMANDLINE_ARGS=${WEBUI_ARGS}"
    -e HOME=/tmp
)

if [[ "${USE_GPU}" == "true" ]]; then
    DOCKER_RUN_ARGS+=(--gpus all)
fi

# ── Launch ────────────────────────────────────────────────────────────────────
if [[ "${ACTION}" == "on" ]]; then
    if docker_exec ps -aq --filter "name=^/${CONTAINER_NAME}$" 2>/dev/null | grep -q .; then
        log "Starting existing container..."
        docker_exec start "${CONTAINER_NAME}" >/dev/null
    else
        log "No existing container found; creating it from installed image..."
        docker_exec run -d "${DOCKER_RUN_ARGS[@]}" "${IMAGE_NAME}" bash webui.sh >/dev/null
    fi

    for _ in {1..60}; do
        if curl -fsS "http://127.0.0.1:${PORT}/" >/dev/null 2>&1; then
            log "Container started at: http://localhost:${PORT}"
            exit 0
        fi
        sleep 1
    done

    log "Container started, but readiness check timed out. Check logs with: sudo docker logs -f ${CONTAINER_NAME}"
    exit 0
fi

if docker_exec ps -q --filter "name=^/${CONTAINER_NAME}$" 2>/dev/null | grep -q .; then
    log "Container is already running."
    exit 0
fi

if docker_exec ps -aq --filter "name=^/${CONTAINER_NAME}$" 2>/dev/null | grep -q .; then
    log "Removing old stopped container before fresh run..."
    docker_exec rm "${CONTAINER_NAME}" >/dev/null
fi

log "=== Starting AUTOMATIC1111 Stable Diffusion WebUI (NO filter) ==="
log "  Web UI : http://localhost:${PORT}"
log "  API    : http://localhost:${PORT}/docs"
log ""
log "Note: on the very first start the container installs Python packages"
log "into the mounted venv — this can take 10-30 min. Subsequent starts"
log "reuse the cached venv and are much faster."
log ""
log "To stop  : press Ctrl+C"
log "To remove: $(basename "$0") -D"
log ""
log "After 'Model loaded...' there may be little/no new log output; WebUI is usually idle and waiting for requests."

(
    for _ in {1..180}; do
        if curl -fsS "http://127.0.0.1:${PORT}/" >/dev/null 2>&1; then
            log "WebUI is ready at: http://localhost:${PORT}"
            exit 0
        fi
        sleep 1
    done
    log "Warning: WebUI readiness check timed out after 180s."
) &
READINESS_PID=$!

docker_exec run "${DOCKER_RUN_ARGS[@]}" "${IMAGE_NAME}" bash webui.sh
wait $READINESS_PID 2>/dev/null || true