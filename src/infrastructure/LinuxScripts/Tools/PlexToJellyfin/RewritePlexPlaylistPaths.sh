#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
EXPORT_DIR="${SCRIPT_DIR}/exports"

require_command() {
	if ! command -v "$1" >/dev/null 2>&1; then
		echo "Error: required command '$1' not found in PATH."
		exit 1
	fi
}

normalize_music_root() {
	local input="$1"
	input="${input%/}"
	echo "${input}"
}

validate_music_root() {
	local value="$1"
	if [[ "${value}" != /* ]]; then
		echo "Error: path must be absolute (start with '/')."
		return 1
	fi
	if [[ "${value}" != */Music ]]; then
		echo "Error: path must end with '/Music'."
		return 1
	fi
	return 0
}

find_old_music_root() {
	python3 - "${EXPORT_DIR}" <<'PY'
import re
import sys
from pathlib import Path

export_dir = Path(sys.argv[1])
pattern = re.compile(r'(/[^\n\r"<>]*?/Music)(?=/)')

for file_path in sorted(export_dir.rglob('*')):
    if not file_path.is_file() or file_path.suffix.lower() not in ('.m3u', '.xml'):
        continue
    try:
        text = file_path.read_text(encoding='utf-8', errors='ignore')
    except Exception:
        continue

    match = pattern.search(text)
    if match:
        print(match.group(1).rstrip('/'))
        sys.exit(0)

sys.exit(1)
PY
}

rewrite_paths_in_place() {
	local old_root="$1"
	local new_root="$2"

	python3 - "${EXPORT_DIR}" "${old_root}" "${new_root}" <<'PY'
import sys
from pathlib import Path

export_dir = Path(sys.argv[1])
old_root = sys.argv[2]
new_root = sys.argv[3]

files = []
for path in sorted(export_dir.rglob('*')):
    if path.is_file() and path.suffix.lower() in ('.m3u', '.xml'):
        files.append(path)

changed_files = 0
changed_entries = 0

for path in files:
    text = path.read_text(encoding='utf-8', errors='ignore')
    count = text.count(old_root + '/')
    if count == 0:
        continue

    updated = text.replace(old_root + '/', new_root + '/')
    if updated != text:
        path.write_text(updated, encoding='utf-8')
        changed_files += 1
        changed_entries += count

print(f"CHANGED_FILES={changed_files}")
print(f"CHANGED_ENTRIES={changed_entries}")
print(f"SCANNED_FILES={len(files)}")
PY
}

main() {
	require_command python3

	if [[ ! -d "${EXPORT_DIR}" ]]; then
		echo "Error: exports directory not found: ${EXPORT_DIR}"
		exit 1
	fi

	echo "Plex Playlist Path Rewriter"
	echo "Exports directory: ${EXPORT_DIR}"
	echo
	echo "This rewrites file paths in-place for all .m3u and .xml playlist files."
	echo "No new files are created."
	echo
	echo "Example:"
	echo "  Old file path: /home/matt/OneDrive-Local/Media/Music/Artists/Bobby Fuller/I Fought The Law.mp3"
	echo "  New file path: /srv/storage/OneDrive/Media/Music/Artists/Bobby Fuller/I Fought The Law.mp3"
	echo
	echo "Enter only the NEW root path up to and including /Music"
	echo "Example input: /srv/storage/OneDrive/Media/Music"
	echo

	old_root="$(find_old_music_root || true)"
	if [[ -z "${old_root}" ]]; then
		echo "Error: could not auto-detect an existing '/.../Music' root in exports files."
		echo "Make sure at least one .m3u or .xml contains full music file paths."
		exit 1
	fi

	echo "Detected current music root: ${old_root}"
	read -r -p "Enter NEW music root path (must end with /Music): " new_root
	new_root="$(normalize_music_root "${new_root}")"

	if ! validate_music_root "${new_root}"; then
		exit 1
	fi

	if [[ "${new_root}" == "${old_root}" ]]; then
		echo "New root is the same as old root. Nothing to change."
		exit 0
	fi

	echo
	echo "Will replace:"
	echo "  ${old_root}/"
	echo "with:"
	echo "  ${new_root}/"
	read -r -p "Proceed? (y/N): " confirm
	if [[ ! "${confirm}" =~ ^[Yy]$ ]]; then
		echo "Canceled."
		exit 0
	fi

	result="$(rewrite_paths_in_place "${old_root}" "${new_root}")"
	echo
	echo "Rewrite complete."
	echo "${result}" | sed 's/^/  /'
}

main
