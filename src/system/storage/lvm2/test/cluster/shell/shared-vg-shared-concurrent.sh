#!/bin/bash
#
# lv-lock-shared-concurrent.sh - Test shared LV lock concurrent activation
#
# Shared locks (-asy) allow concurrent activation from multiple nodes.
# Exclusive locks (-ay) and shared locks mutually exclude each other.
#

set -e

if [ "$CLUSTER_NUM_NODES" -lt 2 ]; then
    echo "SKIP: This test requires at least 2 nodes (found $CLUSTER_NUM_NODES)"
    exit 0
fi

node1 vgcreate --shared testvg $dev1 $dev2
nodep vgchange --lockstart testvg

node1 lvcreate -l20 -n lv1 testvg
node1 lvchange -an testvg/lv1

#
# All nodes activate with shared lock concurrently
#
# Parallel shared activation can fail with "LV locked by other host"
# when a prior exclusive lock has not yet been fully released in sanlock.
# TODO: add a "shared retries" lockopt to enable retrying after -EAGAIN
# in the lockd_lv code path (GL and VG locks retry, LV locks do not).
#
# TODO: consider an option like --lockopt unknown_retry to handle
# occasional EAGAIN errors due to "UNKNOWN node state".
#
nodep lvchange -asy testvg/lv1 || true
if [ "$NODEP_SUCCESS_COUNT" -ne "$CLUSTER_NUM_NODES" ]; then
    sleep 30
    for node_num in $NODEP_FAIL_NODES; do
        noden ${node_num} lvchange -asy testvg/lv1
    done
fi

for node_num in $(seq 1 $CLUSTER_NUM_NODES); do
    verify_lv_active_on "$node_num" testvg lv1
done

#
# Exclusive activation blocked while shared locks held
#
for node_num in $(seq 1 $CLUSTER_NUM_NODES); do
    noden ${node_num} not lvchange -ay testvg/lv1
done

nodep lvchange -an testvg/lv1
assert_all_success

for node_num in $(seq 1 $CLUSTER_NUM_NODES); do
    verify_lv_not_active_on "$node_num" testvg lv1
done

#
# Shared activation blocked by exclusive lock
#
node1 lvchange -ay testvg/lv1
verify_lv_active_on 1 testvg lv1

for node_num in $(seq 2 $CLUSTER_NUM_NODES); do
    noden ${node_num} not lvchange -asy testvg/lv1
    verify_lv_not_active_on "$node_num" testvg lv1
done

node1 lvchange -an testvg/lv1

#
# Rapid shared activation cycles
#
# TODO: add a "shared retries" lockopt to enable retrying after -EAGAIN
# in the lockd_lv code path (GL and VG locks retry, LV locks do not).
#
# TODO: consider an option like --lockopt unknown_retry to handle
# occasional EAGAIN errors due to "UNKNOWN node state".
#
for i in $(seq 1 5); do
    nodep lvchange -asy testvg/lv1 || true
    if [ "$NODEP_SUCCESS_COUNT" -ne "$CLUSTER_NUM_NODES" ]; then
        sleep 30
        for node_num in $NODEP_FAIL_NODES; do
            noden ${node_num} lvchange -asy testvg/lv1
        done
    fi

    nodep lvchange -an testvg/lv1
    assert_all_success
done

node1 lvremove -y testvg/lv1

# Cleanup
cleanup_vg testvg

exit 0
