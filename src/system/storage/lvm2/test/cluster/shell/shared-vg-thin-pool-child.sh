#!/bin/bash
# lv-lock-thin-pool-child.sh - Test thin pool and thin volume parent-child locking
#

set -e

if [ "$CLUSTER_NUM_NODES" -lt 2 ]; then
    echo "SKIP: This test requires at least 2 nodes (found $CLUSTER_NUM_NODES)"
    exit 0
fi

node1 vgcreate --shared testvg $dev1 $dev2
nodep vgchange --lockstart testvg

#
# Create thin-pool and thin volumes for testing
#
node1 lvcreate --type thin-pool -L 100M -n tpool testvg

for i in 1 2 3; do
    node1 lvcreate --type thin -V 50M --thinpool tpool -n thin${i} testvg
done

node1 lvchange -an testvg/tpool
node1 lvchange -an testvg/thin1
node1 lvchange -an testvg/thin2
node1 lvchange -an testvg/thin3

#
# Parallel thin volume activation races (pool NOT active)
#
for i in $(seq 1 5); do
    nodep lvchange -ay testvg/thin1 || true
    assert_one_success

    verify_lv_active_on "$NODEP_SINGLE_SUCCESS_NODE" testvg thin1

    for node_num in $NODEP_FAIL_NODES; do
        verify_lv_not_active_on "$node_num" testvg thin1
    done

    success_node lvchange -an testvg/thin1

    sleep 0.5
done

#
# Pool already active - only that node can activate thin volumes
#
node1 lvchange -ay testvg/tpool

nodep lvchange -ay testvg/thin1 || true
assert_node_success 1
assert_one_success

node1 lvchange -an testvg/thin1
node1 lvchange -an testvg/tpool

#
# Different thin volumes in same pool - pool lock prevents concurrent access
#
node1 lvchange -ay testvg/thin1
verify_lv_active_on 1 testvg thin1

node2 not lvchange -ay testvg/thin2
node2 not lvchange -ay testvg/thin3

node1 lvchange -an testvg/thin1

node2 lvchange -ay testvg/thin2
verify_lv_active_on 2 testvg thin2

node1 not lvchange -ay testvg/thin1

node2 lvchange -an testvg/thin2

#
# Rapid activation cycles
#
for i in $(seq 1 10); do
    nodep lvchange -ay testvg/thin1 || true
    assert_one_success

    success_node lvchange -an testvg/thin1

    sleep 0.1
done

#
# Cleanup
cleanup_vg testvg

exit 0
