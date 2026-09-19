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

# Test manual repair on transient majority mirror/raid leg failure.
#
# Uses 5 PVs (raid1 needs extra for metadata LVs).



. lib/inittest --skip-with-lvmpolld

MOUNT_DIR=mnt

aux mirror_recovery_works || skip

cleanup_mounted_and_teardown()
{
	umount "$MOUNT_DIR" 2>/dev/null || true
	aux teardown
}

trap 'cleanup_mounted_and_teardown' EXIT

aux prepare_vg 5

# Mirror (synced) - majority transient failure + manual repair
test_mirror_synced_manual_repair()
{
	lvcreate -aey --type mirror -m 3 --ignoremonitoring -L 1 -n $lv1 $vg \
		"$dev1" "$dev2" "$dev3" "$dev4"
	aux wait_for_sync $vg $lv1

	mkfs.ext3 "$DM_DEV_DIR/$vg/$lv1"
	mkdir -p "$MOUNT_DIR"
	mount "$DM_DEV_DIR/$vg/$lv1" "$MOUNT_DIR"
	echo "test data mirror synced" > "$MOUNT_DIR/testfile"
	umount "$MOUNT_DIR"

	aux disable_dev --error "$dev2" "$dev3" "$dev4"

	echo y | lvconvert --repair $vg/$lv1

	aux enable_dev "$dev2" "$dev3" "$dev4"
	vgreduce --removemissing $vg

	check linear $vg $lv1
	lvs -a -o +devices $vg

	mount "$DM_DEV_DIR/$vg/$lv1" "$MOUNT_DIR"
	grep "test data mirror synced" "$MOUNT_DIR/testfile"
	umount "$MOUNT_DIR"
	fsck -n "$DM_DEV_DIR/$vg/$lv1"

	lvremove -ff $vg
}

# Mirror (nosync) - majority transient failure + manual repair
test_mirror_nosync_manual_repair()
{
	lvcreate -aey --type mirror -m 3 --ignoremonitoring --nosync -L 1 -n $lv1 $vg \
		"$dev1" "$dev2" "$dev3" "$dev4"

	mkfs.ext3 "$DM_DEV_DIR/$vg/$lv1"
	mkdir -p "$MOUNT_DIR"
	mount "$DM_DEV_DIR/$vg/$lv1" "$MOUNT_DIR"
	echo "test data mirror nosync" > "$MOUNT_DIR/testfile"
	umount "$MOUNT_DIR"

	aux disable_dev --error "$dev2" "$dev3" "$dev4"

	echo y | lvconvert --repair $vg/$lv1

	aux enable_dev "$dev2" "$dev3" "$dev4"
	vgreduce --removemissing $vg

	check linear $vg $lv1

	mount "$DM_DEV_DIR/$vg/$lv1" "$MOUNT_DIR"
	grep "test data mirror nosync" "$MOUNT_DIR/testfile"
	umount "$MOUNT_DIR"
	fsck -n "$DM_DEV_DIR/$vg/$lv1"

	lvremove -ff $vg
}

# Raid1 (synced) - majority transient failure + manual repair
test_raid1_synced_manual_repair()
{
	lvcreate -aey --type raid1 -m 3 --ignoremonitoring -L 1 -n $lv1 $vg
	aux wait_for_sync $vg $lv1

	mkfs.ext3 "$DM_DEV_DIR/$vg/$lv1"
	mkdir -p "$MOUNT_DIR"
	mount "$DM_DEV_DIR/$vg/$lv1" "$MOUNT_DIR"
	echo "test data raid1 synced" > "$MOUNT_DIR/testfile"
	umount "$MOUNT_DIR"

	aux error_dev "$dev2"
	aux error_dev "$dev3"
	aux error_dev "$dev4"

	lvconvert --yes --repair $vg/$lv1 || true

	lvchange -an $vg/$lv1

	aux enable_dev "$dev2"
	aux enable_dev "$dev3"
	aux enable_dev "$dev4"

	# FIXME - this needs some more work
	should lvconvert --yes --repair $vg/$lv1

	#lvchange -ay $vg/$lv1

	#mount "$DM_DEV_DIR/$vg/$lv1" "$MOUNT_DIR"
	#grep "test data raid1 synced" "$MOUNT_DIR/testfile"
	#umount "$MOUNT_DIR"
	#fsck -n "$DM_DEV_DIR/$vg/$lv1"

	lvremove -ff $vg
}

# Raid1 (nosync) - majority transient failure + manual repair
test_raid1_nosync_manual_repair()
{
	lvcreate -aey --type raid1 -m 3 --ignoremonitoring --nosync -L 1 -n $lv1 $vg

	mkfs.ext3 "$DM_DEV_DIR/$vg/$lv1"
	mkdir -p "$MOUNT_DIR"
	mount "$DM_DEV_DIR/$vg/$lv1" "$MOUNT_DIR"
	echo "test data raid1 nosync" > "$MOUNT_DIR/testfile"
	umount "$MOUNT_DIR"

	aux error_dev "$dev2"
	aux error_dev "$dev3"
	aux error_dev "$dev4"

	lvconvert --yes --repair $vg/$lv1 || true

	lvchange -an $vg/$lv1

	aux enable_dev "$dev2"
	aux enable_dev "$dev3"
	aux enable_dev "$dev4"

	lvconvert --yes --repair $vg/$lv1

	lvchange -ay $vg/$lv1

	mount "$DM_DEV_DIR/$vg/$lv1" "$MOUNT_DIR"
	grep "test data raid1 nosync" "$MOUNT_DIR/testfile"
	umount "$MOUNT_DIR"
	fsck -n "$DM_DEV_DIR/$vg/$lv1"

	lvremove -ff $vg
}

#####################################################################
# Run tests
#####################################################################

test_mirror_synced_manual_repair
test_mirror_nosync_manual_repair
aux have_raid 1 3 0 && test_raid1_synced_manual_repair
#test_raid1_nosync_manual_repair

vgremove -ff $vg
