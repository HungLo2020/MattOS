#!/bin/bash

set -e

if [ "$CLUSTER_NUM_NODES" -lt 2 ]; then
    echo "SKIP: This test requires at least 2 nodes (found $CLUSTER_NUM_NODES)"
    exit 0
fi

node1 vgcreate --shared testvg $dev1 $dev2
nodep vgchange --lockstart testvg

#
# Parallel lvrename on linear LV
#
for i in $(seq 1 3); do
    node_rand lvcreate -l10 -n lv1 -an testvg

    nodep lvrename testvg/lv1 testvg/lv2 || true
    assert_one_success

    nodep lvs testvg/lv2 > /dev/null
    assert_all_success

    nodep lvremove -y testvg/lv2 || true
    assert_one_success
done

#
# Parallel lvextend with relative size on inactive linear LV
# (host status UNKNOWN shortly after lockstart)
#
for i in $(seq 1 5); do
    node1 lvcreate -l10 -n lv1 -an testvg

    nodep lvextend --lockopt retries=10 -L+4M testvg/lv1
    assert_all_success

    node1 lvremove -y testvg/lv1
    sleep 0.3
done

wait_host_status_known

#
# Parallel lvextend with relative size on inactive linear LV
# (host status KNOWN after wait)
#
for i in $(seq 1 5); do
    node1 lvcreate -l10 -n lv1 -an testvg

    nodep lvextend --lockopt retries=10 -L+4M testvg/lv1
    assert_all_success

    node1 lvremove -y testvg/lv1
    sleep 0.3
done

#
# Parallel lvextend with relative size on active linear LV
#
for i in $(seq 1 1); do
    node1 lvcreate -l10 -n lv1 testvg

    nodep lvextend -L+4M testvg/lv1 || true
    assert_node_success 1
    assert_one_success

    node1 lvremove -y testvg/lv1
    sleep 0.3
done

#
# Parallel lvextend with absolute size on linear LV
#
for i in $(seq 1 3); do
    node1 lvcreate -l10 -n lv1 -an testvg

    nodep lvextend -L64M testvg/lv1 || true
    assert_one_success

    node1 lvremove -y testvg/lv1
    sleep 0.3
done

#
# lvextend blocked while LV is active on another node
#
node1 lvcreate -l10 -n lv1 testvg

node1 lvchange -ay testvg/lv1
verify_lv_active_on 1 testvg lv1

node2 not lvextend -L+4M testvg/lv1

node1 lvchange -an testvg/lv1

node2 lvextend -L+4M testvg/lv1

node1 lvremove -y testvg/lv1

#
# lvextend blocked while LV is active shared on another node
#
node1 lvcreate -l10 -n lv1 testvg

node1 lvchange -asy testvg/lv1
verify_lv_active_on 1 testvg lv1

node2 not lvextend -L+4M testvg/lv1

node1 lvchange -an testvg/lv1

node2 lvextend -L+4M testvg/lv1

node1 lvremove -y testvg/lv1

# Cleanup
cleanup_vg testvg

exit 0
