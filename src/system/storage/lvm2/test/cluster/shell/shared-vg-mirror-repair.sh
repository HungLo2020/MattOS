#!/bin/bash
#
# lv-lock-raid-repair.sh - Test mirror repair locking in shared VG
#
# Tests lvconvert --repair on a degraded mirror in a shared VG.
# Device failure is simulated by setting the SCSI device state to offline
# via sysfs.  Skipped if devices are not SCSI.
#
# dev1 holds lvmlock for sanlock, so tests should take other devs offline
#

set -e

if [ "$CLUSTER_NUM_NODES" -lt 2 ]; then
    echo "SKIP: This test requires at least 2 nodes (found $CLUSTER_NUM_NODES)"
    exit 0
fi

if [ -z "$dev3" ]; then
    echo "SKIP: This test requires at least 3 devices"
    exit 0
fi

# $dev2 is expanded locally to node1's path; the executor translates it
# per-node.  \$ prevents local evaluation of $(basename ...) so the
# remote shell handles it after translation.
if ! node1 test -f "/sys/block/\$(basename $dev2)/device/state"; then
    echo "SKIP: requires SCSI devices with sysfs state control"
    exit 0
fi

node1 vgcreate --shared testvg $dev1 $dev2 $dev3
nodep vgchange --lockstart testvg

#
# lvconvert --repair blocked while mirror active on another node
#
node1 lvcreate --type mirror -m 1 -L 50M -n lv1 testvg $dev1 $dev2
wait_lv_sync_done 1 testvg lv1

node1 lvchange -ay testvg/lv1
verify_lv_active_on 1 testvg lv1

node2 not lvconvert -y --repair testvg/lv1

node1 lvchange -an testvg/lv1

node1 lvremove -y testvg/lv1

#
# RAID degradation and repair
# Create mirror on dev1+dev2, offline dev2 to degrade, repair onto dev3
#
node1 lvcreate --type mirror -m 1 -L 50M -n lv1 testvg $dev1 $dev2
wait_lv_sync_done 1 testvg lv1
node1 lvchange -ay testvg/lv1
verify_lv_active_on 1 testvg lv1

# Simulate device failure on all nodes
# bash -c with escaped quotes keeps the > redirect on the remote side;
# $dev2 expands locally so the executor translates it per-node.
for ni in $(seq 1 $CLUSTER_NUM_NODES); do
    noden ${ni} bash -c "\"echo offline > /sys/block/\$(basename $dev2)/device/state\""
done
sleep 2

node1 lvconvert -y --repair testvg/lv1
wait_lv_sync_done 1 testvg lv1

node1 lvchange -an testvg/lv1
node1 lvremove -y testvg/lv1

# Restore device
for ni in $(seq 1 $CLUSTER_NUM_NODES); do
    noden ${ni} bash -c "\"echo running > /sys/block/\$(basename $dev2)/device/state\""
done
sleep 2

# update stale PV metadata on dev that was offline
node1 vgck --updatemetadata testvg

# Cleanup
cleanup_vg testvg


#
# Test LV mirror repair if the device failure occurs
# while the mirror LV is inactive
#
node1 vgcreate --shared testvg $dev1 $dev2 $dev3
nodep vgchange --lockstart testvg

node1 lvcreate --type mirror -m 1 -L 50M -n lv1 testvg $dev1 $dev2
wait_lv_sync_done 1 testvg lv1
node1 lvchange -an testvg/lv1

# ensure the LV is usable on all nodes
for ni in $(seq 1 $CLUSTER_NUM_NODES); do
    noden ${ni} lvchange -ay testvg/lv1
    noden ${ni} dd if=/dev/testvg/lv1 of=/dev/null bs=1M count=4 iflag=direct
    noden ${ni} lvchange -an testvg/lv1
done

# Simulate device failure on all nodes
# bash -c with escaped quotes keeps the > redirect on the remote side;
# $dev2 expands locally so the executor translates it per-node.
for ni in $(seq 1 $CLUSTER_NUM_NODES); do
    noden ${ni} bash -c "\"echo offline > /sys/block/\$(basename $dev2)/device/state\""
done
sleep 2

# The first node to lock the LV will replace the image,
# but not activate the LV to begin resync, which will
# happen the next time the LV is activated.
#
# The other nodes will lock the LV and report:
# Volume VG/LV is consistent. Nothing to repair.
#
# The lvconvert returns success in both cases.
nodep lvs -a -o+devices testvg
nodep lvconvert -y --repair testvg/lv1

# syncing the replacement dev happens here by
# whichever node activates the LV
nodep lvchange -ay testvg/lv1 || true
assert_one_success
wait_lv_sync_done "$NODEP_SINGLE_SUCCESS_NODE" testvg lv1
success_node lvchange -an testvg/lv1

# ensure the LV is usable on all nodes
for ni in $(seq 1 $CLUSTER_NUM_NODES); do
    noden ${ni} lvchange -ay testvg/lv1
    noden ${ni} dd if=/dev/testvg/lv1 of=/dev/null bs=1M count=4 iflag=direct
    noden ${ni} lvchange -an testvg/lv1
done

# Restore device
for ni in $(seq 1 $CLUSTER_NUM_NODES); do
    noden ${ni} bash -c "\"echo running > /sys/block/\$(basename $dev2)/device/state\""
done
sleep 2

# update stale PV metadata on dev that was offline
node1 vgck --updatemetadata testvg

# Cleanup
cleanup_vg testvg

exit 0
