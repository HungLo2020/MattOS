#!/usr/bin/env bash
# Cluster device function overrides
# Sourced after aux_t.sh to shadow DM-based device functions with
# versions that work on real cluster devices.

prepare_devs() {
	local n=${1:-3}
	local devsize=${2:-34}  # MB per device; used to scale up real device count
	local pvname=${3:-pv}

	if [[ ! -f "$LVM_TEST_DEVICE_LIST" ]]; then
		die "LVM_TEST_DEVICE_LIST not set or file not found: ${LVM_TEST_DEVICE_LIST:-}"
	fi

	# Scale up device count if the test requests more total space than n real
	# devices provide.  Only applies when devsize exceeds a single real device.
	local real_dev_mb
	real_dev_mb=$(( $(blockdev --getsize64 "$(head -1 "$LVM_TEST_DEVICE_LIST")") / 1048576 ))
	if [[ "$devsize" -gt "$real_dev_mb" ]]; then
		local total_mb=$(( n * devsize ))
		local n_needed=$(( (total_mb + real_dev_mb - 1) / real_dev_mb ))
		[[ "$n_needed" -gt "$n" ]] && n=$n_needed
	fi

	local avail
	avail=$(wc -l < "$LVM_TEST_DEVICE_LIST")
	if [[ "$n" -gt "$avail" ]]; then
		skip "Test requires $n devices but only $avail available ($(( avail * real_dev_mb ))MB)"
	fi

	DEVICES=()
	local count=0
	while read -r path; do
		[[ -n "$path" ]] || continue
		DEVICES+=("$path")
		count=$(( count + 1 ))
		[[ "$count" -ge "$n" ]] && break
	done < "$LVM_TEST_DEVICE_LIST"

	echo -n "## preparing $n real devices..."

	for d in "${DEVICES[@]}"; do
		dd if=/dev/zero of="$d" bs=32k count=1 2>/dev/null
		wipefs -a "$d" 2>/dev/null || true
	done

	if [[ "${LVM_TEST_DEVICES_FILE:-0}" != 0 ]]; then
		mkdir -p "$LVM_SYSTEM_DIR/devices" || true
		rm -f "$LVM_SYSTEM_DIR/devices/system.devices"
		touch "$LVM_SYSTEM_DIR/devices/system.devices"
		for d in "${DEVICES[@]}"; do
			lvmdevices --adddev "$d" || true
		done
	fi

	printf "%s\\n" "${DEVICES[@]}" > DEVICES
	echo "ok"
}

prepare_pvs() {
	prepare_devs "$@"
	pvcreate -ff "${DEVICES[@]}"
}

prepare_vg() {
	teardown_devs
	prepare_devs "$@"
	vgcreate $SHARED -s 512K "$vg" "${DEVICES[@]}"
}

prepare_backing_dev() {
	return 0
}

prepare_loop() {
	skip "loop devices not available in cluster shell mode"
}

prepare_ramdisk() {
	skip "ramdisk not available in cluster shell mode"
}

prepare_dmeventd() {
	if [[ -z "${RUNNING_DMEVENTD-}" ]]; then
		echo -n "## starting dmeventd..."
		dmeventd
		for i in {50..0}; do
			[[ "$i" -eq 0 ]] && die "Startup of dmeventd is too slow."
			[[ -e "${DMEVENTD_PIDFILE}" ]] && break
			sleep .1
		done
		RUNNING_DMEVENTD=$(cat "${DMEVENTD_PIDFILE}")
		echo "$RUNNING_DMEVENTD" > LOCAL_DMEVENTD
		echo "ok"
	else
		echo -n "## using system dmeventd (pid $RUNNING_DMEVENTD)..."
		echo "ok"
	fi
	lvmconf "activation/monitoring = 1"
}

teardown_devs() {
	return 0
}

teardown_devs_prefixed() {
	return 0
}

teardown() {
	echo -n "## teardown..."
	unset LVM_LOG_FILE_EPOCH

	if [[ -f TESTNAME ]] && [[ ! -f SKIP_THIS_TEST ]]; then
		kill_tagged_processes 2>/dev/null || true

		# Re-enable any devices left offline
		if [[ -f DISABLED_DEVICES ]]; then
			while read -r _dev; do
				[[ -n "$_dev" ]] || continue
				local _bdev
				_bdev=$(basename "$_dev")
				echo running > "/sys/block/$_bdev/device/state" 2>/dev/null || true
			done < DISABLED_DEVICES
			rm -f DISABLED_DEVICES
		fi

		# Remove any VGs we created
		local _vg
		for _vg in "$vg" "$vg1" "$vg2" "$vg3" "$vg4"; do
			[[ -n "$_vg" ]] || continue
			vgremove -ff "$_vg" 2>/dev/null || true
		done

		# Stop dmeventd if we started it
		kill_sleep_kill_ LOCAL_DMEVENTD 0

		# Wipe device headers
		if [[ -f DEVICES ]]; then
			local IFS=$'\n'
			local _devs=( $(< DEVICES) )
			for d in "${_devs[@]}"; do
				[[ -n "$d" ]] || continue
				dd if=/dev/zero of="$d" bs=32k count=1 2>/dev/null || true
				wipefs -a "$d" 2>/dev/null || true
			done
		fi
	fi

	echo "ok"

	if [[ -n "${TESTDIR-}" ]]; then
		cd "$TESTOLDPWD" || die "Failed to enter $TESTOLDPWD"
		rm -rf "${TESTDIR:?}" || true
	fi
}

disable_dev() {
	local dev
	local error=""

	while [[ "$1" = "--error" ]]; do
		error=1
		shift
	done

	for dev in "$@"; do
		local bdev
		bdev=$(basename "$dev")
		local state_path="/sys/block/$bdev/device/state"
		if [[ ! -f "$state_path" ]]; then
			skip "disable_dev: no sysfs device state for $dev"
		fi
		echo "Disabling device $dev (offline)"
		echo offline > "$state_path"
		echo "$dev" >> DISABLED_DEVICES
	done
}

enable_dev() {
	local dev

	for dev in "$@"; do
		local bdev
		bdev=$(basename "$dev")
		local state_path="/sys/block/$bdev/device/state"
		echo "Enabling device $dev (running)"
		echo running > "$state_path"
		if [[ -f DISABLED_DEVICES ]]; then
			sed -i "\|^${dev}$|d" DISABLED_DEVICES
		fi
	done
}

delay_dev() {
	skip "delay_dev requires DM devices (not available on real devices)"
}

error_dev() {
	local devs=()
	for arg in "$@"; do
		[[ "$arg" =~ ^[0-9]+:[0-9]+$ ]] || devs+=("$arg")
	done
	disable_dev --error "${devs[@]}"
}

prepare_scsi_debug_dev() {
	skip "scsi_debug not available in cluster shell mode"
}

mdadm_create() {
	which mdadm >/dev/null || skip "mdadm tool is missing!"

	cleanup_md_dev
	rm -f debug.log strace.log

	local devid
	for devid in {127..150}; do
		grep -q "md${devid}" /proc/mdstat || break
	done
	[[ "$devid" -lt "150" ]] || skip "Cannot find free /dev/mdXXX node!"
	local mddev=/dev/md${devid}

	mdadm --create "$mddev" "$@" || {
		mdadm --stop "$mddev" || true
		udev_wait
		for arg in "$@"; do
			[[ "$arg" == /dev/* ]] && mdadm --zero-superblock "$arg" 2>/dev/null || true
		done
		udev_wait
		skip "Test skipped, unreliable mdadm detected!"
	}

	for i in {10..0}; do
		[[ -e "$mddev" ]] && break
		echo "Waiting for $mddev."
		sleep .5
	done

	[[ -b "$mddev" ]] || skip "mdadm has not created device!"
	echo "$mddev" > MD_DEV
	echo "$mddev" > MD_DEV_PV

	# aux_t_defs.sh only writes PREFIX-matched args to MD_DEVICES; real device
	# paths like /dev/sda don't carry the test prefix and are excluded.
	rm -f MD_DEVICES
	local arg
	for arg in "$@"; do
		[[ "$arg" == /dev/* ]] && echo "$arg" >> MD_DEVICES
	done
}
