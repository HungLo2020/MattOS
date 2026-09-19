#!/bin/bash

set -e

if [ "$CLUSTER_NUM_NODES" -lt 2 ]; then
    echo "SKIP: This test requires at least 2 nodes (found $CLUSTER_NUM_NODES)"
    exit 0
fi

node1 vgcreate --shared testvg $dev1 $dev2
nodep vgchange --lockstart testvg

#
# Parallel lvextend on thin-pool
# each node activates, extends, deactivates, so all should
# take a turn and extend
#
for i in $(seq 1 3); do
    node1 lvcreate --type thin-pool -L 64M -n tpool -an testvg
    orig_size=$(node1 lvs --noheadings --nosuffix --units m -o lv_size testvg/tpool | tr -d ' ')

    nodep lvextend -L +8M testvg/tpool
    assert_all_success

    expected=$(echo "$orig_size + 8 * $CLUSTER_NUM_NODES" | bc)
    verify_lv_size testvg tpool "$expected"

    node1 lvremove -y testvg/tpool
    sleep 0.3
done

#
# Parallel lvextend on thin-pool data/meta
# each node activates, extends, deactivates, so all should
# take a turn and extend
#
for i in $(seq 1 3); do
    node1 lvcreate --type thin-pool -L 64M -n tpool -an testvg
    orig_data=$(node1 lvs --noheadings --nosuffix --units m -o lv_size testvg/tpool_tdata | tr -d ' ')
    orig_meta=$(node1 lvs --noheadings --nosuffix --units m -o lv_size testvg/tpool_tmeta | tr -d ' ')

    nodep lvextend -L +8M testvg/tpool_tdata
    assert_all_success
    nodep lvextend -L +4M testvg/tpool_tmeta
    assert_all_success

    expected_data=$(echo "$orig_data + 8 * $CLUSTER_NUM_NODES" | bc)
    expected_meta=$(echo "$orig_meta + 4 * $CLUSTER_NUM_NODES" | bc)
    verify_lv_size testvg tpool_tdata "$expected_data"
    verify_lv_size testvg tpool_tmeta "$expected_meta"

    node1 lvremove -y testvg/tpool
    sleep 0.3
done

#
# Parallel lvextend on COW snapshot
# each node activates, extends, deactivates, so all should
# take a turn and extend
#
node1 lvcreate -L 64M -n lv1 testvg
node1 lvcreate --snapshot --size 4M -n snap1 testvg/lv1
node1 lvchange -an testvg/lv1
orig_size=$(node1 lvs --noheadings --nosuffix --units m -o lv_size testvg/snap1 | tr -d ' ')
for i in $(seq 1 1); do
    nodep lvextend -L +4M testvg/snap1
    assert_all_success

    expected=$(echo "$orig_size + 4 * $CLUSTER_NUM_NODES * $i" | bc)
    verify_lv_size testvg snap1 "$expected"

    sleep 0.3
done
node1 lvremove -y testvg/lv1

#
# Parallel lvextend on origin with snapshot
# each node activates, extends, deactivates, so all should
# take a turn and extend
#
node1 lvcreate -L 64M -n lv1 testvg
node1 lvcreate --snapshot --size 4M -n snap1 testvg/lv1
node1 lvchange -an testvg/lv1
orig_size=$(node1 lvs --noheadings --nosuffix --units m -o lv_size testvg/lv1 | tr -d ' ')
for i in $(seq 1 3); do
    nodep lvextend -L +8M testvg/lv1
    assert_all_success

    expected=$(echo "$orig_size + 8 * $CLUSTER_NUM_NODES * $i" | bc)
    verify_lv_size testvg lv1 "$expected"

    sleep 0.3
done
node1 lvremove -y testvg/lv1

#
# Cannot extend thin-pool while thin volume active on another node
#
node1 lvcreate --type thin-pool -L 64M -n tpool testvg
node1 lvcreate --type thin -V 32M --thinpool tpool -n thin1 testvg
verify_lv_active_on 1 testvg thin1

node2 not lvextend -L +32M testvg/tpool
node2 not lvextend -L +32M testvg/tpool_tdata
node2 not lvextend -L +32M testvg/tpool_tmeta

node1 lvchange -an testvg/thin1
node1 lvchange -an testvg/tpool

node2 lvextend -L +32M testvg/tpool

node1 lvremove -y testvg/tpool

#
# Cannot extend snapshot while active on another node
#
node1 lvcreate -L 64M -n lv1 testvg
node1 lvcreate --snapshot --size 4M -n snap1 testvg/lv1
verify_lv_active_on 1 testvg snap1

node2 not lvextend -L +4M testvg/snap1

node1 lvchange -an testvg/snap1 -y
verify_lv_not_active_on 1 testvg lv1

node2 lvextend -L +4M testvg/snap1

node1 lvremove -y testvg/lv1

wait_host_status_known

#
# Rapid lvextend cycles for thin-pool
# (host status KNOWN after wait)
#
node1 lvcreate --type thin-pool -L 64M -n tpool -an testvg
orig_size=$(node1 lvs --noheadings --nosuffix --units m -o lv_size testvg/tpool | tr -d ' ')
for i in $(seq 1 5); do
    nodep lvextend -L +8M testvg/tpool
    assert_all_success
done
expected=$(echo "$orig_size + 8 * $CLUSTER_NUM_NODES * 5" | bc)
verify_lv_size testvg tpool "$expected"
node1 lvremove -y testvg/tpool

#
# Rapid lvextend cycles for COW snapshot
#
node1 lvcreate -L 64M -n lv1 testvg
node1 lvcreate --snapshot --size 4M -n snap1 testvg/lv1
node1 lvchange -an testvg/snap1 -y
node1 lvchange -an testvg/lv1
for i in $(seq 1 1); do
    nodep lvextend -L +4M testvg/snap1
    assert_all_success
done
node1 lvremove -y testvg/snap1
node1 lvremove -y testvg/lv1

#
# Different nodes extend different thin-pools simultaneously
#
nodep 'lvcreate --type thin-pool -L 64M -n tpool_${NODE_NUM} -an testvg'
assert_all_success

nodep 'lvextend -L +32M testvg/tpool_${NODE_NUM}'
assert_all_success

nodep 'lvremove -y testvg/tpool_${NODE_NUM}'
assert_all_success

# Cleanup
cleanup_vg testvg

exit 0
