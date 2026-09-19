#!/bin/bash
#
# shared-vg-lockstart-errors.sh - Test lockstart error reporting
#
# Verifies that vgchange --lockstart produces descriptive error
# messages for common failure scenarios, and that the EEXIST
# (already started) case is handled silently.
#
# Tests cover:
# - Already started lockspace produces no error (Test 1)
# - Sanlock not running: EMANAGER Phase 1 error (Test 2)
# - wdmd not running: SANLK_WD_ERROR Phase 2 error (Test 3)
# - Invalid host_id: EHOSTID Phase 1 error (Test 4)
# - Host_id conflict: SANLK_HOSTID_BUSY Phase 2 error (Test 5)
#

set -e

LOCKD_SERVICE="lvmlockd"


# ---------------------------------------------------------------
# Test 1: lockstart on already-started lockspace — no error
#
# vgcreate --shared auto-starts the lockspace.  A second
# vgchange --lockstart should silently succeed with no output
# (no "Starting locking..." and no "start failed" messages).
# This verifies the EEXIST fix in lockd_start_vg().
# ---------------------------------------------------------------

echo "== Test 1: already started - no error output =="

node1 vgcreate --shared testvg $dev1 $dev2

noden 1 "vgchange --lockstart testvg 2>&1" > out || true
cat out
if grep -q 'start failed' out; then
    echo "ERROR: unexpected 'start failed' message for already-started VG"
    exit 1
fi
if grep -q 'Starting locking' out; then
    echo "ERROR: unexpected 'Starting locking' message for already-started VG"
    exit 1
fi

cleanup_vg testvg

echo "== Test 1 passed =="


# ---------------------------------------------------------------
# Test 2: EMANAGER — sanlock not running
#
# With sanlock stopped, lm_prepare_lockspace_sanlock() cannot
# connect (sanlock_register fails) and returns -EMANAGER.
# lockd_start_vg() logs "lock manager sanlock is not running".
# This is a Phase 1 (synchronous) error.
# ---------------------------------------------------------------

echo "== Test 2: sanlock not running (EMANAGER) =="

node1 vgcreate --shared testvg $dev1 $dev2
node1 vgchange --lockstop testvg
sleep 1

noden 1 systemctl stop sanlock

noden 1 "vgchange --lockstart testvg 2>&1" > out || true
cat out
if ! grep -q 'lock manager.*not running' out; then
    echo "ERROR: expected 'lock manager not running' error"
    exit 1
fi
echo "EMANAGER error message verified"

noden 1 systemctl start sanlock
noden 1 systemctl restart $LOCKD_SERVICE
sleep 2

node1 vgchange --lockstart testvg
cleanup_vg testvg

echo "== Test 2 passed =="


# ---------------------------------------------------------------
# Test 3: SANLK_WD_ERROR — wdmd not running
#
# With wdmd killed, a standard (non-nowatchdog) lockstart should
# fail because sanlock cannot connect to the watchdog daemon.
# lockd_get_start_result() logs "watchdog connection or
# activation error".  This is a Phase 2 (async) error.
#
# Environment-dependent: if the test VM has direct /dev/watchdog
# access, sanlock may succeed without wdmd.  Skip if so.
# ---------------------------------------------------------------

echo "== Test 3: wdmd not running (SANLK_WD_ERROR) =="

node1 vgcreate --shared testvg $dev1 $dev2
node1 vgchange --lockstop testvg
sleep 1

noden 1 killall -9 wdmd
for i in $(seq 1 10); do
    noden 1 pgrep -x wdmd > /dev/null 2>&1 || break
    sleep 1
done

rc=0
noden 1 "vgchange --lockstart testvg 2>&1" > out || rc=$?
cat out

if [ "$rc" -ne 0 ]; then
    if ! grep -q 'start failed.*watchdog' out; then
        echo "ERROR: expected 'watchdog' error message"
        exit 1
    fi
    echo "SANLK_WD_ERROR message verified"
else
    if grep -q 'start failed.*watchdog' out; then
        echo "SANLK_WD_ERROR message verified (command exited 0 despite error)"
    else
        echo "SKIP: sanlock has direct watchdog access, wdmd not required"
    fi
    node1 vgchange --lockstop testvg || true
fi

# Restore wdmd and services
noden 1 killall -9 sanlock lvmlockd 2>/dev/null || true
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

echo "== Test 3 passed =="


# ---------------------------------------------------------------
# Test 4: EHOSTID — invalid sanlock host_id
#
# Set host_id to an invalid value (0) in lvmlocal.conf, then
# lockstart.  lm_prepare_lockspace_sanlock() rejects invalid
# host_id and returns -EHOSTID.  lockd_start_vg() logs
# "invalid sanlock host_id".  This is a Phase 1 (synchronous)
# error.
# ---------------------------------------------------------------

echo "== Test 4: invalid host_id (EHOSTID) =="

node1 vgcreate --shared testvg $dev1 $dev2
node1 vgchange --lockstop testvg
sleep 1

# Save the original host_id and set an invalid one
noden 1 "cp /etc/lvm/lvmlocal.conf /etc/lvm/lvmlocal.conf.bak"
noden 1 "sed -i 's/host_id = .*/host_id = 0/' /etc/lvm/lvmlocal.conf"

noden 1 "vgchange --lockstart testvg 2>&1" > out || true
cat out
if ! grep -q 'invalid.*host_id' out; then
    echo "ERROR: expected 'invalid host_id' error"
    exit 1
fi
echo "EHOSTID error message verified"

# Restore original host_id and restart lvmlockd to clear stale
# lockspace state from the failed EHOSTID attempt
noden 1 "cp /etc/lvm/lvmlocal.conf.bak /etc/lvm/lvmlocal.conf"
noden 1 systemctl restart $LOCKD_SERVICE
sleep 2

node1 vgchange --lockstart testvg
cleanup_vg testvg

echo "== Test 4 passed =="


# ---------------------------------------------------------------
# Test 5: SANLK_HOSTID_BUSY — host_id conflict
#
# With node1 holding the lockspace using host_id 1, set node2's
# host_id to 1 and try lockstart on node2.  sanlock detects
# another host owns the delta lease for that host_id slot and
# sanlock_add_lockspace_timeout() returns -262.
# lockd_get_start_result() logs "host_id is being used by
# another host".  This is a Phase 2 (async) error.
# ---------------------------------------------------------------

echo "== Test 5: host_id conflict (SANLK_HOSTID_BUSY) =="

node1 vgcreate --shared testvg $dev1 $dev2

# Set node2's host_id to 1 (same as node1) and restart lvmlockd
noden 2 "cp /etc/lvm/lvmlocal.conf /etc/lvm/lvmlocal.conf.bak"
noden 2 "sed -i 's/host_id = .*/host_id = 1/' /etc/lvm/lvmlocal.conf"
noden 2 systemctl restart $LOCKD_SERVICE
sleep 2

rc=0
noden 2 "vgchange --lockstart testvg 2>&1" > out || rc=$?
cat out
if [ "$rc" -eq 0 ]; then
    echo "ERROR: expected lockstart to fail with host_id conflict"
    exit 1
fi
if ! grep -q 'host_id.*being used' out; then
    echo "ERROR: expected 'host_id is being used by another host' error"
    exit 1
fi
echo "SANLK_HOSTID_BUSY error message verified"

# Restore node2's host_id and restart lvmlockd
noden 2 "cp /etc/lvm/lvmlocal.conf.bak /etc/lvm/lvmlocal.conf"
noden 2 systemctl restart $LOCKD_SERVICE
sleep 2

node2 vgchange --lockstart testvg
cleanup_vg testvg

echo "== Test 5 passed =="


exit 0
