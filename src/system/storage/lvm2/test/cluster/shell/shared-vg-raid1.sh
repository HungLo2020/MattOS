#!/bin/bash
#
# lv-lock-raid1.sh - Test RAID1 LV locking
#

set -e

if [ "$CLUSTER_NUM_NODES" -lt 2 ]; then
    echo "SKIP: This test requires at least 2 nodes (found $CLUSTER_NUM_NODES)"
    exit 0
fi

node1 vgcreate --shared testvg $dev1 $dev2
nodep vgchange --lockstart testvg

node1 lvcreate --type raid1 -m 1 -L 50M -n lv1 testvg
node1 lvchange -an testvg/lv1

#
# RAID1 cannot use shared activation
#
nodep lvchange -asy testvg/lv1 || true
assert_all_fail

#
# Each node can activate and deactivate RAID1
#
for ni in $(seq 1 $CLUSTER_NUM_NODES); do
    noden ${ni} lvchange -ay testvg/lv1
    verify_lv_active_on ${ni} testvg lv1
    noden ${ni} lvchange -an testvg/lv1
    verify_lv_not_active_on ${ni} testvg lv1
done

#
# Parallel exclusive activation races
#
for i in $(seq 1 5); do
    nodep lvchange -ay testvg/lv1 || true
    assert_one_success

    verify_lv_active_on "$NODEP_SINGLE_SUCCESS_NODE" testvg lv1

    for node_num in $NODEP_FAIL_NODES; do
        noden ${node_num} not lvchange -ay testvg/lv1
    done

    for node_num in $NODEP_FAIL_NODES; do
        verify_lv_not_active_on "$node_num" testvg lv1
    done

    success_node lvchange -an testvg/lv1
    verify_lv_not_active_on "$NODEP_SINGLE_SUCCESS_NODE" testvg lv1

    sleep 0.5
done

#
# Rapid activation cycles
#
for i in $(seq 1 10); do
    nodep lvchange -ay testvg/lv1 || true
    assert_one_success

    success_node lvchange -an testvg/lv1

    sleep 0.1
done

node1 lvremove -y testvg/lv1

#
# Parallel lvextend with relative size on inactive raid1 LV
#
for i in $(seq 1 3); do
    node1 lvcreate --type raid1 -m 1 -l10 -n lv1 -an testvg

    nodep lvextend -L+4M testvg/lv1
    assert_all_success

    node1 lvremove -y testvg/lv1
    sleep 0.3
done

#
# Parallel lvextend with absolute size on raid1 LV
#
for i in $(seq 1 3); do
    node1 lvcreate --type raid1 -m 1 -l10 -n lv1 -an testvg

    nodep lvextend -L64M testvg/lv1 || true
    assert_one_success

    node1 lvremove -y testvg/lv1
    sleep 0.3
done

#
# lvextend blocked while raid1 LV is active on another node
#
node1 lvcreate --type raid1 -m 1 -l10 -n lv1 testvg

node1 lvchange -ay testvg/lv1
verify_lv_active_on 1 testvg lv1

node2 not lvextend -L+4M testvg/lv1

node1 lvchange -an testvg/lv1

node2 lvextend -L+4M testvg/lv1

node1 lvremove -y testvg/lv1

#
# Parallel lvremove races on raid1 LV
#
for i in $(seq 1 5); do
    node1 lvcreate --type raid1 -m 1 -L 50M -n lv1 testvg

    nodep lvremove -y testvg/lv1 || true
    assert_one_success

    node1 not lvs testvg/lv1 2>/dev/null
    sleep 0.3
done

#
# Cannot remove raid1 LV while active on another node
#
node1 lvcreate --type raid1 -m 1 -L 50M -n lv1 testvg
node2 not lvremove -y testvg/lv1
node1 lvchange -an testvg/lv1
node2 lvremove -y testvg/lv1

#
# Parallel lvrename on raid1 LV
#
for i in $(seq 1 3); do
    node1 lvcreate --type raid1 -m 1 -L 50M -n lv1 -an testvg

    nodep lvrename testvg/lv1 testvg/lv2 || true
    assert_one_success

    nodep lvs testvg/lv2 > /dev/null
    assert_all_success

    nodep lvremove -y testvg/lv2 || true
    assert_one_success
done

#
# lvrename blocked while raid1 LV is active on another node
#
node1 lvcreate --type raid1 -m 1 -L 50M -n lv1 testvg

node1 lvchange -ay testvg/lv1
verify_lv_active_on 1 testvg lv1

node2 not lvrename testvg/lv1 testvg/lv2

node1 lvchange -an testvg/lv1

node2 lvrename testvg/lv1 testvg/lv2

node1 lvremove -y testvg/lv2

#
# lvconvert raid1 to raid5 blocked while active on another node
#
node1 lvcreate --type raid1 -m 1 -L 50M -n lv1 testvg
wait_lv_sync_done 1 testvg lv1

node1 lvchange -ay testvg/lv1
verify_lv_active_on 1 testvg lv1

node2 not lvconvert --type raid5 testvg/lv1 -y

node1 lvconvert --type raid5 testvg/lv1 -y
wait_lv_sync_done 1 testvg lv1
verify_lv_type testvg lv1 raid5

node1 lvremove -y testvg/lv1

#
# lvconvert --mirrors to add raid image blocked while active on another node
#
node1 lvcreate --type raid1 -m 1 -L 50M -n lv1 testvg
wait_lv_sync_done 1 testvg lv1

node2 not lvconvert -m+1 testvg/lv1 -y

node1 lvremove -y testvg/lv1

# Cleanup
cleanup_vg testvg

exit 0
