#!/bin/bash
# lv-lock-lvremove-linear.sh - Test lvremove locking for linear LVs

set -e

if [ "$CLUSTER_NUM_NODES" -lt 2 ]; then
    echo "SKIP: This test requires at least 2 nodes (found $CLUSTER_NUM_NODES)"
    exit 0
fi

node1 vgcreate --shared testvg $dev1 $dev2
nodep vgchange --lockstart testvg

#
# Parallel lvremove races
#
for i in $(seq 1 5); do
    node1 lvcreate -L 50M -n lv1 testvg

    nodep lvremove -y testvg/lv1 || true
    assert_one_success

    node1 not lvs testvg/lv1 2>/dev/null
    sleep 0.3
done

#
# Cannot remove LV while active on another node
#
node1 lvcreate -L 50M -n lv1 testvg
node2 not lvremove -y testvg/lv1
node1 lvchange -an testvg/lv1
node2 lvremove -y testvg/lv1

# Cleanup
cleanup_vg testvg

exit 0
