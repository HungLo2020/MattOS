#!/usr/bin/env bash

# Bootstrap the project-local Python environment on macOS. Homebrew is used
# only when Python 3.10 or newer is not already available.
set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PYTHON_MIN_MAJOR=3
PYTHON_MIN_MINOR=10
VENV_DIR="${PROJECT_ROOT}/.venv"

log() {
  printf '[MacBootstrap] %s\n' "$*"
}

fail() {
  printf '[MacBootstrap] ERROR: %s\n' "$*" >&2
  exit 1
}

python_is_supported() {
  local python_command="$1"
  "${python_command}" -c \
    'import sys; raise SystemExit(0 if sys.version_info >= (int(sys.argv[1]), int(sys.argv[2])) else 1)' \
    "${PYTHON_MIN_MAJOR}" "${PYTHON_MIN_MINOR}" >/dev/null 2>&1
}

find_supported_python() {
  local candidate

  for candidate in python3 python; do
    if command -v "${candidate}" >/dev/null 2>&1 && python_is_supported "${candidate}"; then
      command -v "${candidate}"
      return 0
    fi
  done

  if command -v brew >/dev/null 2>&1; then
    candidate="$(brew --prefix python 2>/dev/null || true)/bin/python3"
    if [[ -x "${candidate}" ]] && python_is_supported "${candidate}"; then
      printf '%s\n' "${candidate}"
      return 0
    fi
  fi

  return 1
}

install_python() {
  if ! command -v brew >/dev/null 2>&1; then
    fail "Python 3.10+ is required. Install Homebrew from https://brew.sh, reopen Terminal, then rerun this script."
  fi

  log "Installing Python with Homebrew."
  brew install python
  if ! find_supported_python >/dev/null; then
    fail "Homebrew installed Python, but Python 3.10+ is still unavailable. Run 'brew doctor', reopen Terminal, and rerun this script."
  fi
}

ensure_virtual_environment() {
  local python_command="$1"

  if [[ -x "${VENV_DIR}/bin/python" ]]; then
    if python_is_supported "${VENV_DIR}/bin/python"; then
      log "Using existing virtual environment: ${VENV_DIR}"
      return 0
    fi
    fail "Existing virtual environment uses an unsupported Python version: ${VENV_DIR}"
  fi

  log "Creating virtual environment: ${VENV_DIR}"
  "${python_command}" -m venv "${VENV_DIR}"
}

main() {
  local python_command

  if [[ "$#" -gt 1 ]]; then
    fail "Expected no arguments or one of --help/-h."
  fi

  case "${1:-}" in
    "") ;;
    --help|-h)
      printf 'Usage: %s\n\n' "$(basename "$0")"
      printf 'Prepare a project-local Python virtual environment on macOS.\n'
      return 0
      ;;
    *) fail "Unknown argument: ${1}. Use --help for usage." ;;
  esac

  if [[ "$(uname -s)" != "Darwin" ]]; then
    fail "MacBootstrap.sh must be run on macOS."
  fi

  if python_command="$(find_supported_python)"; then
    log "Found supported Python: ${python_command} ($("${python_command}" --version 2>&1))"
  else
    install_python
    python_command="$(find_supported_python)"
    log "Found supported Python: ${python_command} ($("${python_command}" --version 2>&1))"
  fi

  ensure_virtual_environment "${python_command}"
  if [[ -f "${PROJECT_ROOT}/requirements.txt" ]]; then
    log "Installing project Python requirements."
    "${VENV_DIR}/bin/python" -m pip install --requirement "${PROJECT_ROOT}/requirements.txt"
  fi

  log "Python bootstrap complete."
  log "Interpreter: ${VENV_DIR}/bin/python"
}

main "$@"