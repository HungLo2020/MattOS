#!/bin/bash
#
# cluster-test-helpers.sh - Helper functions for cluster tests
#
# This file provides assertion and verification functions for testing
# LV locking in shared VGs. It is automatically deployed to all cluster
# nodes by the lvmtest framework.
#

#
# Helper function to print error with file and line number
#
_error_with_location() {
    local msg="$1"
    local caller_line="${BASH_LINENO[1]}"
    local caller_file="${BASH_SOURCE[2]}"

    # Extract just the filename from the full path
    caller_file="$(basename "$caller_file")"

    echo "ERROR: $msg" >&2
    echo "  at $caller_file:$caller_line" >&2
}

#
# assert_one_success - Verify exactly one node succeeded
#
# Usage: assert_one_success
#
# Verifies that exactly one node succeeded in a nodep operation and all
# others failed. This is the expected outcome for exclusive lock races.
#
assert_one_success() {
    if [ "$NODEP_SUCCESS_COUNT" -ne 1 ]; then
        _error_with_location "Expected exactly 1 success, got $NODEP_SUCCESS_COUNT"
        echo "  Success nodes: $NODEP_SUCCESS_NODES" >&2
        echo "  Fail nodes: $NODEP_FAIL_NODES" >&2
        exit 1
    fi

    if [ -z "$NODEP_SINGLE_SUCCESS_NODE" ]; then
        _error_with_location "NODEP_SINGLE_SUCCESS_NODE not set"
        exit 1
    fi

    return 0
}

#
# assert_node_success - Verify specific node succeeded and others failed
#
# Usage: assert_node_success <node_num>
#
# Verifies that the specified node succeeded in a nodep operation and all
# others failed. This is used when testing operations that should only succeed
# on a specific node (e.g., the node that already has the LV active).
#
assert_node_success() {
    local expected_node="$1"

    if [ -z "$expected_node" ]; then
        _error_with_location "assert_node_success requires node number argument"
        exit 1
    fi

    if [ "$NODEP_SUCCESS_COUNT" -ne 1 ]; then
        _error_with_location "Expected exactly 1 success (node $expected_node), got $NODEP_SUCCESS_COUNT"
        echo "  Success nodes: $NODEP_SUCCESS_NODES" >&2
        echo "  Fail nodes: $NODEP_FAIL_NODES" >&2
        exit 1
    fi

    if [ "$NODEP_SINGLE_SUCCESS_NODE" -ne "$expected_node" ]; then
        _error_with_location "Expected node $expected_node to succeed, but node $NODEP_SINGLE_SUCCESS_NODE succeeded"
        exit 1
    fi

    return 0
}

#
# assert_all_success - Verify all nodes succeeded
#
# Usage: assert_all_success
#
# Verifies that all nodes succeeded in a nodep operation. This is the
# expected outcome for shared lock activation or non-conflicting operations.
#
assert_all_success() {
    if [ "$NODEP_SUCCESS_COUNT" -ne "$CLUSTER_NUM_NODES" ]; then
        _error_with_location "Expected $CLUSTER_NUM_NODES successes, got $NODEP_SUCCESS_COUNT"
        echo "  Fail nodes: $NODEP_FAIL_NODES" >&2
        exit 1
    fi

    return 0
}

#
# assert_all_fail - Verify all nodes failed
#
# Usage: assert_all_fail
#
# Verifies that all nodes failed in a nodep operation. This is the expected
# outcome when attempting an operation that should fail on all nodes (e.g.,
# exclusive activation when another node already holds exclusive lock).
#
assert_all_fail() {
    if [ "$NODEP_FAIL_COUNT" -ne "$CLUSTER_NUM_NODES" ]; then
        _error_with_location "Expected all $CLUSTER_NUM_NODES to fail, but $NODEP_SUCCESS_COUNT succeeded"
        echo "  Success nodes: $NODEP_SUCCESS_NODES" >&2
        exit 1
    fi

    return 0
}

#
# assert_count - Verify specific success/fail counts
#
# Usage: assert_count <expected_success> <expected_fail> "operation description"
#
# Verifies that exactly the expected number of nodes succeeded and failed.
# Useful for testing partial success scenarios.
#
assert_count() {
    local expected_success="$1"
    local expected_fail="$2"
    local desc="${3:-operation}"

    if [ "$NODEP_SUCCESS_COUNT" -ne "$expected_success" ]; then
        _error_with_location "$desc: Expected $expected_success successes, got $NODEP_SUCCESS_COUNT"
        echo "  Success nodes: $NODEP_SUCCESS_NODES" >&2
        exit 1
    fi

    if [ "$NODEP_FAIL_COUNT" -ne "$expected_fail" ]; then
        _error_with_location "$desc: Expected $expected_fail failures, got $NODEP_FAIL_COUNT"
        echo "  Fail nodes: $NODEP_FAIL_NODES" >&2
        exit 1
    fi

    echo "✓ $desc: $NODEP_SUCCESS_COUNT succeeded, $NODEP_FAIL_COUNT failed"
    return 0
}

#
# verify_lv_active_on - Verify LV is active on specific node
#
# Usage: verify_lv_active_on <node_num> <vg_name> <lv_name>
#
# Verifies that the specified LV is active on the specified node.
#
verify_lv_active_on() {
    local node_num="$1"
    local vg_name="$2"
    local lv_name="$3"

    local output
    output=$(noden ${node_num} lvs --noheadings -o lv_active ${vg_name}/${lv_name})

    if ! echo "$output" | grep -q "active"; then
        _error_with_location "LV ${vg_name}/${lv_name} not active on node $node_num"
        echo "  lvs output: '$output'" >&2
        exit 1
    fi

    echo "✓ LV ${vg_name}/${lv_name} is active on node $node_num"
}

#
# verify_lv_not_active_on - Verify LV is NOT active on specific node
#
# Usage: verify_lv_not_active_on <node_num> <vg_name> <lv_name>
#
# Verifies that the specified LV is NOT active on the specified node.
#
verify_lv_not_active_on() {
    local node_num="$1"
    local vg_name="$2"
    local lv_name="$3"

    local output
    output=$(noden ${node_num} lvs --noheadings -o lv_active ${vg_name}/${lv_name})

    if echo "$output" | grep -q "active"; then
        _error_with_location "LV ${vg_name}/${lv_name} should not be active on node $node_num but it is"
        echo "  lvs output: '$output'" >&2
        exit 1
    fi

    echo "✓ LV ${vg_name}/${lv_name} is not active on node $node_num"
}

#
# wait_for_lv_inactive - Wait for LV to become inactive on a node
#
# Usage: wait_for_lv_inactive <node_num> <vg_name> <lv_name> [timeout]
#
# Waits up to timeout seconds (default 30) for the LV to become inactive.
#
wait_for_lv_inactive() {
    local node_num="$1"
    local vg_name="$2"
    local lv_name="$3"
    local timeout="${4:-30}"

    local elapsed=0
    while [ $elapsed -lt $timeout ]; do
        local output
        output=$(noden ${node_num} lvs --noheadings -o lv_active ${vg_name}/${lv_name})

        if ! echo "$output" | grep -q "active"; then
            echo "✓ LV ${vg_name}/${lv_name} became inactive on node $node_num after ${elapsed}s"
            return 0
        fi
        sleep 1
        elapsed=$((elapsed + 1))
    done

    _error_with_location "Timeout waiting for LV ${vg_name}/${lv_name} to become inactive on node $node_num"
    exit 1
}

#
# verify_lv_has_lock_args - Verify LV has lock_args field set
#
# Usage: verify_lv_has_lock_args <vg_name> <lv_name>
#
# Verifies that an LV has its own lock by checking that lock_args is not empty.
#
verify_lv_has_lock_args() {
    local vg_name="$1"
    local lv_name="$2"

    if [ -z "$vg_name" ] || [ -z "$lv_name" ]; then
        _error_with_location "verify_lv_has_lock_args requires vg_name and lv_name"
        exit 1
    fi

    local lock_args=$(node1 lvs -a -o lock_args --noheadings ${vg_name}/${lv_name} 2>/dev/null | tr -d ' ')

    if [ -z "$lock_args" ]; then
        _error_with_location "LV ${vg_name}/${lv_name} has no lock_args"
        exit 1
    fi

    echo "✓ LV ${vg_name}/${lv_name} has lock_args: $lock_args"
}

#
# verify_lv_no_lock_args - Verify LV has no lock_args
#
# Usage: verify_lv_no_lock_args <vg_name> <lv_name>
#
# Verifies that an LV has no lock of its own (lock_args is empty).
# Child LVs (thin volumes, cache-pools when attached) should have no lock_args.
#
verify_lv_no_lock_args() {
    local vg_name="$1"
    local lv_name="$2"

    if [ -z "$vg_name" ] || [ -z "$lv_name" ]; then
        _error_with_location "verify_lv_no_lock_args requires vg_name and lv_name"
        exit 1
    fi

    local lock_args=$(node1 lvs -a -o lock_args --noheadings ${vg_name}/${lv_name} 2>/dev/null | tr -d ' ')

    if [ -n "$lock_args" ]; then
        _error_with_location "LV ${vg_name}/${lv_name} has lock_args (expect none): $lock_args"
        exit 1
    fi

    echo "✓ LV ${vg_name}/${lv_name} has no lock_args"
}

#
# verify_lv_type - Verify LV has expected segment type
#
# Usage: verify_lv_type <vg_name> <lv_name> <expected_type>
#
# Verifies that an LV has the expected segment type (from lvs -o segtype).
# Common types: linear, striped, mirror, raid1, thin, thin-pool, cache, cache-pool, writecache
#
verify_lv_type() {
    local vg_name="$1"
    local lv_name="$2"
    local expected_type="$3"

    if [ -z "$vg_name" ] || [ -z "$lv_name" ] || [ -z "$expected_type" ]; then
        _error_with_location "verify_lv_type requires vg_name, lv_name, and expected_type"
        exit 1
    fi

    local actual_type=$(node1 lvs -a -o segtype --noheadings ${vg_name}/${lv_name} 2>/dev/null | tr -d ' ')

    if [ "$actual_type" != "$expected_type" ]; then
        _error_with_location "LV ${vg_name}/${lv_name} has type '$actual_type', expected '$expected_type'"
        exit 1
    fi

    echo "✓ LV ${vg_name}/${lv_name} has type '$actual_type'"
}

#
# verify_lv_size - Verify LV has expected size
#
# Usage: verify_lv_size <vg_name> <lv_name> <expected_size>
#
# Verifies that the specified LV has the expected size in megabytes.
# The expected_size is compared against lvs --nosuffix --units m output
# (e.g., "108.00", "200.00"). Uses node1 to query the size.
#
verify_lv_size() {
    local vg_name="$1"
    local lv_name="$2"
    local expected_size="$3"

    if [ -z "$vg_name" ] || [ -z "$lv_name" ] || [ -z "$expected_size" ]; then
        _error_with_location "verify_lv_size requires vg_name, lv_name, and expected_size"
        exit 1
    fi

    local actual_size=$(node1 lvs --noheadings --nosuffix --units m -o lv_size ${vg_name}/${lv_name} 2>/dev/null | tr -d ' ')

    if [ -z "$actual_size" ]; then
        _error_with_location "Could not get size for LV ${vg_name}/${lv_name}"
        exit 1
    fi

    if [ "$actual_size" != "$expected_size" ]; then
        _error_with_location "LV ${vg_name}/${lv_name} size is ${actual_size}m, expected ${expected_size}m"
        exit 1
    fi

    echo "✓ LV ${vg_name}/${lv_name} size is ${actual_size}m"
}

#
# wait_lv_sync_done - Wait for LV RAID sync to complete
#
# Usage: wait_lv_sync_done <node_num> <vg_name> <lv_name> [timeout]
#
# Polls sync_percent on the specified node until it reaches 100.
# Times out after timeout seconds (default 100).
#
wait_lv_sync_done() {
    local node_num="$1"
    local vg_name="$2"
    local lv_name="$3"
    local timeout="${4:-100}"

    for i in $(seq 1 $timeout); do
        local sync=$(noden ${node_num} lvs --noheadings -o sync_percent ${vg_name}/${lv_name} | cut -d. -f1 | tr -d ' ')
        if [ "$sync" = "100" ]; then
            return 0
        fi
        sleep 1
    done

    _error_with_location "Timeout waiting for ${vg_name}/${lv_name} sync on node $node_num"
    exit 1
}

#
# wait_pvmove_done - Wait for pvmove to finish on a node
#
# Usage: wait_pvmove_done <node_num> <vg_name> <lv_name> <src_dev> [timeout]
#
# Polls lvs -o devices until the source device no longer appears,
# indicating the pvmove has completed. Times out after timeout seconds
# (default 30).
#
wait_pvmove_done() {
    local node_num="$1"
    local vg_name="$2"
    local lv_name="$3"
    local src_dev="$4"
    local timeout="${5:-30}"

    # src_dev path may differ across VMs, so use PV UUID to
    # find the device name as seen on the node being queried.
    local src_uuid
    src_uuid=$(node1 pvs --noheadings -o pv_uuid "$src_dev" 2>/dev/null | tr -d ' ')
    local local_dev
    local_dev=$(noden ${node_num} pvs --noheadings -o pv_name -S "pv_uuid=$src_uuid" 2>/dev/null | tr -d ' ')

    for i in $(seq 1 $timeout); do
        local devs
        devs=$(noden ${node_num} lvs --noheadings -o devices ${vg_name}/${lv_name} 2>/dev/null | tr -d ' ')

        if ! echo "$devs" | grep -qF "$local_dev"; then
            echo "✓ pvmove done for ${vg_name}/${lv_name} on node $node_num (devices: $devs)"
            return 0
        fi
        sleep 1
    done

    _error_with_location "Timeout waiting for pvmove of ${vg_name}/${lv_name} on node $node_num"
    exit 1
}

#
# cleanup_vg - Stop lockspaces and remove VG with retry
#
# Usage: cleanup_vg <vg_name>
#
# Stops the VG lockspace on all nodes except node1, then retries
# vgremove on node1 until it succeeds (up to 20 seconds).
#
cleanup_vg() {
    local vgname="$1"

    if [ -z "$vgname" ]; then
        _error_with_location "cleanup_vg requires vg_name argument"
        exit 1
    fi

    for node_num in $(seq 2 $CLUSTER_NUM_NODES); do
        noden ${node_num} vgchange --lockstop $vgname || true
    done
    sleep 1

    local i
    for i in $(seq 1 20); do
        node1 vgremove -f $vgname && return 0
        sleep 1
    done

    _error_with_location "cleanup_vg: vgremove -f $vgname failed after 20 attempts"
    exit 1
}

# Adds --persist stop to cleanup_vg.
cleanup_vg_pr() {
    local vgname="$1"

    if [ -z "$vgname" ]; then
        _error_with_location "cleanup_vg_pr requires vg_name argument"
        exit 1
    fi

    for node_num in $(seq 2 $CLUSTER_NUM_NODES); do
        noden ${node_num} vgchange --lockstop --persist stop $vgname || true
    done
    sleep 1

    local i
    for i in $(seq 1 20); do
        node1 vgremove -f $vgname && return 0
        sleep 1
    done

    _error_with_location "cleanup_vg_pr: vgremove -f $vgname failed after 20 attempts"
    exit 1
}

#
# verify_lv_lock_in_lvmlockd - Verify LV lock is held in lvmlockd
#
# Usage: verify_lv_lock_in_lvmlockd <node_num> <vg_name> <lv_name>
#
# Checks that lvmlockctl -i on the specified node shows a lock entry
# for the LV's UUID.
#
verify_lv_lock_in_lvmlockd() {
    local node_num="$1"
    local vg_name="$2"
    local lv_name="$3"

    local lv_uuid
    lv_uuid=$(noden ${node_num} lvs --noheadings -o uuid ${vg_name}/${lv_name} | tr -d ' ')

    if [ -z "$lv_uuid" ]; then
        _error_with_location "Could not get UUID for LV ${vg_name}/${lv_name} on node $node_num"
        exit 1
    fi

    local output
    output=$(noden ${node_num} lvmlockctl -i 2>&1)

    if ! echo "$output" | grep -q "$lv_uuid"; then
        _error_with_location "LV lock for ${vg_name}/${lv_name} not found in lvmlockd on node $node_num"
        echo "  LV UUID: $lv_uuid" >&2
        exit 1
    fi

    echo "✓ LV ${vg_name}/${lv_name} lock in lvmlockd on node $node_num"
}

#
# verify_lv_lock_in_sanlock - Verify LV lock is held in sanlock
#
# Usage: verify_lv_lock_in_sanlock <node_num> <vg_name> <lv_name>
#
# Checks that sanlock status on the specified node shows a resource
# entry for the LV's UUID (not marked ORPHAN).
#
verify_lv_lock_in_sanlock() {
    local node_num="$1"
    local vg_name="$2"
    local lv_name="$3"

    local lv_uuid
    lv_uuid=$(noden ${node_num} lvs --noheadings -o uuid ${vg_name}/${lv_name} | tr -d ' ')

    if [ -z "$lv_uuid" ]; then
        _error_with_location "Could not get UUID for LV ${vg_name}/${lv_name} on node $node_num"
        exit 1
    fi

    local output
    output=$(noden ${node_num} sanlock status 2>&1)

    if ! echo "$output" | grep "$lv_uuid" | grep -qv "ORPHAN"; then
        _error_with_location "LV lock for ${vg_name}/${lv_name} not found in sanlock on node $node_num"
        echo "  LV UUID: $lv_uuid" >&2
        exit 1
    fi

    echo "✓ LV ${vg_name}/${lv_name} lock in sanlock on node $node_num"
}

#
# verify_lv_orphan_in_sanlock - Verify LV lock is orphaned in sanlock
#
# Usage: verify_lv_orphan_in_sanlock <node_num> <vg_name> <lv_name>
#
# Checks that sanlock status on the specified node shows an ORPHAN
# resource entry for the LV's UUID.
#
verify_lv_orphan_in_sanlock() {
    local node_num="$1"
    local vg_name="$2"
    local lv_name="$3"

    local lv_uuid
    lv_uuid=$(noden ${node_num} lvs --noheadings -o uuid ${vg_name}/${lv_name} | tr -d ' ')

    if [ -z "$lv_uuid" ]; then
        _error_with_location "Could not get UUID for LV ${vg_name}/${lv_name} on node $node_num"
        exit 1
    fi

    local output
    output=$(noden ${node_num} sanlock status 2>&1)

    if ! echo "$output" | grep "$lv_uuid" | grep -q "ORPHAN"; then
        _error_with_location "LV orphan lock for ${vg_name}/${lv_name} not found in sanlock on node $node_num"
        echo "  LV UUID: $lv_uuid" >&2
        exit 1
    fi

    echo "✓ LV ${vg_name}/${lv_name} orphan lock in sanlock on node $node_num"
}

#
# verify_vg_orphan_in_sanlock - Verify VG lock is orphaned in sanlock
#
# Usage: verify_vg_orphan_in_sanlock <node_num> <vg_name>
#
# Checks that sanlock status on the specified node shows an ORPHAN
# resource entry for the VG lock (resource name VGLK).
#
verify_vg_orphan_in_sanlock() {
    local node_num="$1"
    local vg_name="$2"

    local output
    output=$(noden ${node_num} sanlock status 2>&1)

    if ! echo "$output" | grep "VGLK" | grep -q "ORPHAN"; then
        _error_with_location "VG lock orphan for ${vg_name} not found in sanlock on node $node_num"
        exit 1
    fi

    echo "✓ VG ${vg_name} orphan lock in sanlock on node $node_num"
}

# refresh_devices - Re-discover device paths on all nodes by WWN
#
# Call after test steps that may cause NVMe-oF or iSCSI reconnection
# (e.g., PR fencing that triggers I/O errors and host recovery), which
# can renumber device paths (/dev/nvme0nX, /dev/sdX).
#
# Re-maps devices by matching WWNs from the initial discovery, updates
# $dev1/$dev2/etc., CLUSTER_DEV_MAP, and /root/cluster-dev-map.sh on
# each node.
#
refresh_devices() {
    local num_nodes="${CLUSTER_NUM_NODES}"
    local num_devs="${CLUSTER_DEV_COUNT}"
    local num_mpath="${CLUSTER_NUM_MULTIPATH:-0}"
    local num_scsi="${CLUSTER_NUM_SCSI:-0}"
    local num_nvme="${CLUSTER_NUM_NVME:-0}"

    for node_num in $(seq 1 "$num_nodes"); do
        # Wait for expected devices to appear (e.g. after watchdog reboot)
        if [ "$num_mpath" -gt 0 ]; then
            local attempt=0
            while [ $attempt -lt 60 ]; do
                local mpath_count=$(cluster_node_exec "$node_num" "multipath -ll 2>/dev/null | grep -c '^mpath'" || echo 0)
                [ "$mpath_count" -ge "$num_mpath" ] && break
                echo "refresh_devices: waiting for multipath devices on node $node_num ($mpath_count/$num_mpath)" >&2
                sleep 2
                attempt=$((attempt + 1))
            done
            if [ "$mpath_count" -lt "$num_mpath" ]; then
                echo "refresh_devices: only $mpath_count/$num_mpath multipath devices on node $node_num after 120s" >&2
                return 1
            fi
        fi

        # Build script to get all device WWNs in one SSH call
        local wwn_script='{ lsblk -d -n -o NAME,WWN 2>/dev/null; multipath -ll 2>/dev/null | sed -n '"'"'s/^\([a-z][a-z0-9]*\) (\([^)]*\)).*/\1 \2/p'"'"'; }'
        local wwn_output
        wwn_output=$(cluster_node_exec "$node_num" "$wwn_script")

        local dev_map_script="#!/bin/bash
"
        local scsi_count=0 nvme_count=0 mpath_count=0

        for i in $(seq 0 $((num_devs - 1))); do
            local wwn_var="CLUSTER_DEV_WWN_${i}"
            local target_wwn="${!wwn_var}"
            local new_path=""

            if [[ "$target_wwn" =~ ^uuid\. ]]; then
                # NVMe: WWN format is "uuid.XXXX", lsblk reports same
                new_path=$(echo "$wwn_output" | awk -v w="$target_wwn" '$2 == w {print "/dev/" $1; exit}')
            elif [[ "$target_wwn" =~ ^0x ]]; then
                # SCSI: WWN format is "0xXXXX"
                new_path=$(echo "$wwn_output" | awk -v w="$target_wwn" '$2 == w {print "/dev/" $1; exit}')
            else
                # Multipath or other: match by WWID
                new_path=$(echo "$wwn_output" | awk -v w="$target_wwn" '$2 == w {print "/dev/mapper/" $1; exit}')
            fi

            if [ -z "$new_path" ]; then
                echo "refresh_devices: WWN $target_wwn not found on node $node_num" >&2
                return 1
            fi

            local dev_num=$((i + 1))
            export "CLUSTER_DEV_MAP_${node_num}_${i}=${new_path}"
            dev_map_script+="export dev${dev_num}='${new_path}'
"

            # Type-specific aliases
            if [[ "$target_wwn" =~ ^uuid\. ]]; then
                nvme_count=$((nvme_count + 1))
                dev_map_script+="export nvme${nvme_count}='${new_path}'
"
            elif [[ "$new_path" =~ ^/dev/mapper/mpath ]]; then
                mpath_count=$((mpath_count + 1))
                dev_map_script+="export mpath${mpath_count}='${new_path}'
"
            else
                scsi_count=$((scsi_count + 1))
                dev_map_script+="export scsi${scsi_count}='${new_path}'
"
            fi

            # Update host-side variables for node 1
            if [ "$node_num" -eq 1 ]; then
                export "dev${dev_num}=${new_path}"
            fi
        done

        # Add DEVICES array
        dev_map_script+="export DEVICES=("
        for i in $(seq 0 $((num_devs - 1))); do
            local map_var="CLUSTER_DEV_MAP_${node_num}_${i}"
            dev_map_script+="'${!map_var}' "
        done
        dev_map_script+=")
"

        # Deploy updated mapping to node
        echo "$dev_map_script" | cluster_node_exec "$node_num" \
            "cat > /root/cluster-dev-map.sh && chmod +x /root/cluster-dev-map.sh" > /dev/null
    done
}

# wait_host_status_known - Sleep until sanlock host status transitions
# from UNKNOWN to KNOWN.  After lockspace start, sanlock needs two
# check_other_leases cycles (each 2*io_timeout seconds) before it can
# distinguish a live host from a dead one.
#
# Usage: wait_host_status_known
#
wait_host_status_known() {
    local io_timeout="${CLUSTER_SANLOCK_IO_TIMEOUT:-10}"
    local wait_secs=$(( 4 * io_timeout + 2 ))
    echo "Waiting ${wait_secs}s for sanlock host status to become KNOWN (io_timeout=$io_timeout)"
    sleep "$wait_secs"
}

# Export all functions so they're available in test scripts
export -f refresh_devices
export -f cleanup_vg
export -f assert_one_success
export -f assert_node_success
export -f assert_all_success
export -f assert_all_fail
export -f assert_count
export -f verify_lv_active_on
export -f verify_lv_not_active_on
export -f wait_for_lv_inactive
export -f verify_lv_has_lock_args
export -f verify_lv_no_lock_args
export -f verify_lv_type
export -f verify_lv_size
export -f wait_lv_sync_done
export -f wait_pvmove_done
export -f verify_lv_lock_in_lvmlockd
export -f verify_lv_lock_in_sanlock
export -f verify_lv_orphan_in_sanlock
export -f verify_vg_orphan_in_sanlock
export -f wait_host_status_known
