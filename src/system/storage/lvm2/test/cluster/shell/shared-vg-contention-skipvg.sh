#!/bin/bash

set -e

if [ "$CLUSTER_NUM_NODES" -lt 2 ]; then
    echo "SKIP: This test requires at least 2 nodes (found $CLUSTER_NUM_NODES)"
    exit 0
fi

node1 vgcreate --shared testvg $dev1 $dev2
nodep vgchange --lockstart testvg

#
# Baseline activation races without skipvg
#
node1 lvcreate -L 100M -n lv1 -an testvg

for i in $(seq 1 5); do
    nodep lvchange -ay testvg/lv1 || true
    assert_one_success

    success_node lvchange -an testvg/lv1
    sleep 0.1
done

for node_num in $(seq 1 $CLUSTER_NUM_NODES); do
    verify_lv_not_active_on ${node_num} testvg lv1
done

#
# Activation races with --lockopt skipvg
#
for i in $(seq 1 10); do
    nodep lvchange --lockopt skipvg -ay testvg/lv1 || true
    assert_one_success

    success_node lvchange -an testvg/lv1
    sleep 0.1
done

for node_num in $(seq 1 $CLUSTER_NUM_NODES); do
    verify_lv_not_active_on ${node_num} testvg lv1
done

#
# Rapid activation cycles with skipvg
#
for i in $(seq 1 15); do
    nodep lvchange --lockopt skipvg -ay testvg/lv1 || true
    assert_one_success

    success_node lvchange --lockopt skipvg -an testvg/lv1
done

#
# Exclusive lv lock preserved with skipvg
#
node1 lvchange --lockopt skipvg -ay testvg/lv1
verify_lv_active_on 1 testvg lv1

for node_num in $(seq 2 $CLUSTER_NUM_NODES); do
    noden ${node_num} not lvchange -ay testvg/lv1
    noden ${node_num} not lvchange --lockopt skipvg -ay testvg/lv1
    noden ${node_num} not lvextend -L+4M testvg/lv1
    noden ${node_num} not lvextend --lockopt skipvg -L+4M testvg/lv1
    noden ${node_num} not lvremove -y testvg/lv1
    noden ${node_num} not lvremove -y --lockopt skipvg testvg/lv1
done

node1 lvchange -an testvg/lv1

# Cleanup
node1 lvremove -y testvg/lv1
cleanup_vg testvg

exit 0
