#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
NOTAUTORUN_SCRIPT="$SCRIPT_DIR/../notautorun/RDSetup-Headless.sh"

if [[ ! -f "$NOTAUTORUN_SCRIPT" ]]; then
  echo "Error: required script not found: $NOTAUTORUN_SCRIPT"
  exit 1
fi

bash "$NOTAUTORUN_SCRIPT"
