#!/usr/bin/env bash

# Prepare the repository for Python-based tooling without installing any
# application-specific dependencies. Package profiles and application code are
# intentionally left for later steps in the rebuild.

set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PYTHON_MIN_MAJOR=3
PYTHON_MIN_MINOR=10
VENV_DIR="${PROJECT_ROOT}/.venv"

log() {
  printf '[Bootstrap] %s\n' "$*"
}

fail() {
  printf '[Bootstrap] ERROR: %s\n' "$*" >&2
  exit 1
}

run_privileged() {
  if [[ "${EUID}" -eq 0 ]]; then
    "$@"
  elif command -v sudo >/dev/null 2>&1; then
    sudo "$@"
  else
    fail "Installing Python requires root privileges, but sudo is not available."
  fi
}

# Keep the version check in Python rather than parsing `python --version` text;
# sys.version_info handles prereleases and vendor-specific version formatting.
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

  return 1
}

install_python() {
  local package_manager

  # Install the distribution packages rather than using pip for Python itself.
  # This keeps the interpreter managed by the operating system and avoids
  # modifying a system Python installation under PEP 668.
  if command -v apt-get >/dev/null 2>&1; then
    package_manager="apt"
    log "Installing Python and virtual-environment support with apt."
    run_privileged env DEBIAN_FRONTEND=noninteractive apt-get update
    run_privileged env DEBIAN_FRONTEND=noninteractive apt-get install -y python3 python3-venv python3-pip
  elif command -v dnf >/dev/null 2>&1; then
    package_manager="dnf"
    log "Installing Python and virtual-environment support with dnf."
    run_privileged dnf install -y python3 python3-pip
  elif command -v pacman >/dev/null 2>&1; then
    package_manager="pacman"
    log "Installing Python and virtual-environment support with pacman."
    run_privileged pacman -Sy --needed --noconfirm python python-pip
  else
    fail "Python 3.10 or newer is required, but no supported package manager was found. Install Python manually and rerun this script."
  fi

  if ! find_supported_python >/dev/null; then
    fail "The ${package_manager} installation completed, but Python 3.10 or newer is still unavailable."
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

  # A project-local venv gives future Python tools an isolated dependency
  # directory and leaves the distribution-managed Python untouched.
  log "Creating virtual environment: ${VENV_DIR}"
  if ! "${python_command}" -m venv "${VENV_DIR}"; then
    if command -v apt-get >/dev/null 2>&1; then
      log "Python virtual-environment support is missing; installing it with apt."
      run_privileged env DEBIAN_FRONTEND=noninteractive apt-get update
      run_privileged env DEBIAN_FRONTEND=noninteractive apt-get install -y python3-venv
      if ! "${python_command}" -m venv "${VENV_DIR}"; then
        fail "Python venv support was installed, but ${VENV_DIR} could not be created."
      fi
    else
      fail "Could not create ${VENV_DIR}. Install your distribution's Python venv package and rerun this script."
    fi
  fi
}

install_project_requirements() {
  local python_command="${VENV_DIR}/bin/python"
  local requirements_file="${PROJECT_ROOT}/requirements.txt"

  if [[ ! -f "${requirements_file}" ]]; then
    return 0
  fi

  log "Installing project Python requirements..."
  "${python_command}" -m pip install --requirement "${requirements_file}"
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
      printf 'Prepare src/, resources/, and a project-local Python virtual environment.\n'
      return 0
      ;;
    *)
      fail "Unknown argument: ${1}. Use --help for usage."
      ;;
  esac

  if [[ "${EUID}" -eq 0 ]]; then
    fail "Run this script as your normal user, without sudo. It uses sudo only for package installation so project files remain user-owned."
  fi

  mkdir -p "${PROJECT_ROOT}/src" "${PROJECT_ROOT}/resources"

  if python_command="$(find_supported_python)"; then
    log "Found supported Python: ${python_command} ($("${python_command}" --version 2>&1))"
  else
    log "Python 3.10 or newer was not found."
    install_python
    python_command="$(find_supported_python)"
    log "Found supported Python: ${python_command} ($("${python_command}" --version 2>&1))"
  fi

  ensure_virtual_environment "${python_command}"
  install_project_requirements

  log "Python bootstrap complete."
  log "Interpreter: ${VENV_DIR}/bin/python"
  log "Next step: place Python application code under src/ and declarative TOML resources under resources/."
}

main "$@"