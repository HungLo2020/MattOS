#!/usr/bin/env bash

set -euo pipefail

fail() {
  echo "[SetupValidation] ERROR: $1" >&2
  exit 1
}

echo "[SetupValidation] Checking required commands..."
command -v sudo >/dev/null 2>&1 || fail "sudo is not installed. Install sudo and try again."
command -v apt >/dev/null 2>&1 || fail "apt is not installed. This setup currently requires an apt-based distro."
command -v dpkg >/dev/null 2>&1 || fail "dpkg is not installed. This setup currently requires dpkg tools."

echo "[SetupValidation] Checking sudo access..."
if ! sudo -v; then
  fail "sudo authentication failed. Cannot continue setup without sudo access."
fi

echo "[SetupValidation] Checking internet connectivity..."
if command -v curl >/dev/null 2>&1; then
  if ! curl -fsSL --max-time 10 https://archive.ubuntu.com/ >/dev/null; then
    fail "No internet connectivity (unable to reach archive.ubuntu.com)."
  fi
elif command -v wget >/dev/null 2>&1; then
  if ! wget -q --spider --timeout=10 https://archive.ubuntu.com/; then
    fail "No internet connectivity (unable to reach archive.ubuntu.com)."
  fi
elif command -v ping >/dev/null 2>&1; then
  if ! ping -c 1 -W 5 1.1.1.1 >/dev/null 2>&1; then
    fail "No internet connectivity detected."
  fi
else
  fail "Cannot test internet connectivity (curl/wget/ping not found)."
fi

echo "[SetupValidation] All checks passed."