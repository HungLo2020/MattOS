#!/bin/bash
#
# lock-recovery-persist-timeout.sh - Test lock recovery with persist,timeout
#
# Tests the --setlockargs persist,timeout feature which provides two
# independent recovery mechanisms:
# 1. PR fencing (fast path): lvmlockd removes failed host's PR key
# 2. Watchdog timeout (fallback): sanlock grants lock after host_dead_seconds
#
# With persist,timeout, PR fencing is attempted first.  If it succeeds,
# lock recovery is fast (~host_fail_timeout).  If PR fencing fails (e.g.
# due to a missing device), the lock is still recovered after
# host_dead_seconds when the watchdog has guaranteed the failed host
# is reset.
#
# Unlike lock-recovery-pr.sh (persist,notimeout), the watchdog is
# NOT disabled here.  Both PR and watchdog mechanisms are active,
# and the watchdog will reset any node whose sanlock is killed.
#

set -e

LOCKD_SERVICE="lvmlockd"

if [ "$CLUSTER_NUM_NODES" -lt 2 ]; then
    echo "SKIP: This test requires at least 2 nodes (found $CLUSTER_NUM_NODES)"
    exit 0
fi

#
# Simulate a node failure by killing both sanlock and lvmlockd.
# Both must be killed so that sanlock stops renewing the delta
# lease and the host eventually enters FAIL state as seen by
# other nodes.  The watchdog will fire and reset the node.
#
# Optional VG names can be passed to remove lockspace DM devices.
# Defaults to "testvg" if none specified.
#
kill_node() {
    local nodenum=$1
    shift
    local vg
    noden ${nodenum} killall -9 lvmlockd sanlock
    sleep 1
    echo "Killed sanlock and lvmlockd on node ${nodenum}"
    noden ${nodenum} systemctl reset-failed sanlock || true
    noden ${nodenum} systemctl reset-failed $LOCKD_SERVICE || true
    if [ $# -eq 0 ]; then
        noden ${nodenum} dmsetup remove testvg-lvmlock 2>/dev/null || true
    else
        for vg in "$@"; do
            noden ${nodenum} dmsetup remove ${vg}-lvmlock 2>/dev/null || true
        done
    fi
}

#
# Wait for a node to become reachable via SSH after a watchdog
# reset and reboot.
#
wait_for_node() {
    local nodenum=$1
    local timeout="${2:-180}"

    for i in $(seq 1 $timeout); do
        if noden ${nodenum} true 2>/dev/null; then
            echo "Node ${nodenum} reachable after ${i}s"
            return 0
        fi
        sleep 1
    done

    echo "ERROR: Node ${nodenum} not reachable within ${timeout}s"
    exit 1
}

#
# Wait for the watchdog to fire and reset the node.  Compares
# boot_id to detect the reboot, which is reliable even when the
# VM resets almost instantly and the SSH-unreachable window is
# too brief to catch by polling.
#
# Usage: save boot_id before kill, pass it as $2:
#   BOOT1=$(noden 1 cat /proc/sys/kernel/random/boot_id)
#   kill_node 1
#   ...
#   wait_for_reboot 1 "$BOOT1"
#
wait_for_reboot() {
    local nodenum=$1
    local old_boot_id=$2
    local timeout="${3:-180}"

    echo "Waiting for watchdog to reset node ${nodenum}..."
    for i in $(seq 1 $timeout); do
        local new_boot_id
        new_boot_id=$(noden ${nodenum} cat /proc/sys/kernel/random/boot_id 2>/dev/null) || true
        if [ -n "$new_boot_id" ] && [ "$new_boot_id" != "$old_boot_id" ]; then
            echo "Node ${nodenum} rebooted after ${i}s"
            return 0
        fi
        sleep 1
    done

    echo "ERROR: Node ${nodenum} did not reboot within ${timeout}s"
    exit 1
}

#
# Restart sanlock and lvmlockd after a simulated failure.
# Waits for the node to be reachable first, since the watchdog
# may have reset it.
#
restart_node() {
    local nodenum=$1
    wait_for_node ${nodenum}
    refresh_devices
    noden ${nodenum} systemctl start sanlock
    noden ${nodenum} systemctl start $LOCKD_SERVICE
    echo "Restarted sanlock and lvmlockd on node ${nodenum}"
    sleep 2
}

#
# Wait for lock recovery by retrying lvchange -ay in a loop.
# Uses a 200s timeout to allow for the full host_dead_seconds
# period (~140s with default io_timeout=10).
#
lvchange_ay_loop() {
    local nodenum=$1
    local vgname=$2
    local lvname=$3
    local timeout="${4:-200}"

    for i in $(seq 1 $timeout); do
        if noden ${nodenum} lvchange -ay ${vgname}/${lvname} 2>&1; then
            echo "Lock recovery succeeded after ${i}s"
            return 0
        fi
        sleep 1
    done

    echo "ERROR: Lock recovery did not succeed within ${timeout}s"
    noden ${nodenum} lvmlockctl -id 2>/dev/null || true
    noden ${nodenum} sanlock client status -D 2>/dev/null || true
    exit 1
}

#
# Get the PR key for a node on a VG.
#
get_key() {
    local nodenum=$1
    local vgname=${2:-testvg}
    noden $nodenum vgchange --persist check $vgname | grep "key for local host is registered" | grep -oE '0x[0-9a-fA-F]+'
}

verify_key_removed() {
    local nodenum=$1
    local key=$2
    local vgname=${3:-testvg}
    noden $nodenum lvmpersist read-keys --vg $vgname | tee out
    if grep "^Device" out | grep -q "$key"; then
        _error_with_location "PR key $key still registered on $vgname devices"
        cat out >&2
        exit 1
    fi
    echo "PR key $key removed from $vgname devices"
}


# ---------------------------------------------------------------
# Test 1: verify lock_args metadata for persist,timeout
#
# Create a shared VG with persist,timeout lock_args.
# Verify lock_args version 2.0.0 and flags in VG metadata.
# ---------------------------------------------------------------

echo "== Test 1: vgcreate --setlockargs persist,timeout =="

node1 vgcreate --shared --setpersist y --setlockargs persist,timeout testvg $dev1 $dev2
nodep vgchange --lockstart --persist start testvg

node1 vgs -o+lockargs testvg | grep '2.0.0'
node1 vgs -o lockargs --noheadings testvg | grep 'persist'
node1 vgs -o lockargs --noheadings testvg | not grep 'notimeout'

node1 lvmlockctl -id | grep "testvg" | grep "fence_pr=1"
node1 lvmlockctl -id | grep "testvg" | grep "no_timeout=0"

cleanup_vg_pr testvg

echo "== Test 1 passed =="


# ---------------------------------------------------------------
# Test 2: PR fencing succeeds, watchdog resets target
#
# Both recovery mechanisms active.  PR fencing removes the failed
# host's PR key (fast path recovery).  The watchdog fires and
# resets the failed node afterward.
# ---------------------------------------------------------------

echo "== Test 2: PR fencing succeeds, watchdog resets target =="

node1 vgcreate --shared --setpersist y --setlockargs persist,timeout testvg $dev1 $dev2
nodep vgchange --lockstart --persist start testvg

node1 lvcreate -l5 -n lv1 testvg
verify_lv_active_on 1 testvg lv1

node2 not lvchange -ay testvg/lv1

KEY1=$(get_key 1)
BOOT1=$(noden 1 cat /proc/sys/kernel/random/boot_id)

kill_node 1

lvchange_ay_loop 2 testvg lv1

verify_key_removed 2 $KEY1
verify_lv_active_on 2 testvg lv1

node2 lvchange -an testvg/lv1
verify_lv_not_active_on 2 testvg lv1

wait_for_reboot 1 "$BOOT1"
restart_node 1
node1 vgchange --lockstart --persist start testvg
node1 dmsetup remove testvg-lv1 2>/dev/null || true

node1 lvremove -y testvg/lv1
cleanup_vg_pr testvg

echo "== Test 2 passed =="


# ---------------------------------------------------------------
# Test 3: missing device on fencing node, watchdog fallback
#
# A VG device is missing on the fencing node (node2), so PR
# fencing fails.  The lock is recovered via the timeout mechanism
# after host_dead_seconds, which guarantees the watchdog has
# reset the failed node.
# ---------------------------------------------------------------

echo "== Test 3: missing device, watchdog fallback =="

node1 vgcreate --shared --setpersist y --setlockargs persist,timeout testvg $dev1 $dev2
nodep vgchange --lockstart --persist start testvg

node1 lvcreate -l5 -n lv1 testvg $dev1
verify_lv_active_on 1 testvg lv1

# Populate system.devices on node2 so we can remove dev2 from it.
# Without a devices file, LVM scans all devices; with one missing
# an entry, the device appears as [unknown] and PR fencing fails.
node2 vgimportdevices testvg
node2 lvmdevices --deldev $dev2

BOOT1=$(noden 1 cat /proc/sys/kernel/random/boot_id)

kill_node 1

# PR fencing will fail (missing device).  Recovery falls back to
# the timeout mechanism (~host_dead_seconds).
lvchange_ay_loop 2 testvg lv1

verify_lv_active_on 2 testvg lv1

node2 lvchange -an testvg/lv1
verify_lv_not_active_on 2 testvg lv1

# Restore dev2 before cleanup
node2 lvmdevices --adddev $dev2

wait_for_reboot 1 "$BOOT1"
restart_node 1
node1 vgchange --lockstart --persist start testvg
node1 dmsetup remove testvg-lv1 2>/dev/null || true

node1 lvremove -y testvg/lv1
cleanup_vg_pr testvg

# Remove devices file created by vgimportdevices on node2
node2 rm -f /etc/lvm/devices/system.devices

echo "== Test 3 passed =="


# ---------------------------------------------------------------
# Test 4: multiple LVs, PR recovery
#
# node1 holds locks on two LVs.  Kill node1, verify node2 can
# recover both LVs via PR fencing.  Wait for watchdog reset.
# ---------------------------------------------------------------

echo "== Test 4: multiple LVs, PR recovery =="

node1 vgcreate --shared --setpersist y --setlockargs persist,timeout testvg $dev1 $dev2
nodep vgchange --lockstart --persist start testvg

node1 lvcreate -l3 -n lv1 testvg
node1 lvcreate -l3 -n lv2 testvg

verify_lv_active_on 1 testvg lv1
verify_lv_active_on 1 testvg lv2

node2 not lvchange -ay testvg/lv1
node2 not lvchange -ay testvg/lv2

KEY1=$(get_key 1)
BOOT1=$(noden 1 cat /proc/sys/kernel/random/boot_id)

kill_node 1

lvchange_ay_loop 2 testvg lv1
verify_lv_active_on 2 testvg lv1

verify_key_removed 2 $KEY1

lvchange_ay_loop 2 testvg lv2
verify_lv_active_on 2 testvg lv2

node2 lvchange -an testvg/lv1
node2 lvchange -an testvg/lv2

wait_for_reboot 1 "$BOOT1"
restart_node 1
node1 vgchange --lockstart --persist start testvg
node1 dmsetup remove testvg-lv1 2>/dev/null || true
node1 dmsetup remove testvg-lv2 2>/dev/null || true

node1 lvremove -y testvg/lv1 testvg/lv2
cleanup_vg_pr testvg

echo "== Test 4 passed =="


# ---------------------------------------------------------------
# Test 5: two failed nodes, mixed recovery (3-node, 4 devices)
#
# PR fencing succeeds for one failed node, fails for the other
# (missing device), so the second is recovered via watchdog
# timeout.  Uses two VGs on separate devices so the missing
# device only affects one VG's fencing.
#
# testvg1: dev1 + dev2     -> PR fencing succeeds (all devs visible)
# testvg2: dev3 + dev4     -> PR fencing fails (dev4 missing on node3)
# ---------------------------------------------------------------

if [ "$CLUSTER_NUM_NODES" -ge 3 ] && [ "$CLUSTER_DEV_COUNT" -ge 4 ]; then

echo "== Test 5: two failed nodes, mixed recovery =="

node1 vgcreate --shared --setpersist y --setlockargs persist,timeout testvg1 $dev1 $dev2
node1 vgcreate --shared --setpersist y --setlockargs persist,timeout testvg2 $dev3 $dev4
nodep vgchange --lockstart --persist start testvg1
nodep vgchange --lockstart --persist start testvg2

node1 lvcreate -l5 -n lv1 testvg1
node2 lvcreate -l5 -n lv2 -an testvg2 $dev3
node2 lvchange -ay testvg2/lv2

verify_lv_active_on 1 testvg1 lv1
verify_lv_active_on 2 testvg2 lv2

KEY1=$(get_key 1 testvg1)
KEY2=$(get_key 2 testvg2)

# Populate system.devices on node3 (needs entries for both VGs
# so it can still see testvg1 devices for PR fencing), then
# remove dev4 so PR fencing for testvg2 will fail.
node3 vgimportdevices -a
node3 lvmdevices --deldev $dev4

BOOT1=$(noden 1 cat /proc/sys/kernel/random/boot_id)
BOOT2=$(noden 2 cat /proc/sys/kernel/random/boot_id)

# Kill both nodes
kill_node 1 testvg1 testvg2
kill_node 2 testvg1 testvg2

# node3 recovers testvg1/lv1: PR fencing should succeed
# (testvg1 uses dev1+dev2, both visible on node3)
lvchange_ay_loop 3 testvg1 lv1
verify_key_removed 3 $KEY1 testvg1
verify_lv_active_on 3 testvg1 lv1

# node3 recovers testvg2/lv2: PR fencing fails (dev4 missing),
# falls back to timeout recovery after host_dead_seconds
lvchange_ay_loop 3 testvg2 lv2
verify_lv_active_on 3 testvg2 lv2

node3 lvchange -an testvg1/lv1
node3 lvchange -an testvg2/lv2

# Restore dev4 on node3
node3 lvmdevices --adddev $dev4

# Wait for watchdog to reset both killed nodes
wait_for_reboot 1 "$BOOT1"
wait_for_reboot 2 "$BOOT2"
restart_node 1
restart_node 2

node1 vgchange --lockstart --persist start testvg1
node1 vgchange --lockstart --persist start testvg2
node2 vgchange --lockstart --persist start testvg1
node2 vgchange --lockstart --persist start testvg2

node1 dmsetup remove testvg1-lv1 2>/dev/null || true
node2 dmsetup remove testvg2-lv2 2>/dev/null || true

node1 lvremove -y testvg1/lv1 || true
node1 lvremove -y testvg2/lv2 || true
cleanup_vg_pr testvg2
cleanup_vg_pr testvg1

# Remove devices file created by vgimportdevices on node3
node3 rm -f /etc/lvm/devices/system.devices

echo "== Test 5 passed =="

fi # CLUSTER_NUM_NODES >= 3 && CLUSTER_DEV_COUNT >= 4

exit 0
