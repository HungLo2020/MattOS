#!/bin/bash

set -e

if [ "$CLUSTER_NUM_NODES" -lt 2 ]; then
    echo "SKIP: This test requires at least 2 nodes (found $CLUSTER_NUM_NODES)"
    exit 0
fi

node1 vgcreate --shared testvg $dev1 $dev2
nodep vgchange --lockstart testvg

#
# LOCKED LV activation refused from another node
#
# node1 starts background pvmove (sets LOCKED on lv1, pvmove0 active
# on node1). node2 activation of lv1 fails because pvmove0 is not
# active on node2.
#

node1 lvcreate -L 16M -n lv1 -an testvg $dev1

node1 pvmove -b -i +60 -n testvg/lv1 $dev1 $dev2

node2 not lvchange -aey testvg/lv1
verify_lv_not_active_on 2 testvg lv1

node1 pvmove --abort
node1 lvremove -y testvg/lv1

#
# pvmove --abort releases pvmove0 lock
#
# node1 starts background pvmove, then aborts. After abort, pvmove0
# is gone and node2 can activate the LV.
#

node1 lvcreate -L 16M -n lv1 -an testvg $dev1

node1 pvmove -b -i +60 -n testvg/lv1 $dev1 $dev2
node1 pvmove --abort

# lv1 must be on dev1 (abort reverses the move)
node1 lvs --noheadings -o devices testvg/lv1 | grep -qF "$dev1"

# lv1 must be activatable (LOCKED cleared by abort)
node2 lvchange -aey testvg/lv1
verify_lv_active_on 2 testvg lv1
node2 lvchange -an testvg/lv1

node1 lvremove -y testvg/lv1

#
# Unnamed pvmove skips remotely-locked LV
#
# node1 activates lv2 exclusively (real EX lock). node2 runs unnamed
# pvmove from dev1: skips lv2 (locked by node1), moves lv1.
#

node1 lvcreate -L 16M -n lv1 -an testvg $dev1
node1 lvcreate -L 16M -n lv2 -an testvg $dev1

node1 lvchange -aey testvg/lv2
verify_lv_active_on 1 testvg lv2

node2 pvmove -i0 $dev1 $dev2

node1 lvs --noheadings -o devices testvg/lv1 | grep -qF "$dev2"
node1 lvs --noheadings -o devices testvg/lv2 | grep -qF "$dev1"

node1 lvchange -an testvg/lv2
node1 lvremove -y testvg/lv1
node1 lvremove -y testvg/lv2

#
# Unnamed pvmove fails when ALL LVs are remotely locked
#
# node1 activates both lv1 and lv2. node2 unnamed pvmove fails
# because all LVs on dev1 are skipped.
#

node1 lvcreate -L 16M -n lv1 -an testvg $dev1
node1 lvcreate -L 16M -n lv2 -an testvg $dev1

node1 lvchange -aey testvg/lv1
node1 lvchange -aey testvg/lv2

node2 not pvmove -i0 $dev1 $dev2

node1 lvs --noheadings -o devices testvg/lv1 | grep -qF "$dev1"
node1 lvs --noheadings -o devices testvg/lv2 | grep -qF "$dev1"

node1 lvchange -an testvg/lv1
node1 lvchange -an testvg/lv2
node1 lvremove -y testvg/lv1
node1 lvremove -y testvg/lv2

#
# Normal pvmove completion releases pvmove0 lock
#
# node1 runs foreground pvmove, verify clean completion and
# the LV is on the destination.
#

node1 lvcreate -L 16M -n lv1 -an testvg $dev1

node1 pvmove -i0 -n testvg/lv1 $dev1 $dev2

node1 lvs --noheadings -o devices testvg/lv1 | grep -qF "$dev2"

# LV must be activatable after pvmove (LOCKED cleared)
node1 lvchange -aey testvg/lv1
verify_lv_active_on 1 testvg lv1
node1 lvchange -an testvg/lv1

node1 lvremove -y testvg/lv1

#
# Named pvmove refused when remote EX on target LV
#
# node1 activates lv1. node2 pvmove -n lv1 fails because node1
# holds EX. After node1 deactivates, node2 pvmove succeeds.
#

node1 lvcreate -L 16M -n lv1 -an testvg $dev1

node1 lvchange -aey testvg/lv1
verify_lv_active_on 1 testvg lv1

node2 not pvmove -i0 -n testvg/lv1 $dev1 $dev2

# lv1 must still be on dev1
node1 lvs --noheadings -o devices testvg/lv1 | grep -qF "$dev1"

node1 lvchange -an testvg/lv1

node2 pvmove -i0 -n testvg/lv1 $dev1 $dev2
node1 lvs --noheadings -o devices testvg/lv1 | grep -qF "$dev2"

node1 lvremove -y testvg/lv1

#
# LOCKED LV activation succeeds when pvmove active locally
#
# node1 starts background pvmove, then node1 activates the LV.
# Succeeds because pvmove0 is active on node1.
#

node1 lvcreate -L 16M -n lv1 -an testvg $dev1

node1 pvmove -b -i +60 -n testvg/lv1 $dev1 $dev2

node1 lvchange -aey testvg/lv1
verify_lv_active_on 1 testvg lv1

node1 lvchange -an testvg/lv1
node1 pvmove --abort
node1 lvremove -y testvg/lv1

#
# pvmove of active LV completes successfully
#
# node1 activates LV, then pvmoves it. LV remains active on
# the destination.
#

node1 lvcreate -L 16M -n lv1 -an testvg $dev1

node1 lvchange -aey testvg/lv1
verify_lv_active_on 1 testvg lv1

node1 pvmove -i0 -n testvg/lv1 $dev1 $dev2

node1 lvs --noheadings -o devices testvg/lv1 | grep -qF "$dev2"
verify_lv_active_on 1 testvg lv1

node1 lvchange -an testvg/lv1
node1 lvremove -y testvg/lv1

#
# Double pvmove of same LV refused from another node
#
# node1 starts background pvmove of lv1. node2 tries pvmove of the
# same lv1 and fails (already LOCKED by node1's pvmove).
#

node1 lvcreate -L 16M -n lv1 -an testvg $dev1

node1 pvmove -b -i +60 -n testvg/lv1 $dev1 $dev2

node2 not pvmove -i0 -n testvg/lv1 $dev2 $dev1

node1 pvmove --abort
node1 lvremove -y testvg/lv1

#
# Concurrent pvmove of different LVs from same PV
#
# node1 background pvmove of lv1. node2 pvmove -n lv2 from same PV
# succeeds by creating a separate pvmove. Exercises the concurrent
# pvmove fix for shared VGs.
#

node1 lvcreate -L 16M -n lv1 -an testvg $dev1
node1 lvcreate -L 16M -n lv2 -an testvg $dev1

node1 pvmove -b -i +60 -n testvg/lv1 $dev1 $dev2

node2 pvmove -i0 -n testvg/lv2 $dev1 $dev2

node1 lvs --noheadings -o devices testvg/lv2 | grep -qF "$dev2"

node1 pvmove --abort
node1 lvremove -y testvg/lv1
node1 lvremove -y testvg/lv2

#
# Named pvmove succeeds with unrelated remotely-locked LV
#
# node1 activates lv2. node2 pvmove -n lv1 succeeds because pvmove
# only checks the named LV, not other LVs on the same PV.
#

node1 lvcreate -L 16M -n lv1 -an testvg $dev1
node1 lvcreate -L 16M -n lv2 -an testvg $dev1

node1 lvchange -aey testvg/lv2
verify_lv_active_on 1 testvg lv2

node2 pvmove -i0 -n testvg/lv1 $dev1 $dev2

node1 lvs --noheadings -o devices testvg/lv1 | grep -qF "$dev2"
node1 lvs --noheadings -o devices testvg/lv2 | grep -qF "$dev1"

node1 lvchange -an testvg/lv2
node1 lvremove -y testvg/lv1
node1 lvremove -y testvg/lv2

# Cleanup
cleanup_vg testvg

exit 0
