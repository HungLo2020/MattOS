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

test_description='pvmove RAID redundancy preservation and sibling collocation'

. lib/inittest

aux have_raid 1 3 5 || skip

# 6 PVs: dev1-dev4 for RAID layout, dev5-dev6 as move targets
aux prepare_vg 6 20

# ===================================================================
# Test 1: RAID1 pvmove refuses to collocate mirror legs
#
# RAID1: rimage_0 on dev1, rimage_1 on dev2
# Moving rimage_0 to dev2 would break redundancy -- must fail.
# ===================================================================

lvcreate --type raid1 -m 1 -l 2 --regionsize 16K -n $lv1 $vg "$dev1" "$dev2"
check lv_on $vg ${lv1}_rimage_0 "$dev1"
check lv_on $vg ${lv1}_rimage_1 "$dev2"

not pvmove -n ${lv1}_rimage_0 "$dev1" "$dev2"
check lv_on $vg ${lv1}_rimage_0 "$dev1"
check lv_on $vg ${lv1}_rimage_1 "$dev2"

lvremove -ff $vg

# ===================================================================
# Test 2: RAID1 pvmove to a free PV preserves redundancy
#
# RAID1: rimage_0 on dev1, rimage_1 on dev2
# Move rimage_0 to dev5 -- must succeed, legs on dev5 and dev2.
# ===================================================================

lvcreate --type raid1 -m 1 -l 2 --regionsize 16K -n $lv1 $vg "$dev1" "$dev2"
pvmove -i0 -n ${lv1}_rimage_0 "$dev1" "$dev5"
check lv_on $vg ${lv1}_rimage_0 "$dev5"
check lv_on $vg ${lv1}_rimage_1 "$dev2"

lvremove -ff $vg

# ===================================================================
# Test 3: RAID1 sibling collocation -- rmeta follows rimage
#
# RAID1: rimage_0+rmeta_0 on dev1, rimage_1+rmeta_1 on dev2
# Move rimage_0 to dev5 -- rmeta_0 stays on dev1 (only rimage moves).
# Then move rmeta_0 to dev5 -- allowed because rimage_0 is there
# (sibling collocation permits rmeta on same PV as its rimage).
# ===================================================================

lvcreate --type raid1 -m 1 -l 2 --regionsize 16K -n $lv1 $vg "$dev1" "$dev2"
check lv_on $vg ${lv1}_rimage_0 "$dev1"
check lv_on $vg ${lv1}_rmeta_0 "$dev1"

pvmove -i0 -n ${lv1}_rimage_0 "$dev1" "$dev5"
check lv_on $vg ${lv1}_rimage_0 "$dev5"
check lv_on $vg ${lv1}_rmeta_0 "$dev1"

# Move rmeta_0 to dev5 (where rimage_0 lives) -- sibling collocation
pvmove -i0 -n ${lv1}_rmeta_0 "$dev1" "$dev5"
check lv_on $vg ${lv1}_rmeta_0 "$dev5"
check lv_on $vg ${lv1}_rimage_0 "$dev5"

lvremove -ff $vg

# ===================================================================
# Test 4: RAID1 rmeta cannot move to the other leg's PV
#
# RAID1: rimage_0+rmeta_0 on dev1, rimage_1+rmeta_1 on dev2
# Moving rmeta_0 to dev2 would collocate with rimage_1 -- must fail
# (PV exclusion prevents it since dev2 holds the other leg).
# ===================================================================

lvcreate --type raid1 -m 1 -l 2 --regionsize 16K -n $lv1 $vg "$dev1" "$dev2"

not pvmove -i0 -n ${lv1}_rmeta_0 "$dev1" "$dev2"
check lv_on $vg ${lv1}_rmeta_0 "$dev1"

lvremove -ff $vg

# ===================================================================
# Test 5: RAID10 pvmove preserves stripe and mirror redundancy
#
# RAID10 -i2 -m1: 4 legs on dev1-dev4
# rimage_0 on dev1, rimage_1 on dev2 (stripe pair, mirror of each other)
# rimage_2 on dev3, rimage_3 on dev4 (stripe pair, mirror of each other)
#
# Moving dev1 to dev5 must keep redundancy: rimage_0 moves to dev5.
# ===================================================================

lvcreate --type raid10 -m 1 -l 4 -i 2 -n $lv1 $vg "$dev1" "$dev2" "$dev3" "$dev4"
check lv_tree_on $vg $lv1 "$dev1" "$dev2" "$dev3" "$dev4"

pvmove -i0 "$dev1" "$dev5"
check lv_tree_on $vg $lv1 "$dev2" "$dev3" "$dev4" "$dev5"

lvremove -ff $vg

# ===================================================================
# Test 6: RAID10 refuses cross-mirror collocation
#
# RAID10: rimage_0 on dev1, rimage_1 on dev2 (mirrors of each other)
# Moving rimage_0 to dev2 breaks redundancy -- must fail.
# ===================================================================

lvcreate --type raid10 -m 1 -l 4 -i 2 -n $lv1 $vg "$dev1" "$dev2" "$dev3" "$dev4"

not pvmove -i0 -n ${lv1}_rimage_0 "$dev1" "$dev2"
check lv_on $vg ${lv1}_rimage_0 "$dev1"

# Also cannot cross stripe boundaries
not pvmove -i0 -n ${lv1}_rimage_0 "$dev1" "$dev3"
check lv_on $vg ${lv1}_rimage_0 "$dev1"

lvremove -ff $vg

# ===================================================================
# Test 7: RAID5 pvmove preserves parity distribution
#
# RAID5 -i2: 3 legs on dev1, dev2, dev3
# Moving dev1 to dev5 must succeed, result on dev2, dev3, dev5.
# Moving to dev2 or dev3 must fail (breaks parity redundancy).
# ===================================================================

lvcreate --type raid5 -l 4 -i 2 -n $lv1 $vg "$dev1" "$dev2" "$dev3"
check lv_tree_on $vg $lv1 "$dev1" "$dev2" "$dev3"

not pvmove -i0 -n ${lv1}_rimage_0 "$dev1" "$dev2"
not pvmove -i0 -n ${lv1}_rimage_0 "$dev1" "$dev3"
check lv_on $vg ${lv1}_rimage_0 "$dev1"

pvmove -i0 -n ${lv1}_rimage_0 "$dev1" "$dev5"
check lv_on $vg ${lv1}_rimage_0 "$dev5"
check lv_tree_on $vg $lv1 "$dev2" "$dev3" "$dev5"

lvremove -ff $vg

# ===================================================================
# Test 8: Unnamed pvmove with multiple RAID LVs on same PV
#
# RAID1 lv1: rimage_0 on dev1, rimage_1 on dev2
# RAID1 lv2: rimage_0 on dev1, rimage_1 on dev3
# Unnamed pvmove of dev1 must move both LVs' rimage_0 to dev5.
# ===================================================================

lvcreate --type raid1 -m 1 -l 2 --regionsize 16K -n $lv1 $vg "$dev1" "$dev2"
lvcreate --type raid1 -m 1 -l 2 --regionsize 16K -n $lv2 $vg "$dev1" "$dev3"
check lv_on $vg ${lv1}_rimage_0 "$dev1"
check lv_on $vg ${lv2}_rimage_0 "$dev1"

pvmove -i0 "$dev1" "$dev5"

check lv_on $vg ${lv1}_rimage_0 "$dev5"
check lv_on $vg ${lv1}_rimage_1 "$dev2"
check lv_on $vg ${lv2}_rimage_0 "$dev5"
check lv_on $vg ${lv2}_rimage_1 "$dev3"

lvremove -ff $vg

# ===================================================================
# Test 9: Per-LV RAID PV exclusion with constrained destinations
#
# RAID1 lv1: rimage_0 on dev1, rimage_1 on dev2
# RAID1 lv2: rimage_0 on dev1, rimage_1 on dev3
# Unnamed pvmove from dev1 with destinations limited to dev2+dev3.
# Global exclusion would remove BOTH dev2 (lv1) and dev3 (lv2),
# leaving no allocation space.  Per-LV exclusion lets lv1 use
# dev3 and lv2 use dev2 -- each avoids only its own sibling's PV.
# ===================================================================

lvcreate --type raid1 -m 1 -l 2 --regionsize 16K -n $lv1 $vg "$dev1" "$dev2"
lvcreate --type raid1 -m 1 -l 2 --regionsize 16K -n $lv2 $vg "$dev1" "$dev3"
check lv_on $vg ${lv1}_rimage_0 "$dev1"
check lv_on $vg ${lv1}_rimage_1 "$dev2"
check lv_on $vg ${lv2}_rimage_0 "$dev1"
check lv_on $vg ${lv2}_rimage_1 "$dev3"

pvmove -i0 "$dev1" "$dev2" "$dev3"

check lv_on $vg ${lv1}_rimage_0 "$dev3"
check lv_on $vg ${lv1}_rimage_1 "$dev2"
check lv_on $vg ${lv2}_rimage_0 "$dev2"
check lv_on $vg ${lv2}_rimage_1 "$dev3"

lvremove -ff $vg

# ===================================================================
# Test 10: RAID1 inactive pvmove preserves redundancy
#
# Same as Test 2 but with inactive LV.
# ===================================================================

lvcreate -an --type raid1 -m 1 -l 2 --regionsize 16K -n $lv1 $vg "$dev1" "$dev2"
pvmove -i0 -n ${lv1}_rimage_0 "$dev1" "$dev5"
check lv_on $vg ${lv1}_rimage_0 "$dev5"
check lv_on $vg ${lv1}_rimage_1 "$dev2"
check inactive $vg $lv1

lvremove -ff $vg

vgremove -ff $vg
