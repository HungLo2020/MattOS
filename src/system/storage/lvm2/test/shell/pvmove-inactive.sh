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

test_description="pvmove does not activate inactive LVs"

. lib/inittest

which md5sum || skip

aux prepare_vg 5

# ===================================================================
# Test 1: pvmove -n moves only the named LV when it is inactive;
#         other LVs on the source PV are not touched.
#
# lv1 on dev1 (active)   -- not named, must stay on dev1 and active
# lv2 on dev1 (inactive) -- named LV, must move to dev5 and stay inactive
#
# Expectation: pvmove moves only lv2 to dev5;
#              lv1 remains on dev1, active, data intact;
#              lv2 ends up on dev5, stays inactive until activated
#
# Runs with clustered locking (uses -n).
# ===================================================================

lvcreate -aey -l4 -n $lv1 $vg "$dev1"
lvcreate -aey -l4 -n $lv2 $vg "$dev1"
check lv_on $vg $lv1 "$dev1"
check lv_on $vg $lv2 "$dev1"

aux mkdev_md5sum $vg $lv1
aux mkdev_md5sum $vg $lv2

lvchange -an $vg/$lv2
check inactive $vg $lv2

pvmove -i0 -n $vg/$lv2 "$dev1" "$dev5"

# lv1 must be active and still on dev1 (not moved)
check active $vg $lv1
check lv_on $vg $lv1 "$dev1"

# lv2 must be inactive and on dev5
check inactive $vg $lv2
check lv_on $vg $lv2 "$dev5"

# No leftover pvmove LV
get lv_field $vg name -a >out
not grep "^\[pvmove" out

# Data integrity for active LV
check dev_md5sum $vg $lv1

# Activate lv2 and verify data
lvchange -ay $vg/$lv2
check dev_md5sum $vg $lv2

vgchange -an $vg
lvremove -ff $vg

# ===================================================================
# Test 2: pvmove abort preserves data integrity
#
# lv1 on dev1 (active)
# Start pvmove in background, then abort
# lv1 must remain active and consistent
#
# Runs with clustered locking (uses -n).
# ===================================================================

lvcreate -aey -l4 -n $lv1 $vg "$dev1"
aux mkdev_md5sum $vg $lv1

LVM_TEST_TAG="kill_me_$PREFIX" pvmove -i +3 -b -n $vg/$lv1 "$dev1" "$dev5"
pvmove --abort

check active $vg $lv1
check dev_md5sum $vg $lv1

get lv_field $vg name -a >out
not grep "^\[pvmove" out

aux kill_tagged_processes
vgchange -an $vg
lvremove -ff $vg

# ===================================================================
# Test 3: Deactivate unrelated LV while pvmove of named LV is in progress
#
# lv1 on dev1 (active, not being pvmoved) -- deactivated during pvmove
# lv2 on dev1 (active, being pvmoved with -n)
# Start pvmove -n lv2 in background, deactivate lv1, finish pvmove
#
# Expectation: lv2 moves to dev5 and stays active; lv1 is inactive
#              on dev1 (unmoved); pvmove completes correctly.
#
# Note: deactivating the named LV itself during pvmove is NOT tested
# here because that LV is the sole user of pvmove0; deactivating it
# cascade-deactivates pvmove0 (open_count -> 0), stopping the mirror.
# That scenario requires pvmove0 to survive independently of its user
# LVs -- a future improvement.
#
# Runs with clustered locking (uses -n).
# ===================================================================

lvcreate -aey -l4 -n $lv1 $vg "$dev1"
lvcreate -aey -l4 -n $lv2 $vg "$dev1"

aux mkdev_md5sum $vg $lv1
aux mkdev_md5sum $vg $lv2

# Start pvmove of lv2 only; +2 delays first check by 2 seconds,
# giving us time to deactivate lv1 before pvmove completes.
LVM_TEST_TAG="kill_me_$PREFIX" pvmove -i +2 -b -n $vg/$lv2 "$dev1" "$dev5"

# Deactivate lv1 (not being pvmoved) while pvmove mirror is running
lvchange -an $vg/$lv1
check inactive $vg $lv1
check active $vg $lv2

# pvmove mirror must still exist
get lv_field $vg name -a >out
grep "\[pvmove" out

# Wait for pvmove to complete.
# pvmove -i0 is NOT used here because with lvmpolld it would loop forever
# waiting on an already-registered poll operation managed by lvmpolld.
for i in {50..0} ; do
	lvs -ao name $vg | grep "\[pvmove" || break
	sleep .1
done
test "$i" -ne 0 || die "pvmove did not complete in time"

# lv2 must have moved to dev5 and stayed active; lv1 inactive on dev1
check active $vg $lv2
check inactive $vg $lv1

check lv_on $vg $lv2 "$dev5"
check lv_on $vg $lv1 "$dev1"

check dev_md5sum $vg $lv2

# Activate lv1 and verify data
lvchange -ay $vg/$lv1
check dev_md5sum $vg $lv1

get lv_field $vg name -a >out
not grep "^\[pvmove" out
aux kill_tagged_processes
vgchange -an $vg
lvremove -ff $vg

# ===================================================================
# Test 4: vgchange -an fails when pvmove is in progress;
#         vgchange -ff -an interrupts and deactivates
#
# lv1 on dev1 (active)
# Start pvmove -n in background, attempt vgchange -an (must fail),
# then vgchange -ff -an (must succeed, interrupting pvmove)
#
# Expectation: vgchange -an refuses, pvmove LV stays active;
#              vgchange -ff -an deactivates everything with warning;
#              lvremove -ff removes lv1 and cleans up orphaned pvmove LV from metadata
#
# Runs with clustered locking (uses -n).
# ===================================================================

lvcreate -aey -l4 -n $lv1 $vg "$dev1"
aux mkdev_md5sum $vg $lv1

# Start pvmove in background; +2 delays first poll check
LVM_TEST_TAG="kill_me_$PREFIX" pvmove -i +2 -b -n $vg/$lv1 "$dev1" "$dev5"

# pvmove mirror must be active
get lv_field $vg name -a >out
grep "\[pvmove" out

# vgchange -an must fail while pvmove is running
not vgchange -an $vg

# pvmove mirror must still be active after failed vgchange -an
get lv_field $vg name -a >out
grep "\[pvmove" out
check active $vg $lv1

# vgchange -ff -an must succeed, interrupting pvmove
vgchange -ff -an $vg

# everything must be inactive now
check inactive $vg $lv1
not dmsetup info "${vg}-pvmove0"

lvremove -ff $vg

# pvmove LV must be gone from metadata after lvremove removed the last participant
get lv_field $vg name -a >out
not grep "\[pvmove" out

aux kill_tagged_processes

# ===================================================================
# Tests 5-8 move all LVs from a PV without naming them.
# pvmove in a shared VG currently requires a named LV (see FIXME in
# tools/pvmove.c), so these tests only run in non-clustered configs.
# ===================================================================
[ "${LVM_TEST_LVMLOCKD:-0}" = 0 ] || { vgremove -ff $vg; exit 0; }

# ===================================================================
# Test 5: Mix of active and inactive LVs
#
# lv1 on dev1+dev2 (active)
# lv2 on dev2+dev3 (inactive)
# pvmove from dev2 to dev5
#
# Expectation: lv1 stays active, lv2 stays inactive,
#              both end up on dev5 after pvmove
# ===================================================================

# lv1: 2 extents on dev1 + 2 extents on dev2
lvcreate -aey -l2 -n $lv1 $vg "$dev1"
lvextend -l+2 $vg/$lv1 "$dev2"
# lv2: 2 extents on dev2 + 2 extents on dev3
lvcreate -aey -l2 -n $lv2 $vg "$dev2"
lvextend -l+2 $vg/$lv2 "$dev3"
check lv_on $vg $lv1 "$dev1" "$dev2"
check lv_on $vg $lv2 "$dev2" "$dev3"

aux mkdev_md5sum $vg $lv1
aux mkdev_md5sum $vg $lv2

# Deactivate lv2
lvchange -an $vg/$lv2
check inactive $vg $lv2

pvmove -i0 "$dev2" "$dev5"

# lv1 must still be active, lv2 must still be inactive
check active $vg $lv1
check inactive $vg $lv2

# Both LVs must have moved off dev2
check lv_on $vg $lv1 "$dev1" "$dev5"
check lv_on $vg $lv2 "$dev5" "$dev3"

# Data integrity for the active LV
check dev_md5sum $vg $lv1

# Activate lv2 and verify data integrity
lvchange -ay $vg/$lv2
check dev_md5sum $vg $lv2

get lv_field $vg name -a >out
not grep "^\[pvmove" out
vgchange -an $vg
lvremove -ff $vg

# ===================================================================
# Test 6: All LVs inactive
#
# lv1 on dev1+dev2 (inactive)
# lv2 on dev2+dev3 (inactive)
# pvmove from dev2 to dev5
#
# Expectation: both stay inactive, data moved correctly
# ===================================================================

# lv1: 2 extents on dev1 + 2 extents on dev2
lvcreate -aey -l2 -n $lv1 $vg "$dev1"
lvextend -l+2 $vg/$lv1 "$dev2"
# lv2: 2 extents on dev2 + 2 extents on dev3
lvcreate -aey -l2 -n $lv2 $vg "$dev2"
lvextend -l+2 $vg/$lv2 "$dev3"
check lv_on $vg $lv1 "$dev1" "$dev2"
check lv_on $vg $lv2 "$dev2" "$dev3"

aux mkdev_md5sum $vg $lv1
aux mkdev_md5sum $vg $lv2

# Deactivate both LVs
lvchange -an $vg/$lv1
lvchange -an $vg/$lv2
check inactive $vg $lv1
check inactive $vg $lv2

pvmove -i0 "$dev2" "$dev5"

# Both must still be inactive
check inactive $vg $lv1
check inactive $vg $lv2

# Both must have moved off dev2
check lv_on $vg $lv1 "$dev1" "$dev5"
check lv_on $vg $lv2 "$dev5" "$dev3"

# Activate and verify data
lvchange -ay $vg/$lv1
lvchange -ay $vg/$lv2
check dev_md5sum $vg $lv1
check dev_md5sum $vg $lv2

get lv_field $vg name -a >out
not grep "^\[pvmove" out

vgchange -an $vg
lvremove -ff $vg

# ===================================================================
# Test 7: Inactive LV is first in segment order
#
# lv1 on dev1 only (inactive) - first LV created
# lv2 on dev1+dev2 (active)
# pvmove from dev1 to dev5
#
# This tests the _update_metadata() fix: lv1 is inactive and may
# appear first in lvs_changed, but lv2 (active) must still get
# its DM tables reloaded through the pvmove mirror.
# ===================================================================

# lv1: 4 extents on dev1 only (single PV)
lvcreate -aey -l4 -n $lv1 $vg "$dev1"
# lv2: 2 extents on dev1 + 2 extents on dev2 (forced spanning)
lvcreate -aey -l2 -n $lv2 $vg "$dev1"
lvextend -l+2 $vg/$lv2 "$dev2"
check lv_on $vg $lv1 "$dev1"
check lv_on $vg $lv2 "$dev1" "$dev2"

aux mkdev_md5sum $vg $lv1
aux mkdev_md5sum $vg $lv2

# Deactivate lv1 (first in segment order)
lvchange -an $vg/$lv1
check inactive $vg $lv1

pvmove -i0 "$dev1" "$dev5"

# lv1 inactive, lv2 active
check inactive $vg $lv1
check active $vg $lv2

check lv_on $vg $lv1 "$dev5"
check lv_on $vg $lv2 "$dev5" "$dev2"

check dev_md5sum $vg $lv2

lvchange -ay $vg/$lv1
check dev_md5sum $vg $lv1

vgchange -an $vg
lvremove -ff $vg

# ===================================================================
# Test 8: pvmove with inactive thin-pool sub-LV on source PV
#
# Create thin-pool with tmeta/tdata on dev1, thin LV active
# Create linear LV on dev1 (active)
# Deactivate thin-pool, keep linear active
# pvmove from dev1 to dev5
#
# Expectation: thin-pool stays inactive, linear LV stays active,
#              all data moved correctly
# ===================================================================

if aux have_thin 1 0 0 ; then

lvcreate -T $vg/pool -l8 -V8 -n thin1 "$dev1"
lvcreate -aey -l2 -n $lv1 $vg "$dev1"
check lv_tree_on $vg thin1 "$dev1"
check lv_on $vg $lv1 "$dev1"

aux mkdev_md5sum $vg $lv1

# Deactivate thin-pool (and thin volumes), keep linear active
lvchange -an $vg/thin1
check inactive $vg thin1

pvmove -i0 "$dev1" "$dev5"

# Linear must still be active
check active $vg $lv1

# Everything must have moved off dev1
check lv_on $vg $lv1 "$dev5"
check lv_tree_on $vg thin1 "$dev5"

check dev_md5sum $vg $lv1

# Activate thin and verify
lvchange -ay $vg/thin1
check active $vg thin1

vgchange -an $vg
lvremove -ff $vg

fi

# ===================================================================
# Test 8b: pvmove with ACTIVE thin-pool (component LV deactivation)
#
# thin-pool stays active during pvmove so pvmove_finish sees
# invisible component LVs (_tmeta, _tdata) and exercises the
# component deactivation loop.
# ===================================================================

if aux have_thin 1 0 0 ; then

lvcreate -T $vg/pool -l8 -V8 -n thin1 "$dev1"
lvcreate -aey -l2 -n $lv1 $vg "$dev1"
check lv_tree_on $vg thin1 "$dev1"

aux mkdev_md5sum $vg $lv1

# Keep thin-pool active during pvmove
check active $vg thin1

pvmove -i0 "$dev1" "$dev5"

check active $vg $lv1
check active $vg thin1
check lv_on $vg $lv1 "$dev5"
check lv_tree_on $vg thin1 "$dev5"

check dev_md5sum $vg $lv1

# No leftover pvmove LV
get lv_field $vg name -a >out
not grep "^\[pvmove" out

vgchange -an $vg
lvremove -ff $vg

fi

# ===================================================================
# Test 8c: pvmove of RAID1 LV (PV exclusion list coverage)
#
# RAID LVs require PV exclusion list to maintain redundancy during
# pvmove allocation.  This exercises _allow_sibling_pv_collocation
# and _exclude_pvs code paths.
# ===================================================================

if aux have_raid 1 3 5 ; then

# RAID1 needs images on separate PVs: rimage_0 on dev1, rimage_1 on dev2
lvcreate --type raid1 -m1 -l4 -n $lv1 $vg "$dev1" "$dev2"
check lv_on $vg ${lv1}_rimage_0 "$dev1"
check lv_on $vg ${lv1}_rimage_1 "$dev2"

aux mkdev_md5sum $vg $lv1

# pvmove dev1 to dev3: must maintain RAID1 redundancy
# (rimage_0 moves to dev3, rimage_1 stays on dev2)
pvmove -i0 -n $vg/$lv1 "$dev1" "$dev3"

check lv_on $vg ${lv1}_rimage_0 "$dev3"
check lv_on $vg ${lv1}_rimage_1 "$dev2"

check dev_md5sum $vg $lv1

get lv_field $vg name -a >out
not grep "^\[pvmove" out

vgchange -an $vg
lvremove -ff $vg

fi

# ===================================================================
# Test 9: Abort with mixed active/inactive LVs
#
# lv1 on dev1 (active)
# lv2 on dev1 (inactive)
# Start pvmove in background, then abort
#
# Expectation: both LVs revert to dev1, lv1 stays active,
#              lv2 stays inactive, data intact
# ===================================================================

lvcreate -aey -l4 -n $lv1 $vg "$dev1"
lvcreate -aey -l4 -n $lv2 $vg "$dev1"

aux mkdev_md5sum $vg $lv1
aux mkdev_md5sum $vg $lv2

lvchange -an $vg/$lv2
check inactive $vg $lv2

LVM_TEST_TAG="kill_me_$PREFIX" pvmove -i +3 -b "$dev1" "$dev5"

# pvmove mirror must be active
get lv_field $vg name -a >out
grep "\[pvmove" out

pvmove --abort

check active $vg $lv1
check inactive $vg $lv2
check lv_on $vg $lv1 "$dev1"
check lv_on $vg $lv2 "$dev1"

get lv_field $vg name -a >out
not grep "^\[pvmove" out

check dev_md5sum $vg $lv1
lvchange -ay $vg/$lv2
check dev_md5sum $vg $lv2

aux kill_tagged_processes
vgchange -an $vg
lvremove -ff $vg

# ===================================================================
# Test 10: Concurrent pvmove skips locked LVs
#
# lv1 spans dev1+dev2 (active) -- locked by first pvmove on dev2
# lv2 on dev1 only (active) -- moved by second pvmove
#
# First pvmove: dev2 to dev5 (locks lv1)
# Named pvmove -n lv1 dev1 dev4: must fail (lv1 locked)
# Unnamed pvmove dev1 dev4: skips locked lv1, moves only lv2
# Abort first pvmove: lv1 reverts to dev1+dev2
# ===================================================================

lvcreate -aey -l2 -n $lv1 $vg "$dev1"
lvextend -l+2 $vg/$lv1 "$dev2"
lvcreate -aey -l4 -n $lv2 $vg "$dev1"
check lv_on $vg $lv1 "$dev1" "$dev2"
check lv_on $vg $lv2 "$dev1"

aux mkdev_md5sum $vg $lv1
aux mkdev_md5sum $vg $lv2

# First pvmove on dev2 (locks lv1)
LVM_TEST_TAG="kill_me_$PREFIX" pvmove -i +3 -b "$dev2" "$dev5"

# Named pvmove of locked lv1 must fail
not pvmove -i0 -n $vg/$lv1 "$dev1" "$dev4"

# Unnamed pvmove from dev1 skips locked lv1, moves lv2
pvmove -i0 "$dev1" "$dev4"
check lv_on $vg $lv2 "$dev4"

# Abort first pvmove (lv1 reverts to dev1+dev2)
pvmove --abort

check lv_on $vg $lv1 "$dev1" "$dev2"

check dev_md5sum $vg $lv1
check dev_md5sum $vg $lv2

get lv_field $vg name -a >out
not grep "^\[pvmove" out

aux kill_tagged_processes
vgchange -an $vg
lvremove -ff $vg

# ===================================================================
# Test 11: Resume pvmove preserves inactive state
#
# lv1 on dev1 (active)
# lv2 on dev1 (inactive)
# Start pvmove in background, kill daemon, resume
#
# Expectation: both end up on dev5; lv1 stays active,
#              lv2 stays inactive, data intact.
#              With lvmpolld the operation may finish before
#              the resume -- the result is the same.
# ===================================================================

lvcreate -aey -l4 -n $lv1 $vg "$dev1"
lvcreate -aey -l4 -n $lv2 $vg "$dev1"

aux mkdev_md5sum $vg $lv1
aux mkdev_md5sum $vg $lv2

lvchange -an $vg/$lv2
check inactive $vg $lv2

# Start pvmove with delayed polling
LVM_TEST_TAG="kill_me_$PREFIX" pvmove -i +3 -b "$dev1" "$dev5"

aux kill_tagged_processes

# Restart lvmpolld to clear stale in_progress registration
# (without this, pvmove -i0 resume loops forever on progress_info)
test -e LOCAL_LVMPOLLD && aux prepare_lvmpolld

# Resume (or no-op if lvmpolld already finished)
pvmove -i0 "$dev1" "$dev5"

check active $vg $lv1
check inactive $vg $lv2
check lv_on $vg $lv1 "$dev5"
check lv_on $vg $lv2 "$dev5"

get lv_field $vg name -a >out
not grep "^\[pvmove" out

check dev_md5sum $vg $lv1
lvchange -ay $vg/$lv2
check dev_md5sum $vg $lv2

vgchange -an $vg
lvremove -ff $vg

# ===================================================================
# Test 12: pvmove refuses to move writecache cachevol PV
#
# Create a writecache LV with cachevol on dev2.
# pvmove from dev2 must fail (cachevol cannot be moved).
# ===================================================================

if aux have_writecache 1 0 0 ; then

lvcreate -an -l4 -n $lv1 $vg "$dev1"
lvcreate -an -l4 -n $lv2 $vg "$dev2"
lvconvert --yes --type writecache --cachevol $lv2 $vg/$lv1
lvchange -aey $vg/$lv1

# pvmove from cachevol PV must fail
not pvmove -i0 "$dev2" "$dev3" 2>&1 | tee err
grep -i "writecache" err

vgchange -an $vg
lvremove -ff $vg

fi # have_writecache

# ===================================================================
# Test 13: Named pvmove of locked LV must fail with error message
#
# lv1 spans dev1+dev2 (active)
# Start pvmove from dev2 (background) -- locks lv1
# Named pvmove -n lv1 from dev1 must fail: "locked by another pvmove"
#
# Verifies the error path in _find_moving_lvs when a named LV is
# already LOCKED by a concurrent pvmove on a different source PV.
# ===================================================================

lvcreate -aey -l2 -n $lv1 $vg "$dev1"
lvextend -l+2 $vg/$lv1 "$dev2"

LVM_TEST_TAG="kill_me_$PREFIX" pvmove -i +3 -b "$dev2" "$dev5"

not pvmove -i0 -n $vg/$lv1 "$dev1" "$dev3" 2>&1 | tee err
grep "locked" err

pvmove --abort

aux kill_tagged_processes
vgchange -an $vg
lvremove -ff $vg

# ===================================================================
# Test 14: Empty pvmove mirror when all LVs on source PV are locked
#
# lv1, lv2 both span dev1+dev2 (active)
# Start pvmove from dev2 (background) -- locks both lv1 and lv2
# Unnamed pvmove from dev1 must fail: all LVs skipped, empty mirror
#
# Verifies _finalize_pvmove_lv "All data on source PV skipped" error.
# ===================================================================

lvcreate -aey -l2 -n $lv1 $vg "$dev1"
lvextend -l+2 $vg/$lv1 "$dev2"
lvcreate -aey -l2 -n $lv2 $vg "$dev1"
lvextend -l+2 $vg/$lv2 "$dev2"

LVM_TEST_TAG="kill_me_$PREFIX" pvmove -i +3 -b "$dev2" "$dev5"

not pvmove -i0 "$dev1" "$dev3" 2>&1 | tee err
grep "skipped" err

pvmove --abort

aux kill_tagged_processes
vgchange -an $vg
lvremove -ff $vg

# ===================================================================
# Test 15: RAID sub-LV named pvmove (sibling PV exclusion coverage)
#
# Create RAID1 lv1: rimage_0 on dev1, rimage_1 on dev2
# pvmove -n rimage_0 from dev1 to dev3
#
# By naming the sub-LV directly, _allow_sibling_pv_collocation
# finds rmeta_0 as the sibling and removes its PV from the exclusion list.
# This exercises the RAID collocation PV exclusion code path.
# ===================================================================

if aux have_raid 1 3 5 ; then

lvcreate --type raid1 -m1 -l4 -n $lv1 $vg "$dev1" "$dev2"
check lv_on $vg ${lv1}_rimage_0 "$dev1"
check lv_on $vg ${lv1}_rimage_1 "$dev2"

aux mkdev_md5sum $vg $lv1

pvmove -i0 -n $vg/${lv1}_rimage_0 "$dev1" "$dev3"

check lv_on $vg ${lv1}_rimage_0 "$dev3"
check lv_on $vg ${lv1}_rimage_1 "$dev2"

check dev_md5sum $vg $lv1

get lv_field $vg name -a >out
not grep "^\[pvmove" out

vgchange -an $vg
lvremove -ff $vg

fi # have_raid

# ===================================================================
# Test 16: Inactive RAID holder produces verbose message
#
# Create RAID1 lv1 on dev1+dev2, deactivate it.
# Unnamed pvmove from dev1 to dev5.
#
# Because the holder (RAID LV) is inactive and holder != sub-LV,
# _lv_is_allowed_pvmove prints "Holder %s is inactive" verbose msg.
# The pvmove completes via metadata-only mirror insertion.
# ===================================================================

if aux have_raid 1 3 5 ; then

lvcreate --type raid1 -m1 -l4 -n $lv1 $vg "$dev1" "$dev2"
aux mkdev_md5sum $vg $lv1

vgchange -an $vg

pvmove -v -i0 "$dev1" "$dev5" 2>&1 | tee out
grep -i "inactive" out

check lv_on $vg ${lv1}_rimage_0 "$dev5"
check lv_on $vg ${lv1}_rimage_1 "$dev2"

lvchange -ay $vg/$lv1
check dev_md5sum $vg $lv1

vgchange -an $vg
lvremove -ff $vg

fi # have_raid

vgremove -ff $vg
