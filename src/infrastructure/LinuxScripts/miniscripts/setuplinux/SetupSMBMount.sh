#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
BW_MASTER_PASSWORD_FILE="$PROJECT_ROOT/.bw_master_password"

TARGET_USER="${SUDO_USER:-$(id -un)}"
TARGET_HOME="$(getent passwd "$TARGET_USER" | cut -d: -f6)"
TARGET_UID="$(id -u "$TARGET_USER")"
TARGET_GID="$(id -g "$TARGET_USER")"

SMB_SERVER_IP="100.72.33.98"
SMB_SHARE_NAME="storage"
SMB_MOUNT_POINT="/mnt/storage"
SMB_CREDENTIALS_FILE="/etc/samba/credentials-storage-${TARGET_USER}"

SYSTEMD_SERVICE_NAME="storage-smb-mount.service"
SYSTEMD_SERVICE_PATH="/etc/systemd/system/${SYSTEMD_SERVICE_NAME}"
HELPER_SCRIPT_PATH="/usr/local/sbin/storage-smb-mount.sh"

BITWARDEN_ITEM_NAME="PCPassword"
SMB_PASSWORD=""

if [[ -z "$TARGET_HOME" || ! -d "$TARGET_HOME" ]]; then
  echo "Error: Could not resolve home directory for user '$TARGET_USER'."
  exit 1
fi

run_as_target_user() {
  if [[ "$(id -un)" == "$TARGET_USER" ]]; then
    "$@"
  else
    sudo -H -u "$TARGET_USER" "$@"
  fi
}

bw_exec() {
  if [[ "$(id -un)" == "$TARGET_USER" ]]; then
    BW_SESSION="${BW_SESSION:-}" bw "$@"
  else
    sudo -H -u "$TARGET_USER" env BW_SESSION="${BW_SESSION:-}" bw "$@"
  fi
}

bitwarden_status() {
  local status_json
  local parsed

  status_json="$(bw_exec status 2>/dev/null || true)"
  parsed="$(printf '%s' "$status_json" | sed -n 's/.*"status"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p')"

  if [[ -z "$parsed" ]]; then
    echo "unknown"
  else
    echo "$parsed"
  fi
}

ensure_smb_client() {
  local missing=()

  if ! command -v smbclient >/dev/null 2>&1; then
    missing+=("smbclient")
  fi

  if ! command -v mount.cifs >/dev/null 2>&1; then
    missing+=("cifs-utils")
  fi

  if [[ ${#missing[@]} -eq 0 ]]; then
    echo "SMB client prerequisites already installed."
    return 0
  fi

  if ! command -v apt-get >/dev/null 2>&1; then
    echo "Error: missing SMB prerequisites (${missing[*]}), and apt-get is unavailable for auto-install."
    exit 1
  fi

  echo "Installing SMB prerequisites: ${missing[*]}"
  sudo apt-get update
  sudo apt-get install -y "${missing[@]}"

  if ! command -v smbclient >/dev/null 2>&1 || ! command -v mount.cifs >/dev/null 2>&1; then
    echo "Error: SMB prerequisites are still missing after install attempt."
    exit 1
  fi
}

ensure_tailscale_installed() {
  if ! command -v tailscale >/dev/null 2>&1; then
    echo "Error: tailscale is not installed. please run RDSetup.sh before running this script."
    exit 1
  fi
}

ensure_tailscale_running() {
  local status_json
  if ! status_json="$(tailscale status --json 2>/dev/null)"; then
    echo "Error: tailscale is not running or not logged in. Start tailscaled and connect with 'tailscale up' first. Or run RDSetup.sh"
    exit 1
  fi

  local readiness
  readiness="$(python3 - <<'PY' "$status_json"
import json, sys

obj = json.loads(sys.argv[1])
backend = obj.get("BackendState", "")
self_node = obj.get("Self") or {}
online = bool(self_node.get("Online", False))

if backend != "Running":
    print(f"ERR\tTailscale backend state is '{backend or 'unknown'}' (expected 'Running').")
    raise SystemExit(0)

if not self_node:
    print("ERR\tNo active Tailscale node identity found (not logged in).")
    raise SystemExit(0)

if not online:
    print("ERR\tTailscale is logged in but currently offline/disconnected.")
    raise SystemExit(0)

print("OK")
PY
)"

  if [[ "$readiness" != "OK" ]]; then
    echo "Error: ${readiness#ERR$'\t'}"
    echo "This script does not install, start, or log in to tailscale automatically."
    exit 1
  fi
}

resolve_bitwarden_password_from_item() {
  local item_name="$1"
  bw_exec get password "$item_name" 2>/dev/null || true
}

resolve_bitwarden_smb_password() {
  if ! command -v bw >/dev/null 2>&1; then
    return 1
  fi

  local status session password
  status="$(bitwarden_status)"

  if [[ "$status" == "unauthenticated" || "$status" == "unknown" ]]; then
    if [[ ! -t 0 ]]; then
      return 1
    fi

    echo "Bitwarden is not authenticated. Attempting 'bw login'..."
    if [[ "$(id -un)" == "$TARGET_USER" ]]; then
      bw login </dev/tty >/dev/tty 2>&1 || return 1
    else
      sudo -H -u "$TARGET_USER" bw login </dev/tty >/dev/tty 2>&1 || return 1
    fi
    status="$(bitwarden_status)"
  fi

  if [[ "$status" == "locked" ]]; then
    echo "Bitwarden vault is locked. Attempting 'bw unlock'..."
    if [[ -f "$BW_MASTER_PASSWORD_FILE" ]]; then
      local bw_master_password
      bw_master_password="$(<"$BW_MASTER_PASSWORD_FILE")"
      [[ -n "$bw_master_password" ]] || return 1

      if [[ "$(id -un)" == "$TARGET_USER" ]]; then
        export BW_MASTER_PASSWORD="$bw_master_password"
        session="$(bw unlock --passwordenv BW_MASTER_PASSWORD --nointeraction --raw 2>/dev/null || true)"
        unset BW_MASTER_PASSWORD
      else
        session="$(sudo -H -u "$TARGET_USER" env BW_MASTER_PASSWORD="$bw_master_password" bw unlock --passwordenv BW_MASTER_PASSWORD --nointeraction --raw 2>/dev/null || true)"
      fi
    else
      if [[ ! -t 0 ]]; then
        return 1
      fi

      if [[ "$(id -un)" == "$TARGET_USER" ]]; then
        session="$(bw unlock --raw </dev/tty 2>/dev/null || true)"
      else
        session="$(sudo -H -u "$TARGET_USER" bw unlock --raw </dev/tty 2>/dev/null || true)"
      fi
    fi

    session="$(printf '%s' "$session" | tr -d '\r\n')"
    [[ -n "$session" ]] || return 1
    export BW_SESSION="$session"
  fi

  password="$(resolve_bitwarden_password_from_item "$BITWARDEN_ITEM_NAME")"
  if [[ -z "$password" ]]; then
    bw_exec sync >/dev/null 2>&1 || true
    password="$(resolve_bitwarden_password_from_item "$BITWARDEN_ITEM_NAME")"
  fi

  if [[ -n "$password" ]]; then
    SMB_PASSWORD="$password"
    return 0
  fi

  return 1
}

prompt_smb_password_fallback() {
  if [[ ! -t 0 ]]; then
    echo "Error: Could not resolve SMB password from Bitwarden and no interactive terminal is available for prompt fallback."
    exit 1
  fi

  local prompt_password
  while true; do
    read -r -s -p "Enter SMB password for user '${TARGET_USER}': " prompt_password </dev/tty
    echo
    if [[ -n "$prompt_password" ]]; then
      SMB_PASSWORD="$prompt_password"
      return 0
    fi
    echo "Password cannot be empty."
  done
}

cleanup_existing_storage_mounts() {
  echo "Cleaning any existing '${SMB_SHARE_NAME}' mounts (idempotent reset)..."

  local mountpoints=()
  mapfile -t mountpoints < <(findmnt -rn -t cifs -o TARGET,SOURCE | awk -v ip="$SMB_SERVER_IP" -v share="$SMB_SHARE_NAME" '$2 ~ ("^//" ip "/" share "$") {print $1}')

  if [[ ${#mountpoints[@]} -eq 0 ]]; then
    echo "No existing cifs mounts for //${SMB_SERVER_IP}/${SMB_SHARE_NAME} found."
    return 0
  fi

  local mp
  for mp in "${mountpoints[@]}"; do
    echo "Unmounting existing mount: ${mp}"
    sudo umount "$mp" 2>/dev/null || sudo umount -l "$mp" 2>/dev/null || true
  done
}

write_credentials_file() {
  sudo mkdir -p /etc/samba

  sudo tee "$SMB_CREDENTIALS_FILE" >/dev/null <<EOF
username=${TARGET_USER}
password=${SMB_PASSWORD}
EOF

  sudo chmod 600 "$SMB_CREDENTIALS_FILE"
}

write_mount_helper_script() {
  sudo tee "$HELPER_SCRIPT_PATH" >/dev/null <<EOF
#!/usr/bin/env bash
set -euo pipefail

MOUNT_POINT="${SMB_MOUNT_POINT}"
SERVER_IP="${SMB_SERVER_IP}"
SHARE_NAME="${SMB_SHARE_NAME}"
CREDENTIALS_FILE="${SMB_CREDENTIALS_FILE}"
TARGET_UID="${TARGET_UID}"
TARGET_GID="${TARGET_GID}"

if ! command -v tailscale >/dev/null 2>&1; then
  echo "tailscale not found"
  exit 1
fi

if ! tailscale status --json >/dev/null 2>&1; then
  echo "tailscale not ready"
  exit 1
fi

mkdir -p "\$MOUNT_POINT"

if findmnt -rn "\$MOUNT_POINT" >/dev/null 2>&1; then
  exit 0
fi

mount -t cifs "//\$SERVER_IP/\$SHARE_NAME" "\$MOUNT_POINT" \
  -o "credentials=\$CREDENTIALS_FILE,uid=\$TARGET_UID,gid=\$TARGET_GID,iocharset=utf8,noperm,vers=3.1.1,_netdev"
EOF

  sudo chmod 700 "$HELPER_SCRIPT_PATH"
}

write_systemd_service() {
  sudo tee "$SYSTEMD_SERVICE_PATH" >/dev/null <<EOF
[Unit]
Description=Mount SMB share //${SMB_SERVER_IP}/${SMB_SHARE_NAME}
After=network-online.target tailscaled.service
Wants=network-online.target tailscaled.service
StartLimitIntervalSec=0

[Service]
Type=oneshot
ExecStart=${HELPER_SCRIPT_PATH}
RemainAfterExit=yes
Restart=on-failure
RestartSec=20

[Install]
WantedBy=multi-user.target
EOF
}

enable_and_start_service() {
  sudo systemctl daemon-reload
  sudo systemctl enable "$SYSTEMD_SERVICE_NAME" >/dev/null
  sudo systemctl restart "$SYSTEMD_SERVICE_NAME" || true

  if findmnt -rn "$SMB_MOUNT_POINT" >/dev/null 2>&1; then
    echo "SMB share mounted at ${SMB_MOUNT_POINT}."
  else
    echo "Mount is not up yet. Service will keep retrying every 20s until tailscale/share becomes available."
    echo "Check status with: systemctl status ${SYSTEMD_SERVICE_NAME}"
  fi
}

ensure_smb_client
ensure_tailscale_installed
ensure_tailscale_running

if ! resolve_bitwarden_smb_password; then
  echo "Bitwarden password resolution failed for '${BITWARDEN_ITEM_NAME}'."
  prompt_smb_password_fallback
fi

cleanup_existing_storage_mounts
write_credentials_file
write_mount_helper_script
write_systemd_service
enable_and_start_service

echo "Setup complete. SMB share //${SMB_SERVER_IP}/${SMB_SHARE_NAME} is managed by ${SYSTEMD_SERVICE_NAME}."
