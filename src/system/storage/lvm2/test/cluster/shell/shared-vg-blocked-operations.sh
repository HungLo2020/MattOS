#!/bin/bash
# lv-lock-blocked-operations.sh - Verify operations blocked in shared VGs

set -e

if [ "$CLUSTER_NUM_NODES" -lt 2 ]; then
    echo "SKIP: This test requires at least 2 nodes (found $CLUSTER_NUM_NODES)"
    exit 0
fi

node1 vgcreate --shared testvg $dev1 $dev2
nodep vgchange --lockstart testvg

#
# Snapshot split blocked
#
node1 lvcreate -L 100M -n origin testvg
node1 lvcreate --snapshot --size 50M -n snap1 testvg/origin

node1 not lvconvert --splitsnapshot testvg/snap1

# Verify both still exist (split didn't happen)
node1 lvs testvg/origin > /dev/null
node1 lvs testvg/snap1 > /dev/null

node1 lvremove -y testvg/snap1
node1 lvremove -y testvg/origin

#
# Mirror split blocked
#
node1 lvcreate --type mirror -m 1 -L 100M -n mirrorlv testvg

node1 not lvconvert --splitmirrors 1 -n split_image testvg/mirrorlv

# Verify mirror exists, split_image does not
node1 lvs testvg/mirrorlv > /dev/null
node1 not lvs testvg/split_image 2>/dev/null

node1 lvremove -y testvg/mirrorlv

#
# External origin conversion blocked
#
node1 lvcreate -L 100M -n origin testvg
node1 lvcreate --type thin-pool -L 100M -n tpool testvg

node1 not lvconvert --type thin --thinpool testvg/tpool testvg/origin -y

# Verify origin is still regular LV (not converted)
node1 lvs -o lv_layout --noheadings testvg/origin | grep -q "linear\|striped"

node1 lvremove -y testvg/origin
node1 lvremove -y testvg/tpool

# Cleanup
cleanup_vg testvg

exit 0
