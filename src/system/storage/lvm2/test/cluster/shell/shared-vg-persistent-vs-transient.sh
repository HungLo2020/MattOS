#!/bin/bash
#
# lv-lock-persistent-vs-transient.sh - Test persistent vs transient LV locks
#
# Persistent locks (from activation) remain held until deactivation.
# Transient locks (from modification) are released when the command exits.
#

set -e

if [ "$CLUSTER_NUM_NODES" -lt 2 ]; then
    echo "SKIP: This test requires at least 2 nodes (found $CLUSTER_NUM_NODES)"
    exit 0
fi

node1 vgcreate --shared testvg $dev1 $dev2
nodep vgchange --lockstart testvg

#
# Persistent lock: activation blocks activation and modification from other nodes
#
node1 lvcreate -l20 -n lv1 testvg

node2 not lvchange -ay testvg/lv1
verify_lv_not_active_on 2 testvg lv1
node2 not lvextend -L+4M testvg/lv1
node2 not lvrename testvg/lv1 testvg/lv2

node1 lvchange -an testvg/lv1
verify_lv_not_active_on 1 testvg lv1

node2 lvchange -ay testvg/lv1
verify_lv_active_on 2 testvg lv1
node2 lvchange -an testvg/lv1

node1 lvremove -y testvg/lv1

#
# Transient lock: modification does not block other nodes
#
node1 lvcreate -l10 -n lv1 -an testvg

node1 lvextend -L+4M testvg/lv1
node2 lvextend -L+4M testvg/lv1

node1 lvrename testvg/lv1 testvg/lv2
node2 lvextend -L+4M testvg/lv2
node2 lvrename testvg/lv2 testvg/lv1

node1 lvremove -y testvg/lv1

#
# Transient lock does not prevent activation
#
node1 lvcreate -l10 -n lv1 -an testvg

node1 lvextend -L+4M testvg/lv1
node2 lvchange -ay testvg/lv1
verify_lv_active_on 2 testvg lv1

node1 not lvextend -L+4M testvg/lv1

node2 lvchange -an testvg/lv1

node1 lvremove -y testvg/lv1

# Cleanup
cleanup_vg testvg

exit 0
