#!/bin/bash

set -e

if [ "$CLUSTER_NUM_NODES" -lt 2 ]; then
    echo "SKIP: This test requires at least 2 nodes (found $CLUSTER_NUM_NODES)"
    exit 0
fi

node1 vgcreate --shared testvg $dev1 $dev2
nodep vgchange --lockstart testvg
node1 lvcreate -l10 -n lv1 testvg

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
# LV can be locked and activated by any node
# if it is not already locked and activated
#
for ni in $(seq 1 $CLUSTER_NUM_NODES); do
    noden ${ni} lvchange -ay testvg/lv1
    verify_lv_active_on ${ni} testvg lv1

    for nj in $(seq 1 $CLUSTER_NUM_NODES); do
         if [ "$ni" -ne "$nj" ]; then
             noden ${nj} not lvchange -ay testvg/lv1
             verify_lv_not_active_on ${nj} testvg lv1
             noden ${nj} not lvchange -asy testvg/lv1
             verify_lv_not_active_on ${nj} testvg lv1
         fi
    done

    noden ${ni} lvchange -an testvg/lv1
    verify_lv_not_active_on ${ni} testvg lv1

    for nj in $(seq 1 $CLUSTER_NUM_NODES); do
         if [ "$ni" -ne "$nj" ]; then
             noden ${nj} lvchange -ay testvg/lv1
             verify_lv_active_on ${nj} testvg lv1
             noden ${nj} lvchange -an testvg/lv1
             noden ${nj} lvchange -asy testvg/lv1
             verify_lv_active_on ${nj} testvg lv1
             noden ${nj} lvchange -an testvg/lv1
         fi
    done
done

for ni in $(seq 1 $CLUSTER_NUM_NODES); do
    verify_lv_not_active_on ${ni} testvg lv1
done

#
# LV lock conersion
#

for ni in $(seq 1 $CLUSTER_NUM_NODES); do
    # lock ex
    noden ${ni} lvchange -ay testvg/lv1
    verify_lv_active_on ${ni} testvg lv1
    # convert ex->sh
    noden ${ni} lvchange -asy testvg/lv1
    verify_lv_active_on ${ni} testvg lv1

    # other nodes cannot get ex, but can get sh
    for nj in $(seq 1 $CLUSTER_NUM_NODES); do
         if [ "$ni" -ne "$nj" ]; then
             noden ${nj} not lvchange -ay testvg/lv1
             verify_lv_not_active_on ${nj} testvg lv1
             noden ${nj} lvchange -asy testvg/lv1
             verify_lv_active_on ${nj} testvg lv1
         fi
    done

    # original lock holder cannot convert sh->ex with other sh lock holders
    noden ${ni} not lvchange -ay testvg/lv1
    verify_lv_active_on ${ni} testvg lv1

    # other sh lock holders unlock
    for nj in $(seq 1 $CLUSTER_NUM_NODES); do
         if [ "$ni" -ne "$nj" ]; then
             noden ${nj} lvchange -an testvg/lv1
             verify_lv_not_active_on ${nj} testvg lv1
         fi
    done

    # original lock holder can convert sh->ex after other sh lock holders have unlocked
    noden ${ni} lvchange -ay testvg/lv1
    verify_lv_active_on ${ni} testvg lv1

    noden ${ni} lvchange -an testvg/lv1
done

for ni in $(seq 1 $CLUSTER_NUM_NODES); do
    verify_lv_not_active_on ${ni} testvg lv1
done

#
# Activation races with --lockopt skipvg
#
for i in $(seq 1 5); do
    nodep lvchange --lockopt skipvg -ay testvg/lv1 || true
    assert_one_success
    verify_lv_active_on "$NODEP_SINGLE_SUCCESS_NODE" testvg lv1

    success_node lvchange -an testvg/lv1
    verify_lv_not_active_on "$NODEP_SINGLE_SUCCESS_NODE" testvg lv1

    sleep 0.5
done

# Rapid activation cycles
for i in $(seq 1 10); do
    nodep lvchange -ay testvg/lv1 || true
    assert_one_success

    success_node lvchange -an testvg/lv1

    sleep 0.1
done

node1 lvremove -y testvg/lv1
cleanup_vg testvg

exit 0
