#!/bin/bash
#
# lv-lock-snapshot-cow-parent.sh - Test COW snapshot and origin lock relationship and merge
#

set -e

if [ "$CLUSTER_NUM_NODES" -lt 2 ]; then
    echo "SKIP: This test requires at least 2 nodes (found $CLUSTER_NUM_NODES)"
    exit 0
fi

node1 vgcreate --shared testvg $dev1 $dev2
nodep vgchange --lockstart testvg

#
# Origin has lock_args, COW snapshot does not
#
node1 lvcreate -L 50M -n lv1 testvg
node1 lvcreate --snapshot --size 20M -n snap1 testvg/lv1

verify_lv_has_lock_args testvg lv1
verify_lv_no_lock_args testvg snap1

node1 lvremove -y testvg/snap1
node1 lvremove -y testvg/lv1

#
# LV with cow snapshot cannot use shared activation
#
node1 lvcreate -L 50M -n lv1 testvg
node1 lvcreate --snapshot --size 20M -n snap1 testvg/lv1
node1 lvchange -an testvg/lv1

nodep lvchange -asy testvg/lv1 || true
assert_all_fail
nodep lvchange -asy -y testvg/snap1 || true
assert_all_fail

for node_num in $(seq 1 $CLUSTER_NUM_NODES); do
    verify_lv_not_active_on ${node_num} testvg lv1
    verify_lv_not_active_on ${node_num} testvg snap1
done

node1 lvremove -y testvg/lv1

#
# Snapshot cannot be activated on different node while origin active on another
#
node1 lvcreate -L 50M -n lv1 testvg
node1 lvcreate --snapshot --size 20M -n snap1 testvg/lv1

verify_lv_active_on 1 testvg lv1
verify_lv_active_on 1 testvg snap1

node2 not lvchange -ay -y testvg/snap1
node2 not lvchange -ay testvg/lv1
verify_lv_not_active_on 2 testvg snap1
verify_lv_not_active_on 2 testvg lv1

node1 lvremove -y testvg/lv1

#
# Parallel COW snapshot merge races (LVs not active)
#
for i in $(seq 1 3); do
    node1 lvcreate -L 50M -n lv1 testvg
    node1 lvcreate --snapshot --size 20M -n snap1 testvg/lv1
    node1 lvchange -an testvg/lv1

    nodep lvconvert --merge testvg/snap1 || true
    assert_one_success

    # snap1 will go away after merge begins at next activation of lv1
    node1 lvs testvg/snap1 2>/dev/null
    node1 lvs testvg/lv1 > /dev/null

    nodep lvchange -ay testvg/lv1 || true
    assert_one_success

    # Wait for async lvpoll to complete the merge and remove snap1 from metadata
    for _w in $(seq 1 20); do
        success_node lvs testvg/snap1 2>/dev/null || break
        sleep 0.5
    done
    node1 not lvs testvg/snap1 2>/dev/null
    node1 lvs testvg/lv1 > /dev/null

    nodep lvremove -y testvg/lv1 || true
    sleep 0.3
done

#
# COW snapshot merge blocked while origin active on another node
#
node1 lvcreate -L 50M -n lv1 testvg
node1 lvcreate --snapshot --size 20M -n snap1 testvg/lv1

verify_lv_active_on 1 testvg lv1

node2 not lvconvert --merge testvg/snap1

node1 lvs testvg/snap1 > /dev/null

node1 lvchange -an testvg/lv1

node2 lvconvert --merge testvg/snap1

# snap1 will go away after merge begins at next activation of lv1
node1 lvs testvg/snap1 2>/dev/null
node1 lvs testvg/lv1 > /dev/null

node1 lvchange -ay testvg/lv1

# Wait for async lvpoll to complete the merge and remove snap1 from metadata
for _w in $(seq 1 20); do
    node1 lvs testvg/snap1 2>/dev/null || break
    sleep 0.5
done
node1 not lvs testvg/snap1 2>/dev/null

node1 lvremove -y testvg/lv1

# Cleanup
cleanup_vg testvg

exit 0
