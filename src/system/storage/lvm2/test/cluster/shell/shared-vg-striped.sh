#!/bin/bash
#
# lv-lock-striped.sh - Test striped LV locking
#

set -e

if [ "$CLUSTER_NUM_NODES" -lt 2 ]; then
    echo "SKIP: This test requires at least 2 nodes (found $CLUSTER_NUM_NODES)"
    exit 0
fi

node1 vgcreate --shared testvg $dev1 $dev2
nodep vgchange --lockstart testvg

#
# Parallel lvcreate races for striped LVs
#
for i in $(seq 1 3); do
    nodep lvcreate -i 2 -L 50M -n lv1 testvg || true
    assert_one_success

    verify_lv_has_lock_args testvg lv1
    verify_lv_type testvg lv1 striped

    success_node lvremove -y testvg/lv1
    sleep 0.3
done

node1 lvcreate -i 2 -L 100M -n lv1 testvg
node1 lvchange -an testvg/lv1

#
# Striped LV can use shared activation
#
# Parallel shared activation can fail with "LV locked by other host"
# when a prior exclusive lock has not yet been fully released in sanlock.
# TODO: add a "shared retries" lockopt to enable retrying after -EAGAIN
# in the lockd_lv code path (GL and VG locks retry, LV locks do not).
#
nodep lvchange -asy testvg/lv1 || true
if [ "$NODEP_SUCCESS_COUNT" -ne "$CLUSTER_NUM_NODES" ]; then
    sleep 2
    for node_num in $NODEP_FAIL_NODES; do
        noden ${node_num} lvchange -asy testvg/lv1
    done
fi

for node_num in $(seq 1 $CLUSTER_NUM_NODES); do
    verify_lv_active_on "$node_num" testvg lv1
done

nodep lvchange -an testvg/lv1
assert_all_success

#
# Parallel exclusive activation races
#
for i in $(seq 1 5); do
    nodep lvchange -ay testvg/lv1 || true
    assert_one_success

    verify_lv_active_on "$NODEP_SINGLE_SUCCESS_NODE" testvg lv1

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
# Parallel lvextend with relative size on inactive striped LV
#
for i in $(seq 1 3); do
    node1 lvcreate -i 2 -l10 -n lv1 -an testvg

    nodep lvextend -L+4M testvg/lv1
    assert_all_success

    node1 lvremove -y testvg/lv1
    sleep 0.3
done

#
# Parallel lvextend with absolute size on striped LV
#
for i in $(seq 1 3); do
    node1 lvcreate -i 2 -l10 -n lv1 -an testvg

    nodep lvextend -L64M testvg/lv1 || true
    assert_one_success

    node1 lvremove -y testvg/lv1
    sleep 0.3
done

#
# lvextend blocked while striped LV is active on another node
#
node1 lvcreate -i 2 -l10 -n lv1 testvg

node1 lvchange -ay testvg/lv1
verify_lv_active_on 1 testvg lv1

node2 not lvextend -L+4M testvg/lv1

node1 lvchange -an testvg/lv1

node2 lvextend -L+4M testvg/lv1

node1 lvremove -y testvg/lv1

#
# Parallel lvremove races on striped LV
#
for i in $(seq 1 5); do
    node1 lvcreate -i 2 -L 50M -n lv1 testvg

    nodep lvremove -y testvg/lv1 || true
    assert_one_success

    node1 not lvs testvg/lv1 2>/dev/null
    sleep 0.3
done

#
# Cannot remove striped LV while active on another node
#
node1 lvcreate -i 2 -L 50M -n lv1 testvg
node2 not lvremove -y testvg/lv1
node1 lvchange -an testvg/lv1
node2 lvremove -y testvg/lv1

# Cleanup
cleanup_vg testvg

exit 0
