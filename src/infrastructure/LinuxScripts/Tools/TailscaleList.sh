#!/usr/bin/env bash

set -euo pipefail

# Colors
BLUE='\033[0;34m'
ORANGE='\033[0;33m'
GREEN='\033[0;32m'
RED='\033[0;31m'
BOLD='\033[1m'
NC='\033[0m' # No Color

# Check if tailscale is available
if ! command -v tailscale &>/dev/null; then
    echo "Error: tailscale is not installed or not in PATH."
    exit 1
fi

# Check if jq is available
if ! command -v jq &>/dev/null; then
    echo "Error: jq is not installed or not in PATH."
    exit 1
fi

# Get tailscale status as JSON
STATUS_JSON=$(tailscale status --json 2>/dev/null) || {
    echo "Error: Failed to retrieve Tailscale status. Is tailscaled running?"
    exit 1
}

# Print header
printf "${BOLD}%-35s %-20s %s${NC}\n" "HOSTNAME" "IP ADDRESS" "STATUS"
printf '%0.s-' {1..65}
printf '\n'

# Helper function to print a device row
print_row() {
    local hostname="$1"
    local ip="$2"
    local online="$3"

    if [[ "$online" == "true" ]]; then
        status_str="${GREEN}Online${NC}"
    else
        status_str="${RED}Offline${NC}"
    fi

    printf "${BLUE}%-35s${NC} ${ORANGE}%-20s${NC} ${status_str}\n" "$hostname" "$ip"
}

# Print self (always online)
SELF_HOST=$(echo "$STATUS_JSON" | jq -r '.Self.HostName')
SELF_IP=$(echo "$STATUS_JSON" | jq -r '.Self.TailscaleIPs[0]')
print_row "$SELF_HOST" "$SELF_IP" "true"

# Print peers
while IFS=$'\t' read -r hostname ip online; do
    print_row "$hostname" "$ip" "$online"
done < <(echo "$STATUS_JSON" | jq -r '
    .Peer // {} | to_entries[] |
    [.value.HostName, (.value.TailscaleIPs[0] // "N/A"), (if .value.Online then "true" else "false" end)] |
    @tsv
')
