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

# Test that "-missing_*" error-target DM devices created for degraded
# sub-LVs are automatically cleaned up by the DM tree mechanism when
# the sub-LV tables are reconfigured to no longer reference them.
#
# When a PV disappears and an LV is refreshed with --activationmode partial,
# the activation layer creates error-target DM devices named
# <VG>-<sublv>-missing_<segno>_<area>.  When the PV returns and the LV
# is refreshed again, these missing-device nodes are removed by the
# DM tree CLEAN action via _add_missing_devs_to_clean_tree().

. lib/inittest --skip-with-lvmpolld --skip-with-lvmlockd

aux have_raid 1 3 0 || skip
aux prepare_vg 5 80

_get_missing_devs() {
	dmsetup ls 2>/dev/null | grep "$PREFIX" | grep 'missing_' || true
}

# Check no missing-device DM nodes exist for this VG
_check_no_missing_devs() {
	local devs
	devs=$(_get_missing_devs)
	if [ -n "$devs" ]; then
		echo "ERROR: leftover missing-device DM nodes found:"
		echo "$devs"
		return 1
	fi
}

# Verify missing-device DM nodes DO exist (test precondition)
_check_missing_devs_exist() {
	local devs
	devs=$(_get_missing_devs)
	if [ -z "$devs" ]; then
		echo "ERROR: expected missing-device DM nodes but found none"
		dmsetup ls 2>/dev/null | grep "$PREFIX" || true
		return 1
	fi
	echo "Found expected missing-device DM nodes:"
	echo "$devs"
}

##################################################
# RAID1: missing devs cleaned up after PV restore
##################################################
# Cleanup is handled by the DM tree CLEAN mechanism.

lvcreate --type raid1 -m 1 -L 8 -n $lv1 $vg "$dev1" "$dev2"
aux wait_for_sync $vg $lv1

# Remove PV device -- makes PV invisible to LVM
aux disable_dev "$dev2"

# Refresh with partial activation -- forces error-target creation
# for rimage_1 whose PV is now missing
lvchange --refresh --activationmode partial $vg/$lv1

# Precondition: missing-device nodes must exist at this point
_check_missing_devs_exist

# Restore PV and refresh -- tables rebuilt without error device
aux enable_dev "$dev2"
lvchange --refresh $vg/$lv1 -vvvv

# Missing-device nodes should be gone after refresh
_check_no_missing_devs

lvremove -ff $vg/$lv1

####################################################
# Mirror: missing devs leaked after PV restore (BUG)
####################################################
# The DM tree CLEAN action handles this for mirrors too.

aux mirror_recovery_works || skip "mirror recovery broken"

lvcreate -aey --type mirror -m 1 --ignoremonitoring -L 8 -n $lv1 $vg "$dev1" "$dev2" "$dev3":0
aux wait_for_sync $vg $lv1

# Remove PV device
aux disable_dev "$dev2"

# Refresh with partial activation -- creates missing_0_0 for mimage_1
lvchange --refresh --activationmode partial $vg/$lv1

# Precondition: missing-device nodes must exist
_check_missing_devs_exist

# Restore PV and refresh -- tables rebuilt without error device
aux enable_dev "$dev2"
lvchange --refresh $vg/$lv1

# Missing-device nodes should be cleaned up by DM tree CLEAN
_check_no_missing_devs

# Also verify deactivation cleans up any stragglers
lvchange -an $vg/$lv1
_check_no_missing_devs

lvremove -ff $vg/$lv1

#################################################################
# RAID1: leaked missing devs after dropping damaged spanning leg
#################################################################
# Create raid1 on 2 PVs, extend onto 2 more so each image spans
# 2 PVs.  Lose 1 PV so one leg is partially missing, drop the
# leg via repair, and check the missing_N_M device is cleaned up.

lvcreate --type raid1 -m 1 --nosync -L 8 -n $lv1 $vg "$dev1" "$dev2"
lvextend --nosync -L+8 $vg/$lv1 "$dev3" "$dev4"

aux disable_dev "$dev4"

lvchange --refresh --activationmode partial $vg/$lv1

_check_missing_devs_exist

lvconvert --repair -y $vg/$lv1

_check_no_missing_devs

vgreduce --removemissing $vg
aux enable_dev "$dev4"

lvremove -ff $vg/$lv1

vgremove -ff $vg
