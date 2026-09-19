#!/bin/bash
# lv-lock-lvremove-thin.sh - Test lvremove locking for thin volumes and pools

set -e

if [ "$CLUSTER_NUM_NODES" -lt 2 ]; then
    echo "SKIP: This test requires at least 2 nodes (found $CLUSTER_NUM_NODES)"
    exit 0
fi

node1 vgcreate --shared testvg $dev1 $dev2
nodep vgchange --lockstart testvg

#
# Pool already active - only that node can remove thin volumes
#
node1 lvcreate --type thin-pool -L 100M -n tpool testvg
for i in 1 2 3; do
    node1 lvcreate --type thin -V 50M --thinpool tpool -n thin${i} testvg
done

nodep lvremove -y testvg/thin1 || true
assert_node_success 1
assert_one_success

node1 lvremove -y testvg/thin2

node2 not lvremove -y testvg/thin3

node1 lvremove -y testvg/thin3
node1 lvchange -an testvg/tpool
node1 lvremove -y testvg/tpool

#
# Pool NOT active - race to remove thin volume
#
for iter in $(seq 1 3); do
    node1 lvcreate --type thin-pool -L 100M -n tpool testvg
    for i in 1 2 3; do
        node1 lvcreate --type thin -V 50M --thinpool tpool -n thin${i} testvg
    done

    node1 vgchange -an testvg

    nodep lvremove -y testvg/thin1 || true
    assert_one_success

    nodep lvremove -y testvg/thin2 || true
    assert_one_success

    nodep lvremove -y testvg/thin3 || true
    assert_one_success

    nodep lvremove -y testvg/tpool || true
    assert_one_success
    sleep 0.3
done

#
# Cannot remove thin volume while active
#
node1 lvcreate --type thin-pool -L 100M -n tpool testvg
node1 lvcreate --type thin -V 50M --thinpool tpool -n thin1 testvg
node2 not lvremove -y testvg/thin1
node1 lvchange -an testvg/thin1
node1 lvchange -an testvg/tpool
node2 lvremove -y testvg/thin1
node1 lvremove -y testvg/tpool

# Cleanup
cleanup_vg testvg

exit 0
