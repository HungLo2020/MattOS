#!/bin/bash
# Exercise vgchange --persist and vgchange --setpersist commands
# for shared VGs using sanlock.
# Shared VGs use WEAR reservation type and host_id+generation keys.
# See lvmpersist.8 for details.

set -e

if [ "$CLUSTER_NUM_NODES" -lt 2 ]; then
    echo "SKIP: This test requires at least 2 nodes (found $CLUSTER_NUM_NODES)"
    exit 0
fi

# ============================================================
# Shared VG PR keys use host_id + sanlock generation:
# 0x100000YYYYYYXXXX (XXXX=host_id, YYYYYY=generation)
# Keys are extracted dynamically since the generation varies.
# pr_key in lvmlocal.conf is ignored for shared VGs.
# Shared VGs always use WEAR (Write Exclusive - all registrants).
# ============================================================

nodes 'sed -i -e "/pr_key/d" -e "/host_id/d" -e "/local {/a\\    host_id = ${NODE_NUM}" /etc/lvm/lvmlocal.conf'

node1 lvmconfig local/host_id
node2 lvmconfig local/host_id

get_key() {
    noden $1 vgchange --persist check testvg | grep "key for local host is registered" | grep -oE '0x[0-9a-fA-F]+'
}

check_key() {
    local key=$1 file=$2
    local total=$(grep -c "^Device" "$file")
    local match=$(grep "^Device" "$file" | grep -c "$key")
    [ "$total" -gt 0 ] && [ "$total" -eq "$match" ]
}

check_key_not() {
    local key=$1 file=$2
    local match=$(grep "^Device" "$file" | grep -c "$key")
    [ "$match" -eq 0 ]
}

check_key_none() {
    local file=$1
    local total=$(grep -c "^Device" "$file")
    local match=$(grep "^Device" "$file" | grep -c "keys: none")
    [ "$total" -gt 0 ] && [ "$total" -eq "$match" ]
}

check_reservation() {
    local type=$1 file=$2
    local total=$(grep -c "^Device" "$file")
    local match=$(grep "^Device" "$file" | grep -c "reservation: $type")
    [ "$total" -gt 0 ] && [ "$total" -eq "$match" ]
}

check_reservation_not() {
    local type=$1 file=$2
    local match=$(grep "^Device" "$file" | grep -c "reservation: $type")
    [ "$match" -eq 0 ]
}

check_reservation_none() {
    local file=$1
    local total=$(grep -c "^Device" "$file")
    local match=$(grep "^Device" "$file" | grep -c "reservation: none")
    [ "$total" -gt 0 ] && [ "$total" -eq "$match" ]
}

node1_vgremove_retry() {
    while ! node1 vgremove -y -f testvg; do
        sleep 1
    done
}

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
# Basic start/stop
# ============================================================

#
# basic: start/stop PR, verify WEAR and key state
#
node1 vgcreate --shared testvg $d1 $d2
node1 vgchange --persist start testvg
node1 vgchange --persist check testvg

KEY1=$(get_key 1)
node1 lvmpersist check-key --key $KEY1 --vg testvg
node1 lvmpersist read-keys --vg testvg | tee out
check_key $KEY1 out
node1 lvmpersist read-reservation --vg testvg | tee out
check_reservation WEAR out

node2 lvmpersist check-key --key $KEY1 --vg testvg

# without --setpersist y, lockstop and persist stop need
# to be run by separate vgchange commands, not together.
node1 vgchange --lockstop testvg
node1 vgchange --persist stop testvg
node1 lvmpersist read-keys --vg testvg | tee out
check_key_none out
node1 not vgchange --persist check testvg

node1 vgchange --lockstart testvg
node1_vgremove_retry

#
# vgcreate --shared --setpersist y: auto-starts PR
#
node1 vgcreate --shared --setpersist y testvg $d1 $d2
node1 vgchange --persist check testvg

KEY1=$(get_key 1)
node1 lvmpersist check-key --key $KEY1 --vg testvg
node1 lvmpersist read-reservation --vg testvg | tee out
check_reservation WEAR out
node1 vgs -o persist testvg | grep 'require,autostart'

node1_vgremove_retry

#
# vgcreate --shared --setpersist y --persist start
#
node1 vgcreate --shared --setpersist y --persist start testvg $d1 $d2
node1 vgchange --persist check testvg

KEY1=$(get_key 1)
node1 lvmpersist check-key --key $KEY1 --vg testvg
node1 vgs -o persist testvg | grep 'require,autostart'

node1_vgremove_retry

# ============================================================
# Multi-node WEAR: all registrants can write
# ============================================================

#
# two nodes started: both can write, stopped node cannot
#
node1 vgcreate --shared --setpersist y testvg $d1 $d2
node1 vgchange --persist start testvg
KEY1=$(get_key 1)

node2 vgchange --persist start testvg
KEY2=$(get_key 2)

node1 lvmpersist check-key --key $KEY1 --vg testvg
node1 lvmpersist check-key --key $KEY2 --vg testvg
node1 lvmpersist read-keys --vg testvg | tee out
check_key $KEY1 out
check_key $KEY2 out
node1 lvmpersist read-reservation --vg testvg | tee out
check_reservation WEAR out

node1 dd if=/dev/zero of=$d1 bs=4096 count=1 seek=76800 oflag=direct,sync
node2 dd if=/dev/zero of=$d1 bs=4096 count=1 seek=76800 oflag=direct,sync
node1 dd if=/dev/zero of=$d2 bs=4096 count=1 seek=76800 oflag=direct,sync
node2 dd if=/dev/zero of=$d2 bs=4096 count=1 seek=76800 oflag=direct,sync

node2 vgchange --persist stop testvg
node2 not vgchange --persist check testvg

node1 vgchange --persist check testvg | grep -q "$KEY1"
node1 not lvmpersist check-key --key $KEY2 --vg testvg

node2 not dd if=/dev/zero of=$d1 bs=4096 count=1 seek=76800 oflag=direct,sync
node2 not dd if=/dev/zero of=$d2 bs=4096 count=1 seek=76800 oflag=direct,sync
node1 dd if=/dev/zero of=$d1 bs=4096 count=1 seek=76800 oflag=direct,sync

node1_vgremove_retry

#
# remove key: one host removes another host's registration
#
node1 vgcreate --shared testvg --setpersist y $d1 $d2
node1 vgchange --persist start testvg
KEY1=$(get_key 1)

node2 vgchange --persist start --lockstart testvg
KEY2=$(get_key 2)

node1 lvmpersist check-key --key $KEY1 --vg testvg
node1 lvmpersist check-key --key $KEY2 --vg testvg

node1 dd if=/dev/zero of=$d1 bs=4096 count=1 seek=76800 oflag=direct,sync
node2 dd if=/dev/zero of=$d1 bs=4096 count=1 seek=76800 oflag=direct,sync

node1 lvcreate -l1 -n lv1 testvg
node2 lvcreate -l1 -n lv2 testvg

node1 dd if=/dev/zero of=/dev/testvg/lv1 bs=4096 count=1 oflag=direct,sync
node2 dd if=/dev/zero of=/dev/testvg/lv2 bs=4096 count=1 oflag=direct,sync

node1 vgchange --persist remove --removekey $KEY2 testvg

node1 not lvmpersist check-key --key $KEY2 --vg testvg
node1 lvmpersist check-key --key $KEY1 --vg testvg

node1 dd if=/dev/zero of=/dev/testvg/lv1 bs=4096 count=1 oflag=direct,sync

node2 not dd if=/dev/zero of=/dev/testvg/lv2 bs=4096 count=1 oflag=direct,sync
node2 not dd if=/dev/zero of=$d1 bs=4096 count=1 seek=76800 oflag=direct,sync
node2 not dd if=/dev/zero of=$d2 bs=4096 count=1 seek=76800 oflag=direct,sync
node2 not lvcreate -l1 testvg
node2 not lvremove -y testvg/lv2

node1 lvchange -an testvg/lv1
node1 lvremove testvg/lv1

# node2 still holds an LV lock on lv2
# this retries until the node2 has been
# reset by the watchdog after host_fail_timeout
# TODO: should tests with node recovery delays
# be moved to another test file?
while ! node1 lvremove testvg/lv2; do
        sleep 1
done

node1_vgremove_retry

# The "remove key" section above causes I/O errors on node2 (PR fencing),
# which can trigger NVMe-oF/iSCSI reconnection and device renumbering.
refresh_devices

#
# write protection with LVs
#
node1 vgcreate --shared testvg --setpersist y $d1 $d2

node1 vgchange --persist start testvg
KEY1=$(get_key 1)

node2 vgchange --persist start testvg
node2 vgchange --lockstart testvg
KEY2=$(get_key 2)

node1 lvcreate -l1 -n lv1 -an testvg $d1
node1 lvcreate -l1 -n lv2 -an testvg $d2

node1 lvchange -ay testvg/lv1
node2 lvchange -ay testvg/lv2

node1 dd if=/dev/zero of=/dev/testvg/lv1 bs=4096 count=1 oflag=direct,sync
node2 dd if=/dev/zero of=/dev/testvg/lv2 bs=4096 count=1 oflag=direct,sync

node2 lvchange -an testvg/lv2
node2 vgchange --lockstop testvg
node2 vgchange --persist stop testvg

node2 not dd if=/dev/zero of=$d1 bs=4096 count=1 seek=76800 oflag=direct,sync
node2 not dd if=/dev/zero of=$d2 bs=4096 count=1 seek=76800 oflag=direct,sync

node1 dd if=/dev/zero of=/dev/testvg/lv1 bs=4096 count=1 oflag=direct,sync

node1 lvremove -y testvg/lv1
node1_vgremove_retry

# ============================================================
# General tests
# ============================================================

#
# vgcreate --shared --setpersist y: require enforcement
#
node1 vgcreate --shared --setpersist y testvg $d1 $d2
node1 vgs -o persist testvg | grep 'require,autostart'
node1 vgchange --persist check testvg

node1 lvcreate -l1 -n lv1 -an testvg

# Note: lvm doesn't allow stopping PR before locking,
# so we go behind lvm and use lvmpersist to directly
# stop PR just to test this.
# node1 vgchange --persist stop testvg
KEY1=$(get_key 1)
node1 lvmpersist stop --ourkey $KEY1 --vg testvg
node1 not vgchange --persist check testvg

node1 not lvcreate -l1 testvg
node1 not vgchange -ay testvg

node1 vgchange --persist start testvg
node1 vgchange --persist check testvg
node1 vgchange -ay testvg
node1 vgchange -an testvg
node1 lvremove -y testvg

node1_vgremove_retry

#
# vgchange --setpersist: individual settings
#
node1 vgcreate --shared testvg $d1 $d2

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

node1_vgremove_retry

#
# vgchange --persist read/clear
#
node1 vgcreate --shared testvg $d1 $d2
node1 vgchange --persist start testvg
KEY1=$(get_key 1)
node1 vgchange --persist check testvg | grep -q "$KEY1"

node1 vgchange --persist read testvg

node1 vgchange --persist clear --yes testvg
node1 not vgchange --persist check testvg
node1 lvmpersist read-keys --vg testvg | tee out
check_key_none out
node1 lvmpersist read-reservation --vg testvg | tee out
check_reservation_none out

node1_vgremove_retry

#
# supplementary PR start: vgchange --lockstart --persist start
#
node1 vgcreate --shared --setpersist y testvg $d1 $d2
KEY1=$(get_key 1)

node2 vgchange --lockstart --persist start testvg
node2 vgchange --persist check testvg
KEY2=$(get_key 2)

node1 lvmpersist check-key --key $KEY1 --vg testvg
node1 lvmpersist check-key --key $KEY2 --vg testvg

node2 vgchange --lockstop testvg
node2 vgchange --persist stop testvg

node1_vgremove_retry

#
# vgchange --setpersist y --persist start: combined set and start
#
node1 vgcreate --shared testvg $d1 $d2

node1 vgchange --setpersist y --persist start testvg
node1 vgchange --persist check testvg

KEY1=$(get_key 1)
node1 lvmpersist check-key --key $KEY1 --vg testvg
node1 vgs -o persist testvg | grep 'require,autostart'

node1_vgremove_retry

#
# multiple keys warning: detect stale registrations, cleanup with --persist remove
#
node1 vgcreate --shared --setpersist y testvg $d1 $d2
KEY1=$(get_key 1)

# Construct a stale key with same host_id as node1 but different generation.
host_id_hex=$(echo $KEY1 | sed 's/.*\(....\)$/\1/')
STALE_KEY="0x100000999999${host_id_hex}"

# Re-register so stale key appears before our real key in device key list.
# This exercises the code that prefers the key matching the current generation.
node1 lvmpersist stop --ourkey $KEY1 --device $d1 --device $d2
node2 lvmpersist start --ourkey $STALE_KEY --access sh --device $d1 --device $d2
node1 lvmpersist start --ourkey $KEY1 --access sh --device $d1 --device $d2

node1 lvmpersist check-key --key $KEY1 --vg testvg
node1 lvmpersist check-key --key $STALE_KEY --vg testvg

# persist check should warn about multiple keys for the local host_id.
node1 'vgchange --persist check testvg 2>&1' | tee out
grep "Multiple keys are registered for local host_id" out

# persist check should report the correct key, not the stale one.
grep "key for local host is registered" out | grep -q "$KEY1"
if grep "key for local host is registered" out | grep -q "$STALE_KEY"; then
	echo "FAIL: persist check reported stale key"
	exit 1
fi

# any command reading PR keys should succeed and warn
node1 'vgs testvg 2>&1' > out
grep "Multiple keys are registered for local host_id" out
node1 'lvcreate -l1 -n lv1 -an testvg 2>&1' > out
grep "Multiple keys are registered for local host_id" out
node1 'lvremove testvg/lv1 2>&1' > out
grep "Multiple keys are registered for local host_id" out

# Remove the stale key using the recommended cleanup command.
node1 vgchange --persist remove --removekey $STALE_KEY testvg

node1 not lvmpersist check-key --key $STALE_KEY --vg testvg
node1 lvmpersist check-key --key $KEY1 --vg testvg

# persist check should no longer warn about multiple keys.
node1 'vgchange --persist check testvg 2>&1' | tee out
if grep -q "Multiple keys are registered for local host_id" out; then
	echo "FAIL: unexpected multiple keys warning after cleanup"
	exit 1
fi

node1_vgremove_retry

# ============================================================
# vgextend / vgremove
# ============================================================

#
# vgextend: all hosts must start PR on new device before vgextend
#
if [ -n "$d3" ]; then

node1 vgcreate --shared --setpersist y testvg $d1 $d2
KEY1=$(get_key 1)

node2 vgchange --persist start testvg
node2 vgchange --lockstart testvg
KEY2=$(get_key 2)

node1 lvmpersist start --ourkey $KEY1 --access sh --device $d3
node2 lvmpersist start --ourkey $KEY2 --access sh --device $d3

node1 lvmpersist check-key --key $KEY1 --device $d3
node1 lvmpersist check-key --key $KEY2 --device $d3

node1 vgextend testvg $d3

node1 vgchange --persist check testvg | grep -q "$KEY1"
node1 lvmpersist check-key --key $KEY1 --device $d3

node2 vgchange --lockstop testvg
node2 vgchange --persist stop testvg

node1_vgremove_retry

fi # d3

#
# vgremove stops PR when setpersist is set
#
node1 vgcreate --shared --setpersist y testvg $d1 $d2
KEY1=$(get_key 1)

node1 lvmpersist check-key --key $KEY1 --device $d1
node1 lvmpersist check-key --key $KEY1 --device $d2

node1_vgremove_retry

node1 not lvmpersist check-key --key $KEY1 --device $d1
node1 not lvmpersist check-key --key $KEY1 --device $d2
node1 lvmpersist read-keys --device $d1 | tee out
check_key_none out
node1 lvmpersist read-keys --device $d2 | tee out
check_key_none out

#
# vgremove does NOT stop PR when setpersist is not set
#
node1 vgcreate --shared testvg $d1 $d2
node1 vgchange --persist start testvg
KEY1=$(get_key 1)

node1 lvmpersist check-key --key $KEY1 --device $d1

node1_vgremove_retry

node1 lvmpersist check-key --key $KEY1 --device $d1
node1 lvmpersist check-key --key $KEY1 --device $d2

node1 lvmpersist clear --ourkey $KEY1 --device $d1 --device $d2
node1 lvmpersist read-keys --device $d1 | tee out
check_key_none out

# ============================================================
# Each node can start independently
# ============================================================

#
# each node starts and all can coexist
#
node1 vgcreate --shared --setpersist y testvg $d1 $d2
KEY1=$(get_key 1)

for node_num in $(seq 2 $CLUSTER_NUM_NODES); do
    noden ${node_num} vgchange --persist start testvg
    noden ${node_num} vgchange --persist check testvg
done

for node_num in $(seq 1 $CLUSTER_NUM_NODES); do
    noden ${node_num} dd if=/dev/zero of=$d1 bs=4096 count=1 seek=76800 oflag=direct,sync
done

for node_num in $(seq 2 $CLUSTER_NUM_NODES); do
    noden ${node_num} vgchange --persist stop testvg || true
done

node1 vgchange --persist stop --lockstop testvg

node1 lvmpersist read-keys --device $d1 | tee out
check_key_none out
node1 lvmpersist read-keys --device $d2 | tee out
check_key_none out

nodep vgchange --persist start --lockstart testvg
assert_all_success
nodep 'lvcreate -l1 -n lv_${NODE_NUM} testvg'
assert_all_success
nodep 'lvremove -y testvg/lv_${NODE_NUM}'
assert_all_success
nodep vgchange --lockstop --persist stop testvg
assert_all_success
nodep vgchange --lockstart --persist start testvg
assert_all_success
nodep vgchange --persist check testvg
assert_all_success
nodep 'lvcreate -l1 -n lv_${NODE_NUM} testvg'
assert_all_success
nodep 'lvremove -y testvg/lv_${NODE_NUM}'
assert_all_success

for node_num in $(seq 2 $CLUSTER_NUM_NODES); do
    noden ${node_num} vgchange --persist stop --lockstop testvg
done

node1_vgremove_retry

#
# parallel start only PR
#
node1 vgcreate --shared --setpersist y testvg $d1 $d2

nodep vgchange --persist start testvg
assert_all_success

nodep vgchange --persist check testvg
assert_all_success

for node_num in $(seq 1 $CLUSTER_NUM_NODES); do
    noden ${node_num} dd if=/dev/zero of=$d1 bs=4096 count=1 seek=76800 oflag=direct,sync
done

nodep vgchange --lockstop --persist stop testvg
assert_all_success

node1 lvmpersist read-keys --vg testvg | tee out
check_key_none out

node1 vgchange --persist start --lockstart testvg
node1_vgremove_retry

#
# parallel start locking and PR
#
node1 vgcreate --shared --setpersist y testvg $d1 $d2
node1 vgchange --persist stop --lockstop testvg

nodep vgchange --persist start --lockstart testvg
assert_all_success

nodep vgchange --persist check testvg
assert_all_success

nodep 'lvcreate -l1 -n lv_${NODE_NUM} testvg'
assert_all_success

nodep 'lvremove -y testvg/lv_${NODE_NUM}'
assert_all_success

nodep vgchange --persist stop --lockstop testvg
assert_all_success

node1 lvmpersist read-keys --vg testvg | tee out
check_key_none out

node1 vgchange --persist start --lockstart testvg
node1_vgremove_retry

#
# lockstart --persist start with no VG arg: ignore VG without PR
#
node1 vgcreate --shared --setpersist y testvg1 $d1
node1 vgcreate --shared testvg2 $d2

node1 vgchange --lockstop testvg1
node1 vgchange --persist stop testvg1

node1 vgchange --lockstart --persist start

node1 vgchange --persist check testvg1
node1 not vgchange --persist check testvg2

node1 vgchange --lockstop --persist stop testvg1
node1 vgchange --lockstop testvg2
node1 vgchange --lockstart --persist start testvg1
node1 vgchange --lockstart testvg2
while ! node1 vgremove -y -f testvg2; do sleep 1; done
while ! node1 vgremove -y -f testvg1; do sleep 1; done

nodes rm -f /etc/lvm/devices/system.devices
nodes rm -f /run/lvm/hints

done

# ============================================================
# All devices
# ============================================================

#
# PR on VG using all imported devices
#
ALL_DEVS=""
for i in $(seq 1 $CLUSTER_DEV_COUNT); do
    ALL_DEVS="$ALL_DEVS $(eval echo \$dev$i)"
done

node1 vgcreate --shared --setpersist y testvg $ALL_DEVS
KEY1=$(get_key 1)
node1 vgchange --persist check testvg | grep -q "$KEY1"

for i in $(seq 1 $CLUSTER_DEV_COUNT); do
    node1 lvmpersist check-key --key $KEY1 --device $(eval echo \$dev$i)
done

node2 vgchange --persist start --lockstart testvg
KEY2=$(get_key 2)

for i in $(seq 1 $CLUSTER_DEV_COUNT); do
    node1 lvmpersist check-key --key $KEY2 --device $(eval echo \$dev$i)
done

for i in $(seq 1 $CLUSTER_DEV_COUNT); do
    node1 lvcreate -l1 -n lv_1_${i} testvg $(eval echo \$dev$i)
    node2 lvcreate -l1 -n lv_2_${i} testvg $(eval echo \$dev$i)

    node1 dd if=/dev/zero of=/dev/testvg/lv_1_${i} bs=4096 count=1 oflag=direct,sync
    node2 dd if=/dev/zero of=/dev/testvg/lv_2_${i} bs=4096 count=1 oflag=direct,sync

    node1 lvchange -an testvg/lv_1_${i}
    node2 lvchange -an testvg/lv_2_${i}
done

node1 vgchange --persist remove --removekey $KEY2 testvg

for i in $(seq 1 $CLUSTER_DEV_COUNT); do
    node1 not lvmpersist check-key --key $KEY2 --device $(eval echo \$dev$i)
    node1 lvmpersist check-key --key $KEY1 --device $(eval echo \$dev$i)
done

for i in $(seq 1 $CLUSTER_DEV_COUNT); do
    node2 not dd if=/dev/zero of=$(eval echo \$dev$i) bs=4096 count=1 seek=76800 oflag=direct,sync
done

node1 vgchange --persist stop --lockstop testvg
node1 not vgchange --persist check testvg

for i in $(seq 1 $CLUSTER_DEV_COUNT); do
    node1 lvmpersist read-keys --device $(eval echo \$dev$i) | tee out
    check_key_none out
done

# node2's PR key was removed by node1, so sanlock can no longer
# renew its delta lease; reboot node2 to cleanly reset all state.
noden 2 sync
noden 2 "systemctl reboot --force" || true
sleep 5
for i in $(seq 1 180); do
    if noden 2 true 2>/dev/null; then
        echo "Node 2 reachable after ${i}s"
        break
    fi
    if [ "$i" -eq 180 ]; then
        echo "ERROR: Node 2 not reachable within 180s"
        exit 1
    fi
    sleep 1
done
noden 2 udevadm settle --timeout=30
sleep 2

node1 vgchange --persist start --lockstart testvg
node1_vgremove_retry

exit 0
