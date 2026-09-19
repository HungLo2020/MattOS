#!/bin/bash

if ! node1 'pgrep -x sanlock >/dev/null 2>&1'; then
    echo "SKIP: sanlock is not running"
    exit 0
fi

nodep 'rm -f /etc/lvm/archive/testvg*'

node1 vgcreate --shared testvg $dev1 $dev2
nodep vgchange --lockstart testvg

node1 lvcreate -l1 -n lv1 -an testvg

stress_loop create_remove_lv1 '
    lvcreate -l1 -an -n lv1 testvg
    sleep .2
    lvremove testvg/lv1
    sleep .2
'

IO_TIMEOUT=$(node1 'sanlock status -D' | sed -n 's/^[[:space:]]*io_timeout=//p' | head -1)
DELAY=$((IO_TIMEOUT * 2 + 1))

if [ "$DELAY" -lt 5 ] || [ "$DELAY" -gt 21 ]; then
    echo "ERROR: DELAY=$DELAY (IO_TIMEOUT=$IO_TIMEOUT) is out of range 5-21"
    exit 1
fi

nodep 'dmsetup table testvg-lvmlock > /tmp/lvmlock_table.txt'

# Force io errors in sanlock while acquiring leases.
stress_loop error_lvmlock "
    TABLE=\$(cat /tmp/lvmlock_table.txt)
    SIZE=\$(echo \"\$TABLE\" | awk '{print \$2}')
    dmsetup load testvg-lvmlock --table \"0 \$SIZE error\"
    dmsetup resume testvg-lvmlock
    sleep $DELAY
    dmsetup load testvg-lvmlock --table \"\$TABLE\"
    dmsetup resume testvg-lvmlock
    sleep $DELAY
"

stress 5min
nodep 'dmsetup load testvg-lvmlock --table "$(cat /tmp/lvmlock_table.txt)" 2>/dev/null; dmsetup resume testvg-lvmlock 2>/dev/null' || true

# Collect the seqno from each testvg archive file on each node.
# Each VG metadata write increments seqno and is serialized by the
# VG lock, so across all nodes the seqno values should be strictly
# increasing with no duplicates.

tmpdir=$(mktemp -d)

for node_num in $(seq 1 $CLUSTER_NUM_NODES); do
    noden $node_num "grep -h '^[[:space:]]*seqno' /etc/lvm/archive/testvg_*.vg" \
        | awk -v n=$node_num '{print $3, n}' >> $tmpdir/seqnos.txt || true
done

sort -n $tmpdir/seqnos.txt > $tmpdir/sorted.txt

violations=0
prev_seqno=0
prev_node=0

while read seqno node; do
    if [ "$seqno" = "$prev_seqno" ] && [ "$node" != "$prev_node" ]; then
        echo "VIOLATION: seqno $seqno archived on both node $prev_node and node $node"
        violations=$((violations + 1))
    elif [ "$seqno" -lt "$prev_seqno" ]; then
        echo "VIOLATION: seqno $seqno on node $node is less than previous seqno $prev_seqno"
        violations=$((violations + 1))
    fi
    prev_seqno=$seqno
    prev_node=$node
done < $tmpdir/sorted.txt

rm -rf $tmpdir

if [ $violations -gt 0 ]; then
    echo "FAIL: $violations metadata serialization violations detected"
    exit 1
fi
echo "OK: VG metadata seqno values are properly serialized"

# Cleanup
cleanup_vg testvg
