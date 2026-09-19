#!/bin/bash

if ! node1 'pgrep -x sanlock >/dev/null 2>&1'; then
    echo "SKIP: sanlock is not running"
    exit 0
fi

node1 vgcreate --shared testvg $dev1 $dev2
nodep vgchange --lockstart testvg

node1 lvcreate -l1 -n lv1 -an testvg

nodep 'rm -f /tmp/stress-excl.log'

stress_loop ay_an_lv1 '
    if lvchange -ay testvg/lv1 2>/dev/null; then
        echo "$(date +%s%N) ay" >> /tmp/stress-excl.log
        sleep .2
        echo "$(date +%s%N) an" >> /tmp/stress-excl.log
        lvchange -an testvg/lv1
    fi
    sleep .2
'

IO_TIMEOUT=$(node1 'sanlock status -D' | sed -n 's/^[[:space:]]*io_timeout=//p' | head -1)
DELAY=$((IO_TIMEOUT * 2 + 1))

# Force io timeouts in sanlock while acquiring leases.
# Note: double quotes required for variable
stress_loop delay_lvmlock "
    dmsetup suspend testvg-lvmlock
    sleep $DELAY
    dmsetup resume testvg-lvmlock
    sleep $DELAY
"

stress 5min
nodep 'dmsetup resume testvg-lvmlock 2>/dev/null' || true

# Collect per-node timestamp logs, prepend node number, merge, and
# check that no two nodes held the LV active at the same time.
#
# Timestamps are captured just after activation and just before
# deactivation, so the actual active window is slightly wider than
# recorded — brief overlaps at the edges can go undetected.
#
# Overlaps shorter than OVERLAP_THRESHOLD are ignored because VM
# clocks may not be perfectly synchronized, e.g. when chrony
# switches sources or detects falsetickers mid-test.

OVERLAP_THRESHOLD=100000000 # 100ms in nanoseconds

tmpdir=$(mktemp -d)

for node_num in $(seq 1 $CLUSTER_NUM_NODES); do
    noden $node_num "cat /tmp/stress-excl.log 2>/dev/null" \
        | awk -v n=$node_num '{print $1, n, $2}' >> $tmpdir/events.txt || true
done

sort -n $tmpdir/events.txt > $tmpdir/sorted.txt

declare -A node_active
violations=0
overlap_start=0
overlap_nodes=""

while read ts node event; do
    if [ "$event" = "ay" ]; then
        node_active[$node]=$ts
        if [ ${#node_active[@]} -gt 1 ] && [ "$overlap_start" = "0" ]; then
            overlap_start=$ts
            overlap_nodes=""
            for n in "${!node_active[@]}"; do
                overlap_nodes="$overlap_nodes $n"
            done
        fi
    elif [ "$event" = "an" ]; then
        unset node_active[$node]
        if [ ${#node_active[@]} -le 1 ] && [ "$overlap_start" != "0" ]; then
            duration=$((ts - overlap_start))
            if [ $duration -gt $OVERLAP_THRESHOLD ]; then
                echo "VIOLATION: nodes${overlap_nodes} overlapped for ${duration}ns at $overlap_start"
                violations=$((violations + 1))
            fi
            overlap_start=0
        fi
    fi
done < $tmpdir/sorted.txt

if [ "$overlap_start" != "0" ]; then
    echo "VIOLATION: nodes${overlap_nodes} still overlapping at end of log"
    violations=$((violations + 1))
fi

rm -rf $tmpdir

if [ $violations -gt 0 ]; then
    echo "FAIL: $violations exclusive lock violations detected"
    exit 1
fi
echo "OK: no overlapping activations detected"

# Cleanup
cleanup_vg testvg
