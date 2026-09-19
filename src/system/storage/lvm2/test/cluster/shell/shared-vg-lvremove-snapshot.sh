#!/bin/bash
# lv-lock-lvremove-snapshot.sh - Test lvremove locking for COW snapshots

set -e

if [ "$CLUSTER_NUM_NODES" -lt 2 ]; then
    echo "SKIP: This test requires at least 2 nodes (found $CLUSTER_NUM_NODES)"
    exit 0
fi

node1 vgcreate --shared testvg $dev1 $dev2
nodep vgchange --lockstart testvg

#
# lvremove snapshot leaves origin intact
#
node1 lvcreate -L 50M -n lv1 testvg
node1 lvcreate --snapshot --size 20M -n snap1 testvg/lv1

verify_lv_has_lock_args testvg lv1
verify_lv_no_lock_args testvg snap1
node1 lvremove -y testvg/snap1
node1 not lvs testvg/snap1 2>/dev/null
verify_lv_has_lock_args testvg lv1
node1 lvremove -y testvg/lv1

#
# Parallel lvremove races for snapshots
#
for i in $(seq 1 3); do
    node1 lvcreate -L 50M -n lv1 testvg
    node1 lvcreate --snapshot --size 20M -n snap1 testvg/lv1
    node1 lvchange -an testvg/lv1

    nodep lvremove -y testvg/snap1 || true
    assert_one_success

    node_rand not lvs testvg/snap1 2>/dev/null
    node_rand lvremove -y testvg/lv1
    sleep 0.3
done

#
# Cannot remove snapshot while active elsewhere
#
node1 lvcreate -L 50M -n lv1 testvg
node1 lvcreate --snapshot --size 20M -n snap1 testvg/lv1
node2 not lvremove -y testvg/snap1
node2 not lvremove -y testvg/lv1
node1 lvchange -an -y testvg/snap1
node2 lvremove -y testvg/snap1
node1 lvremove -y testvg/lv1

#
# Multiple snapshots of same origin
#
node1 lvcreate -L 50M -n lv1 -an testvg
for node_num in $(seq 1 $CLUSTER_NUM_NODES); do
    # have each node create a snap on the same origin
    noden ${node_num} lvcreate --snapshot --size 20M -n snap_${node_num} testvg/lv1
    noden ${node_num} lvchange -an testvg/lv1
done

# remove each snap from a random node
for node_num in $(seq 1 $CLUSTER_NUM_NODES); do
    node_rand lvremove -y testvg/snap_${node_num}
done

# verify all snaps are removed
for node_num in $(seq 1 $CLUSTER_NUM_NODES); do
    node1 not lvs testvg/snap_${node_num} 2>/dev/null
done
node_rand lvremove -y testvg/lv1

# Cleanup
cleanup_vg testvg

exit 0
