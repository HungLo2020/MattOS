#!/bin/bash
#
# lv-lock-thin-ops.sh - Test thin volume extend, rename, snapshot, and merge operations
#

set -e

if [ "$CLUSTER_NUM_NODES" -lt 2 ]; then
    echo "SKIP: This test requires at least 2 nodes (found $CLUSTER_NUM_NODES)"
    exit 0
fi

node1 vgcreate --shared testvg $dev1 $dev2
nodep vgchange --lockstart testvg

#
# Parallel lvextend on thin volume (pool not active)
# (host status UNKNOWN shortly after lockstart)
#
for i in $(seq 1 5); do
    node1 lvcreate --type thin-pool -L 100M -n tpool testvg
    node1 lvcreate --type thin -V 20M --thinpool tpool -n thin1 testvg
    node1 lvchange -an testvg/thin1
    node1 lvchange -an testvg/tpool

    nodep lvextend -L+4M testvg/thin1
    assert_all_success

    node1 lvremove -y testvg/tpool
    sleep 0.3
done

wait_host_status_known

#
# Parallel lvextend on thin volume (pool not active)
# (host status KNOWN after wait)
#
for i in $(seq 1 5); do
    node1 lvcreate --type thin-pool -L 100M -n tpool testvg
    node1 lvcreate --type thin -V 20M --thinpool tpool -n thin1 testvg
    node1 lvchange -an testvg/thin1
    node1 lvchange -an testvg/tpool

    nodep lvextend -L+4M testvg/thin1
    assert_all_success

    node1 lvremove -y testvg/tpool
    sleep 0.3
done

#
# lvextend on thin volume blocked while active on another node
#
node1 lvcreate --type thin-pool -L 100M -n tpool testvg
node1 lvcreate --type thin -V 20M --thinpool tpool -n thin1 testvg

verify_lv_active_on 1 testvg thin1

node2 not lvextend -L+4M testvg/thin1

node1 lvchange -an testvg/thin1
node1 lvchange -an testvg/tpool

node2 lvextend -L+4M testvg/thin1

node1 lvremove -y testvg/tpool

#
# Parallel lvextend with absolute size on thin volume
#
for i in $(seq 1 3); do
    node1 lvcreate --type thin-pool -L 100M -n tpool testvg
    node1 lvcreate --type thin -V 20M --thinpool tpool -n thin1 testvg
    node1 lvchange -an testvg/thin1
    node1 lvchange -an testvg/tpool

    nodep lvextend -L64M testvg/thin1 || true
    assert_one_success

    node1 lvremove -y testvg/tpool
    sleep 0.3
done

#
# Parallel lvremove races on thin snapshot (pool not active)
#
for i in $(seq 1 3); do
    node1 lvcreate --type thin-pool -L 100M -n tpool testvg
    node1 lvcreate --type thin -V 50M --thinpool tpool -n thin1 testvg
    node1 lvcreate --type thin --snapshot -n snap1 testvg/thin1
    node1 lvchange -an testvg/snap1
    node1 lvchange -an testvg/thin1
    node1 lvchange -an testvg/tpool

    nodep lvremove -y testvg/snap1 || true
    assert_one_success

    node1 not lvs testvg/snap1 2>/dev/null

    node1 lvremove -y testvg/tpool
    sleep 0.3
done

#
# Cannot remove thin snapshot while active on another node
#
node1 lvcreate --type thin-pool -L 100M -n tpool testvg
node1 lvcreate --type thin -V 50M --thinpool tpool -n thin1 testvg
node1 lvcreate --type thin --snapshot -n snap1 testvg/thin1
node1 lvchange -an testvg/thin1

node2 not lvremove -y testvg/snap1
node1 lvchange -an testvg/snap1
node1 lvchange -an testvg/tpool
node2 lvremove -y testvg/snap1

node1 lvremove -y testvg/tpool

#
# Parallel thin snapshot activation races
#
node1 lvcreate --type thin-pool -L 100M -n tpool testvg
node1 lvcreate --type thin -V 50M --thinpool tpool -n thin1 testvg
node1 lvcreate --type thin --snapshot -n snap1 testvg/thin1
node1 lvchange -an testvg/thin1
node1 lvchange -an testvg/snap1
node1 lvchange -an testvg/tpool

for i in $(seq 1 3); do
    # without -K, lvchange -ay does nothing and returns success for a thin snapshot by default
    nodep lvchange -ay -K testvg/snap1 || true
    assert_one_success

    verify_lv_active_on "$NODEP_SINGLE_SUCCESS_NODE" testvg snap1

    for node_num in $NODEP_FAIL_NODES; do
        verify_lv_not_active_on "$node_num" testvg snap1
    done

    success_node lvchange -an testvg/snap1
    success_node lvchange -an testvg/tpool

    sleep 0.5
done

node1 lvremove -y testvg/tpool

#
# Thin snapshot cannot be activated while origin active on another node
#
node1 lvcreate --type thin-pool -L 100M -n tpool testvg
node1 lvcreate --type thin -V 50M --thinpool tpool -n thin1 testvg
node1 lvcreate --type thin --snapshot -n snap1 testvg/thin1

verify_lv_active_on 1 testvg thin1

node2 not lvchange -ay -K testvg/snap1
verify_lv_not_active_on 2 testvg snap1

node1 lvchange -an testvg/thin1
node1 lvchange -an testvg/tpool

node2 lvchange -ay -K testvg/snap1
verify_lv_active_on 2 testvg snap1

node2 lvchange -an testvg/snap1
node2 lvchange -an testvg/tpool

node1 lvremove -y testvg/tpool

#
# Parallel thin snapshot merge races (pool and LVs not active)
#
for i in $(seq 1 3); do
    node1 lvcreate --type thin-pool -L 100M -n tpool testvg
    node1 lvcreate --type thin -V 50M --thinpool tpool -n thin1 testvg
    node1 lvcreate --type thin --snapshot -n snap1 testvg/thin1
    node1 lvchange -an testvg/snap1
    node1 lvchange -an testvg/thin1
    node1 lvchange -an testvg/tpool

    nodep lvconvert --merge testvg/snap1 || true
    assert_one_success

    node1 not lvs testvg/snap1 2>/dev/null
    node1 lvs testvg/thin1 > /dev/null

    node1 lvremove -y testvg/tpool
    sleep 0.3
done

#
# Thin snapshot merge blocked while origin active on another node
#
node1 lvcreate --type thin-pool -L 100M -n tpool testvg
node1 lvcreate --type thin -V 50M --thinpool tpool -n thin1 testvg
node1 lvcreate --type thin --snapshot -n snap1 testvg/thin1
node1 lvchange -an testvg/snap1

verify_lv_active_on 1 testvg thin1

node2 not lvconvert --merge testvg/snap1

node1 lvs testvg/snap1 > /dev/null

node1 lvchange -an testvg/thin1
node1 lvchange -an testvg/tpool

node2 lvconvert --merge testvg/snap1

node1 not lvs testvg/snap1 2>/dev/null
node1 lvs testvg/thin1 > /dev/null

node1 lvremove -y testvg/tpool

#
# Thin snapshot merge blocked while snapshot active on another node
#
node1 lvcreate --type thin-pool -L 100M -n tpool testvg
node1 lvcreate --type thin -V 50M --thinpool tpool -n thin1 testvg
node1 lvcreate --type thin --snapshot -n snap1 testvg/thin1
node1 lvchange -an testvg/thin1

node1 lvchange -ay -K testvg/snap1
verify_lv_active_on 1 testvg snap1

node2 not lvconvert --merge testvg/snap1

node1 lvs testvg/snap1 > /dev/null

node1 lvchange -an testvg/snap1
node1 lvchange -an testvg/tpool

node2 lvconvert --merge testvg/snap1

node1 not lvs testvg/snap1 2>/dev/null
node1 lvs testvg/thin1 > /dev/null

node1 lvremove -y testvg/tpool

#
# Parallel lvrename on thin volume (pool not active)
#
for i in $(seq 1 3); do
    node1 lvcreate --type thin-pool -L 100M -n tpool testvg
    node1 lvcreate --type thin -V 50M --thinpool tpool -n thin1 testvg
    node1 lvchange -an testvg/thin1
    node1 lvchange -an testvg/tpool

    nodep lvrename testvg/thin1 testvg/thin2 || true
    assert_one_success

    nodep lvs testvg/thin2 > /dev/null
    assert_all_success

    node1 lvremove -y testvg/tpool
done

#
# lvrename blocked while thin volume active on another node
#
node1 lvcreate --type thin-pool -L 100M -n tpool testvg
node1 lvcreate --type thin -V 50M --thinpool tpool -n thin1 testvg

verify_lv_active_on 1 testvg thin1

node2 not lvrename testvg/thin1 testvg/thin2

node1 lvchange -an testvg/thin1
node1 lvchange -an testvg/tpool

node2 lvrename testvg/thin1 testvg/thin2

node1 lvremove -y testvg/tpool

# Cleanup
cleanup_vg testvg

exit 0
