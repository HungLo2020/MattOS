#!/usr/bin/env bash

# Copyright (C) 2026 Red Hat, Inc. All rights reserved.
#
# This copyrighted material is made available to anyone wishing to use,
# modify, copy, or redistribute it subject to the terms and conditions
# of the GNU General Public License v.2.
#
# You should have received a copy of the GNU General Public License
# along with this program; if not, write to the Free Software Foundation,
# Inc., 51 Franklin Street, Fifth Floor, Boston, MA 02110-1301 USA

test_description='pvmove cluster lock protection (lvmlockd --test mode)'

. lib/inittest

# Requires lvmlockd running with --test (daemon_test mode).
[[ "${LVM_TEST_LVMLOCKD_TEST:-0}" = 0 ]] && skip

which md5sum || skip

aux prepare_vg 5

# ===================================================================
# Test 1: lvmlockctl --set-remote-lv-lock injects and clears remote
#         lock state
#
# Verify round-trip: lvmlockctl sends set_remote_lv_lock, the
# lockspace thread creates a resource with test_remote_ex/sh set,
# and lm_lock returns EAGAIN for incompatible lock requests.
# ===================================================================

lvcreate -an -l4 -n $lv1 $vg "$dev1"

lv1_uuid=$(get lv_field $vg/$lv1 lv_uuid)

# Inject remote EX lock.  This sets test_remote_ex on the resource,
# simulating another cluster node holding an exclusive lock.
lvmlockctl --set-remote-lv-lock $vg --lv-uuid "$lv1_uuid" --lock-mode ex

# Activation must fail: lm_lock returns EAGAIN because test_remote_ex is set,
# simulating another cluster node holding the EX lock.
not lvchange -ay $vg/$lv1
check inactive $vg $lv1

# Clear the remote state.
lvmlockctl --set-remote-lv-lock $vg --lv-uuid "$lv1_uuid" --lock-mode un

# Activation must succeed now.
lvchange -aey $vg/$lv1
check active $vg $lv1
lvchange -an $vg/$lv1

lvremove -ff $vg

# ===================================================================
# Test 2: pre-activation of pvmove LV on restart
#
# After reboot (or daemon kill + DM removal), pvmove0 is not active
# in DM but the local lvmlockd still holds persistent EX on it.
# lv_active_change() pre-activates pvmove0 before lockd_lv() so
# _lv_pvmove_is_active() sees it active and allows activation.
#
# This is the "restart on same node" case -- the local host can
# re-acquire the pvmove0 lock and resume.
# ===================================================================

lvcreate -an -l4 -n $lv1 $vg "$dev1"
check inactive $vg $lv1

lv1_uuid=$(get lv_field $vg/$lv1 lv_uuid)

# Start background pvmove with long poll delay so the daemon is idle.
# pvmove creates pvmove0 (activated in DM) and sets LOCKED on lv1.
LVM_TEST_TAG="kill_me_$PREFIX" pvmove -n $vg/$lv1 -i +3 -b "$dev1" "$dev5"

pvmove0_uuid=$(get lv_field $vg/pvmove0 lv_uuid)

# pvmove holds persistent EX lock on pvmove0.
lvmlockctl --info | tee out
grep "LK LV ex $pvmove0_uuid" out

# Confirm pvmove0 is active in DM.
dmsetup info "${vg}-pvmove0"

# Kill the poll daemon -- persistent lock on pvmove0 must survive.
aux kill_tagged_processes
lvmlockctl --info | tee out
grep "LK LV ex $pvmove0_uuid" out

# Remove the pvmove0 DM device to simulate restart.
dmsetup remove "${vg}-pvmove0"
not dmsetup info "${vg}-pvmove0"

# lv_active_change() pre-activates pvmove0 (lock re-acquired from
# local lvmlockd), then _lv_pvmove_is_active() sees it active.
lvchange -ay $vg/$lv1
check active $vg $lv1

# pvmove0 must be active again (pre-activated by lv_active_change).
dmsetup info "${vg}-pvmove0"

# Clean up: deactivate lv1, abort the in-progress pvmove.
lvchange -an $vg/$lv1
pvmove --abort

# pvmove0 must be gone after abort.
not dmsetup info "${vg}-pvmove0"
check inactive $vg $lv1

lvremove -ff $vg

# ===================================================================
# Test 2b: LOCKED LV activation refused when pvmove lock held remotely
#
# Same setup as Test 2, but inject remote EX on pvmove0 to simulate
# another cluster node running pvmove.  lv_active_change() tries to
# pre-activate pvmove0 but lockd_lv() fails (remote EX blocks lock
# acquisition in test mode), so _lv_pvmove_is_active() returns 0
# and activation is refused.
# ===================================================================

lvcreate -an -l4 -n $lv1 $vg "$dev1"
check inactive $vg $lv1

lv1_uuid=$(get lv_field $vg/$lv1 lv_uuid)

# Start background pvmove.
LVM_TEST_TAG="kill_me_$PREFIX" pvmove -n $vg/$lv1 -i +3 -b "$dev1" "$dev5"

pvmove0_uuid=$(get lv_field $vg/pvmove0 lv_uuid)

# Kill the poll daemon and remove pvmove0 from DM.
aux kill_tagged_processes
dmsetup remove "${vg}-pvmove0"
not dmsetup info "${vg}-pvmove0"

# Inject remote EX on pvmove0 -- simulates another node running pvmove.
# lm_lock_dlm test mode returns EAGAIN when test_remote_ex is set,
# blocking the pre-activation lock attempt.
lvmlockctl --set-remote-lv-lock $vg --lv-uuid "$pvmove0_uuid" --lock-mode ex

# Pre-activation of pvmove0 fails (remote EX blocks lock).
# _lv_pvmove_is_active(lv1) returns 0 -> activation refused.
not lvchange -ay $vg/$lv1

# Clear remote lock, abort, clean up.
lvmlockctl --set-remote-lv-lock $vg --lv-uuid "$pvmove0_uuid" --lock-mode un
pvmove --abort

not dmsetup info "${vg}-pvmove0"
check inactive $vg $lv1

lvremove -ff $vg

# ===================================================================
# Test 3: pvmove --abort releases pvmove0 lock space in shared VG
#
# pvmove holds persistent EX lock on pvmove0.
# After --abort, pvmove_finish() calls lv_remove(pvmove0) which
# triggers lockd_lvremove_done() to release and queue the lock, then
# lockd_free_removed_lvs() to free the lock space from lvmlockd.
#
# Verification: pvmove0 UUID must be absent from lvmlockctl --info
# after abort, and lv1 must be usable.
# ===================================================================

lvcreate -an -l4 -n $lv1 $vg "$dev1"
lv1_uuid=$(get lv_field $vg/$lv1 lv_uuid)

# Start background pvmove with long poll delay
LVM_TEST_TAG="kill_me_$PREFIX" pvmove -n $vg/$lv1 -i +3 -b "$dev1" "$dev5"

pvmove0_uuid=$(get lv_field $vg/pvmove0 lv_uuid)

# pvmove0 must have persistent EX lock
lvmlockctl --info | tee out
grep "LK LV ex $pvmove0_uuid" out

# Abort pvmove -- must release and free pvmove0 lock space
pvmove --abort

# pvmove0 lock space must be gone completely from lvmlockd
lvmlockctl --info | tee out
not grep "$pvmove0_uuid" out

# No pvmove LV left in metadata
get lv_field $vg name -a | not grep "pvmove"

# lv1 must be activatable (LOCKED cleared by abort)
lvchange -ay $vg/$lv1
check active $vg $lv1

aux kill_tagged_processes
lvchange -an $vg/$lv1
lvremove -ff $vg

# ===================================================================
# Test 4: Unnamed pvmove in shared VG skips remotely-locked LV
#
# lv1 and lv2 both on dev1.  Inject remote EX on lv2.
# Unnamed pvmove from dev1 must:
#   - Acquire EX on lv1's holder (succeeds, no remote lock)
#   - Skip lv2 (remote EX blocks lockd_lv)
#   - Move lv1 to dev5; lv2 stays on dev1
#
# Exercises _skip_remote_lvs() unnamed path in shared VGs.
# ===================================================================

lvcreate -an -l4 -n $lv1 $vg "$dev1"
lvcreate -an -l4 -n $lv2 $vg "$dev1"
lv2_uuid=$(get lv_field $vg/$lv2 lv_uuid)

# Inject remote EX on lv2 -- simulates another node holding it
lvmlockctl --set-remote-lv-lock $vg --lv-uuid "$lv2_uuid" --lock-mode ex

# Unnamed pvmove: must skip lv2 (remotely locked), move lv1
pvmove -i0 "$dev1" "$dev5"

check lv_on $vg $lv1 "$dev5"
check lv_on $vg $lv2 "$dev1"

# Clear remote lock
lvmlockctl --set-remote-lv-lock $vg --lv-uuid "$lv2_uuid" --lock-mode un

get lv_field $vg name -a | not grep "pvmove"

lvremove -ff $vg

# ===================================================================
# Test 4b: Unnamed pvmove fails when ALL LVs are remotely locked
#
# lv1 and lv2 both on dev1.  Inject remote EX on both.
# Unnamed pvmove from dev1 must fail: all LVs skipped, empty mirror,
# "No data to move" error.
# ===================================================================

lvcreate -an -l4 -n $lv1 $vg "$dev1"
lvcreate -an -l4 -n $lv2 $vg "$dev1"
lv1_uuid=$(get lv_field $vg/$lv1 lv_uuid)
lv2_uuid=$(get lv_field $vg/$lv2 lv_uuid)

lvmlockctl --set-remote-lv-lock $vg --lv-uuid "$lv1_uuid" --lock-mode ex
lvmlockctl --set-remote-lv-lock $vg --lv-uuid "$lv2_uuid" --lock-mode ex

not pvmove -i0 "$dev1" "$dev5" 2>&1 | tee err
grep "skipped" err

# Both LVs must still be on dev1
check lv_on $vg $lv1 "$dev1"
check lv_on $vg $lv2 "$dev1"

lvmlockctl --set-remote-lv-lock $vg --lv-uuid "$lv1_uuid" --lock-mode un
lvmlockctl --set-remote-lv-lock $vg --lv-uuid "$lv2_uuid" --lock-mode un

get lv_field $vg name -a | not grep "pvmove"

lvremove -ff $vg

# ===================================================================
# Test 5: Normal pvmove completion releases pvmove0 lock space
#
# Counterpart to Test 3 (abort path): verify that a clean pvmove
# completion also frees the pvmove0 lock space.  pvmove_finish()
# calls lv_remove(pvmove0) -> lockd_lvremove_done() (unlock) ->
# lockd_free_removed_lvs() (free).
#
# Verification: pvmove0 UUID absent from --info after completion,
# no pvmove LV in metadata, lv1 on destination PV.
# ===================================================================

lvcreate -an -l4 -n $lv1 $vg "$dev1"
lv1_uuid=$(get lv_field $vg/$lv1 lv_uuid)

# Start background pvmove with a short initial delay so pvmove0 is
# visible in metadata before the poll daemon cleans it up.
LVM_TEST_TAG="kill_me_$PREFIX" pvmove -n $vg/$lv1 -i +2 -b "$dev1" "$dev5"

pvmove0_uuid=$(get lv_field $vg/pvmove0 lv_uuid)

lvmlockctl --info | tee out
grep "LK LV ex $pvmove0_uuid" out

# Wait for pvmove to complete normally (poll daemon handles it).
i=60; while get lv_field $vg/pvmove0 lv_name -a 2>/dev/null && [ "$i" -gt 0 ] ; do sleep .5; i=$((i-1)); done
test "$i" -ne 0 || die "pvmove did not complete in time"

# pvmove0 lock space must be freed after clean completion
lvmlockctl --info | tee out
not grep "$pvmove0_uuid" out

# No pvmove LV in metadata
get lv_field $vg name -a | not grep "pvmove"

# lv1 must be on dev5 (moved successfully)
check lv_on $vg $lv1 "$dev5"

aux kill_tagged_processes
lvremove -ff $vg

# ===================================================================
# Test 6: Named pvmove is refused when another node holds EX on
#         the named LV
#
# _skip_remote_lvs() acquires EX on the named LV's holder before
# inserting mirror segments.  If another cluster node already holds
# EX (--remote simulates this), lockd_lv() returns EAGAIN and
# pvmove must fail gracefully without modifying metadata.
#
# After clearing the foreign lock, the LV must still be on dev1
# and pvmove must succeed normally.
# ===================================================================

lvcreate -an -l4 -n $lv1 $vg "$dev1"
lv1_uuid=$(get lv_field $vg/$lv1 lv_uuid)

# Inject remote EX: simulates another cluster node holding the lock
# while lv1 is not active locally.  _skip_remote_lvs's lockd_lv("ex")
# gets EAGAIN (test_remote_ex is set on the resource).
lvmlockctl --set-remote-lv-lock $vg --lv-uuid "$lv1_uuid" --lock-mode ex

# pvmove must fail: cannot acquire EX on the named LV
not pvmove -n $vg/$lv1 "$dev1" "$dev5"

# lv1 must still be on dev1, untouched
check lv_on $vg $lv1 "$dev1"
check inactive $vg $lv1

# No pvmove LV created in metadata
get lv_field $vg name -a | not grep "pvmove"

# Clear the remote lock state, lv1 accessible again.
lvmlockctl --set-remote-lv-lock $vg --lv-uuid "$lv1_uuid" --lock-mode un

# pvmove must now succeed with the inactive LV
pvmove -i0 -n $vg/$lv1 "$dev1" "$dev5"
check lv_on $vg $lv1 "$dev5"
check inactive $vg $lv1

lvremove -ff $vg

# ===================================================================
# Test 7: LOCKED LV activation succeeds when pvmove is active
#
# _lv_pvmove_is_active() allows lockd_lv() for LOCKED LVs when
# the associated pvmove LV is active.  Verify that activation and
# deactivation work on the pvmove node.
# ===================================================================

lvcreate -an -l4 -n $lv1 $vg "$dev1"
lv1_uuid=$(get lv_field $vg/$lv1 lv_uuid)

# Start background pvmove
LVM_TEST_TAG="kill_me_$PREFIX" pvmove -n $vg/$lv1 -i +3 -b "$dev1" "$dev5"

pvmove0_uuid=$(get lv_field $vg/pvmove0 lv_uuid)

# pvmove0 must hold EX lock
lvmlockctl --info | tee out
grep "LK LV ex $pvmove0_uuid" out

# Activate lv1 while LOCKED -- _lv_pvmove_is_active() sees pvmove0 active
lvchange -ay $vg/$lv1
check active $vg $lv1

# Deactivate
lvchange -an $vg/$lv1
check inactive $vg $lv1

# Clean up
aux kill_tagged_processes
pvmove --abort
lvremove -ff $vg

# ===================================================================
# Test 8: pvmove of active LV completes successfully
#
# Start with an active LV, run pvmove to completion.
# Verify LV is on the destination and still active.
# pvmove0 lock must be freed.
# ===================================================================

lvcreate -an -l4 -n $lv1 $vg "$dev1"
lv1_uuid=$(get lv_field $vg/$lv1 lv_uuid)

# Activate lv1 before pvmove -- it will be an active participant
lvchange -aey $vg/$lv1
check active $vg $lv1

# LV lock acquired during activation
lvmlockctl --info | tee out
grep "LK LV ex $lv1_uuid" out

# Start pvmove (foreground with -i0 for immediate completion)
pvmove -i0 -n $vg/$lv1 "$dev1" "$dev5"

# pvmove completed: lv1 must be on dev5 and still active
check lv_on $vg $lv1 "$dev5"
check active $vg $lv1

# LV lock persists (held from activation, not pvmove)
lvmlockctl --info | tee out
grep "LK LV ex $lv1_uuid" out

# pvmove0 must be gone
lvchange -an $vg/$lv1
lvremove -ff $vg

# ===================================================================
# Test 9: Named pvmove of already-LOCKED LV is refused
#
# If lv1 is already participating in a pvmove (LOCKED flag set),
# a second pvmove -n targeting the same LV must be refused with
# "already locked by another pvmove".  This guards against double
# pvmove insertion.
# ===================================================================

lvcreate -an -l4 -n $lv1 $vg "$dev1"

# Start first pvmove in background
LVM_TEST_TAG="kill_me_$PREFIX" pvmove -n $vg/$lv1 -i +3 -b "$dev1" "$dev5"

# lv1 is now LOCKED in metadata (pvmove0 exists as proof)
get lv_field $vg/pvmove0 lv_uuid

# Second pvmove targeting the same LV must be refused
not pvmove -n $vg/$lv1 "$dev5" "$dev1" 2>err
grep "locked" err

# Clean up
aux kill_tagged_processes
pvmove --abort
lvremove -ff $vg

# ===================================================================
# Test 10: Persistent pvmove0 lock survives poll daemon death;
#          abort cleans up
#
# Start pvmove in background, kill the poll daemon, verify
# persistent EX lock on pvmove0 survives.
# Then abort the pvmove -- lock must be properly released.
# ===================================================================

lvcreate -aey -l4 -n $lv1 $vg "$dev1"
lv1_uuid=$(get lv_field $vg/$lv1 lv_uuid)

aux mkdev_md5sum $vg $lv1

# Start pvmove with long poll delay
LVM_TEST_TAG="kill_me_$PREFIX" pvmove -n $vg/$lv1 -i +100 -b "$dev1" "$dev5"

pvmove0_uuid=$(get lv_field $vg/pvmove0 lv_uuid)

# pvmove0 lock must be held
lvmlockctl --info | tee out
grep "LK LV ex $pvmove0_uuid" out

# Kill poll daemon -- persistent lock survives in lvmlockd
aux kill_tagged_processes

# Lock must still be present after daemon death
lvmlockctl --info | tee out
grep "LK LV ex $pvmove0_uuid" out

# Abort releases pvmove0 lock
pvmove --abort

lvmlockctl --info | tee out
not grep "$pvmove0_uuid" out

# lv1 must be back on dev1, data intact
check lv_on $vg $lv1 "$dev1"
check active $vg $lv1
check dev_md5sum $vg $lv1

get lv_field $vg name -a | not grep "pvmove"

vgchange -an $vg
lvremove -ff $vg

# ===================================================================
# Test 11: Concurrent pvmove of different LV in shared VG
#
# Start pvmove of lv1, kill daemon, then pvmove lv2.
# In a shared VG the resume path detects that lv2 is not part of
# the in-progress pvmove and creates a separate pvmove (pvmove1)
# for it, allowing concurrent pvmoves of different LVs from the
# same PV.
# ===================================================================

lvcreate -an -l4 -n $lv1 $vg "$dev1"
lvcreate -an -l4 -n $lv2 $vg "$dev1"

# Start pvmove of lv1 only
LVM_TEST_TAG="kill_me_$PREFIX" pvmove -n $vg/$lv1 -i +100 -b "$dev1" "$dev5"

# pvmove0 must exist
get lv_field $vg/pvmove0 lv_uuid

aux kill_tagged_processes

# pvmove of lv2 creates a new pvmove1 (concurrent with pvmove0)
pvmove -i0 -n $vg/$lv2 "$dev1" "$dev5"

# lv2 must have moved to dev5
check lv_on $vg $lv2 "$dev5"

# Original pvmove0 for lv1 must still be in metadata
get lv_field $vg name -a | tee out
grep "pvmove0" out

# Clean up via abort (clears pvmove0 for lv1)
pvmove --abort

get lv_field $vg name -a | not grep "pvmove"

lvremove -ff $vg

# ===================================================================
# Test 12: Named pvmove succeeds with remotely-locked unrelated LV
#
# lv1 and lv2 both on dev1.  Inject remote EX on lv2 to simulate
# another node holding it.  Named pvmove of lv1 must succeed
# because _skip_remote_lvs only checks the named LV, not others.
# lv2 must remain untouched on dev1.
# ===================================================================

lvcreate -an -l4 -n $lv1 $vg "$dev1"
lvcreate -an -l4 -n $lv2 $vg "$dev1"
lv2_uuid=$(get lv_field $vg/$lv2 lv_uuid)

# Inject remote EX on lv2
lvmlockctl --set-remote-lv-lock $vg --lv-uuid "$lv2_uuid" --lock-mode ex

# Named pvmove of lv1 must succeed -- lv2 is not involved
pvmove -i0 -n $vg/$lv1 "$dev1" "$dev5"

check lv_on $vg $lv1 "$dev5"

# lv2 must still be on dev1 (not touched by named pvmove)
check lv_on $vg $lv2 "$dev1"

# Clear remote lock on lv2
lvmlockctl --set-remote-lv-lock $vg --lv-uuid "$lv2_uuid" --lock-mode un

get lv_field $vg name -a | not grep "pvmove"

lvremove -ff $vg

vgremove -ff $vg
