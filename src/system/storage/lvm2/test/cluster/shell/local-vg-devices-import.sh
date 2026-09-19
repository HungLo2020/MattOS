#!/bin/bash
#
# devices-import.sh - Test lvm-devices-import.path and lvm-devices-import.service
#
# Tests the first-boot auto-creation of /etc/lvm/devices/system.devices
# via the lvm-devices-import systemd units.
#
# The flow being tested:
# 1. OS image is prepared: system.devices removed, auto-import-rootvg created
# 2. On boot, vgchange -aay <rootvg> detects conditions, creates
#    /run/lvm/lvm-devices-import
# 3. lvm-devices-import.path watches for that file, triggers .service
# 4. .service runs vgimportdevices --rootvg --auto, which creates
#    system.devices for root VG PVs, removes marker files
#
# Requires LVM root (e.g. RHEL guest image, not Fedora Cloud).
#

set -e

DF="/etc/lvm/devices/system.devices"
DF_BAK="/etc/lvm/devices/system.devices.test-backup"
MARKER="/etc/lvm/devices/auto-import-rootvg"
TRIGGER="/run/lvm/lvm-devices-import"

# -------------------------------------------------------------------
# Helper functions
# -------------------------------------------------------------------

wait_for_node() {
    local timeout="${1:-180}"
    for i in $(seq 1 $timeout); do
        if node1 true 2>/dev/null; then
            echo "Node 1 reachable after ${i}s"
            return 0
        fi
        sleep 1
    done
    echo "ERROR: Node 1 not reachable within ${timeout}s"
    exit 1
}

rediscover_devices() {
    local result
    result=$(node1 "
        mpath_slaves=\$(multipath -ll 2>/dev/null | grep -oP '[0-9]+:[0-9]+:[0-9]+:[0-9]+\s+\K\w+' || true)
        scsi_devs=\$(lsblk -d -n -o NAME,VENDOR 2>/dev/null | grep -i 'LIO-ORG' | awk '{print \$1}' | sort)
        for dev in \$scsi_devs; do
            if [ -z \"\$mpath_slaves\" ] || ! echo \"\$mpath_slaves\" | grep -qw \"\$dev\"; then
                echo \"scsi /dev/\$dev\"
            fi
        done
        nvme list 2>/dev/null | grep -oP '/dev/nvme[0-9]+n[0-9]+' | sort | while read d; do echo \"nvme \$d\"; done
        multipath -ll 2>/dev/null | grep -oP '^[a-z0-9]+(?= \()' | sort | while read m; do echo \"mpath /dev/mapper/\$m\"; done
    ")

    scsi_count=0; nvme_count=0; mpath_count=0; dev_count=0

    while IFS= read -r line; do
        local dtype=$(echo "$line" | awk '{print $1}')
        local dpath=$(echo "$line" | awk '{print $2}')
        [ -z "$dpath" ] && continue

        dev_count=$((dev_count + 1))
        eval "dev${dev_count}=$dpath"

        case "$dtype" in
            scsi)  scsi_count=$((scsi_count + 1));  eval "scsi${scsi_count}=$dpath" ;;
            nvme)  nvme_count=$((nvme_count + 1));  eval "nvme${nvme_count}=$dpath" ;;
            mpath) mpath_count=$((mpath_count + 1)); eval "mpath${mpath_count}=$dpath" ;;
        esac
    done <<< "$result"

    echo "Rediscovered devices: scsi=$scsi_count nvme=$nvme_count mpath=$mpath_count total=$dev_count"
}

reboot_and_wait_force() {
    echo "Rebooting node1 (force)..."
    node1 sync
    node1 "systemctl reboot --force" || true
    sleep 5
    wait_for_node
    node1 udevadm settle --timeout=30
    sleep 5
    rediscover_devices
    echo "Node1 back after reboot"
}

reboot_and_wait() {
    echo "Rebooting node1..."
    node1 systemctl reboot || true
    sleep 5
    wait_for_node
    node1 udevadm settle --timeout=30
    sleep 5
    rediscover_devices
    echo "Node1 back after reboot"
}

cleanup_test() {
    for vg in "$@"; do
        node1 vgchange -an $vg 2>/dev/null || true
        node1 vgremove -f $vg 2>/dev/null || true
    done
}

# -------------------------------------------------------------------
# Detect LVM root
# -------------------------------------------------------------------

root_dev=$(node1 findmnt -n -o SOURCE / 2>/dev/null || true)
root_vg=$(node1 "lvs --noheadings -o vg_name $root_dev 2>/dev/null | awk '{print \$1}'" || true)
root_vg=$(echo $root_vg | tr -d ' ')

if [ -z "$root_vg" ]; then
    echo "SKIP: root filesystem is not on LVM"
    exit 0
fi

echo "Root VG: $root_vg (root device: $root_dev)"

root_pv_uuids=$(node1 "pvs --noheadings -o pv_uuid -S vg_name=$root_vg 2>/dev/null | awk '{print \$1}'" || true)
echo "Root VG PV UUIDs: $root_pv_uuids"

if [ -z "$root_pv_uuids" ]; then
    echo "SKIP: cannot determine PV UUIDs for root VG"
    exit 0
fi

# Save original system.devices (if it exists)
had_df=0
if node1 test -f $DF 2>/dev/null; then
    node1 cp $DF $DF_BAK
    had_df=1
fi

restore_df() {
    if [ "$had_df" -eq 1 ]; then
        node1 cp $DF_BAK $DF
    else
        node1 rm -f $DF
    fi
}


# ===================================================================
# Test 1: Full first-boot import (end-to-end)
# ===================================================================

echo "== Test 1: full first-boot import =="

node1 rm -f $DF
node1 touch $MARKER
node1 systemctl enable lvm-devices-import.path
node1 systemctl enable lvm-devices-import.service

reboot_and_wait_force

# Verify system.devices was created
node1 test -f $DF
echo "  system.devices created"
node1 cat $DF
node1 "grep 'Created by LVM command vgimportdevices (auto)' $DF"

# Verify marker files were removed
node1 "test ! -f $MARKER"
echo "  auto-import-rootvg removed"

node1 "test ! -f $TRIGGER"
echo "  trigger file removed"

# Verify system.devices contains root VG PVs
for pvid in $root_pv_uuids; do
    pvid=$(echo $pvid | tr -d ' -')
    node1 "grep -q $pvid $DF"
    echo "  system.devices contains PVID $pvid"
done

# Verify the service ran
node1 journalctl -u lvm-devices-import --no-pager -b 2>&1 | grep -q "vgimportdevices"
echo "  lvm-devices-import service ran"

# Verify root VG LVs are active
root_lv_count=$(node1 "lvs --noheadings -o lv_name -S vg_name=$root_vg 2>/dev/null | wc -l")
root_active_count=$(node1 "dmsetup ls 2>/dev/null | grep -c '^${root_vg}-' || true")
echo "  root VG: $root_active_count of $root_lv_count LVs active"
[ "$root_active_count" -ge 1 ]

# Save the auto-generated system.devices for Test 2
auto_generated_md5=$(node1 "md5sum $DF | awk '{print \$1}'" || true)
echo "  auto-generated system.devices md5: $auto_generated_md5"

echo "== Test 1 passed =="


# ===================================================================
# Test 2: Verify no-op on second boot
# ===================================================================

echo "== Test 2: no-op on second boot =="

# system.devices exists, auto-import-rootvg is gone — should be a no-op
reboot_and_wait_force

# Verify system.devices is unchanged
current_md5=$(node1 "md5sum $DF | awk '{print \$1}'" || true)
echo "  system.devices md5: $current_md5"
[ "$current_md5" = "$auto_generated_md5" ]
echo "  system.devices unchanged"

# Verify lvm-devices-import did NOT run this boot
if node1 journalctl -u lvm-devices-import --no-pager -b 2>&1 | grep -q "vgimportdevices"; then
    echo "  ERROR: lvm-devices-import service ran unexpectedly"
    exit 1
fi
echo "  lvm-devices-import service did not run"

# Verify root VG LVs are still active
root_active_count=$(node1 "dmsetup ls 2>/dev/null | grep -c '^${root_vg}-' || true")
[ "$root_active_count" -ge 1 ]
echo "  root VG LVs active"

echo "== Test 2 passed =="


# ===================================================================
# Test 3: Non-root VG is not imported
# ===================================================================

if [ ${dev_count:-0} -ge 1 ]; then
    echo "== Test 3: non-root VG not imported =="

    # Restore original system.devices so we can create testvg
    restore_df

    node1 vgcreate testvg $dev1
    node1 lvcreate -l1 -n lv1 testvg

    testvg_pvid=$(node1 "pvs --noheadings -o pv_uuid $dev1 2>/dev/null | awk '{print \$1}'" || true)
    testvg_pvid=$(echo $testvg_pvid | tr -d ' -')
    echo "  testvg PV UUID: $testvg_pvid"

    # Set up first-boot conditions
    node1 rm -f $DF
    node1 touch $MARKER
    node1 systemctl enable lvm-devices-import.path
    node1 systemctl enable lvm-devices-import.service

    reboot_and_wait_force

    # Verify system.devices was created with root VG PVs
    node1 test -f $DF
    for pvid in $root_pv_uuids; do
        pvid=$(echo $pvid | tr -d ' -')
        node1 "grep -q $pvid $DF"
    done
    echo "  system.devices contains root VG PVs"

    # Verify testvg PV is NOT in system.devices
    if node1 "grep -q $testvg_pvid $DF" 2>/dev/null; then
        echo "  ERROR: testvg PV found in system.devices (should not be)"
        exit 1
    fi
    echo "  testvg PV not in system.devices (correct)"

    # Restore and cleanup
    restore_df
    node1 lvmdevices --addpvid $testvg_pvid 2>/dev/null || true
    cleanup_test testvg

    echo "== Test 3 passed =="
else
    echo "== Test 3: SKIP (no devices) =="
fi


# ===================================================================
# Test 4: No-op when system.devices already exists
# ===================================================================

echo "== Test 4: no-op when system.devices exists =="

# Ensure system.devices exists; use original if available,
# otherwise use the one Test 1 auto-generated (via vgimportdevices --rootvg).
if [ "$had_df" -eq 1 ]; then
    restore_df
elif ! node1 test -f $DF 2>/dev/null; then
    node1 vgimportdevices --rootvg 2>/dev/null || true
fi
original_md5=$(node1 "md5sum $DF | awk '{print \$1}'" || true)

# Create marker file but leave system.devices in place
node1 touch $MARKER
node1 systemctl enable lvm-devices-import.path
node1 systemctl enable lvm-devices-import.service

reboot_and_wait

# Verify system.devices is unchanged
current_md5=$(node1 "md5sum $DF | awk '{print \$1}'" || true)
[ "$current_md5" = "$original_md5" ]
echo "  system.devices unchanged"

# Verify auto-import-rootvg still exists (nothing removed it because
# the vgchange trigger and the service both skip when system.devices exists)
node1 test -f $MARKER
echo "  auto-import-rootvg still exists (expected)"

# Verify lvm-devices-import did NOT run
if node1 journalctl -u lvm-devices-import --no-pager -b 2>&1 | grep -q "vgimportdevices"; then
    echo "  ERROR: lvm-devices-import service ran unexpectedly"
    exit 1
fi
echo "  lvm-devices-import service did not run"

# Cleanup
node1 rm -f $MARKER

echo "== Test 4 passed =="


# -------------------------------------------------------------------
# Restore original state
# -------------------------------------------------------------------

restore_df
node1 rm -f $DF_BAK
node1 rm -f $MARKER

echo "All tests completed"
exit 0
