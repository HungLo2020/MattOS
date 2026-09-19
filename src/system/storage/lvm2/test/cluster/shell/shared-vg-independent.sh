#!/bin/bash

set -e

if [ "$CLUSTER_NUM_NODES" -lt 2 ]; then
    echo "SKIP: This test requires at least 2 nodes (found $CLUSTER_NUM_NODES)"
    exit 0
fi

node1 vgcreate --shared testvg $dev1 $dev2
nodep vgchange --lockstart testvg

#
# nodes activate different LVs
#
for node_num in $(seq 1 $CLUSTER_NUM_NODES); do
    noden ${node_num} lvcreate -l1 -n lva_${node_num} testvg
done

for node_num in $(seq 1 $CLUSTER_NUM_NODES); do
    noden ${node_num} lvcreate -l1 -n lvb_${node_num} testvg
done

for node_num in $(seq 1 $CLUSTER_NUM_NODES); do
    verify_lv_active_on ${node_num} testvg lva_${node_num}
    verify_lv_active_on ${node_num} testvg lvb_${node_num}
done

for node_num in $(seq 1 $CLUSTER_NUM_NODES); do
    noden ${node_num} lvchange -an testvg/lva_${node_num}
    noden ${node_num} lvchange -an testvg/lvb_${node_num}
done

for node_num in $(seq 1 $CLUSTER_NUM_NODES); do
    noden ${node_num} lvchange -ay testvg/lva_${node_num}
    noden ${node_num} lvchange -ay testvg/lvb_${node_num}
done

for node_num in $(seq 1 $CLUSTER_NUM_NODES); do
    verify_lv_active_on ${node_num} testvg lva_${node_num}
    verify_lv_active_on ${node_num} testvg lvb_${node_num}
done

nodep vgchange -an testvg

#
# nodes activate different LVs in parallel
#
for i in $(seq 1 3); do
    nodep 'lvchange -ay testvg/lva_${NODE_NUM} testvg/lvb_${NODE_NUM}'
    assert_all_success

    for node_num in $(seq 1 $CLUSTER_NUM_NODES); do
        verify_lv_active_on "$node_num" testvg lva_${node_num}
        verify_lv_active_on "$node_num" testvg lvb_${node_num}
    done

    nodep 'lvchange -an testvg/lva_${NODE_NUM} testvg/lvb_${NODE_NUM}'
    assert_all_success

    sleep 0.3
done

#
# nodes activate different LVs in parallel (no vg lock)
#
for i in $(seq 1 3); do
    nodep 'lvchange -ay --lockopt skipvg testvg/lva_${NODE_NUM} testvg/lvb_${NODE_NUM}'
    assert_all_success

    for node_num in $(seq 1 $CLUSTER_NUM_NODES); do
        verify_lv_active_on "$node_num" testvg lva_${node_num}
        verify_lv_active_on "$node_num" testvg lvb_${node_num}
    done

    nodep 'lvchange -an --lockopt skipvg testvg/lva_${NODE_NUM} testvg/lvb_${NODE_NUM}'
    assert_all_success

    sleep 0.3
done

nodep vgchange -an testvg
node1 lvremove -y testvg

# Cleanup
cleanup_vg testvg

exit 0
