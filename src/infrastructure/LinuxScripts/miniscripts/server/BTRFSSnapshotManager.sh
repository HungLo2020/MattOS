#!/usr/bin/env bash

set -euo pipefail

MOUNT_POINT="/srv/storage"
SNAPSHOT_ROOT="${MOUNT_POINT}/snapshots"
DEFAULT_SOURCE="${MOUNT_POINT}"

reexec_with_sudo_if_needed() {
	if [[ "${EUID}" -ne 0 ]]; then
		echo "Re-running with sudo..."
		exec sudo bash "$0" "$@"
	fi
}

require_command() {
	if ! command -v "$1" >/dev/null 2>&1; then
		echo "Error: required command '$1' is not installed or not in PATH."
		exit 1
	fi
}

is_btrfs_mount() {
	local fs_type
	fs_type="$(findmnt -no FSTYPE "${MOUNT_POINT}" 2>/dev/null || true)"
	[[ "${fs_type}" == "btrfs" ]]
}

warn_snapshots_subvolume_status() {
	if [[ ! -d "${SNAPSHOT_ROOT}" ]]; then
		echo "Warning: ${SNAPSHOT_ROOT} does not exist yet."
		echo "If you want snapshots there, create it (preferably as a Btrfs subvolume)."
		return 0
	fi

	if btrfs subvolume show "${SNAPSHOT_ROOT}" >/dev/null 2>&1; then
		echo "Snapshot root check: ${SNAPSHOT_ROOT} is a Btrfs subvolume."
	else
		echo "Warning: ${SNAPSHOT_ROOT} exists but is NOT a Btrfs subvolume."
		echo "Snapshots can still be placed there, but intended setup is a subvolume."
	fi
}

print_btrfs_usage_warning() {
	echo "=== Btrfs Filesystem Usage (${MOUNT_POINT}) ==="
	btrfs filesystem usage "${MOUNT_POINT}" || true
	echo
}

validate_environment() {
	require_command btrfs
	require_command findmnt

	if [[ ! -d "${MOUNT_POINT}" ]]; then
		echo "Error: mount point '${MOUNT_POINT}' does not exist."
		exit 1
	fi

	if ! is_btrfs_mount; then
		echo "Error: ${MOUNT_POINT} is not mounted as btrfs."
		echo "Current FSTYPE: $(findmnt -no FSTYPE "${MOUNT_POINT}" 2>/dev/null || echo "unknown")"
		exit 1
	fi

	warn_snapshots_subvolume_status
	print_btrfs_usage_warning
}

pretty_print_subvolume_list() {
	local tmp_file
	tmp_file="$(mktemp)"
	trap 'rm -f "${tmp_file}"' RETURN

	btrfs subvolume list -p "${MOUNT_POINT}" >"${tmp_file}"

	echo "=== Subvolumes (ID | TOP_LEVEL | PATH) ==="
	awk '
		{
			id="?"; top="?"; path=""
			for (i = 1; i <= NF; i++) {
				if ($i == "ID" && (i + 1) <= NF) id = $(i + 1)
				if ($i == "top" && (i + 1) <= NF) top = $(i + 1)
				if ($i == "path") {
					path = $(i + 1)
					for (j = i + 2; j <= NF; j++) path = path " " $j
					break
				}
			}
			printf("%-8s %-10s %s\n", id, top, path)
		}
	' "${tmp_file}"
	echo
}

list_subvolumes() {
	echo "=== btrfs subvolume list -p ${MOUNT_POINT} ==="
	pretty_print_subvolume_list

	echo "=== btrfs subvolume show ${MOUNT_POINT} ==="
	btrfs subvolume show "${MOUNT_POINT}" || true
	echo

	echo "=== findmnt -no SOURCE,OPTIONS ${MOUNT_POINT} ==="
	findmnt -no SOURCE,OPTIONS "${MOUNT_POINT}" || true
	echo
}

create_snapshot() {
	local source destination default_destination mode_choice
	local readonly_flag="-r"

	source="${DEFAULT_SOURCE}"
	default_destination="${SNAPSHOT_ROOT}/@data-$(date +%F-%H%M)"
	destination="${default_destination}"

	echo "Default source subvolume: ${source}"
	read -r -p "Enter source subvolume path [${source}]: " mode_choice
	if [[ -n "${mode_choice}" ]]; then
		source="${mode_choice}"
	fi

	if [[ ! -d "${source}" ]]; then
		echo "Error: source path does not exist: ${source}"
		return 1
	fi

	if ! btrfs subvolume show "${source}" >/dev/null 2>&1; then
		echo "Error: source is not a Btrfs subvolume: ${source}"
		return 1
	fi

	read -r -p "Enter destination snapshot path [${default_destination}]: " mode_choice
	if [[ -n "${mode_choice}" ]]; then
		destination="${mode_choice}"
	fi

	if [[ -e "${destination}" ]]; then
		echo "Error: destination already exists: ${destination}"
		return 1
	fi

	if [[ "${destination}" != "${SNAPSHOT_ROOT}"/* ]]; then
		echo "Error: destination must be inside ${SNAPSHOT_ROOT}."
		return 1
	fi

	echo "Create read-only snapshot? [Y/n]"
	read -r mode_choice
	if [[ "${mode_choice}" =~ ^[Nn]$ ]]; then
		readonly_flag=""
	fi

	mkdir -p "$(dirname "${destination}")"

	if [[ -n "${readonly_flag}" ]]; then
		btrfs subvolume snapshot -r "${source}" "${destination}"
		echo "Created read-only snapshot: ${destination}"
	else
		btrfs subvolume snapshot "${source}" "${destination}"
		echo "Created read-write snapshot: ${destination}"
	fi
}

load_snapshot_entries() {
	local all_lines
	all_lines="$(btrfs subvolume list -p "${MOUNT_POINT}" || true)"
	awk '
		{
			path=""
			for (i = 1; i <= NF; i++) {
				if ($i == "path") {
					path = $(i + 1)
					for (j = i + 2; j <= NF; j++) path = path " " $j
					break
				}
			}
			if (path ~ /^snapshots\//) print path
		}
	' <<<"${all_lines}"
}

delete_snapshot() {
	local entries snapshot_count idx selected_index selected_rel selected_name selected_path confirm
	entries="$(load_snapshot_entries)"

	if [[ -z "${entries}" ]]; then
		echo "No snapshot subvolumes found under snapshots/."
		return 0
	fi

	echo "=== Snapshots under ${SNAPSHOT_ROOT} ==="
	snapshot_count=0
	while IFS= read -r line; do
		snapshot_count=$((snapshot_count + 1))
		printf "%3d) %s\n" "${snapshot_count}" "${line}"
	done <<<"${entries}"
	echo

	read -r -p "Select snapshot number to delete: " selected_index
	if [[ ! "${selected_index}" =~ ^[0-9]+$ ]]; then
		echo "Error: invalid selection."
		return 1
	fi

	if (( selected_index < 1 || selected_index > snapshot_count )); then
		echo "Error: selection out of range."
		return 1
	fi

	idx=0
	selected_rel=""
	while IFS= read -r line; do
		idx=$((idx + 1))
		if (( idx == selected_index )); then
			selected_rel="${line}"
			break
		fi
	done <<<"${entries}"

	if [[ -z "${selected_rel}" ]]; then
		echo "Error: failed to resolve selected snapshot."
		return 1
	fi

	selected_name="${selected_rel#snapshots/}"
	selected_path="${MOUNT_POINT}/${selected_rel}"

	if [[ "${selected_path}" != "${SNAPSHOT_ROOT}"/* ]]; then
		echo "Safety stop: refusing to delete outside ${SNAPSHOT_ROOT}."
		return 1
	fi

	if [[ ! -e "${selected_path}" ]]; then
		echo "Error: snapshot path no longer exists: ${selected_path}"
		return 1
	fi

	echo "Selected snapshot: ${selected_rel}"
	read -r -p "Type exact snapshot name to confirm deletion ('${selected_name}'): " confirm
	if [[ "${confirm}" != "${selected_name}" ]]; then
		echo "Confirmation name mismatch. Deletion canceled."
		return 1
	fi

	read -r -p "Final confirmation: delete '${selected_rel}'? (y/N): " confirm
	if [[ ! "${confirm}" =~ ^[Yy]$ ]]; then
		echo "Deletion canceled."
		return 0
	fi

	btrfs subvolume delete "${selected_path}"
	echo "Deleted snapshot: ${selected_rel}"
}

print_menu() {
	echo "=== Btrfs Snapshot Manager ==="
	echo "Mount point: ${MOUNT_POINT}"
	echo "Snapshot root: ${SNAPSHOT_ROOT}"
	echo
	echo "1) List subvolumes"
	echo "2) Create snapshot"
	echo "3) Delete snapshot"
	echo "4) Show btrfs usage"
	echo "5) Exit"
	echo
}

main_loop() {
	local choice
	while true; do
		print_menu
		read -r -p "Choose an option [1-5]: " choice
		case "${choice}" in
			1)
				list_subvolumes
				;;
			2)
				create_snapshot
				;;
			3)
				delete_snapshot
				;;
			4)
				print_btrfs_usage_warning
				;;
			5)
				echo "Goodbye."
				exit 0
				;;
			*)
				echo "Invalid selection."
				;;
		esac
		echo
	done
}

reexec_with_sudo_if_needed "$@"
validate_environment
main_loop
