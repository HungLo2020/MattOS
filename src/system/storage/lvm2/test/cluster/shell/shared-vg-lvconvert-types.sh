#!/bin/bash
# lv-lock-lvconvert-types.sh - Test parallel lvconvert type conversions

set -e

if [ "$CLUSTER_NUM_NODES" -lt 2 ]; then
    echo "SKIP: This test requires at least 2 nodes (found $CLUSTER_NUM_NODES)"
    exit 0
fi

node1 vgcreate --shared testvg $dev1 $dev2
nodep vgchange --lockstart testvg

#
# Parallel lvconvert to thin-pool
#
for i in $(seq 1 3); do
    node1 lvcreate -L 100M -n lv1 -an testvg

    nodep lvconvert --type thin-pool testvg/lv1 -y || true
    assert_one_success

    verify_lv_has_lock_args testvg lv1
    verify_lv_type testvg lv1 thin-pool

    node1 lvremove -y testvg/lv1
    sleep 0.3
done

#
# Parallel lvconvert to RAID1
#
for i in $(seq 1 2); do
    noden ${i} lvcreate -L 100M -n lv1 testvg

    # lv1 must be active to perform this
    nodep lvconvert --type raid1 -m 1 testvg/lv1 -y || true
    assert_node_success ${i}

    verify_lv_has_lock_args testvg lv1
    verify_lv_type testvg lv1 raid1

    noden ${i} lvremove -y testvg/lv1
    sleep 0.5
done

#
# Cannot convert LV to thin-pool while active on another node
#
node1 lvcreate -L 100M -n lv1 testvg

node1 lvchange -ay testvg/lv1
verify_lv_active_on 1 testvg lv1

node2 not lvconvert --type thin-pool testvg/lv1 -y

node1 lvchange -an testvg/lv1
verify_lv_not_active_on 1 testvg lv1

node2 lvconvert --type thin-pool testvg/lv1 -y

node1 lvremove -y testvg/lv1

#
# Rapid lvconvert to thin-pool cycles
#
for i in $(seq 1 5); do
    node1 lvcreate -L 100M -n lv1 -an testvg

    nodep lvconvert --type thin-pool testvg/lv1 -y || true
    assert_one_success

    success_node lvremove -y testvg/lv1
done

#
# Different nodes convert different LVs simultaneously (thin-pool)
#
nodep 'lvcreate -L 100M -n lv_${NODE_NUM} -an testvg'
nodep 'lvconvert --type thin-pool testvg/lv_${NODE_NUM} -y'
nodep 'lvchange -ay testvg/lv_${NODE_NUM}'
# check that the thin pool is functional
nodep 'lvcreate --type thin --thinpool testvg/lv_${NODE_NUM} -n thin1_${NODE_NUM} -V 200M testvg'
nodep 'lvextend -L+100M testvg/lv_${NODE_NUM}'
nodep 'lvcreate --type thin --thinpool testvg/lv_${NODE_NUM} -n thin2_${NODE_NUM} -V 200M testvg'
nodep 'lvchange -ay -K testvg/thin1_${NODE_NUM}'
nodep 'lvremove -y testvg/thin2_${NODE_NUM}'
nodep 'lvchange -an testvg/thin1_${NODE_NUM}'
nodep 'lvremove -y testvg/lv_${NODE_NUM}'

#
# Different nodes convert different LVs simultaneously (raid1)
#
nodep 'lvcreate -L 100M -n lv_${NODE_NUM} testvg'
nodep 'lvconvert --type raid1 testvg/lv_${NODE_NUM} -y'
for node_num in $(seq 1 $CLUSTER_NUM_NODES); do
    wait_lv_sync_done ${node_num} testvg lv_${node_num}
done
# check that the raid1 is functional
nodep 'lvchange --syncaction check testvg/lv_${NODE_NUM}' || true
for node_num in $(seq 1 $CLUSTER_NUM_NODES); do
    wait_lv_sync_done ${node_num} testvg lv_${node_num}
done
nodep 'lvs --lockopt skipvg -o name,segtype,sync_percent,devices testvg/lv_${NODE_NUM}'
nodep 'lvchange -an testvg/lv_${NODE_NUM}'
nodep 'lvremove -y testvg/lv_${NODE_NUM}'

# Cleanup
cleanup_vg testvg

exit 0
