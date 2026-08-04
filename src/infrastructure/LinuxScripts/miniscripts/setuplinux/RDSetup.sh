#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
INSTALL_TAILSCALE_SCRIPT="$SCRIPT_DIR/../notautorun/RDSetup-Headless.sh"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
BW_MASTER_PASSWORD_FILE="$PROJECT_ROOT/.bw_master_password"

TARGET_USER="${SUDO_USER:-$(id -un)}"
TARGET_HOME="$(getent passwd "$TARGET_USER" | cut -d: -f6)"

# Bitwarden item name that holds the RustDesk permanent password.
# Override by setting this env var before running the script.
BITWARDEN_RUSTDESK_ITEM="${BITWARDEN_RUSTDESK_ITEM:-PCPassword}"

if [[ -z "$TARGET_HOME" || ! -d "$TARGET_HOME" ]]; then
  echo "Error: Could not determine home directory for user '$TARGET_USER'."
  exit 1
fi

# ── Helpers ───────────────────────────────────────────────────────────────────

run_as_target_user() {
  if [[ "$(id -un)" == "$TARGET_USER" ]]; then
    "$@"
  else
    sudo -H -u "$TARGET_USER" "$@"
  fi
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

# Sets RUSTDESK_PERMANENT_PASSWORD from the Bitwarden vault.
# Returns 0 on success, 1 if Bitwarden is unavailable or the item is missing.
try_bitwarden_rustdesk_password() {
  if ! command -v bw >/dev/null 2>&1; then
    echo "Bitwarden CLI (bw) not found; falling back to manual password entry."
    return 1
  fi

  echo "Attempting Bitwarden lookup for RustDesk password (item: ${BITWARDEN_RUSTDESK_ITEM})..."
  local status
  local session

  status="$(bitwarden_status)"

  if [[ "$status" == "unauthenticated" || "$status" == "unknown" ]]; then
    echo "Bitwarden is not authenticated. Attempting 'bw login'..."
    if ! bw login </dev/tty >/dev/tty 2>&1; then
      echo "Bitwarden login failed; falling back to manual password entry."
      return 1
    fi
    status="$(bitwarden_status)"
  fi

  if [[ "$status" == "locked" ]]; then
    echo "Bitwarden vault is locked. Attempting 'bw unlock'..."
    if [[ -f "$BW_MASTER_PASSWORD_FILE" ]]; then
      echo "Using master password file for non-interactive unlock..."
      IFS= read -r BW_MASTER_PASSWORD < "$BW_MASTER_PASSWORD_FILE"
      export BW_MASTER_PASSWORD
      session="$(bw unlock --passwordenv BW_MASTER_PASSWORD --nointeraction --raw 2>/dev/null || true)"
      unset BW_MASTER_PASSWORD
    else
      echo "Master password file not found. Run miniscripts/notautorun/BitwardenSetupAndLogin.sh to set it up initially."
      session="$(bw unlock --raw </dev/tty 2>/dev/null || true)"
    fi
    if [[ -z "$session" ]]; then
      echo "Bitwarden unlock failed; falling back to manual password entry."
      return 1
    fi
    export BW_SESSION="$session"
  fi

  RUSTDESK_PERMANENT_PASSWORD="$(bw get password "$BITWARDEN_RUSTDESK_ITEM" 2>/dev/null || true)"

  if [[ -z "$RUSTDESK_PERMANENT_PASSWORD" ]]; then
    echo "Bitwarden item '${BITWARDEN_RUSTDESK_ITEM}' not found or has no password; falling back to manual entry."
    return 1
  fi

  return 0
}

# Configures RustDesk for unattended access (permanent password) and direct IP.
configure_rustdesk() {
  local rd_password="$1"

  # The RustDesk system service runs as root (the unit file has no User= directive).
  # When this script is executed as root (e.g. via sudo), write the config to
  # root's XDG config directory so the service actually picks it up.
  # When run as a regular user (no sudo), fall back to that user's home.
  local rd_config_dir
  if [[ "$EUID" -eq 0 ]]; then
    rd_config_dir="/root/.config/rustdesk"
  else
    rd_config_dir="$TARGET_HOME/.config/rustdesk"
  fi
  local rd_config_file="$rd_config_dir/RustDesk2.toml"

  # ── Stop and disable service before modifying config ─────────────────────
  echo "Stopping and disabling RustDesk service before configuration..."
  if [[ "$EUID" -eq 0 ]]; then
    systemctl stop rustdesk 2>/dev/null || true
    systemctl disable rustdesk 2>/dev/null || true
  else
    sudo systemctl stop rustdesk 2>/dev/null || true
    sudo systemctl disable rustdesk 2>/dev/null || true
  fi
  echo "RustDesk service stopped and disabled."

  echo "Configuring RustDesk (config dir: $rd_config_dir)..."
  mkdir -p "$rd_config_dir"

  # ── Permanent password (enables unattended / no-confirm access) ───────────
  echo "Setting RustDesk permanent password (unattended access)..."
  # Note: the password is visible in the process list while this command runs;
  # this is an inherent limitation of the 'rustdesk --password' CLI API.
  # The CLI writes the password as plain text; RustDesk encrypts it on first start.
  local rd_pw_set=false
  if [[ "$EUID" -eq 0 ]]; then
    rustdesk --password "$rd_password" 2>/dev/null && rd_pw_set=true
  else
    sudo rustdesk --password "$rd_password" 2>/dev/null && rd_pw_set=true
  fi
  if [[ "$rd_pw_set" == true ]]; then
    echo "Permanent password set via RustDesk CLI."
  else
    echo "Warning: 'rustdesk --password' failed; writing password directly to config."
    # Use python3 (already required by this script) to write the TOML entry
    # safely — the password is passed via sys.argv so no shell escaping is needed.
    # RustDesk will hash the plaintext value on the next service start.
    python3 - "$rd_config_file" "$rd_password" <<'PY'
import sys, re

config_file, password = sys.argv[1], sys.argv[2]
try:
    content = open(config_file).read()
except FileNotFoundError:
    content = ""

entry = 'permanent-password = "{}"'.format(password.replace('\\', '\\\\').replace('"', '\\"'))
if re.search(r'^\[options\]', content, re.MULTILINE):
    if 'permanent-password' not in content:
        content = re.sub(r'^(\[options\])', r'\1\n' + entry, content, flags=re.MULTILINE)
    else:
        content = re.sub(r'permanent-password\s*=\s*"[^"]*"', entry, content)
else:
    content += '\n[options]\n{}\n'.format(entry)

with open(config_file, 'w') as f:
    f.write(content)
PY
  fi

  # ── Direct IP access (direct-server = "Y" in RustDesk2.toml) ─────────────
  echo "Enabling direct IP access in RustDesk config..."
  python3 - "$rd_config_file" <<'PY'
import sys, re, os

config_file = sys.argv[1]
os.makedirs(os.path.dirname(config_file), exist_ok=True)
try:
    content = open(config_file).read()
except FileNotFoundError:
    content = ""

entry = 'direct-server = "Y"'
if re.search(r'^\[options\]', content, re.MULTILINE):
    if 'direct-server' not in content:
        content = re.sub(r'^(\[options\])', r'\1\n' + entry, content, flags=re.MULTILINE)
    else:
        content = re.sub(r'direct-server\s*=\s*"[^"]*"', entry, content)
else:
    content += '\n[options]\n{}\n'.format(entry)

with open(config_file, 'w') as f:
    f.write(content)

print('Direct IP access enabled in: {}'.format(config_file))
PY

  # ── Enable and start service to apply config changes ─────────────────────
  echo "Enabling and starting RustDesk service..."
  if [[ "$EUID" -eq 0 ]]; then
    systemctl enable rustdesk
    systemctl start rustdesk
  else
    sudo systemctl enable rustdesk
    sudo systemctl start rustdesk
  fi
  echo "RustDesk service enabled and started."
}

# ── Pre-flight check ──────────────────────────────────────────────────────────

if [[ ! -f "$INSTALL_TAILSCALE_SCRIPT" ]]; then
  echo "Error: required script not found: $INSTALL_TAILSCALE_SCRIPT"
  exit 1
fi

echo "Running shared headless setup installer..."
bash "$INSTALL_TAILSCALE_SCRIPT"

# ── Download and install RustDesk ─────────────────────────────────────────────

echo "Downloading latest RustDesk release..."
RUSTDESK_DOWNLOAD_DIR="$TARGET_HOME/Downloads/RustDesk"
mkdir -p "$RUSTDESK_DOWNLOAD_DIR"
chown -R "$TARGET_USER":"$TARGET_USER" "$RUSTDESK_DOWNLOAD_DIR"

if ! command -v curl >/dev/null 2>&1; then
  echo "Error: curl is required to download RustDesk."
  exit 1
fi

if ! command -v python3 >/dev/null 2>&1; then
  echo "Error: python3 is required to parse RustDesk release metadata."
  exit 1
fi

RUSTDESK_URL="$(curl -fsSL https://api.github.com/repos/rustdesk/rustdesk/releases/latest | python3 -c 'import sys, json; data=json.load(sys.stdin); assets=data.get("assets", []);
for asset in assets:
    name=asset.get("name", "")
    if name.endswith(".deb") and ("amd64" in name or "x86_64" in name):
        print(asset.get("browser_download_url", ""));
        break')"

if [[ -z "$RUSTDESK_URL" ]]; then
  echo "Error: Could not find a RustDesk .deb asset in the latest release."
  exit 1
fi

RUSTDESK_FILE="$RUSTDESK_DOWNLOAD_DIR/$(basename "$RUSTDESK_URL")"
curl -fL "$RUSTDESK_URL" -o "$RUSTDESK_FILE"
chown "$TARGET_USER":"$TARGET_USER" "$RUSTDESK_FILE"

echo "Downloaded RustDesk package to: $RUSTDESK_FILE"

echo "Installing RustDesk..."
if [[ "$EUID" -eq 0 ]]; then
  apt install -y "$RUSTDESK_FILE"
else
  sudo apt install -y "$RUSTDESK_FILE"
fi

echo "Cleaning up RustDesk installer files..."
rm -f "$RUSTDESK_FILE"
rmdir "$RUSTDESK_DOWNLOAD_DIR" 2>/dev/null || true

echo "RustDesk installed."

# ── Configure unattended access and direct IP ─────────────────────────────────

RUSTDESK_PERMANENT_PASSWORD=""

if try_bitwarden_rustdesk_password; then
  echo "Using RustDesk permanent password from Bitwarden (item: ${BITWARDEN_RUSTDESK_ITEM})."
else
  while true; do
    read -r -s -p "Enter RustDesk permanent password for unattended access: " \
      RUSTDESK_PERMANENT_PASSWORD </dev/tty
    echo
    if [[ -n "$RUSTDESK_PERMANENT_PASSWORD" ]]; then
      break
    fi
    echo "Password cannot be empty."
  done
fi

configure_rustdesk "$RUSTDESK_PERMANENT_PASSWORD"

echo "Setup complete. RustDesk installed with unattended access and direct IP access enabled."
