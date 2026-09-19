#!/bin/bash
#
# lv-lock-lvreduce --fs ignore-linear.sh - Test lvreduce --fs ignore locking for linear LVs
#

set -e

if [ "$CLUSTER_NUM_NODES" -lt 2 ]; then
    echo "SKIP: This test requires at least 2 nodes (found $CLUSTER_NUM_NODES)"
    exit 0
fi

node1 vgcreate --shared testvg $dev1 $dev2
nodep vgchange --lockstart testvg

#
# Parallel lvreduce --fs ignore races with absolute size on inactive linear LV
#
for i in $(seq 1 3); do
    node1 lvcreate -L 100M -n lv1 -an testvg

    nodep lvreduce --fs ignore -y -L64M testvg/lv1 || true
    assert_one_success

    node1 lvremove -y testvg/lv1
    sleep 0.3
done

#
# Parallel lvreduce --fs ignore races with relative size on inactive linear LV
#
for i in $(seq 1 3); do
    node1 lvcreate -L 100M -n lv1 -an testvg

    nodep lvreduce --fs ignore -y -L-4M testvg/lv1
    assert_all_success

    node1 lvremove -y testvg/lv1
    sleep 0.3
done

#
# lvreduce --fs ignore blocked while LV is active on another node
#
node1 lvcreate -L 100M -n lv1 testvg

node1 lvchange -ay testvg/lv1
verify_lv_active_on 1 testvg lv1

node2 not lvreduce --fs ignore -y -L-4M testvg/lv1

node1 lvchange -an testvg/lv1

node2 lvreduce --fs ignore -y -L-4M testvg/lv1

node1 lvremove -y testvg/lv1

#
# lvreduce --fs ignore blocked while LV is active shared on another node
#
node1 lvcreate -L 100M -n lv1 testvg

node1 lvchange -asy testvg/lv1
verify_lv_active_on 1 testvg lv1

node2 not lvreduce --fs ignore -y -L-4M testvg/lv1

node1 lvchange -an testvg/lv1

node2 lvreduce --fs ignore -y -L-4M testvg/lv1

node1 lvremove -y testvg/lv1

# Cleanup
cleanup_vg testvg

exit 0
