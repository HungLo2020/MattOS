#!/bin/bash
#
# lock-recovery-pr.sh - Test lock recovery using persistent reservations
#
# Tests the --setlockargs persist,notimeout feature which uses
# persistent reservations (PR) for lock recovery with sanlock.
#
# Recovery process:
# 1. host A owns a lock
# 2. host A fails (sanlock and lvmlockd killed)
# 3. host B requests the lock
# 4. host B request fails because A owns the lock
# 5. host A stops renewing its delta lease; after host_fail_timeout
#    host A enters the FAIL state in sanlock
# 6. host B retries the lock, sees owner A is failed
# 7. host B runs lvmpersist to remove the PR key of host A
# 8. host B tells sanlock that host A is dead
# 9. host B retries the lock, which is now granted by sanlock
#
# Node failure is simulated by killing both sanlock and lvmlockd.
# Killing only lvmlockd is insufficient because sanlock would
# continue renewing the delta lease, so the host would never
# enter the FAIL state.
#
# The watchdog is disabled (use_watchdog=0) on all nodes before
# running tests so that killing sanlock does not trigger a
# machine reset.  Resetting a machine with the watchdog is
# unnecessary here because node recovery is handled by removing
# persistent reservations.  (A future sanlock update may
# automatically disable watchdog when PR is being used for
# node recovery, so this use_watchdog=0 step would become
# unnecessary.)
#

set -e

LOCKD_SERVICE="lvmlockd"
SANLOCK_CONF="/etc/sanlock/sanlock.conf"

if [ "$CLUSTER_NUM_NODES" -lt 2 ]; then
    echo "SKIP: This test requires at least 2 nodes (found $CLUSTER_NUM_NODES)"
    exit 0
fi

#
# Disable the watchdog on all nodes so that killing sanlock
# does not trigger a machine reset.
#
nodep cp $SANLOCK_CONF ${SANLOCK_CONF}.orig 2>/dev/null || true
nodep "grep -q '^use_watchdog' $SANLOCK_CONF 2>/dev/null && sed -i 's/^use_watchdog.*/use_watchdog = 0/' $SANLOCK_CONF || echo 'use_watchdog = 0' >> $SANLOCK_CONF"
nodep systemctl stop $LOCKD_SERVICE
nodep systemctl stop sanlock
nodep systemctl stop wdmd
sleep 1
nodep systemctl start wdmd
nodep systemctl start sanlock
nodep systemctl start $LOCKD_SERVICE
sleep 1

#
# Simulate a node failure by killing both sanlock and lvmlockd.
# Both must be killed so that sanlock stops renewing the delta
# lease and the host eventually enters FAIL state as seen by
# other nodes.
#
kill_node() {
    local nodenum=$1
    noden ${nodenum} killall -9 lvmlockd sanlock
    sleep 1
    echo "Killed sanlock and lvmlockd on node ${nodenum}"
    noden ${nodenum} systemctl reset-failed sanlock || true
    noden ${nodenum} systemctl reset-failed $LOCKD_SERVICE || true
    noden ${nodenum} dmsetup remove testvg-lvmlock
}

#
# Restart sanlock and lvmlockd after a simulated failure.
#
restart_node() {
    local nodenum=$1
    refresh_devices
    noden ${nodenum} systemctl start sanlock
    noden ${nodenum} systemctl start $LOCKD_SERVICE
    echo "Restarted sanlock and lvmlockd on node ${nodenum}"
    sleep 2
}

lvchange_ay_loop() {
    local nodenum=$1
    local vgname=$2
    local lvname=$3
    local timeout="${4:-120}"

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

lvchange_ay_loop_opts() {
    local nodenum=$1
    local opts=$2
    local vgname=$3
    local lvname=$4
    local timeout="${5:-120}"

    for i in $(seq 1 $timeout); do
        if noden ${nodenum} lvchange -ay ${opts} ${vgname}/${lvname} 2>&1; then
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
# Get the PR key for a node.
# Shared VG PR keys use host_id + sanlock generation:
# 0x100000YYYYYYXXXX (XXXX=host_id, YYYYYY=generation)
#
get_key() {
    noden $1 vgchange --persist check testvg | grep "key for local host is registered" | grep -oE '0x[0-9a-fA-F]+'
}

verify_key_removed() {
    local key=$1
    local nodenum=${2:-2}
    noden $nodenum lvmpersist read-keys --vg testvg | tee out
    if grep "^Device" out | grep -q "$key"; then
        _error_with_location "PR key $key still registered on testvg devices"
        cat out >&2
        exit 1
    fi
    echo "PR key $key removed from testvg devices"
}


# ---------------------------------------------------------------
# Test 1: vgcreate --setlockargs persist,notimeout
#
# Create a shared VG with persist and notimeout lock_args,
# verify lock_args version 2.0.0 and flags in VG metadata.
# ---------------------------------------------------------------

echo "== Test 1: vgcreate --setlockargs persist,notimeout =="

node1 vgcreate --shared --setpersist y --setlockargs persist,notimeout testvg $dev1 $dev2
nodep vgchange --lockstart --persist start testvg

node1 vgs -o+lockargs testvg | grep '2.0.0'
node1 vgs -o lockargs --noheadings testvg | grep 'persist'
node1 vgs -o lockargs --noheadings testvg | grep 'notimeout'

node1 lvcreate -l5 -n lv1 testvg
verify_lv_active_on 1 testvg lv1

node1 lvremove -y testvg/lv1
cleanup_vg_pr testvg

echo "== Test 1 passed =="


# ---------------------------------------------------------------
# Test 2: vgchange --setlockargs persist,notimeout
#
# Create a shared VG without lockargs, then add persist,notimeout
# via vgchange --setlockargs. Verify the lock_args are updated.
# ---------------------------------------------------------------

echo "== Test 2: vgchange --setlockargs persist,notimeout =="

node1 vgcreate --shared --setpersist y testvg $dev1 $dev2
node1 vgchange --persist start testvg

node1 vgs -o+lockargs testvg | grep '1.0.0'

# setlockargs requires no other hosts in lockspace
# (do not lockstart on node2 before setlockargs,
# because setlockargs does not allow other nodes
# to have the VG started.)
node1 vgchange --setlockargs persist,notimeout testvg

# lockspace was stopped by setlockargs, restart it
node1 vgchange --lockstart --persist start testvg

node1 vgs -o+lockargs testvg | grep '2.0.0'
node1 vgs -o lockargs --noheadings testvg | grep 'persist'
node1 vgs -o lockargs --noheadings testvg | grep 'notimeout'

node2 vgchange --lockstart --persist start testvg

node1 lvcreate -l5 -n lv1 testvg
verify_lv_active_on 1 testvg lv1

node1 lvchange -an testvg/lv1
node1 lvremove -y testvg/lv1
cleanup_vg_pr testvg

echo "== Test 2 passed =="


# ---------------------------------------------------------------
# Test 3: setlockargs requires setpersist require
#
# Verify that --setlockargs persist,notimeout fails if the VG
# does not have the persist require setting.
# ---------------------------------------------------------------

echo "== Test 3: setlockargs requires setpersist require =="

node1 vgcreate --shared testvg $dev1 $dev2
node1 vgchange --lockstart testvg

# Without --setpersist y, the VG has no PR require setting.
# setlockargs persist should fail.
node1 not vgchange --setlockargs persist,notimeout testvg

cleanup_vg testvg

echo "== Test 3 passed =="


# ---------------------------------------------------------------
# Test 4: lock recovery after node failure
#
# The core recovery test:
# - node1 activates LV (holds ex lock)
# - kill sanlock and lvmlockd on node1 (simulates failure)
# - node2 attempts to activate LV
# - after host_fail_timeout, sanlock sees node1 as FAIL
# - node2 lvmlockd removes node1's PR key via lvmpersist,
#   tells sanlock node1 is dead, and retries lock acquisition
# - node2 gets the lock and activates LV
# ---------------------------------------------------------------

echo "== Test 4: lock recovery after node failure =="

node1 vgcreate --shared --setpersist y --setlockargs persist,notimeout testvg $dev1 $dev2
nodep vgchange --lockstart --persist start testvg

node1 lvcreate -l5 -n lv1 testvg
verify_lv_active_on 1 testvg lv1

# node2 cannot activate while node1 holds the lock
node2 not lvchange -ay testvg/lv1

KEY1=$(get_key 1)

# Kill sanlock and lvmlockd on node1, simulating a node failure.
# node1 still has the DM device and PR key, but sanlock stops
# renewing the delta lease.  Eventually node2's sanlock will
# see node1 in the FAIL state.
kill_node 1

# node2 attempts to activate the LV.
# After sanlock sees node1 as FAIL, lvmlockd on node2 will
# use lvmpersist to remove node1's PR key, tell sanlock
# that node1 is dead, and retry the lock request.
lvchange_ay_loop 2 testvg lv1

verify_key_removed $KEY1

verify_lv_active_on 2 testvg lv1

# Verify node2 holds the lock
node2 lvchange -an testvg/lv1
verify_lv_not_active_on 2 testvg lv1

# Restart sanlock and lvmlockd on node1 and clean up
restart_node 1
node1 vgchange --lockstart --persist start testvg

# node1's DM device may still be present from before the kill;
# deactivate it if so
node1 dmsetup remove testvg-lv1 2>/dev/null || true

node1 lvremove -y testvg/lv1
cleanup_vg_pr testvg

echo "== Test 4 passed =="


# ---------------------------------------------------------------
# Test 5: lock recovery for VG lock (metadata operations)
#
# Tests recovery of the VG lock which is needed
# for metadata operations (lvcreate, lvremove, etc).
# - node1 runs lvcreate under gdb, paused at lv_create_single
#   while holding the VG EX lock
# - kill sanlock and lvmlockd on node1 (simulates failure with
#   VG lock held)
# - node2 should be able to perform metadata operations after
#   lock recovery fences node1
# ---------------------------------------------------------------

echo "== Test 5: lock recovery for VG lock =="

node1 vgcreate --shared --setpersist y --setlockargs persist,notimeout testvg $dev1 $dev2
nodep vgchange --lockstart --persist start testvg

node1 lvcreate -l5 -n lv1 -an testvg

KEY1=$(get_key 1)

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
timeout=120
for i in $(seq 1 $timeout); do
    if node2 lvcreate -l5 -n lv3 -an testvg 2>&1; then
        echo "VG lock recovery succeeded after ${i}s"
        verify_key_removed $KEY1
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
node1 vgchange --lockstart --persist start testvg
node1 lvremove -y testvg/lv1 testvg/lv3 || true
cleanup_vg_pr testvg

echo "== Test 5 passed =="


# ---------------------------------------------------------------
# Test 6: lock recovery with multiple LVs
#
# Test that recovery works when node1 holds locks on multiple LVs.
# After node1 fails, node2 should be able to activate each LV.
# ---------------------------------------------------------------

echo "== Test 6: lock recovery with multiple LVs =="

node1 vgcreate --shared --setpersist y --setlockargs persist,notimeout testvg $dev1 $dev2
nodep vgchange --lockstart --persist start testvg

node1 lvcreate -l3 -n lv1 testvg
node1 lvcreate -l3 -n lv2 testvg

verify_lv_active_on 1 testvg lv1
verify_lv_active_on 1 testvg lv2

node2 not lvchange -ay testvg/lv1
node2 not lvchange -ay testvg/lv2

KEY1=$(get_key 1)

kill_node 1

lvchange_ay_loop 2 testvg lv1
verify_lv_active_on 2 testvg lv1

verify_key_removed $KEY1

# lv2 recovery should also work (may already be done from same fencing)
lvchange_ay_loop 2 testvg lv2
verify_lv_active_on 2 testvg lv2

node2 lvchange -an testvg/lv1
node2 lvchange -an testvg/lv2

restart_node 1
node1 vgchange --lockstart --persist start testvg
node1 dmsetup remove testvg-lv1 2>/dev/null || true
node1 dmsetup remove testvg-lv2 2>/dev/null || true
node1 lvremove -y testvg/lv1 testvg/lv2
cleanup_vg_pr testvg

echo "== Test 6 passed =="


# ---------------------------------------------------------------
# Test 7: normal operations still work with persist,notimeout
#
# Verify that normal lock acquisition and release works correctly
# when persist,notimeout is configured (no failure scenario).
# ---------------------------------------------------------------

echo "== Test 7: normal operations with persist,notimeout =="

node1 vgcreate --shared --setpersist y --setlockargs persist,notimeout testvg $dev1 $dev2
nodep vgchange --lockstart --persist start testvg

# Normal LV creation and activation
node1 lvcreate -l5 -n lv1 testvg
verify_lv_active_on 1 testvg lv1

# Exclusive lock prevents node2 activation
node2 not lvchange -ay testvg/lv1

# Normal deactivation releases the lock
node1 lvchange -an testvg/lv1
verify_lv_not_active_on 1 testvg lv1

# node2 can now activate
node2 lvchange -ay testvg/lv1
verify_lv_active_on 2 testvg lv1

# And node1 is blocked
node1 not lvchange -ay testvg/lv1

node2 lvchange -an testvg/lv1

# Each node can take turns
for ni in 1 2; do
    noden ${ni} lvchange -ay testvg/lv1
    verify_lv_active_on ${ni} testvg lv1
    noden ${ni} lvchange -an testvg/lv1
    verify_lv_not_active_on ${ni} testvg lv1
done

node1 lvremove -y testvg/lv1
cleanup_vg_pr testvg

echo "== Test 7 passed =="


# ---------------------------------------------------------------
# Test 8: remove persist,notimeout with setlockargs nopersist,timeout
#
# Verify that persist,notimeout can be removed, reverting lock_args
# back to version 1.0.0.
# ---------------------------------------------------------------

echo "== Test 8: remove persist,notimeout =="

node1 vgcreate --shared --setpersist y --setlockargs persist,notimeout testvg $dev1 $dev2
node1 vgchange --persist start testvg

node1 vgs -o lockargs --noheadings testvg | grep '2.0.0'
node1 vgs -o lockargs --noheadings testvg | grep 'persist'

# Remove the lockargs settings (no other hosts)
node1 vgchange --setlockargs nopersist,timeout testvg

node1 vgchange --lockstart --persist start testvg

node1 vgs -o lockargs --noheadings testvg | grep '1.0.0'

node2 vgchange --lockstart --persist start testvg

node1 lvcreate -l5 -n lv1 testvg
verify_lv_active_on 1 testvg lv1
node1 lvchange -an testvg/lv1

node1 lvremove -y testvg/lv1
cleanup_vg_pr testvg

echo "== Test 8 passed =="


# ---------------------------------------------------------------
# Test 9: lvmlockctl -i shows fence_pr and no_timeout state
#
# Verify that lvmlockctl dump info shows the expected fence_pr
# and no_timeout state for the lockspace.
# ---------------------------------------------------------------

echo "== Test 9: lvmlockctl shows fence_pr and no_timeout =="

node1 vgcreate --shared --setpersist y --setlockargs persist,notimeout testvg $dev1 $dev2
nodep vgchange --lockstart --persist start testvg

node1 lvmlockctl -id | grep "testvg" | grep "fence_pr=1"
node1 lvmlockctl -id | grep "testvg" | grep "no_timeout=1"

cleanup_vg_pr testvg

echo "== Test 9 passed =="


# ---------------------------------------------------------------
# Test 10: lock recovery with two failed nodes
#
# Three nodes each hold locks on two LVs.  Kill node1 and node2,
# verify node3 can activate all four LVs after both nodes are
# recovered (fenced via PR key removal).
# Requires at least 3 cluster nodes.
# ---------------------------------------------------------------

if [ "$CLUSTER_NUM_NODES" -ge 3 ]; then

echo "== Test 10: lock recovery with two failed nodes =="

node1 vgcreate --shared --setpersist y --setlockargs persist,notimeout testvg $dev1 $dev2
nodep vgchange --lockstart --persist start testvg

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

KEY1=$(get_key 1)
KEY2=$(get_key 2)

# Kill both node1 and node2
kill_node 1
kill_node 2

# node3 recovers LVs from node1
lvchange_ay_loop 3 testvg lv1a
verify_lv_active_on 3 testvg lv1a
verify_key_removed $KEY1

lvchange_ay_loop 3 testvg lv1b
verify_lv_active_on 3 testvg lv1b

# node3 recovers LVs from node2
lvchange_ay_loop 3 testvg lv2a
verify_lv_active_on 3 testvg lv2a
verify_key_removed $KEY2

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
node1 vgchange --lockstart --persist start testvg
node2 vgchange --lockstart --persist start testvg
node1 dmsetup remove testvg-lv1a 2>/dev/null || true
node1 dmsetup remove testvg-lv1b 2>/dev/null || true
node2 dmsetup remove testvg-lv2a 2>/dev/null || true
node2 dmsetup remove testvg-lv2b 2>/dev/null || true
node3 lvremove -y testvg/lv1a testvg/lv1b testvg/lv2a testvg/lv2b testvg/lv3a testvg/lv3b
cleanup_vg_pr testvg

echo "== Test 10 passed =="

fi # CLUSTER_NUM_NODES >= 3


# ---------------------------------------------------------------
# Test 11: PR fencing fails with missing device, reattach and retry
#
# node2 holds a lock on lv1.  Kill node2 (simulates failure).
# node1 attempts to activate lv1: after host_fail_timeout, sanlock
# sees node2 as FAIL, lvmlockd on node1 attempts PR fencing but
# fails because a VG device is missing on node1.  Activation
# fails.  Reattach the missing device on node1, retry activation,
# and it should succeed because PR fencing can now operate on all
# devices.
# ---------------------------------------------------------------

echo "== Test 11: PR fencing fails with missing device, reattach and retry =="

node1 vgcreate --shared --setpersist y --setlockargs persist,notimeout testvg $dev1 $dev2
nodep vgchange --lockstart --persist start testvg

node2 lvcreate -l5 -n lv1 testvg $dev1
node1 not lvchange -ay testvg/lv1

KEY2=$(get_key 2)

# Remove dev2 from node1 devices file so it appears missing
node1 vgimportdevices testvg
node1 lvmdevices --deldev $dev2

kill_node 2

# After host_fail_timeout, sanlock sees node2 as FAIL.
# node1 attempts PR fencing which fails because dev2 is missing.
sleep 100
node1 not lvchange -ay testvg/lv1

# Reattach the missing device on node1
node1 lvmdevices --adddev $dev2

# lvmlockd caches the failed fence result in fence_history for the
# host_id+generation, so retrying lvchange -ay reuses the cached failure
# without re-attempting PR fencing.  Use --lockopt force to discard the
# cached failure and re-attempt PR fencing.
lvchange_ay_loop_opts 1 "--lockopt force" testvg lv1

verify_key_removed $KEY2 1
verify_lv_active_on 1 testvg lv1

node1 lvchange -an testvg/lv1
verify_lv_not_active_on 1 testvg lv1

# Restart node2 and clean up
restart_node 2
node2 vgchange --lockstart --persist start testvg
node2 dmsetup remove testvg-lv1 2>/dev/null || true

node1 lvremove -y testvg/lv1
node1 rm -f /etc/lvm/devices/system.devices
cleanup_vg_pr testvg

echo "== Test 11 passed =="


# ---------------------------------------------------------------
# Test 12: PR fencing fails with missing device, reboot target node
#
# Begins like Test 11: node2 holds lv1, kill node2, node1 attempts
# activation but PR fencing fails due to missing device.  Instead
# of reattaching the device on node1, reboot node2.  After node2
# restarts, node2 activates lv1, then deactivates it, releasing
# the lock.  node1 then activates lv1 which should succeed because
# the lock is free and no fencing is needed.
# ---------------------------------------------------------------

echo "== Test 12: PR fencing fails with missing device, reboot target node =="

node1 vgcreate --shared --setpersist y --setlockargs persist,notimeout testvg $dev1 $dev2
nodep vgchange --lockstart --persist start testvg

node2 lvcreate -l5 -n lv1 testvg $dev1
node1 not lvchange -ay testvg/lv1

# Remove dev2 from node1 devices file so it appears missing
node1 vgimportdevices testvg
node1 lvmdevices --deldev $dev2

kill_node 2

# After host_fail_timeout, sanlock sees node2 as FAIL.
# node1 attempts PR fencing which fails because dev2 is missing.
sleep 100
node1 not lvchange -ay testvg/lv1

# Restart node2 instead of fixing the missing device on node1
restart_node 2
node2 vgchange --lockstart --persist start testvg
node2 dmsetup remove testvg-lv1 2>/dev/null || true

# node2 activates lv1 (lock is free after restart), then deactivates
node2 lvchange -ay testvg/lv1
verify_lv_active_on 2 testvg lv1
node2 lvchange -an testvg/lv1
verify_lv_not_active_on 2 testvg lv1

# node1 activates lv1: lock is free (no fencing needed), lv1 is
# on dev1 which is visible on node1
node1 lvchange -ay testvg/lv1
verify_lv_active_on 1 testvg lv1
node1 lvchange -an testvg/lv1
verify_lv_not_active_on 1 testvg lv1

# Restore dev2 on node1 for cleanup
node1 lvmdevices --adddev $dev2

node1 lvremove -y testvg/lv1
node1 rm -f /etc/lvm/devices/system.devices
cleanup_vg_pr testvg

echo "== Test 12 passed =="


# ---------------------------------------------------------------
# Restore original sanlock.conf
# ---------------------------------------------------------------

nodep systemctl stop $LOCKD_SERVICE
nodep systemctl stop sanlock
nodep systemctl stop wdmd
sleep 1
nodep cp ${SANLOCK_CONF}.orig $SANLOCK_CONF 2>/dev/null || true
nodep rm -f ${SANLOCK_CONF}.orig 2>/dev/null || true
nodep systemctl start wdmd
nodep systemctl start sanlock
nodep systemctl start $LOCKD_SERVICE

exit 0
