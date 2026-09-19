#!/bin/bash
#
# lv-lock-thin-creation.sh - Test thin-pool and thin volume creation locking
#

set -e

if [ "$CLUSTER_NUM_NODES" -lt 2 ]; then
    echo "SKIP: This test requires at least 2 nodes (found $CLUSTER_NUM_NODES)"
    exit 0
fi

node1 vgcreate --shared testvg $dev1 $dev2
nodep vgchange --lockstart testvg

#
# Create thin-pool with explicit --poolmetadata
#
node1 lvcreate -L 100M -n lv1 testvg
node1 lvcreate -L 8M -n lv1_meta testvg

node1 lvconvert --type thin-pool --poolmetadata testvg/lv1_meta testvg/lv1 -y

verify_lv_has_lock_args testvg lv1
verify_lv_no_lock_args testvg lv1_meta

node1 lvremove -y testvg/lv1

#
# Parallel races to create thin-pool with --poolmetadata
#
for i in $(seq 1 3); do
    node1 lvcreate -L 100M -n lv1 -an testvg
    node1 lvcreate -L 8M -n lv1_meta -an testvg

    nodep lvconvert --type thin-pool --poolmetadata testvg/lv1_meta testvg/lv1 -y || true
    assert_one_success
    success_node lvchange -an testvg/lv1

    verify_lv_has_lock_args testvg lv1
    verify_lv_no_lock_args testvg lv1_meta

    nodep lvchange -ay testvg/lv1 || true
    assert_one_success
    success_node lvchange -an testvg/lv1

    node1 lvremove -y testvg/lv1
    sleep 0.3
done

#
# Cannot convert with --poolmetadata if metadata LV is active elsewhere
#
node1 lvcreate -L 100M -n lv1 testvg
node1 lvcreate -L 8M -n lv1_meta testvg

node1 lvchange -an testvg/lv1
node1 lvchange -ay testvg/lv1_meta

node2 not lvconvert --type thin-pool --poolmetadata testvg/lv1_meta testvg/lv1 -y

node1 lvchange -an testvg/lv1_meta

node2 lvconvert --type thin-pool --poolmetadata testvg/lv1_meta testvg/lv1 -y

node1 lvremove -y testvg/lv1

#
# Cannot convert with --poolmetadata if data LV is active elsewhere
#
node1 lvcreate -L 100M -n lv1 -an testvg
node1 lvcreate -L 8M -n lv1_meta -an testvg

node1 lvchange -ay testvg/lv1

node2 not lvconvert --type thin-pool --poolmetadata testvg/lv1_meta testvg/lv1 -y

node1 lvchange -an testvg/lv1

node2 lvconvert --type thin-pool --poolmetadata testvg/lv1_meta testvg/lv1 -y

node1 lvremove -y testvg/lv1

#
# Cannot create thin volume while pool active on another node
#
node1 lvcreate --type thin-pool -L 100M -n tpool testvg

node2 not lvcreate --type thin -V 50M --thinpool tpool -n thin1 testvg

node1 lvchange -an testvg/tpool

node2 lvcreate --type thin -V 50M --thinpool tpool -n thin1 testvg

node2 lvchange -an testvg/thin1
node2 lvchange -an testvg/tpool

node1 lvremove -y testvg/tpool

#
# Parallel thin snapshot creation races
#
node1 lvcreate --type thin-pool -L 100M -n tpool testvg
node1 lvcreate --type thin -V 50M --thinpool tpool -n thin1 testvg
node1 lvchange -an testvg/tpool
node1 lvchange -an testvg/thin1

for i in $(seq 1 3); do
    nodep lvcreate --type thin --snapshot -n snap1 testvg/thin1 || true
    assert_one_success

    verify_lv_no_lock_args testvg snap1

    node1 lvremove -y testvg/snap1
    sleep 0.3
done

node1 lvremove -y testvg/tpool

#
# Cannot create thin snapshot while origin active on another node
#
node1 lvcreate --type thin-pool -L 100M -n tpool testvg
node1 lvcreate --type thin -V 50M --thinpool tpool -n thin1 testvg

node2 not lvcreate --type thin --snapshot -n snap1 testvg/thin1

node1 lvchange -an testvg/thin1
node1 lvchange -an testvg/tpool

node2 lvcreate --type thin --snapshot -n snap1 testvg/thin1

node1 lvremove -y testvg/snap1
node1 lvremove -y testvg/thin1
node1 lvremove -y testvg/tpool

# Cleanup
cleanup_vg testvg

exit 0
