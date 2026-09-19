#!/bin/bash

set -e

if [ "$CLUSTER_NUM_NODES" -lt 2 ]; then
    echo "SKIP: This test requires at least 2 nodes (found $CLUSTER_NUM_NODES)"
    exit 0
fi

node1 vgcreate --shared testvg $dev1 $dev2
nodep vgchange --lockstart testvg

#
# Parallel lvcreate for linear LVs
#
for i in $(seq 1 3); do
    nodep lvcreate -L 50M -n lv1 testvg || true
    assert_one_success

    verify_lv_has_lock_args testvg lv1

    success_node lvremove -y testvg/lv1
    sleep 0.3
done

for i in $(seq 1 3); do
    nodep 'lvcreate -l1 -n lv_${NODE_NUM} testvg'
    assert_all_success

    nodep 'lvremove -y testvg/lv_${NODE_NUM}'
    assert_all_success

    sleep 0.3
done

#
# Parallel lvcreate for thin-pool
#
for i in $(seq 1 3); do
    nodep lvcreate --type thin-pool -L 100M -n tpool testvg || true
    assert_one_success

    verify_lv_has_lock_args testvg tpool
    verify_lv_type testvg tpool thin-pool

    success_node lvremove -y testvg/tpool
    sleep 0.3
done

#
# Parallel lvcreate for thin volumes
#
node1 lvcreate --type thin-pool -L 100M -n tpool -an testvg
for node_num in $(seq 1 $CLUSTER_NUM_NODES); do
    noden $node_num lvchange -ay testvg/tpool
    noden $node_num lvchange -an testvg/tpool
done
for i in $(seq 1 3); do
    # the node that succeeds in activating the thin pool can create the thin lv
    nodep lvcreate --type thin -V 50M --thinpool tpool -n thin1 testvg || true
    assert_one_success
    success_node lvchange -an testvg/thin1
    success_node lvchange -an testvg/tpool

    verify_lv_no_lock_args testvg thin1

    for node_num in $(seq 1 $CLUSTER_NUM_NODES); do
        noden $node_num lvchange -ay -K testvg/thin1
        noden $node_num dd if=/dev/urandom of=/dev/testvg/thin1 bs=4K skip=${node_num} count=1
        noden $node_num lvchange -an testvg/thin1
    done

    node1 lvremove -y testvg/thin1
    node1 lvchange -an testvg/tpool
    sleep 0.3
done
node_rand lvremove -y testvg/tpool
assert_one_success

#
# Parallel lvcreate for RAID1
#
for i in $(seq 1 3); do
    nodep lvcreate --type raid1 -m 1 -L 50M -n lv1 testvg || true
    assert_one_success

    verify_lv_has_lock_args testvg lv1
    verify_lv_type testvg lv1 raid1

    success_node lvremove -y testvg/lv1
    sleep 0.5
done

#
# Parallel lvcreate for COW snapshots
#
for i in $(seq 1 3); do
    node1 lvcreate -L 50M -n lv1 -an testvg

    # the node that's able to activate lv1 can create snap1
    nodep lvcreate --snapshot --size 20M -n snap1 testvg/lv1 || true
    assert_one_success

    success_node lvs -a -o+segtype,lock_args testvg
    # verify_lv_has_lock_args testvg snap1

    success_node lvremove -y testvg/lv1
    sleep 0.3
done

#
# Rapid lvcreate/lvremove cycles for thin volumes
#
node1 lvcreate --type thin-pool -L 100M -n tpool -an testvg
for i in $(seq 1 10); do
    nodep lvcreate --type thin -V 50M --thinpool tpool -n thin_${i} testvg || true
    assert_one_success
    success_node lvchange -an testvg/thin_${i}
done
node1 lvremove -y testvg/tpool

#
# Different nodes create different LVs simultaneously
#
for i in $(seq 1 3); do
    nodep 'lvcreate -L 20M -n lv_${NODE_NUM} testvg'
    assert_all_success

    nodep 'lvremove -y testvg/lv_${NODE_NUM}'
    assert_all_success

    sleep 0.3
done

# Cleanup
cleanup_vg testvg

exit 0
