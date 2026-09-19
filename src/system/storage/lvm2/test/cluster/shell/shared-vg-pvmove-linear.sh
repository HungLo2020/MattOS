#!/bin/bash
# lv-lock-pvmove-linear.sh - Test pvmove locking for linear LVs

set -e

if [ "$CLUSTER_NUM_NODES" -lt 2 ]; then
    echo "SKIP: This test requires at least 2 nodes (found $CLUSTER_NUM_NODES)"
    exit 0
fi

node1 vgcreate --shared testvg $dev1 $dev2
nodep vgchange --lockstart testvg

#
# Parallel pvmove races
# all nodes try to pvmove the same LV, only one should succeed
#
for i in $(seq 1 3); do
    node1 lvcreate -L 16M -n lv1 -an testvg $dev1

    nodep pvmove -i0 -n testvg/lv1 $dev1 $dev2 || true
    assert_one_success
    wait_pvmove_done $NODEP_SINGLE_SUCCESS_NODE testvg lv1 $dev1

    success_node lvchange -an testvg/lv1
    node1 lvremove -y testvg/lv1
    sleep 0.3
done

#
# pvmove blocked while LV active on another node
#
node1 lvcreate -L 16M -n lv1 testvg $dev1
verify_lv_active_on 1 testvg lv1

node2 not pvmove -i0 -n testvg/lv1 $dev1 $dev2

node1 lvchange -an testvg/lv1

node2 pvmove -i0 -n testvg/lv1 $dev1 $dev2
node2 lvchange -an testvg/lv1

node1 lvremove -y testvg/lv1

#
# pvmove blocked while LV active shared on another node
#
node1 lvcreate -L 16M -n lv1 -an testvg $dev2

node1 lvchange -asy testvg/lv1
verify_lv_active_on 1 testvg lv1

node2 not pvmove -i0 -n testvg/lv1 $dev2 $dev1

node1 lvchange -an testvg/lv1

node2 pvmove -i0 -n testvg/lv1 $dev2 $dev1
node2 lvchange -an testvg/lv1

node1 lvremove -y testvg/lv1

#
# pvmove cycles
#
node1 lvcreate -L 16M -n lv1 -an testvg $dev1
for i in $(seq 1 2); do
    nodep lvs -a -o+devices testvg/lv1
    nodep pvmove -i0 -n testvg/lv1 $dev1 $dev2 || true
    assert_one_success
    wait_pvmove_done $NODEP_SINGLE_SUCCESS_NODE testvg lv1 $dev1
    success_node lvchange -an testvg/lv1

    nodep lvs -a -o+devices testvg/lv1
    nodep pvmove -i0 -n testvg/lv1 $dev2 $dev1 || true
    assert_one_success
    wait_pvmove_done $NODEP_SINGLE_SUCCESS_NODE testvg lv1 $dev2
    success_node lvchange -an testvg/lv1

    sleep 0.1
done
node1 lvremove -y testvg/lv1

#
# Different nodes pvmove different LVs simultaneously
#
nodep 'lvcreate -L 16M -n lv_${NODE_NUM} -an testvg '"$dev1"
assert_all_success

nodep 'pvmove -i0 -n testvg/lv_${NODE_NUM} '"$dev1 $dev2"
assert_all_success
for node_num in $(seq 1 $CLUSTER_NUM_NODES); do
    wait_pvmove_done 1 testvg lv_${node_num} $dev1
done

nodep 'pvmove -i0 -n testvg/lv_${NODE_NUM} '"$dev2 $dev1"
assert_all_success
for node_num in $(seq 1 $CLUSTER_NUM_NODES); do
    wait_pvmove_done 1 testvg lv_${node_num} $dev2
done

nodep lvchange -an testvg
nodep 'lvremove -y testvg/lv_${NODE_NUM}'
assert_all_success

# Cleanup
cleanup_vg testvg

exit 0
