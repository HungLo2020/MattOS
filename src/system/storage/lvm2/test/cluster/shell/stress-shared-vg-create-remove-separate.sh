#!/bin/bash

nodep 'rm -f /etc/lvm/archive/testvg*'

node1 vgcreate --shared testvg $dev1 $dev2
nodep vgchange --lockstart testvg

stress_loop create_remove_hostname '
    lvcreate -l1 -an -n $(hostname) testvg
    sleep .2
    lvremove testvg/$(hostname)
    sleep .2
'

stress 2min

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
