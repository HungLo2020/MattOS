#!/bin/bash
#
# lv-lock-vdo.sh - Test VDO LV locking in shared VG
#
# VDO uses exclusive activation only (LDLV_MODE_NO_SH).
# The lock is held on the VDO pool LV.
#

set -e

if [ "$CLUSTER_NUM_NODES" -lt 2 ]; then
    echo "SKIP: This test requires at least 2 nodes (found $CLUSTER_NUM_NODES)"
    exit 0
fi

# VDO requires the dm-vdo kernel module and vdoformat userspace tool
if ! node1 modprobe dm-vdo; then
    echo "SKIP: dm-vdo kernel module not available"
    exit 0
fi
if ! node1 which vdoformat > /dev/null 2>&1; then
    echo "SKIP: vdoformat not available"
    exit 0
fi

# VDO minimum pool size is ~3GB (even with 128MB slabs)
if [ -z "$dev4" ]; then
    echo "SKIP: This test requires at least 4 devices for VDO pool sizing"
    exit 0
fi

# Default slab size is 2GB; use 128MB to fit on test devices
VDO_CONF="--config allocation/vdo_slab_size_mb=128"

node1 vgcreate --shared testvg $dev1 $dev2 $dev3 $dev4
nodep vgchange --lockstart testvg

#
# Basic VDO creation (two-step: lvcreate + lvconvert --type vdo-pool)
#
node1 lvcreate -n vpool -L 3200M testvg -y
node1 lvconvert --type vdo-pool -n lv1 -V 5G $VDO_CONF testvg/vpool -y

verify_lv_has_lock_args testvg vpool

node1 lvchange -an testvg/lv1
node1 lvremove -y testvg/vpool

#
# VDO cannot use shared activation
#
node1 lvcreate -n vpool -L 3200M testvg -y
node1 lvconvert --type vdo-pool -n lv1 -V 5G $VDO_CONF testvg/vpool -y
node1 lvchange -an testvg/lv1

nodep lvchange -asy testvg/lv1 || true
assert_all_fail

#
# Each node can activate and deactivate VDO
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

node1 lvremove -y testvg/vpool

#
# Parallel lvconvert --type vdo-pool races
#
for i in $(seq 1 3); do
    node1 lvcreate -n vpool -L 3200M -an testvg

    nodep lvconvert --type vdo-pool -n lv1 -V 5G $VDO_CONF testvg/vpool -y || true
    assert_one_success
    success_node lvchange -an testvg/lv1

    node1 lvremove -y testvg/vpool
    sleep 0.3
done

#
# lvconvert --type vdo-pool blocked while data LV active on another node
#
node1 lvcreate -n vpool -L 3200M testvg -y

node1 lvchange -ay testvg/vpool
verify_lv_active_on 1 testvg vpool

node2 not lvconvert --type vdo-pool -n lv1 -V 5G $VDO_CONF testvg/vpool -y

node1 lvchange -an testvg/vpool

node2 lvconvert --type vdo-pool -n lv1 -V 5G $VDO_CONF testvg/vpool -y
node2 lvchange -an testvg/lv1

node1 lvremove -y testvg/vpool

#
# Cannot remove VDO while active on another node
#
node1 lvcreate -n vpool -L 3200M testvg -y
node1 lvconvert --type vdo-pool -n lv1 -V 5G $VDO_CONF testvg/vpool -y

node2 not lvremove -y testvg/vpool

node1 lvchange -an testvg/lv1
node2 lvremove -y testvg/vpool

#
# Parallel lvremove races on VDO
#
for i in $(seq 1 3); do
    node1 lvcreate -n vpool -L 3200M testvg -y
    node1 lvconvert --type vdo-pool -n lv1 -V 5G $VDO_CONF testvg/vpool -y
    node1 lvchange -an testvg/lv1

    nodep lvremove -y testvg/vpool || true
    assert_one_success

    node1 not lvs testvg/vpool 2>/dev/null
    sleep 0.3
done

# Cleanup
cleanup_vg testvg

exit 0
