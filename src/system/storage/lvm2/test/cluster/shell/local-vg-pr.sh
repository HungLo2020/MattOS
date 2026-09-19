#!/bin/bash

set -e

if [ "$CLUSTER_NUM_NODES" -lt 2 ]; then
    echo "SKIP: This test requires at least 2 nodes (found $CLUSTER_NUM_NODES)"
    exit 0
fi

DEV_TYPES=""
[ "${CLUSTER_NUM_SCSI:-0}" -ge 2 ]     && DEV_TYPES="$DEV_TYPES scsi"
[ "${CLUSTER_NUM_NVME:-0}" -ge 2 ]     && DEV_TYPES="$DEV_TYPES nvme"
[ "${CLUSTER_NUM_MULTIPATH:-0}" -ge 2 ] && DEV_TYPES="$DEV_TYPES mpath"

[ -z "$DEV_TYPES" ] && { echo "SKIP: no device type with >= 2 devices"; exit 0; }

for devtype in $DEV_TYPES; do
    case $devtype in
        scsi)  d1=$scsi1; d2=$scsi2; d3=${scsi3:-} ;;
        nvme)  d1=$nvme1; d2=$nvme2; d3=${nvme3:-} ;;
        mpath) d1=$mpath1; d2=$mpath2; d3=${mpath3:-} ;;
    esac

    echo "=== Testing device type: $devtype ($d1, $d2) ==="

# ============================================================
# Basic single-device verification
# ============================================================

KEY1=0xaa01
KEY2=0xaa02

node1 'sed -i -e "/pr_key/d" -e "/local {/a\\    pr_key = \"0xaa01\"" /etc/lvm/lvmlocal.conf'
node2 'sed -i -e "/pr_key/d" -e "/local {/a\\    pr_key = \"0xaa02\"" /etc/lvm/lvmlocal.conf'

if [ "$CLUSTER_NUM_NODES" -gt 2 ]; then
    for n in $(seq 3 $CLUSTER_NUM_NODES); do
        noden ${n} "sed -i -e '/pr_key/d' -e '/local {/a\\    pr_key = \"0xaa0${n}\"' /etc/lvm/lvmlocal.conf"
    done
fi

node1 lvmconfig local/pr_key
node2 lvmconfig local/pr_key

# pr_key: start/stop/check
node1 vgcreate testvg $d1
node1 vgchange --persist start testvg
node1 vgchange --persist check testvg | grep -q "$KEY1"
node1 lvmpersist check-key --key $KEY1 --vg testvg
node2 lvmpersist check-key --key $KEY1 --vg testvg
node2 not lvmpersist check-key --key $KEY2 --vg testvg
node1 vgchange --persist stop testvg
node1 not vgchange --persist check testvg
node1 lvmpersist read-keys --vg testvg | grep -q "keys: none"
node1 vgremove -ff testvg

# pr_key: combined vgcreate --setpersist y --persist start
node1 vgcreate --setpersist y --persist start testvg $d1
node1 vgchange --persist check testvg | grep -q "$KEY1"
node1 lvmpersist check-key --key $KEY1 --vg testvg
node1 vgremove -ff testvg

# pr_key: takeover with --removekey
node1 vgcreate testvg $d1
node1 vgchange --persist start testvg
node2 not vgchange --persist start testvg
node2 vgchange --persist start --removekey $KEY1 testvg
node2 vgchange --persist check testvg | grep -q "$KEY2"
node1 not vgchange --persist check testvg
node1 not dd if=/dev/zero of=$d1 bs=4096 count=1 oflag=direct,sync
node2 dd if=/dev/zero of=$d1 bs=4096 count=1 seek=256 oflag=direct,sync
node1 vgchange --persist start --removekey $KEY2 testvg
node1 vgchange --persist check testvg | grep -q "$KEY1"
node1 vgchange --persist stop testvg
node1 vgremove -ff testvg

# pr_key: vgremove stops PR when setpersist is set
node1 vgcreate --setpersist y --persist start testvg $d1
node1 lvmpersist check-key --key $KEY1 --device $d1
node1 vgremove -ff testvg
node1 not lvmpersist check-key --key $KEY1 --device $d1
node1 lvmpersist read-keys --device $d1 | grep -q "keys: none"

# host_id-derived keys
KEY1=0x1000000000000001
KEY2=0x1000000000000002

nodes 'sed -i "/pr_key/d" /etc/lvm/lvmlocal.conf'
nodes 'sed -i -e "/host_id/d" -e "/local {/a\\    host_id = ${NODE_NUM}" /etc/lvm/lvmlocal.conf'

node1 vgcreate testvg $d1
node1 vgchange --persist start testvg
node1 vgchange --persist check testvg | grep -q "$KEY1"
node1 lvmpersist check-key --key $KEY1 --vg testvg
node2 lvmpersist check-key --key $KEY1 --vg testvg
node2 not lvmpersist check-key --key $KEY2 --vg testvg
node1 vgchange --persist stop testvg
node1 lvmpersist read-keys --vg testvg | grep -q "keys: none"
node1 vgremove -ff testvg

# setpersist require enforcement
node1 vgcreate --setpersist y testvg $d1
node1 vgs -o persist testvg | grep 'require,autostart'
node1 not lvcreate -l1 testvg
node1 not vgchange -ay testvg
node1 vgchange --persist start testvg
node1 lvcreate -l1 -n lv1 -an testvg
node1 vgchange -ay testvg
node1 vgchange -an testvg
node1 vgchange --persist stop testvg
node1 not lvcreate -l1 testvg
node1 not vgchange -ay testvg
node1 vgchange --persist start testvg
node1 vgremove -ff testvg

# setpersist individual settings
node1 vgcreate testvg $d1
node1 vgchange --setpersist y testvg
node1 vgs -o persist testvg | grep 'require,autostart'
node1 vgchange --setpersist noautostart testvg
node1 vgs -o persist testvg | grep 'require'
node1 vgchange --setpersist autostart testvg
node1 vgs -o persist testvg | grep 'require,autostart'
node1 vgchange --setpersist norequire testvg
node1 vgs -o persist testvg | grep 'autostart'
node1 vgchange --setpersist n testvg
node1 'test -z "$(vgs --noheadings -o persist testvg | tr -d " ")"'
node1 vgchange --setpersist ptpl testvg
node1 vgs -o persist testvg | grep 'ptpl'
node1 vgchange --setpersist noptpl testvg
node1 vgremove -ff testvg

# persist read/clear
node1 vgcreate testvg $d1
node1 vgchange --persist start testvg
node1 vgchange --persist check testvg | grep -q "$KEY1"
node1 vgchange --persist read testvg
node1 vgchange --persist clear --yes testvg
node1 not vgchange --persist check testvg
node1 lvmpersist read-keys --vg testvg | grep -q "keys: none"
node1 vgremove -ff testvg

# persist autostart
node1 vgcreate testvg $d1
node1 vgchange --persist autostart testvg || true
node1 not vgchange --persist check testvg
node1 vgchange --setpersist autostart testvg
node1 vgchange --persist autostart testvg
node1 vgchange --persist check testvg | grep -q "$KEY1"
node1 vgchange --persist stop testvg
node1 vgremove -ff testvg

# vgchange -ay --persist start
node1 vgcreate --setpersist y --persist start testvg $d1
node1 lvcreate -l1 -n lv1 -an testvg
node1 vgchange --persist stop testvg
node1 not vgchange -ay testvg
node1 vgchange -ay --persist start testvg
node1 vgchange --persist check testvg | grep -q "$KEY1"
node1 vgchange -an testvg
node1 vgremove -ff testvg

# combined setpersist and start
node1 vgcreate testvg $d1
node1 vgchange --setpersist y --persist start testvg
node1 vgchange --persist check testvg | grep -q "$KEY1"
node1 vgs -o persist testvg | grep 'require,autostart'
node1 vgremove -ff testvg

# multi-node exclusion and write protection
node1 vgcreate testvg $d1
node1 vgchange --persist start testvg
node1 vgchange --persist check testvg | grep -q "$KEY1"
node2 vgchange --persist read testvg
node2 not vgchange --persist check testvg
node2 not vgchange --persist start testvg
node2 not dd if=/dev/zero of=$d1 bs=4096 count=1 oflag=direct,sync
node1 lvcreate -l1 -n lv1 -an testvg
node1 lvchange -ay testvg/lv1
node2 lvchange -ay testvg/lv1
node1 dd if=/dev/zero of=/dev/testvg/lv1 bs=4096 count=1 oflag=direct,sync
node2 not dd if=/dev/zero of=/dev/testvg/lv1 bs=4096 count=1 oflag=direct,sync
node1 lvchange -an testvg/lv1
node2 lvchange -an testvg/lv1
node1 vgchange --persist stop testvg
node1 vgremove -ff testvg

# vgexport --persist stop / vgimport --persist start
node1 vgcreate --setpersist y --persist start testvg $d1
node1 vgchange --persist check testvg | grep -q "$KEY1"
node1 lvmpersist check-key --key $KEY1 --device $d1
node1 vgexport --persist stop testvg
node1 not lvmpersist check-key --key $KEY1 --device $d1
node1 vgimport --persist start testvg
node1 vgchange --persist check testvg | grep -q "$KEY1"
node1 lvmpersist check-key --key $KEY1 --device $d1
node1 vgremove -ff testvg

# vgexport/vgimport without setpersist: PR not affected
node1 vgcreate testvg $d1
node1 vgchange --persist start testvg
node1 lvmpersist check-key --key $KEY1 --device $d1
node1 vgexport testvg
node1 lvmpersist check-key --key $KEY1 --device $d1
node1 vgimport testvg
node1 lvmpersist check-key --key $KEY1 --device $d1
node1 vgchange --persist stop testvg
node1 vgremove -ff testvg

# vgexport --persist stop then vgimport --persist start on different node
node1 vgcreate --setpersist y --persist start testvg $d1
node1 lvmpersist check-key --key $KEY1 --device $d1
node1 vgexport --persist stop testvg
node1 not lvmpersist check-key --key $KEY1 --device $d1
node2 vgimport --persist start testvg
node2 vgchange --persist check testvg | grep -q "$KEY2"
node2 lvmpersist check-key --key $KEY2 --device $d1
node1 not lvmpersist check-key --key $KEY1 --device $d1
node2 vgremove -ff testvg

# each node can start
node1 vgcreate --setpersist y testvg $d1
for node_num in $(seq 1 $CLUSTER_NUM_NODES); do
    noden ${node_num} not lvcreate -l1 testvg
    noden ${node_num} vgchange --persist start testvg
    noden ${node_num} vgchange --persist check testvg | grep -q "$(printf '0x%016x' $((0x1000000000000000 + node_num)))"
    noden ${node_num} lvcreate -l1 -an testvg
    noden ${node_num} vgchange --persist stop testvg
done
node1 not vgremove -ff testvg
node1 vgchange --persist start testvg
node1 vgremove -ff testvg

# sequential starts: only one can start
node1 vgcreate --setpersist y testvg $d1
node1 vgchange --persist start testvg
node1 lvcreate -l1 -an testvg
for node_num in $(seq 2 $CLUSTER_NUM_NODES); do
    noden ${node_num} not vgchange --persist start testvg
    noden ${node_num} not lvcreate -l1 testvg
    noden ${node_num} not dd if=/dev/zero of=$d1 bs=4096 count=1 oflag=direct,sync
done
node1 vgchange --persist stop testvg
node2 vgchange --persist start testvg
node2 lvcreate -l1 -an testvg
node1 not vgchange --persist start testvg
node1 not lvcreate -l1 testvg
node1 not dd if=/dev/zero of=$d1 bs=4096 count=1 oflag=direct,sync
node2 vgremove -ff testvg

# clean up lvm state so 2-device section starts fresh
nodes rm -f /etc/lvm/devices/system.devices
nodes rm -f /run/lvm/hints

# ============================================================
# Two device tests
# ============================================================

# ============================================================
# pr_key tests: verify PR uses configured local/pr_key values
# ============================================================

KEY1=0xaa01
KEY2=0xaa02

node1 'sed -i -e "/pr_key/d" -e "/local {/a\\    pr_key = \"0xaa01\"" /etc/lvm/lvmlocal.conf'
node2 'sed -i -e "/pr_key/d" -e "/local {/a\\    pr_key = \"0xaa02\"" /etc/lvm/lvmlocal.conf'

if [ "$CLUSTER_NUM_NODES" -gt 2 ]; then
    for n in $(seq 3 $CLUSTER_NUM_NODES); do
        noden ${n} "sed -i -e '/pr_key/d' -e '/local {/a\\    pr_key = \"0xaa0${n}\"' /etc/lvm/lvmlocal.conf"
    done
fi

node1 lvmconfig local/pr_key
node2 lvmconfig local/pr_key

#
# pr_key: basic start/stop with key verification
#
node1 vgcreate testvg $d1 $d2
node1 vgchange --persist start testvg
node1 vgchange --persist check testvg | grep -q "$KEY1"

node1 lvmpersist check-key --key $KEY1 --vg testvg
node1 lvmpersist read-keys --vg testvg | grep -q "$KEY1"
node1 lvmpersist read-reservation --vg testvg | grep -qE "reservation: (WE |WEAR)"

node2 lvmpersist check-key --key $KEY1 --vg testvg
node2 not lvmpersist check-key --key $KEY2 --vg testvg

node1 vgchange --persist stop testvg
node1 not vgchange --persist check testvg
node1 lvmpersist read-keys --vg testvg | grep -q "keys: none"

node1 vgremove -ff testvg

#
# pr_key: combined vgcreate --setpersist y --persist start
#
node1 vgcreate --setpersist y --persist start testvg $d1 $d2
node1 vgchange --persist check testvg | grep -q "$KEY1"
node1 lvmpersist check-key --key $KEY1 --vg testvg
node1 vgremove -ff testvg

#
# pr_key: takeover with --removekey
#
node1 vgcreate testvg $d1 $d2
node1 vgchange --persist start testvg

node2 not vgchange --persist start testvg

node2 vgchange --persist start --removekey $KEY1 testvg
node2 vgchange --persist check testvg | grep -q "$KEY2"
node1 not vgchange --persist check testvg

node2 lvmpersist check-key --key $KEY2 --vg testvg
node2 not lvmpersist check-key --key $KEY1 --vg testvg

node1 not dd if=/dev/zero of=$d1 bs=4096 count=1 oflag=direct,sync
node2 dd if=/dev/zero of=$d1 bs=4096 count=1 seek=256 oflag=direct,sync

node1 vgchange --persist start --removekey $KEY2 testvg
node1 vgchange --persist check testvg | grep -q "$KEY1"
node1 lvmpersist check-key --key $KEY1 --vg testvg
node1 not lvmpersist check-key --key $KEY2 --vg testvg

node1 vgchange --persist stop testvg
node1 vgremove -ff testvg

#
# pr_key: vgextend propagates key to new device
#
if [ -n "$d3" ]; then

node1 vgcreate --setpersist y --persist start testvg $d1 $d2
node1 vgextend testvg $d3

node1 vgchange --persist check testvg | grep -q "$KEY1"
node1 lvmpersist check-key --key $KEY1 --device $d3

node1 vgremove -ff testvg

fi # d3

#
# pr_key: vgremove stops PR when setpersist is set
#
node1 vgcreate --setpersist y --persist start testvg $d1 $d2
node1 lvmpersist check-key --key $KEY1 --device $d1
node1 lvmpersist check-key --key $KEY1 --device $d2

node1 vgremove -ff testvg

node1 not lvmpersist check-key --key $KEY1 --device $d1
node1 not lvmpersist check-key --key $KEY1 --device $d2
node1 lvmpersist read-keys --device $d1 | grep -q "keys: none"
node1 lvmpersist read-keys --device $d2 | grep -q "keys: none"

# ============================================================
# host_id-derived keys
# key format: 0x100000000000XXXX where XXXX is hex of host_id.
# host_id is used when local/pr_key is not set.
# ============================================================

KEY1=0x1000000000000001
KEY2=0x1000000000000002

nodes 'sed -i "/pr_key/d" /etc/lvm/lvmlocal.conf'
nodes 'sed -i -e "/host_id/d" -e "/local {/a\\    host_id = ${NODE_NUM}" /etc/lvm/lvmlocal.conf'

node1 lvmconfig local/host_id
node2 lvmconfig local/host_id

#
# host_id: basic start/stop with key verification
#
node1 vgcreate testvg $d1 $d2
node1 vgchange --persist start testvg
node1 vgchange --persist check testvg | grep -q "$KEY1"

node1 lvmpersist check-key --key $KEY1 --vg testvg
node1 lvmpersist read-keys --vg testvg | grep -q "$KEY1"
node1 lvmpersist read-reservation --vg testvg | grep -qE "reservation: (WE |WEAR)"

node2 lvmpersist check-key --key $KEY1 --vg testvg
node2 not lvmpersist check-key --key $KEY2 --vg testvg

node1 vgchange --persist stop testvg
node1 not vgchange --persist check testvg
node1 lvmpersist read-keys --vg testvg | grep -q "keys: none"

node1 vgremove -ff testvg

#
# host_id: combined vgcreate --setpersist y --persist start
#
node1 vgcreate --setpersist y --persist start testvg $d1 $d2
node1 vgchange --persist check testvg | grep -q "$KEY1"
node1 lvmpersist check-key --key $KEY1 --vg testvg
node1 vgremove -ff testvg

#
# host_id: takeover with --removekey
#
node1 vgcreate testvg $d1 $d2
node1 vgchange --persist start testvg

node2 not vgchange --persist start testvg

node2 vgchange --persist start --removekey $KEY1 testvg
node2 vgchange --persist check testvg | grep -q "$KEY2"
node1 not vgchange --persist check testvg

node2 lvmpersist check-key --key $KEY2 --vg testvg
node2 not lvmpersist check-key --key $KEY1 --vg testvg

node1 not dd if=/dev/zero of=$d1 bs=4096 count=1 oflag=direct,sync
node2 dd if=/dev/zero of=$d1 bs=4096 count=1 seek=256 oflag=direct,sync

node1 vgchange --persist start --removekey $KEY2 testvg
node1 vgchange --persist check testvg | grep -q "$KEY1"
node1 lvmpersist check-key --key $KEY1 --vg testvg
node1 not lvmpersist check-key --key $KEY2 --vg testvg

node1 vgchange --persist stop testvg
node1 vgremove -ff testvg

#
# host_id: vgextend propagates key to new device
#
if [ -n "$d3" ]; then

node1 vgcreate --setpersist y --persist start testvg $d1 $d2
node1 vgextend testvg $d3

node1 vgchange --persist check testvg | grep -q "$KEY1"
node1 lvmpersist check-key --key $KEY1 --device $d3

node1 vgremove -ff testvg

fi # d3

#
# host_id: vgremove stops PR when setpersist is set
#
node1 vgcreate --setpersist y --persist start testvg $d1 $d2
node1 lvmpersist check-key --key $KEY1 --device $d1
node1 lvmpersist check-key --key $KEY1 --device $d2

node1 vgremove -ff testvg

node1 not lvmpersist check-key --key $KEY1 --device $d1
node1 not lvmpersist check-key --key $KEY1 --device $d2
node1 lvmpersist read-keys --device $d1 | grep -q "keys: none"
node1 lvmpersist read-keys --device $d2 | grep -q "keys: none"

# ============================================================
# General tests (not key-method dependent)
# ============================================================

#
# vgcreate --setpersist y: require enforcement
#
node1 vgcreate --setpersist y testvg $d1 $d2
node1 vgs -o persist testvg | grep 'require,autostart'

node1 not vgchange --persist check testvg

# require is set: write/activate commands fail without PR
node1 not lvcreate -l1 testvg
node1 not vgchange -ay testvg

# start PR
node1 vgchange --persist start testvg
node1 vgchange --persist check testvg | grep -q "$KEY1"

# commands work with PR started
node1 lvcreate -l1 -n lv1 -an testvg
node1 vgchange -ay testvg
node1 vgchange -an testvg

# stop PR
node1 vgchange --persist stop testvg
node1 not vgchange --persist check testvg

# require enforcement again
node1 not lvcreate -l1 testvg
node1 not vgchange -ay testvg

node1 vgchange --persist start testvg
node1 vgremove -ff testvg

#
# vgchange --setpersist: individual settings
#
node1 vgcreate testvg $d1 $d2

node1 vgchange --setpersist y testvg
node1 vgs -o persist testvg | grep 'require,autostart'

node1 vgchange --setpersist noautostart testvg
node1 vgs -o persist testvg | grep 'require'

node1 vgchange --setpersist autostart testvg
node1 vgs -o persist testvg | grep 'require,autostart'

node1 vgchange --setpersist norequire testvg
node1 vgs -o persist testvg | grep 'autostart'

node1 vgchange --setpersist n testvg
node1 'test -z "$(vgs --noheadings -o persist testvg | tr -d " ")"'

node1 vgchange --setpersist ptpl testvg
node1 vgs -o persist testvg | grep 'ptpl'

node1 vgchange --setpersist noptpl testvg

node1 vgremove -ff testvg

#
# vgchange --persist read/clear
#
node1 vgcreate testvg $d1 $d2
node1 vgchange --persist start testvg
node1 vgchange --persist check testvg | grep -q "$KEY1"

node1 vgchange --persist read testvg

node1 vgchange --persist clear --yes testvg
node1 not vgchange --persist check testvg
node1 lvmpersist read-keys --vg testvg | grep -q "keys: none"
node1 lvmpersist read-reservation --vg testvg | grep -q "reservation: none"

node1 vgremove -ff testvg

#
# vgchange --persist autostart: conditional start
#
node1 vgcreate testvg $d1 $d2

# autostart not set: persist autostart does not start PR
node1 vgchange --persist autostart testvg || true
node1 not vgchange --persist check testvg

# set autostart: persist autostart now starts PR
node1 vgchange --setpersist autostart testvg
node1 vgchange --persist autostart testvg
node1 vgchange --persist check testvg | grep -q "$KEY1"

node1 vgchange --persist stop testvg
node1 vgremove -ff testvg

#
# Supplementary PR start: vgchange -ay --persist start
#
node1 vgcreate --setpersist y --persist start testvg $d1 $d2
node1 lvcreate -l1 -n lv1 -an testvg
node1 vgchange --persist stop testvg

# without --persist start, activation fails (require is set, PR not started)
node1 not vgchange -ay testvg

# with --persist start, PR starts first, then activation succeeds
node1 vgchange -ay --persist start testvg
node1 vgchange --persist check testvg | grep -q "$KEY1"

node1 vgchange -an testvg
node1 vgremove -ff testvg

#
# vgchange --setpersist y --persist start: combined setpersist and start
#
node1 vgcreate testvg $d1 $d2

node1 vgchange --setpersist y --persist start testvg
node1 vgchange --persist check testvg | grep -q "$KEY1"
node1 vgs -o persist testvg | grep 'require,autostart'

node1 vgremove -ff testvg

#
# vgremove does NOT stop PR when setpersist is not set
#
node1 vgcreate testvg $d1 $d2
node1 vgchange --persist start testvg
node1 lvmpersist check-key --key $KEY1 --device $d1

node1 vgremove -ff testvg

node1 lvmpersist check-key --key $KEY1 --device $d1
node1 lvmpersist check-key --key $KEY1 --device $d2

node1 lvmpersist clear --ourkey $KEY1 --device $d1 --device $d2
node1 lvmpersist read-keys --device $d1 | grep -q "keys: none"

#
# Multi-node: exclusion and write protection
#
node1 vgcreate testvg $d1 $d2
node1 vgchange --persist start testvg
node1 vgchange --persist check testvg | grep -q "$KEY1"

# node2 can read PR state but is not started
node2 vgchange --persist read testvg
node2 not vgchange --persist check testvg

# node2 cannot start (WE is exclusive)
node2 not vgchange --persist start testvg

# node2 cannot write to devices
node2 not dd if=/dev/zero of=$d1 bs=4096 count=1 oflag=direct,sync
node2 not dd if=/dev/zero of=$d2 bs=4096 count=1 oflag=direct,sync

# write protection with LVs
node1 lvcreate -l1 -n lv1 -an testvg $d1
node1 lvcreate -l1 -n lv2 -an testvg $d2
node1 lvchange -ay testvg/lv1 testvg/lv2
node2 lvchange -ay testvg/lv1 testvg/lv2

node1 dd if=/dev/zero of=/dev/testvg/lv1 bs=4096 count=1 oflag=direct,sync
node1 dd if=/dev/zero of=/dev/testvg/lv2 bs=4096 count=1 oflag=direct,sync

node2 not dd if=/dev/zero of=/dev/testvg/lv1 bs=4096 count=1 oflag=direct,sync
node2 not dd if=/dev/zero of=/dev/testvg/lv2 bs=4096 count=1 oflag=direct,sync
node2 not dd if=/dev/zero of=$d1 bs=4096 count=1 oflag=direct,sync
node2 not dd if=/dev/zero of=$d2 bs=4096 count=1 oflag=direct,sync

node1 lvchange -an testvg/lv1 testvg/lv2
node2 lvchange -an testvg/lv1 testvg/lv2

node1 vgchange --persist stop testvg
node1 vgremove -ff testvg

#
# vgexport --persist stop / vgimport --persist start
#
node1 vgcreate --setpersist y --persist start testvg $d1 $d2
node1 vgchange --persist check testvg | grep -q "$KEY1"
node1 lvmpersist check-key --key $KEY1 --device $d1

node1 vgexport --persist stop testvg
node1 not lvmpersist check-key --key $KEY1 --device $d1
node1 lvmpersist read-keys --device $d1 | grep -q "keys: none"

node1 vgimport --persist start testvg
node1 vgchange --persist check testvg | grep -q "$KEY1"
node1 lvmpersist check-key --key $KEY1 --device $d1

node1 vgremove -ff testvg

#
# vgexport/vgimport without setpersist: PR not affected
#
node1 vgcreate testvg $d1 $d2
node1 vgchange --persist start testvg
node1 lvmpersist check-key --key $KEY1 --device $d1

node1 vgexport testvg
node1 lvmpersist check-key --key $KEY1 --device $d1

node1 vgimport testvg
node1 lvmpersist check-key --key $KEY1 --device $d1

node1 vgchange --persist stop testvg
node1 vgremove -ff testvg

#
# vgexport --persist stop then vgimport --persist start on different node
#
node1 vgcreate --setpersist y --persist start testvg $d1 $d2
node1 lvmpersist check-key --key $KEY1 --device $d1

node1 vgexport --persist stop testvg
node1 not lvmpersist check-key --key $KEY1 --device $d1

node2 vgimport --persist start testvg
node2 vgchange --persist check testvg | grep -q "$KEY2"
node2 lvmpersist check-key --key $KEY2 --device $d1
node1 not lvmpersist check-key --key $KEY1 --device $d1

node2 vgremove -ff testvg

#
# Each node can start
#
node1 vgcreate --setpersist y testvg $d1 $d2

for node_num in $(seq 1 $CLUSTER_NUM_NODES); do
    noden ${node_num} not lvcreate -l1 testvg
    noden ${node_num} vgchange --persist start testvg
    noden ${node_num} vgchange --persist check testvg | grep -q "$(printf '0x%016x' $((0x1000000000000000 + node_num)))"
    noden ${node_num} lvcreate -l1 -an testvg
    noden ${node_num} vgchange --persist stop testvg
done

node1 not vgremove -ff testvg
node1 vgchange --persist start testvg
node1 vgremove -ff testvg

#
# Parallel start race: only one node wins WE reservation
#
# multipath uses WEAR even for local VGs, which allows
# multiple nodes to be started.  vgchange --persist start
# tries to prevent this by first checking if another node
# is started and if so not starting a second node, but
# this check doesn't solve parallel starts where multiple
# nodes check at the same time.
#
if [ "$devtype" != "mpath" ]; then
node1 vgcreate testvg $d1 $d2

for i in $(seq 1 5); do
    nodep vgchange --persist start testvg || true
    assert_one_success

    success_node vgchange --persist check testvg

    for node_num in $NODEP_FAIL_NODES; do
        noden ${node_num} not vgchange --persist check testvg
        noden ${node_num} not dd if=/dev/zero of=$d1 bs=4096 count=1 oflag=direct,sync
    done

    success_node vgchange --persist stop testvg

    sleep 0.5
done

# parallel race with clear between iterations
for i in $(seq 1 3); do
    nodep vgchange --persist start testvg || true
    assert_one_success

    success_node vgchange --persist clear --yes testvg

    sleep 0.5
done

node1 vgremove -ff testvg
fi

#
# Sequential starts: only one can start
#
node1 vgcreate --setpersist y testvg $d1 $d2
node1 vgchange --persist start testvg
node1 lvcreate -l1 -an testvg

for node_num in $(seq 2 $CLUSTER_NUM_NODES); do
    noden ${node_num} not vgchange --persist start testvg
    noden ${node_num} not lvcreate -l1 testvg
    noden ${node_num} not dd if=/dev/zero of=$d1 bs=4096 count=1 oflag=direct,sync
done

node1 vgchange --persist stop testvg
node2 vgchange --persist start testvg
node2 lvcreate -l1 -an testvg
node1 not vgchange --persist start testvg
node1 not lvcreate -l1 testvg
node1 not dd if=/dev/zero of=$d1 bs=4096 count=1 oflag=direct,sync
node2 vgremove -ff testvg

#
# vgchange --systemid with --persist start/stop
#
nodes 'sed -i -e "/system_id_source/d" -e "/global {/a\\    system_id_source = \"uname\"" /etc/lvm/lvm.conf'

SID1="${CLUSTER_ID}-node1"
SID2="${CLUSTER_ID}-node2"

node1 vgcreate --setpersist y --persist start testvg $d1 $d2
node1 vgs -o systemid --noheadings testvg | grep -q "$SID1"
node1 vgchange --persist check testvg | grep -q "$KEY1"

# node1 assigns vg ownership to node2
node1 vgchange --systemid $SID2 --persist stop --yes testvg
node1 not lvmpersist check-key --key $KEY1 --device $d1

node2 vgs -o systemid --noheadings testvg | grep -q "$SID2"
node2 vgchange --persist start testvg
node2 vgchange --persist check testvg | grep -q "$KEY2"
node2 lvmpersist check-key --key $KEY2 --device $d1
node2 not lvmpersist check-key --key $KEY1 --device $d1

node1 not vgs testvg
node1 vgs --foreign testvg

# node2 assigns vg ownership to node1
node2 vgchange --systemid $SID1 --persist stop --yes testvg
node1 vgs -o systemid --noheadings testvg | grep -q "$SID1"
node1 vgchange --persist start testvg
node1 vgchange --persist check testvg | grep -q "$KEY1"
node1 vgremove -ff testvg

#
# vgchange --systemid --persist start with --removekey (forcible takeover)
#
node1 vgcreate --setpersist y --persist start testvg $d1 $d2
node1 vgchange --persist check testvg | grep -q "$KEY1"

node2 'sed -i -e "/extra_system_ids/d" -e "/local {/a\\    extra_system_ids = [ \"'"$SID1"'\" ]" /etc/lvm/lvmlocal.conf'

# node2 forcibly takes over VG with --removekey
node2 vgchange --systemid $SID2 --persist start --removekey $KEY1 testvg
node2 vgchange --persist check testvg | grep -q "$KEY2"
node2 lvmpersist check-key --key $KEY2 --device $d1
node2 not lvmpersist check-key --key $KEY1 --device $d1

# node1 cannot write
node1 not dd if=/dev/zero of=$d1 bs=4096 count=1 oflag=direct,sync

nodes 'sed -i "/extra_system_ids/d" /etc/lvm/lvmlocal.conf'

node2 vgremove -ff testvg

nodes 'sed -i "/system_id_source/d" /etc/lvm/lvm.conf'

nodes rm -f /etc/lvm/devices/system.devices
nodes rm -f /run/lvm/hints

done

# ============================================================
# All devices: basic PR operations on a VG using all imported devices
# ============================================================

ALL_DEVS=""
for i in $(seq 1 $CLUSTER_DEV_COUNT); do
    ALL_DEVS="$ALL_DEVS $(eval echo \$dev$i)"
done

node1 vgcreate --setpersist y --persist start testvg $ALL_DEVS
node1 vgchange --persist check testvg | grep -q "$KEY1"

for i in $(seq 1 $CLUSTER_DEV_COUNT); do
    node1 lvmpersist check-key --key $KEY1 --device $(eval echo \$dev$i)
done

node2 not vgchange --persist start testvg
node2 not dd if=/dev/zero of=$dev1 bs=4096 count=1 oflag=direct,sync

node1 vgchange --persist stop testvg
node1 not vgchange --persist check testvg

for i in $(seq 1 $CLUSTER_DEV_COUNT); do
    node1 lvmpersist read-keys --device $(eval echo \$dev$i) | grep -q "keys: none"
done

node1 vgchange --persist start testvg
node1 vgchange --persist check testvg | grep -q "$KEY1"

node2 vgchange --persist start --removekey $KEY1 testvg
node2 vgchange --persist check testvg | grep -q "$KEY2"

for i in $(seq 1 $CLUSTER_DEV_COUNT); do
    node2 lvmpersist check-key --key $KEY2 --device $(eval echo \$dev$i)
    node2 not lvmpersist check-key --key $KEY1 --device $(eval echo \$dev$i)
done

node2 vgremove -ff testvg

exit 0
