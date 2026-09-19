#!/bin/bash
#
# shared-vg-nowatchdog.sh - Test the nowatchdog feature for shared VGs
#
# Tests the nowatchdog option which tells sanlock to skip connecting
# to wdmd/watchdog when joining a lockspace (SANLK_ADD_NO_WATCHDOG).
# This is useful for VGs using notimeout where the watchdog-based
# host fencing model is not used.
#
# Two mechanisms for nowatchdog:
# 1. Metadata-stored: --setlockargs notimeout,nowatchdog
#    Applies to all hosts starting the VG.
# 2. Transient per-host: --lockopt nowatchdog on lockstart
#    Only affects the local host; requires notimeout in VG metadata.
#
# Tests cover:
# - Positive metadata and lockopt usage (Tests 1-4)
# - Negative/invalid combinations (Tests 5-7)
# - Watchdog not fired with nowatchdog (Test 8)
# - wdmd not needed with nowatchdog (Test 9)
# - Lock recovery via PR with nowatchdog (Test 10)
#

set -e

LOCKD_SERVICE="lvmlockd"

if [ "$CLUSTER_NUM_NODES" -lt 2 ]; then
    echo "SKIP: This test requires at least 2 nodes (found $CLUSTER_NUM_NODES)"
    exit 0
fi

kill_node() {
    local nodenum=$1
    noden ${nodenum} killall -9 lvmlockd sanlock
    sleep 1
    echo "Killed sanlock and lvmlockd on node ${nodenum}"
    noden ${nodenum} systemctl reset-failed sanlock || true
    noden ${nodenum} systemctl reset-failed $LOCKD_SERVICE || true
    noden ${nodenum} dmsetup remove testvg-lvmlock 2>/dev/null || true
}

restart_node() {
    local nodenum=$1
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

get_key() {
    noden $1 vgchange --persist check testvg | grep "key for local host is registered" | grep -oE '0x[0-9a-fA-F]+'
}

verify_key_removed() {
    local key=$1
    noden 2 lvmpersist read-keys --vg testvg | tee out
    if grep "^Device" out | grep -q "$key"; then
        echo "ERROR: PR key $key still registered on testvg devices"
        cat out >&2
        exit 1
    fi
    echo "PR key $key removed from testvg devices"
}


# ---------------------------------------------------------------
# Test 1: vgcreate --setlockargs notimeout,nowatchdog (no persist)
#
# Verify that notimeout,nowatchdog can be set without persist,
# lock_args format is correct, and normal operations work.
# ---------------------------------------------------------------

echo "== Test 1: vgcreate --setlockargs notimeout,nowatchdog =="

node1 vgcreate --shared --setlockargs notimeout,nowatchdog testvg $dev1 $dev2
node1 vgchange --lockstart testvg

node1 vgs -o+lockargs testvg | grep '2.0.0'
node1 vgs -o lockargs --noheadings testvg | grep 'notimeout'
node1 vgs -o lockargs --noheadings testvg | grep 'nowatchdog'

node1 lvmlockctl -id | grep "testvg" | grep "no_watchdog=1"
node1 lvmlockctl -id | grep "testvg" | grep "no_timeout=1"

node1 lvcreate -l5 -n lv1 testvg
verify_lv_active_on 1 testvg lv1
node1 lvchange -an testvg/lv1
verify_lv_not_active_on 1 testvg lv1

node1 lvremove -y testvg/lv1
cleanup_vg testvg

echo "== Test 1 passed =="


# ---------------------------------------------------------------
# Test 2: vgcreate --setlockargs persist,notimeout,nowatchdog
#
# Verify all three flags together: persist, notimeout, nowatchdog.
# ---------------------------------------------------------------

echo "== Test 2: vgcreate --setlockargs persist,notimeout,nowatchdog =="

node1 vgcreate --shared --setpersist y --setlockargs persist,notimeout,nowatchdog testvg $dev1 $dev2
node1 vgchange --lockstart --persist start testvg

node1 vgs -o+lockargs testvg | grep '2.0.0'
node1 vgs -o lockargs --noheadings testvg | grep 'persist'
node1 vgs -o lockargs --noheadings testvg | grep 'notimeout'
node1 vgs -o lockargs --noheadings testvg | grep 'nowatchdog'

node1 lvmlockctl -id | grep "testvg" | grep "no_watchdog=1"
node1 lvmlockctl -id | grep "testvg" | grep "no_timeout=1"
node1 lvmlockctl -id | grep "testvg" | grep "fence_pr=1"

node1 lvcreate -l5 -n lv1 testvg
verify_lv_active_on 1 testvg lv1
node1 lvchange -an testvg/lv1

node1 lvremove -y testvg/lv1
cleanup_vg_pr testvg

echo "== Test 2 passed =="


# ---------------------------------------------------------------
# Test 3: vgchange --setlockargs: add and remove nowatchdog
#
# Start with persist,notimeout (no nowatchdog), add nowatchdog
# via setlockargs, verify it appears, then remove it and verify
# it is gone.
# ---------------------------------------------------------------

echo "== Test 3: add and remove nowatchdog via setlockargs =="

node1 vgcreate --shared --setpersist y --setlockargs persist,notimeout testvg $dev1 $dev2
node1 vgchange --lockstart --persist start testvg

node1 vgs -o lockargs --noheadings testvg | grep 'notimeout'
node1 lvmlockctl -id | grep "testvg" | grep "no_watchdog=0"

# Add nowatchdog (setlockargs requires sole host in lockspace;
# retry because sanlock needs one renewal cycle (2*io_timeout)
# after lockspace start before get_hosts returns host info)
for i in $(seq 1 30); do
	node1 vgchange --setlockargs persist,notimeout,nowatchdog testvg && break
	sleep 1
done
node1 vgchange --lockstart --persist start testvg

node1 vgs -o lockargs --noheadings testvg | grep 'nowatchdog'
node1 lvmlockctl -id | grep "testvg" | grep "no_watchdog=1"

# Remove nowatchdog (lockspace was just restarted above, so
# sanlock needs a full renewal cycle before get_hosts works)
for i in $(seq 1 30); do
	node1 vgchange --setlockargs persist,notimeout testvg && break
	sleep 1
done
node1 vgchange --lockstart --persist start testvg

node1 lvmlockctl -id | grep "testvg" | grep "no_watchdog=0"

cleanup_vg_pr testvg

echo "== Test 3 passed =="


# ---------------------------------------------------------------
# Test 4: --lockopt nowatchdog (transient, per-host)
#
# VG metadata has notimeout but not nowatchdog.  Using --lockopt
# nowatchdog on lockstart applies nowatchdog transiently for the
# local host only, without modifying VG metadata.
# ---------------------------------------------------------------

echo "== Test 4: --lockopt nowatchdog (transient) =="

node1 vgcreate --shared --setlockargs notimeout testvg $dev1 $dev2

# vgcreate auto-starts the lockspace without nowatchdog;
# stop it and restart with --lockopt nowatchdog
node1 vgchange --lockstop testvg
node1 vgchange --lockstart --lockopt nowatchdog testvg

node1 lvmlockctl -id | grep "testvg" | grep "no_watchdog=1"
node1 lvmlockctl -id | grep "testvg" | grep "no_timeout=1"

node1 lvcreate -l5 -n lv1 testvg
verify_lv_active_on 1 testvg lv1
node1 lvchange -an testvg/lv1

node1 lvremove -y testvg/lv1
cleanup_vg testvg

echo "== Test 4 passed =="


# ---------------------------------------------------------------
# Test 5: --setlockargs nowatchdog without notimeout (fail)
#
# nowatchdog requires notimeout.  Setting nowatchdog alone should
# be rejected by the lockargs validation.
# ---------------------------------------------------------------

echo "== Test 5: nowatchdog without notimeout fails =="

node1 vgcreate --shared testvg $dev1 $dev2
node1 vgchange --lockstart testvg

node1 not vgchange --setlockargs nowatchdog testvg

cleanup_vg testvg

echo "== Test 5 passed =="


# ---------------------------------------------------------------
# Test 6: --setlockargs nowatchdog,watchdog (fail)
#
# nowatchdog and watchdog are contradictory.  This combination
# should be rejected.
# ---------------------------------------------------------------

echo "== Test 6: nowatchdog,watchdog combination fails =="

node1 vgcreate --shared testvg $dev1 $dev2
node1 vgchange --lockstart testvg

node1 not vgchange --setlockargs nowatchdog,watchdog,notimeout testvg

cleanup_vg testvg

echo "== Test 6 passed =="


# ---------------------------------------------------------------
# Test 7: --lockopt nowatchdog on default VG (fail)
#
# --lockopt nowatchdog requires the VG metadata to have notimeout.
# Using it on a default VG (no notimeout) should fail.
# ---------------------------------------------------------------

echo "== Test 7: --lockopt nowatchdog on default VG fails =="

node1 vgcreate --shared testvg $dev1 $dev2
node1 vgchange --lockstart testvg
node1 vgchange --lockstop testvg

node1 not vgchange --lockstart --lockopt nowatchdog testvg

# Restart lockspace normally so cleanup can acquire the global lock
node1 vgchange --lockstart testvg

cleanup_vg testvg

echo "== Test 7 passed =="


# ---------------------------------------------------------------
# Test 8: kill sanlock with nowatchdog -- host NOT reset
#
# With nowatchdog, sanlock joined the lockspace with
# SANLK_ADD_NO_WATCHDOG, so wdmd never armed a watchdog timer.
# Killing sanlock should NOT trigger a machine reset.
# Wait well past the ~60s watchdog timeout to confirm.
# ---------------------------------------------------------------

echo "== Test 8: kill sanlock with nowatchdog - no watchdog reset =="

node1 vgcreate --shared --setlockargs notimeout,nowatchdog testvg $dev1 $dev2
node1 vgchange --lockstart testvg

node1 lvcreate -l5 -n lv1 testvg
verify_lv_active_on 1 testvg lv1

kill_node 1

echo "Waiting 90s to verify watchdog does not fire..."
sleep 90

# Node1 should still be reachable (no reboot occurred)
noden 1 true
echo "Node 1 still reachable after 90s - nowatchdog confirmed"

restart_node 1
node1 vgchange --lockstart testvg
node1 dmsetup remove testvg-lv1 2>/dev/null || true
node1 lvremove -y testvg/lv1 || true
cleanup_vg testvg

echo "== Test 8 passed =="


# ---------------------------------------------------------------
# Test 9: wdmd not running - nowatchdog lockstart succeeds
#
# With wdmd killed, a nowatchdog lockstart should succeed because
# SANLK_ADD_NO_WATCHDOG bypasses the wdmd connection.
#
# TODO: also test that a default (non-nowatchdog) lockstart fails
# when wdmd is not running.  sanlock correctly returns
# SANLK_WD_ERROR (-203) but lvmlockd lockstart is async:
# lm_prepare_lockspace returns success immediately, and the
# actual sanlock_add_lockspace failure (create_fail=1) is not
# reported back through count_lockspace_starting/start_wait.
# Add a negative assertion once the async start failure is
# detectable by the lvm command.
# ---------------------------------------------------------------

echo "== Test 9: wdmd not running =="

node1 vgcreate --shared --setlockargs notimeout,nowatchdog testvg $dev1 $dev2

# Stop lockspace before killing wdmd
node1 vgchange --lockstop testvg
sleep 2

# Kill wdmd directly (not via systemctl to avoid cascading to sanlock)
noden 1 killall -9 wdmd

# Wait until wdmd is actually dead (avoid race with socket cleanup)
for i in $(seq 1 10); do
	noden 1 pgrep -x wdmd > /dev/null 2>&1 || break
	sleep 1
done

# nowatchdog lockstart should succeed without wdmd
node1 vgchange --lockstart testvg

node1 lvcreate -l5 -n lv1 testvg
verify_lv_active_on 1 testvg lv1
node1 lvchange -an testvg/lv1
node1 lvremove -y testvg/lv1

# Cleanup: restore wdmd (kill sanlock first since it may hold /dev/watchdog,
# and remove stale /dev/shm/wdmd left by the unclean wdmd kill)
node1 vgchange --lockstop testvg || true
noden 1 killall -9 sanlock lvmlockd || true
sleep 1
noden 1 systemctl reset-failed sanlock || true
noden 1 systemctl reset-failed $LOCKD_SERVICE || true
noden 1 systemctl reset-failed wdmd || true
noden 1 dmsetup remove testvg-lvmlock 2>/dev/null || true
noden 1 rm -f /dev/shm/wdmd
noden 1 systemctl start wdmd
noden 1 systemctl start sanlock
noden 1 systemctl start $LOCKD_SERVICE
sleep 2

node1 vgchange --lockstart testvg
cleanup_vg testvg

echo "== Test 9 passed =="


# ---------------------------------------------------------------
# Test 10: lock recovery via PR with persist,notimeout,nowatchdog
#
# With nowatchdog, the failed node is NOT reset by the watchdog.
# Lock recovery still works via persistent reservation key removal.
# This is the intended usage: nowatchdog + persist + notimeout.
# ---------------------------------------------------------------

echo "== Test 10: lock recovery with nowatchdog =="

node1 vgcreate --shared --setpersist y --setlockargs persist,notimeout,nowatchdog testvg $dev1 $dev2
nodep vgchange --lockstart --persist start testvg

node1 lvcreate -l5 -n lv1 testvg
verify_lv_active_on 1 testvg lv1

# node2 cannot activate while node1 holds the lock
node2 not lvchange -ay testvg/lv1

KEY1=$(get_key 1)

kill_node 1

# Verify node1 is still reachable (no watchdog reset)
noden 1 true
echo "Node 1 still reachable after kill - nowatchdog confirmed"

# node2 attempts activation; after host_fail_timeout, lvmlockd
# on node2 removes node1's PR key and retries the lock
lvchange_ay_loop 2 testvg lv1

verify_key_removed $KEY1
verify_lv_active_on 2 testvg lv1

node2 lvchange -an testvg/lv1
verify_lv_not_active_on 2 testvg lv1

# Restart node1 and clean up
restart_node 1
node1 vgchange --lockstart --persist start testvg
node1 dmsetup remove testvg-lv1 2>/dev/null || true

node1 lvremove -y testvg/lv1
cleanup_vg_pr testvg

echo "== Test 10 passed =="


exit 0
