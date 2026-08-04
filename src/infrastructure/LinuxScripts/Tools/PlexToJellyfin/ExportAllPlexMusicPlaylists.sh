#!/usr/bin/env bash

set -euo pipefail

PLEX_URL="http://localhost:32400"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OUTPUT_DIR="${SCRIPT_DIR}/exports"

require_command() {
	if ! command -v "$1" >/dev/null 2>&1; then
		echo "Error: required command '$1' not found in PATH."
		exit 1
	fi
}

sanitize_filename() {
	local value="$1"
	value="${value//\//-}"
	value="${value//:/-}"
	value="${value//$'\n'/ }"
	printf '%s' "$value" | sed 's/[[:space:]]\+/ /g; s/^ //; s/ $//; s/[^[:alnum:]_. -]/_/g'
}

prompt_defaults() {
	echo "Using Plex URL: ${PLEX_URL}"

	read -r -p "Plex token (X-Plex-Token): " PLEX_TOKEN
	PLEX_TOKEN="$(printf '%s' "${PLEX_TOKEN}" | sed 's/^[[:space:]]*//; s/[[:space:]]*$//')"
	if [[ -z "${PLEX_TOKEN}" ]]; then
		echo "Error: Plex token cannot be empty."
		exit 1
	fi

	echo "Export directory: ${OUTPUT_DIR}"
}

fetch_audio_playlists_xml() {
	plex_api_get "/playlists" "playlistType=audio"
}

fetch_all_playlists_xml() {
	plex_api_get "/playlists"
}

plex_api_get() {
	local path="$1"
	shift || true

	local args=()
	local param
	for param in "$@"; do
		args+=(--data-urlencode "$param")
	done

	curl -fsSLG \
		-H "X-Plex-Token: ${PLEX_TOKEN}" \
		"${args[@]}" \
		"${PLEX_URL}${path}"
}

plex_api_get_to_file() {
	local path="$1"
	local output_file="$2"
	shift 2 || true

	local args=()
	local param
	for param in "$@"; do
		args+=(--data-urlencode "$param")
	done

	curl -fsSLG \
		-H "X-Plex-Token: ${PLEX_TOKEN}" \
		"${args[@]}" \
		"${PLEX_URL}${path}" \
		-o "${output_file}"
}

parse_playlists_to_tsv() {
	python3 -c '
import sys
import xml.etree.ElementTree as ET

xml_data = sys.stdin.read()
if not xml_data.strip():
	sys.exit(0)

try:
	root = ET.fromstring(xml_data)
except ET.ParseError as exc:
	print(f"ERROR\tPARSE\t{exc}")
	sys.exit(2)

for playlist in root.findall(".//Playlist"):
	key = playlist.get("ratingKey", "").strip()
	title = playlist.get("title", "").strip() or "Untitled Playlist"
	if key:
		print(f"{key}\t{title}")
'
}

write_m3u_from_xml() {
	local xml_file="$1"
	local m3u_file="$2"

	python3 - "$xml_file" "$m3u_file" <<'PY'
import sys
import xml.etree.ElementTree as ET

xml_path, m3u_path = sys.argv[1], sys.argv[2]

tree = ET.parse(xml_path)
root = tree.getroot()

count = 0
with open(m3u_path, 'w', encoding='utf-8') as out:
    out.write('#EXTM3U\n')
    for track in root.findall('.//Track'):
        part = track.find('./Media/Part')
        if part is None:
            continue
        file_path = part.get('file')
        if not file_path:
            continue
        out.write(file_path + '\n')
        count += 1

print(count)
PY
}

main() {
	require_command curl
	require_command python3

	prompt_defaults
	mkdir -p "${OUTPUT_DIR}"
	rm -f "${OUTPUT_DIR}"/*.xml "${OUTPUT_DIR}"/*.m3u 2>/dev/null || true

	echo "Fetching Plex audio playlists from ${PLEX_URL} ..."
	if ! playlist_xml="$(fetch_audio_playlists_xml 2>/tmp/plex_export_error.log)"; then
		if grep -q '401' /tmp/plex_export_error.log 2>/dev/null; then
			echo "Error: Plex returned 401 Unauthorized."
			echo "Check your X-Plex-Token and make sure it is a server-valid token."
			echo
			echo "Quick token test command:"
			echo "curl -s -H 'X-Plex-Token: YOUR_TOKEN' '${PLEX_URL}/identity'"
			echo "If token works, output should include '<MediaContainer ...>'."
		else
			echo "Error: failed to contact Plex at ${PLEX_URL}."
			cat /tmp/plex_export_error.log
		fi
		rm -f /tmp/plex_export_error.log
		exit 1
	fi
	rm -f /tmp/plex_export_error.log

	playlist_tsv="$(printf '%s' "${playlist_xml}" | parse_playlists_to_tsv)"

	if printf '%s\n' "${playlist_tsv}" | grep -q '^ERROR'; then
		echo "Error: failed to parse Plex playlist response."
		printf '%s\n' "${playlist_tsv}"
		exit 1
	fi

	if [[ -z "${playlist_tsv}" ]]; then
		echo "No playlists returned by audio filter; retrying with all playlists endpoint..."
		playlist_xml="$(fetch_all_playlists_xml)"
		playlist_tsv="$(printf '%s' "${playlist_xml}" | parse_playlists_to_tsv)"
	fi

	if [[ -z "${playlist_tsv}" ]]; then
		echo "No Plex playlists found."
		echo "Done."
		exit 0
	fi

	local total=0
	local succeeded=0
	local failed=0
	local skipped_non_audio=0

	echo "Exporting playlists to ${OUTPUT_DIR} ..."
	while IFS=$'\t' read -r rating_key title; do
		[[ -z "${rating_key}" ]] && continue
		total=$((total + 1))

		safe_title="$(sanitize_filename "${title}")"
		if [[ -z "${safe_title}" ]]; then
			safe_title="Playlist_${rating_key}"
		fi

		xml_file="${OUTPUT_DIR}/${safe_title}.xml"
		m3u_file="${OUTPUT_DIR}/${safe_title}.m3u"

		if ! plex_api_get_to_file "/playlists/${rating_key}/items" "${xml_file}"; then
			echo "[FAIL] ${title} (key=${rating_key}) - could not fetch items"
			failed=$((failed + 1))
			continue
		fi

		track_count="$(write_m3u_from_xml "${xml_file}" "${m3u_file}" || true)"
		if [[ -z "${track_count}" || ! "${track_count}" =~ ^[0-9]+$ ]]; then
			echo "[FAIL] ${title} (key=${rating_key}) - could not parse playlist XML"
			failed=$((failed + 1))
			continue
		fi

		if (( track_count == 0 )); then
			echo "[SKIP] ${title} (key=${rating_key}) - contains no audio tracks"
			skipped_non_audio=$((skipped_non_audio + 1))
			rm -f "${m3u_file}"
			continue
		fi

		echo "[OK]   ${title} -> ${safe_title}.m3u (${track_count} tracks)"
		succeeded=$((succeeded + 1))
	done <<<"${playlist_tsv}"

	echo
	echo "Export complete."
	echo "  Total playlists found : ${total}"
	echo "  Successfully exported : ${succeeded}"
	echo "  Failed                : ${failed}"
	echo "  Skipped (no audio)    : ${skipped_non_audio}"
	echo "  Output directory      : ${OUTPUT_DIR}"
}

main
