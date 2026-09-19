#!/bin/bash
#
# lock-recovery-wd.sh - Test lock recovery using the sanlock watchdog timeout
#
# Tests the default lock recovery mechanism where sanlock uses
# host_dead_seconds to determine when a failed host's locks can
# be taken by another host.  This is the behavior when no
# --setlockargs is specified, or equivalently with
# --setlockargs timeout,nopersist.
#
# Recovery process:
# 1. host A owns a lock
# 2. host A fails (sanlock and lvmlockd killed)
# 3. wdmd on host A detects sanlock failure, stops petting watchdog
# 4. watchdog fires, host A machine resets
# 5. host B requests the lock
# 6. host B request fails because A still owns the lock
# 7. after host_dead_seconds, sanlock considers host A dead
# 8. host B retries the lock, which is now granted by sanlock
#
# Node failure is simulated by killing both sanlock and lvmlockd.
# Killing only lvmlockd is insufficient because sanlock would
# continue renewing the delta lease, so the host would never
# be seen as failed.
#
# The watchdog must be enabled so that the failed node is reset,
# ensuring its I/O has stopped before another host takes the lock.
# host_dead_seconds includes watchdog_fire_timeout, so the failed
# host is guaranteed to have been reset before locks are granted
# to another host.  After the watchdog resets the failed node,
# it reboots and must be waited for before cleanup.
#

set -e

LOCKD_SERVICE="lvmlockd"

if [ "$CLUSTER_NUM_NODES" -lt 2 ]; then
    echo "SKIP: This test requires at least 2 nodes (found $CLUSTER_NUM_NODES)"
    exit 0
fi

#
# Simulate a node failure by killing both sanlock and lvmlockd.
# After killall, wdmd detects sanlock is gone and stops petting
# the watchdog.  The watchdog fires after ~60s, resetting the
# machine.  Post-kill cleanup commands run immediately (within
# the watchdog window) while the node is still up.
#
kill_node() {
    local nodenum=$1
    noden ${nodenum} killall -9 lvmlockd sanlock
    sleep 1
    echo "Killed sanlock and lvmlockd on node ${nodenum}"
    noden ${nodenum} systemctl reset-failed sanlock || true
    noden ${nodenum} systemctl reset-failed $LOCKD_SERVICE || true
    noden ${nodenum} dmsetup remove testvg-lvmlock 2>/dev/null || true
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


# ---------------------------------------------------------------
# Test 1: lock recovery after node failure
#
# The core recovery test:
# - node1 activates LV (holds ex lock)
# - kill sanlock and lvmlockd on node1 (simulates failure)
# - watchdog resets node1, sanlock on node2 sees node1 as dead
# - node2 acquires the lock and activates LV
# ---------------------------------------------------------------

echo "== Test 1: lock recovery after node failure =="

node1 vgcreate --shared testvg $dev1 $dev2
nodep vgchange --lockstart testvg

node1 lvcreate -l5 -n lv1 testvg
verify_lv_active_on 1 testvg lv1

# node2 cannot activate while node1 holds the lock
node2 not lvchange -ay testvg/lv1

kill_node 1

# node2 retries activation until sanlock considers node1 dead
lvchange_ay_loop 2 testvg lv1

verify_lv_active_on 2 testvg lv1

# Verify node2 holds the lock
node2 lvchange -an testvg/lv1
verify_lv_not_active_on 2 testvg lv1

# Restart node1 and clean up
restart_node 1
node1 vgchange --lockstart testvg
node1 dmsetup remove testvg-lv1 2>/dev/null || true

node1 lvremove -y testvg/lv1
cleanup_vg testvg

echo "== Test 1 passed =="


# ---------------------------------------------------------------
# Test 2: lock recovery for VG lock (metadata operations)
#
# Tests recovery of the VG lock which is needed for metadata
# operations (lvcreate, lvremove, etc).
# - node1 runs lvcreate under gdb, paused at lv_create_single
#   while holding the VG EX lock
# - kill sanlock and lvmlockd on node1 (simulates failure with
#   VG lock held)
# - node2 should be able to perform metadata operations after
#   sanlock considers node1 dead
# ---------------------------------------------------------------

echo "== Test 2: lock recovery for VG lock =="

node1 vgcreate --shared testvg $dev1 $dev2
nodep vgchange --lockstart testvg

node1 lvcreate -l5 -n lv1 -an testvg

# Run lvcreate under gdb, break at lv_create_single (VG lock held),
# kill both lvmlockd and sanlock to simulate node failure while VG
# lock is held, then kill lvcreate and exit gdb.
noden 1 "gdb -batch -ex 'set confirm off' -ex 'break lv_create_single' -ex run -ex 'shell killall -9 lvmlockd sanlock' -ex kill -ex quit --args lvcreate -l5 -n lv2 -an testvg 2>&1 || true"
sleep 1
noden 1 systemctl reset-failed sanlock || true
noden 1 systemctl reset-failed $LOCKD_SERVICE || true
noden 1 dmsetup remove testvg-lvmlock 2>/dev/null || true

# node2 should be able to create an LV after lock recovery
# (requires acquiring VG lock held by failed node1)
timeout=200
for i in $(seq 1 $timeout); do
    if node2 lvcreate -l5 -n lv3 -an testvg 2>&1; then
        echo "VG lock recovery succeeded after ${i}s"
        break
    fi
    if [ "$i" -eq "$timeout" ]; then
        echo "ERROR: VG lock recovery did not succeed within ${timeout}s"
        node2 lvmlockctl -id 2>/dev/null || true
        node2 sanlock client status -D 2>/dev/null || true
        exit 1
    fi
    sleep 1
done

# Clean up
restart_node 1
node1 vgchange --lockstart testvg
node1 lvremove -y testvg/lv1 testvg/lv3 || true
cleanup_vg testvg

echo "== Test 2 passed =="


# ---------------------------------------------------------------
# Test 3: lock recovery with multiple LVs
#
# Test that recovery works when node1 holds locks on multiple LVs.
# After node1 fails, node2 should be able to activate each LV.
# ---------------------------------------------------------------

echo "== Test 3: lock recovery with multiple LVs =="

node1 vgcreate --shared testvg $dev1 $dev2
nodep vgchange --lockstart testvg

node1 lvcreate -l3 -n lv1 testvg
node1 lvcreate -l3 -n lv2 testvg

verify_lv_active_on 1 testvg lv1
verify_lv_active_on 1 testvg lv2

node2 not lvchange -ay testvg/lv1
node2 not lvchange -ay testvg/lv2

kill_node 1

lvchange_ay_loop 2 testvg lv1
verify_lv_active_on 2 testvg lv1

# lv2 recovery should also work (may already be done)
lvchange_ay_loop 2 testvg lv2
verify_lv_active_on 2 testvg lv2

node2 lvchange -an testvg/lv1
node2 lvchange -an testvg/lv2

restart_node 1
node1 vgchange --lockstart testvg
node1 dmsetup remove testvg-lv1 2>/dev/null || true
node1 dmsetup remove testvg-lv2 2>/dev/null || true
node1 lvremove -y testvg/lv1 testvg/lv2
cleanup_vg testvg

echo "== Test 3 passed =="


# ---------------------------------------------------------------
# Test 4: explicit --setlockargs timeout,nopersist
#
# Verify that --setlockargs timeout,nopersist is equivalent to
# the default (no --setlockargs).  Recovery should work the same
# way through sanlock host_dead_seconds.
# ---------------------------------------------------------------

echo "== Test 4: --setlockargs timeout,nopersist =="

node1 vgcreate --shared testvg $dev1 $dev2
node1 vgchange --lockstart testvg

# Retry: sanlock may not have host info yet after lockspace just started
for i in $(seq 1 10); do
	node1 vgchange --setlockargs timeout,nopersist testvg && break
	sleep 1
done

node1 vgs -o lockargs --noheadings testvg | grep '1.0.0'

node1 vgchange --lockstart testvg
node2 vgchange --lockstart testvg

node1 lvcreate -l5 -n lv1 testvg
verify_lv_active_on 1 testvg lv1

node2 not lvchange -ay testvg/lv1

kill_node 1

lvchange_ay_loop 2 testvg lv1
verify_lv_active_on 2 testvg lv1

node2 lvchange -an testvg/lv1

restart_node 1
node1 vgchange --lockstart testvg
node1 dmsetup remove testvg-lv1 2>/dev/null || true

node1 lvremove -y testvg/lv1
cleanup_vg testvg

echo "== Test 4 passed =="


# ---------------------------------------------------------------
# Test 5: lock recovery with two failed nodes
#
# Three nodes each hold locks on two LVs.  Kill node1 and node2,
# verify node3 can activate all four LVs after both nodes are
# recovered through sanlock host_dead_seconds.
# Requires at least 3 cluster nodes.
# ---------------------------------------------------------------

if [ "$CLUSTER_NUM_NODES" -ge 3 ]; then

echo "== Test 5: lock recovery with two failed nodes =="

node1 vgcreate --shared testvg $dev1 $dev2
nodep vgchange --lockstart testvg

node1 lvcreate -l2 -n lv1a testvg
node1 lvcreate -l2 -n lv1b testvg
node2 lvcreate -l2 -n lv2a -an testvg
node2 lvcreate -l2 -n lv2b -an testvg
node3 lvcreate -l2 -n lv3a -an testvg
node3 lvcreate -l2 -n lv3b -an testvg

node2 lvchange -ay testvg/lv2a
node2 lvchange -ay testvg/lv2b
node3 lvchange -ay testvg/lv3a
node3 lvchange -ay testvg/lv3b

verify_lv_active_on 1 testvg lv1a
verify_lv_active_on 1 testvg lv1b
verify_lv_active_on 2 testvg lv2a
verify_lv_active_on 2 testvg lv2b
verify_lv_active_on 3 testvg lv3a
verify_lv_active_on 3 testvg lv3b

# Kill both node1 and node2
kill_node 1
kill_node 2

# node3 recovers LVs from node1
lvchange_ay_loop 3 testvg lv1a
verify_lv_active_on 3 testvg lv1a

lvchange_ay_loop 3 testvg lv1b
verify_lv_active_on 3 testvg lv1b

# node3 recovers LVs from node2
lvchange_ay_loop 3 testvg lv2a
verify_lv_active_on 3 testvg lv2a

lvchange_ay_loop 3 testvg lv2b
verify_lv_active_on 3 testvg lv2b

# node3's own LVs should still be active
verify_lv_active_on 3 testvg lv3a
verify_lv_active_on 3 testvg lv3b

# Deactivate all LVs on node3
node3 lvchange -an testvg/lv1a testvg/lv1b testvg/lv2a testvg/lv2b testvg/lv3a testvg/lv3b

# Restart failed nodes and clean up
restart_node 1
restart_node 2
node1 vgchange --lockstart testvg
node2 vgchange --lockstart testvg
node1 dmsetup remove testvg-lv1a 2>/dev/null || true
node1 dmsetup remove testvg-lv1b 2>/dev/null || true
node2 dmsetup remove testvg-lv2a 2>/dev/null || true
node2 dmsetup remove testvg-lv2b 2>/dev/null || true
node3 lvremove -y testvg/lv1a testvg/lv1b testvg/lv2a testvg/lv2b testvg/lv3a testvg/lv3b
cleanup_vg testvg

echo "== Test 5 passed =="

fi # CLUSTER_NUM_NODES >= 3

exit 0
