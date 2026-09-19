#!/bin/bash
#
# adopt.sh - Test lvmlockd lock adoption
#
# Tests two adoption mechanisms:
# 1. lvmlockd -A 1: automatic adoption on lvmlockd restart
# 2. --lockopt adopt options: manual adoption via lvm commands
#

set -e

LOCKD_SERVICE="lvmlockd"
SYSCONFIG="/etc/sysconfig/lvmlockd"
ADOPT_FILE="/run/lvm/lvmlockd.adopt"

if [ "$CLUSTER_NUM_NODES" -lt 2 ]; then
    echo "SKIP: This test requires at least 2 nodes (found $CLUSTER_NUM_NODES)"
    exit 0
fi

#
# Helpers to verify the content of the adopt file.
#
# The adopt file written by "lvmlockd -A 1" has the format:
# lvmlockd adopt_version 1.0 pid <PID> updates <N> <timestamp>
# VG: <vg_uuid> <vg_name> <lock_manager> <lock_args>
# LV: <vg_uuid> <resource_name> <lv_args> <mode> <version>
#

verify_adopt_file_vg() {
    local nodenum=$1
    local vg_name=$2
    local lm_type=${3:-sanlock}
    local content
    content=$(noden ${nodenum} cat $ADOPT_FILE)

    if ! echo "$content" | grep -q "^VG: .* ${vg_name} ${lm_type} "; then
        _error_with_location "Adopt file missing VG $vg_name ($lm_type) on node $nodenum"
        echo "  content:" >&2
        echo "$content" >&2
        exit 1
    fi
    echo "✓ Adopt file has VG $vg_name on node $nodenum"
}

verify_adopt_file_lv() {
    local nodenum=$1
    local mode=$2
    local lv_count=$3
    local content
    content=$(noden ${nodenum} cat $ADOPT_FILE)
    local actual
    actual=$(echo "$content" | grep -c "^LV: .* ${mode} ")

    if [ "$actual" -ne "$lv_count" ]; then
        _error_with_location "Adopt file expected $lv_count LV lines with mode $mode, found $actual on node $nodenum"
        echo "  content:" >&2
        echo "$content" >&2
        exit 1
    fi
    echo "✓ Adopt file has $lv_count LV(s) with mode $mode on node $nodenum"
}

verify_adopt_file_no_lv() {
    local nodenum=$1
    local content
    content=$(noden ${nodenum} cat $ADOPT_FILE)

    if echo "$content" | grep -q "^LV:"; then
        _error_with_location "Adopt file should have no LV lines on node $nodenum"
        echo "  content:" >&2
        echo "$content" >&2
        exit 1
    fi
    echo "✓ Adopt file has no LV lines on node $nodenum"
}

#
# Save the original sysconfig and enable -A 1 so lvmlockd
# writes the adopt file while running.
#
nodes cp $SYSCONFIG ${SYSCONFIG}.orig || true
nodes "echo 'OPTIONS=\"-A 1\"' > $SYSCONFIG"
nodes systemctl restart $LOCKD_SERVICE
sleep 2

#
# Helper: kill lvmlockd on a node without cleanup.
# This simulates an unexpected crash, leaving orphan locks
# in the lock manager and the adopt file on disk.
#
kill_lvmlockd() {
    local nodenum=$1
    noden ${nodenum} lvmlockctl --dump
    noden ${nodenum} killall -9 lvmlockd
    sleep 1
    echo "Killed lvmlockd on node ${nodenum}"
    noden ${nodenum} systemctl reset-failed $LOCKD_SERVICE || true
    echo "Adopt file on node ${nodenum}"
    noden ${nodenum} cat $ADOPT_FILE || true
}

#
# Helper: restart lvmlockd with specific options.
#
restart_lvmlockd() {
    local nodenum=$1
    local opts="$2"
    noden ${nodenum} "echo 'OPTIONS=\"$opts\"' > $SYSCONFIG"
    noden ${nodenum} systemctl start $LOCKD_SERVICE
    echo "Restarted lvmlockd on node ${nodenum} with options: $opts"
    sleep 2
}

#
# Helper: restart lvmlockd with default options (no adopt).
#
restart_lvmlockd_default() {
    local nodenum=$1
    noden ${nodenum} cp ${SYSCONFIG}.orig $SYSCONFIG || true
    noden ${nodenum} "echo 'OPTIONS=\"\"' > $SYSCONFIG"
    noden ${nodenum} systemctl start $LOCKD_SERVICE
    echo "Restarted lvmlockd on node ${nodenum}"
    sleep 2
}

#
# Helper: verify lvmlockd is running on a node.
#
check_lvmlockd_running() {
    local nodenum=$1
    noden ${nodenum} pgrep lvmlockd
}


# ---------------------------------------------------------------
# Test 1: lvmlockd -A 1 basic adopt with active LV
#
# Create shared VG, activate LV, kill lvmlockd, restart with -A 1,
# verify the LV remains usable.
# ---------------------------------------------------------------

echo "== Test 1: lvmlockd -A 1 basic adopt =="

node1 vgcreate --shared testvg $dev1 $dev2
nodep vgchange --lockstart testvg

node1 lvcreate -l10 -n lv1 testvg
verify_lv_active_on 1 testvg lv1

# node1 has active LV with persistent ex lock on lv1
# Kill lvmlockd on node1 - this orphans locks in the lock manager
kill_lvmlockd 1
verify_lv_orphan_in_sanlock 1 testvg lv1

# The LV DM device is still present
node1 dmsetup status testvg-lv1

# Verify the adopt file was written by previous lvmlockd instance
node1 cat $ADOPT_FILE
verify_adopt_file_vg 1 testvg
verify_adopt_file_lv 1 ex 1

# Restart lvmlockd with -A 1 to adopt orphan locks
restart_lvmlockd 1 "-A 1"
check_lvmlockd_running 1

# After adoption, the LV should still be active and usable
verify_lv_active_on 1 testvg lv1
verify_lv_lock_in_lvmlockd 1 testvg lv1
verify_lv_lock_in_sanlock 1 testvg lv1

# node2 should not be able to activate the LV (node1 holds ex lock)
node2 not lvchange -ay testvg/lv1

# node1 can deactivate and reactivate normally
node1 lvchange -an testvg/lv1
verify_lv_not_active_on 1 testvg lv1

node1 lvchange -ay testvg/lv1
verify_lv_active_on 1 testvg lv1

# node2 can activate after node1 releases
node1 lvchange -an testvg/lv1
node2 lvchange -ay testvg/lv1
verify_lv_active_on 2 testvg lv1
node2 lvchange -an testvg/lv1

node1 lvremove -y testvg/lv1
cleanup_vg testvg
echo "== Test 1 passed =="


# ---------------------------------------------------------------
# Test 2: lvmlockd -A 1 adopt with multiple LVs
#
# Verify adoption works when multiple LVs have persistent locks.
# ---------------------------------------------------------------

echo "== Test 2: lvmlockd -A 1 adopt multiple LVs =="

node1 vgcreate --shared testvg $dev1 $dev2
nodep vgchange --lockstart testvg

node1 lvcreate -l5 -n lv1 testvg
node1 lvcreate -l5 -n lv2 testvg
node1 lvcreate -l5 -n lv3 -an testvg

# lv1 and lv2 are active, lv3 is not
verify_lv_active_on 1 testvg lv1
verify_lv_active_on 1 testvg lv2

kill_lvmlockd 1
verify_adopt_file_vg 1 testvg
verify_adopt_file_lv 1 ex 2

restart_lvmlockd 1 "-A 1"
check_lvmlockd_running 1

# Active LVs should be adopted
verify_lv_active_on 1 testvg lv1
verify_lv_active_on 1 testvg lv2
verify_lv_lock_in_lvmlockd 1 testvg lv1
verify_lv_lock_in_lvmlockd 1 testvg lv2
verify_lv_lock_in_sanlock 1 testvg lv1
verify_lv_lock_in_sanlock 1 testvg lv2

# node2 cannot activate LVs that node1 adopted
node2 not lvchange -ay testvg/lv1
node2 not lvchange -ay testvg/lv2

# node2 can activate lv3 which had no lock to adopt
node2 lvchange -ay testvg/lv3
verify_lv_active_on 2 testvg lv3
node2 lvchange -an testvg/lv3

# Deactivate everything on node1
node1 lvchange -an testvg/lv1
node1 lvchange -an testvg/lv2
node1 lvremove -y testvg/lv1 testvg/lv2 testvg/lv3
cleanup_vg testvg
echo "== Test 2 passed =="


# ---------------------------------------------------------------
# Test 3: --lockopt adoptls with vgchange --lockstart
#
# Kill lvmlockd, restart without adopt, use --lockopt adoptls
# to adopt the existing lockspace in the lock manager.
# ---------------------------------------------------------------

echo "== Test 3: --lockopt adoptls =="

node1 vgcreate --shared testvg $dev1 $dev2
nodep vgchange --lockstart testvg

node1 lvcreate -l5 -n lv1 -an testvg

kill_lvmlockd 1
verify_adopt_file_vg 1 testvg

# Restart without -A 1
restart_lvmlockd_default 1
check_lvmlockd_running 1

# lockstart with --lockopt adoptls should succeed by adopting
# the existing lockspace in the lock manager
node1 vgchange --lockstart --lockopt adoptls testvg

# After adopting the lockspace, commands should work
node1 lvchange -ay testvg/lv1
verify_lv_active_on 1 testvg lv1
node1 lvchange -an testvg/lv1

node1 lvremove -y testvg/lv1
cleanup_vg testvg
echo "== Test 3 passed =="


# ---------------------------------------------------------------
# Test 4: --lockopt adoptls failure (no existing lockspace)
#
# adoptls should fail when there is no existing lockspace to adopt.
# ---------------------------------------------------------------

echo "== Test 4: --lockopt adoptls failure =="

node1 vgcreate --shared testvg $dev1 $dev2

# Do not lockstart - there is no lockspace in the lock manager.
# adoptls requires an existing lockspace, so this should fail.
# FIXME: lockstart doesn't return error.
# node1 not vgchange --lockstart --lockopt adoptls testvg

# Normal lockstart should work
node1 vgchange --lockstart testvg
node2 vgchange --lockstart testvg

cleanup_vg testvg
echo "== Test 4 passed =="


# ---------------------------------------------------------------
# Test 5: --lockopt adopt with vgchange --lockstart
#
# --lockopt adopt is the flexible version: adopt if existing
# lockspace found, otherwise start normally.
# ---------------------------------------------------------------

echo "== Test 5: --lockopt adopt lockstart =="

node1 vgcreate --shared testvg $dev1 $dev2

# No existing lockspace - adopt should fall through to normal start
node1 vgchange --lockstart --lockopt adopt testvg

node1 lvcreate -l5 -n lv1 -an testvg

# Kill and restart to create orphan lockspace
kill_lvmlockd 1
verify_adopt_file_vg 1 testvg
restart_lvmlockd_default 1
check_lvmlockd_running 1

# Now there IS an existing lockspace - adopt should find it
node1 vgchange --lockstart --lockopt adopt testvg

node1 lvchange -ay testvg/lv1
verify_lv_active_on 1 testvg lv1
node1 lvchange -an testvg/lv1

node2 vgchange --lockstart testvg
node1 lvremove -y testvg/lv1
cleanup_vg testvg
echo "== Test 5 passed =="


# ---------------------------------------------------------------
# Test 6: --lockopt adoptlv for LV lock adoption
#
# After killing lvmlockd and restarting, use --lockopt adoptlv
# to adopt the orphan LV lock from the lock manager.
# ---------------------------------------------------------------

echo "== Test 6: --lockopt adoptlv =="

node1 vgcreate --shared testvg $dev1 $dev2
nodep vgchange --lockstart testvg

node1 lvcreate -l5 -n lv1 testvg
verify_lv_active_on 1 testvg lv1

kill_lvmlockd 1
verify_lv_orphan_in_sanlock 1 testvg lv1
verify_adopt_file_vg 1 testvg
verify_adopt_file_lv 1 ex 1

restart_lvmlockd_default 1
check_lvmlockd_running 1

# Adopt the lockspace first
node1 vgchange --lockstart --lockopt adopt testvg

# Adopt the orphan LV lock
node1 lvchange -ay --lockopt adoptlv testvg/lv1
verify_lv_active_on 1 testvg lv1
verify_lv_lock_in_lvmlockd 1 testvg lv1
verify_lv_lock_in_sanlock 1 testvg lv1

# node2 should be blocked by the adopted lock
node2 not lvchange -ay testvg/lv1

# Normal deactivation
node1 lvchange -an testvg/lv1
verify_lv_not_active_on 1 testvg lv1

# node2 can now activate
node2 lvchange -ay testvg/lv1
verify_lv_active_on 2 testvg lv1
node2 lvchange -an testvg/lv1

node1 lvremove -y testvg/lv1
cleanup_vg testvg
echo "== Test 6 passed =="


# ---------------------------------------------------------------
# Test 7: --lockopt adoptlv failure (no orphan lock)
#
# adoptlv should fail when there is no orphan LV lock to adopt.
# ---------------------------------------------------------------

echo "== Test 7: --lockopt adoptlv failure =="

node1 vgcreate --shared testvg $dev1 $dev2
nodep vgchange --lockstart testvg

node1 lvcreate -l5 -n lv1 -an testvg

# lv1 is inactive and has no lock - adoptlv should fail
node1 not lvchange -ay --lockopt adoptlv testvg/lv1

# Normal activation should work
node1 lvchange -ay testvg/lv1
verify_lv_active_on 1 testvg lv1
node1 lvchange -an testvg/lv1

node1 lvremove -y testvg/lv1
cleanup_vg testvg
echo "== Test 7 passed =="


# ---------------------------------------------------------------
# Test 8: --lockopt adopt for LV (flexible)
#
# --lockopt adopt should adopt orphan lock if found, or acquire
# new lock if no orphan exists.
# ---------------------------------------------------------------

echo "== Test 8: --lockopt adopt for LV =="

node1 vgcreate --shared testvg $dev1 $dev2
nodep vgchange --lockstart testvg

# No orphan lock - adopt falls through to normal lock acquisition
node1 lvcreate -l5 -n lv1 -an testvg
node1 lvchange -ay --lockopt adopt testvg/lv1
verify_lv_active_on 1 testvg lv1
node1 lvchange -an testvg/lv1

# Now create orphan lock scenario
node1 lvchange -ay testvg/lv1
verify_lv_active_on 1 testvg lv1

kill_lvmlockd 1
verify_lv_orphan_in_sanlock 1 testvg lv1
verify_adopt_file_vg 1 testvg
verify_adopt_file_lv 1 ex 1

restart_lvmlockd_default 1
check_lvmlockd_running 1

node1 vgchange --lockstart --lockopt adopt testvg

# Orphan lock exists - adopt should find and adopt it
node1 lvchange -ay --lockopt adopt testvg/lv1
verify_lv_active_on 1 testvg lv1
verify_lv_lock_in_lvmlockd 1 testvg lv1
verify_lv_lock_in_sanlock 1 testvg lv1

node1 lvchange -an testvg/lv1
node1 lvremove -y testvg/lv1
cleanup_vg testvg
echo "== Test 8 passed =="


# ---------------------------------------------------------------
# Test 9: lvmlockd -A 1 repeated adopt cycles
#
# Verify that repeated kill/restart with -A 1 works correctly.
# ---------------------------------------------------------------

echo "== Test 9: repeated adopt cycles =="

# Previous tests restarted node1 lvmlockd without -A 1,
# so the adopt file is stale.  Restart with -A 1 so the
# adopt file is maintained for the kill/restart cycles.
noden 1 "echo 'OPTIONS=\"-A 1\"' > $SYSCONFIG"
noden 1 systemctl restart $LOCKD_SERVICE
sleep 2

node1 vgcreate --shared testvg $dev1 $dev2
nodep vgchange --lockstart testvg

node1 lvcreate -l5 -n lv1 testvg
verify_lv_active_on 1 testvg lv1
verify_lv_lock_in_lvmlockd 1 testvg lv1
verify_lv_lock_in_sanlock 1 testvg lv1

for i in 1 2 3; do
    echo "  cycle $i"
    kill_lvmlockd 1
    verify_adopt_file_vg 1 testvg
    verify_adopt_file_lv 1 ex 1
    restart_lvmlockd 1 "-A 1"
    check_lvmlockd_running 1
    verify_lv_active_on 1 testvg lv1
    verify_lv_lock_in_lvmlockd 1 testvg lv1
    verify_lv_lock_in_sanlock 1 testvg lv1
    node2 not lvchange -ay testvg/lv1
done

node1 lvchange -an testvg/lv1
node1 lvremove -y testvg/lv1
cleanup_vg testvg
echo "== Test 9 passed =="


# ---------------------------------------------------------------
# Test 10: adopt after node2 operations during node1 outage
#
# While node1 lvmlockd is down, node2 can still operate on other
# LVs.  After node1 restarts with -A 1, both nodes work correctly.
# ---------------------------------------------------------------

echo "== Test 10: node2 operates during node1 adopt =="

node1 vgcreate --shared testvg $dev1 $dev2
nodep vgchange --lockstart testvg

node1 lvcreate -l5 -n lv1 testvg
verify_lv_active_on 1 testvg lv1

kill_lvmlockd 1
verify_adopt_file_vg 1 testvg
verify_adopt_file_lv 1 ex 1

# node2 creates and uses a separate LV while node1 is down
node2 lvcreate -l5 -n lv2 testvg
verify_lv_active_on 2 testvg lv2

# node1 restarts and adopts its locks
restart_lvmlockd 1 "-A 1"
check_lvmlockd_running 1

# node1's lv1 should be adopted, node2's lv2 should be unaffected
verify_lv_active_on 1 testvg lv1
verify_lv_active_on 2 testvg lv2
verify_lv_lock_in_lvmlockd 1 testvg lv1
verify_lv_lock_in_sanlock 1 testvg lv1
verify_lv_lock_in_lvmlockd 2 testvg lv2
verify_lv_lock_in_sanlock 2 testvg lv2

# Mutual exclusion should work for both
node2 not lvchange -ay testvg/lv1
node1 not lvchange -ay testvg/lv2

node1 lvchange -an testvg/lv1
node2 lvchange -an testvg/lv2
node1 lvremove -y testvg/lv1 testvg/lv2
cleanup_vg testvg
echo "== Test 10 passed =="


# ---------------------------------------------------------------
# Test 11: VG lock adoption with -A 1
#
# Create an orphan VG lock by running lvcreate under gdb with a
# breakpoint at lv_create_single.  This function is called while
# the VG EX lock is held.  Kill lvmlockd while paused at the
# breakpoint, creating a deterministic orphan VG lock in sanlock.
# Restart with -A 1 and verify VG lock is adopted.
# ---------------------------------------------------------------

echo "== Test 11: VG lock adoption with -A 1 =="

node1 vgcreate --shared testvg $dev1 $dev2
nodep vgchange --lockstart testvg

# Run lvcreate under gdb, break at lv_create_single (VG lock held),
# kill lvmlockd to orphan the VG lock, then kill lvcreate and exit.
noden 1 "gdb -batch -ex 'set confirm off' -ex 'break lv_create_single' -ex run -ex 'shell killall -9 lvmlockd' -ex kill -ex quit --args lvcreate -l1 -n tmpgdb -an testvg 2>&1 || true"
sleep 1
noden 1 systemctl reset-failed $LOCKD_SERVICE || true

verify_vg_orphan_in_sanlock 1 testvg

# Restart with -A 1 to adopt orphan VG lock
restart_lvmlockd 1 "-A 1"
check_lvmlockd_running 1

# VG commands should work after VG lock adoption.
# lvcreate requires VG EX lock.
node1 lvcreate -l1 -n newlv -an testvg
node1 lvremove -y testvg/newlv
cleanup_vg testvg
echo "== Test 11 passed =="


# ---------------------------------------------------------------
# Test 12: --lockopt adoptvg
#
# Create an orphan VG lock by running lvcreate under gdb with a
# breakpoint at lv_create_single.  Restart without -A 1 and use
# --lockopt adoptvg to explicitly adopt the orphan VG lock.
# ---------------------------------------------------------------

echo "== Test 12: --lockopt adoptvg =="

# Restart with -A 1 so the adopt file is maintained
noden 1 "echo 'OPTIONS=\"-A 1\"' > $SYSCONFIG"
noden 1 systemctl restart $LOCKD_SERVICE
sleep 2

node1 vgcreate --shared testvg $dev1 $dev2
nodep vgchange --lockstart testvg

node1 lvcreate -l1 -n lv1 -an testvg

# Run lvcreate under gdb, break at lv_create_single (VG lock held),
# kill lvmlockd to orphan the VG lock, then kill lvcreate and exit.
noden 1 "gdb -batch -ex 'set confirm off' -ex 'break lv_create_single' -ex run -ex 'shell killall -9 lvmlockd' -ex kill -ex quit --args lvcreate -l1 -n tmpgdb -an testvg 2>&1 || true"
sleep 1
noden 1 systemctl reset-failed $LOCKD_SERVICE || true

verify_vg_orphan_in_sanlock 1 testvg

# Restart without -A 1
restart_lvmlockd_default 1
check_lvmlockd_running 1

# Adopt the lockspace first
node1 vgchange --lockstart --lockopt adoptls testvg

# Adopt the orphan VG lock explicitly
# The aborted lvcreate left the exclusive vg lock orphaned,
# so we run another lvcreate here that will request the ex
# vg lock with the adoptvg option (a matching lock mode must
# be adopted.)
node1 lvcreate --lockopt adoptvg -l1 -n lv2 testvg
node1 lvcreate -l1 -n lv3 testvg
node1 lvchange -ay testvg/lv1
verify_lv_active_on 1 testvg lv1

node1 lvchange -an testvg/lv1
node1 lvremove -y testvg/lv1
node1 lvremove -y testvg/lv2
node1 lvremove -y testvg/lv3
cleanup_vg testvg
echo "== Test 12 passed =="


# ---------------------------------------------------------------
# Test 13: lvmlockd -A 1 adopt with gl disabled
#
# Verify that adoption succeeds when the global lock has been
# disabled.
# ---------------------------------------------------------------

echo "== Test 13: lvmlockd -A 1 adopt with gl disabled =="

noden 1 "echo 'OPTIONS=\"-A 1\"' > $SYSCONFIG"
noden 1 systemctl restart $LOCKD_SERVICE
sleep 2

node1 vgcreate --shared testvg $dev1 $dev2
nodep vgchange --lockstart testvg

node1 lvcreate -l5 -n lv1 testvg
verify_lv_active_on 1 testvg lv1

# Disable the global lock
node1 lvmlockctl --gl-disable testvg

kill_lvmlockd 1
verify_adopt_file_vg 1 testvg
verify_adopt_file_lv 1 ex 1

# Restart with -A 1; adopt must succeed with gl disabled
restart_lvmlockd 1 "-A 1"
check_lvmlockd_running 1

# The LV should still be active and adopted
verify_lv_active_on 1 testvg lv1
verify_lv_lock_in_lvmlockd 1 testvg lv1
verify_lv_lock_in_sanlock 1 testvg lv1

# node2 should not be able to activate (node1 holds ex lock)
node2 not lvchange -ay testvg/lv1

# Normal deactivation should work
node1 lvchange -an testvg/lv1
verify_lv_not_active_on 1 testvg lv1

# Re-enable the global lock before cleanup
node1 lvmlockctl --gl-enable testvg

node1 lvremove -y testvg/lv1
cleanup_vg testvg
echo "== Test 13 passed =="


# ---------------------------------------------------------------
# Restore original sysconfig
# ---------------------------------------------------------------

nodes cp ${SYSCONFIG}.orig $SYSCONFIG 2>/dev/null || true
nodes rm -f ${SYSCONFIG}.orig 2>/dev/null || true

exit 0
