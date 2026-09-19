#!/bin/bash
#
# boot-activation.sh - Test LVM autoactivation of LVs after reboot
#
# Tests the modern autoactivation mechanism using 69-dm-lvm.rules
# (not the old 69-dm-lvm-metad.rules / lvm2-pvscan service).
#
# The 69-dm-lvm.rules flow:
# 1. udev detects block device, blkid identifies it as LVM PV
# 2. udev rule runs: pvscan --cache --listvg --checkcomplete
#    --vgonline --autoactivation event --udevoutput --journal=output
# 3. pvscan records PV online in /run/lvm/pvs_online/<pvid>
# 4. When all PVs in a VG are online, pvscan exports
#    LVM_VG_NAME_COMPLETE=<vgname>
# 5. udev rule runs transient systemd service:
#    lvm-activate-<vgname> which runs vgchange -aay
#
# Each test creates local VGs/LVs, reboots node1, and verifies
# autoactivation succeeded.
#

set -e

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

setup_persistent_nvme() {
    if [ "${CLUSTER_NUM_NVME:-0}" -eq 0 ]; then
        return
    fi
    local target_ip
    target_ip=$(node1 "nvme list-subsys -o json 2>/dev/null | grep -oP '\"address\"\\s*:\\s*\"traddr=\\K[0-9.]+' | head -1" || true)
    if [ -z "$target_ip" ]; then
        target_ip=$(node1 "iscsiadm -m session -P0 2>/dev/null | grep -oP '[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+' | head -1" || true)
    fi
    if [ -z "$target_ip" ]; then
        echo "WARNING: cannot determine node0 IP for NVMe persistence"
        return
    fi
    node1 "mkdir -p /etc/nvme"
    node1 "echo '-t tcp -a ${target_ip} -s 4420' > /etc/nvme/discovery.conf"
    node1 "systemctl enable nvmf-autoconnect.service 2>/dev/null || true"
    echo "Configured persistent NVMe-oF (target=$target_ip)"
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

# Force reboot skips systemd shutdown services, preventing LVM commands
# (e.g. lvm2-monitor ExecStop) from running during shutdown.
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

#
# verify_autoactivation <vgname> <lv1> [lv2] ...
#
# After reboot, verify:
# 1. Each LV is active (dm device exists)
# 2. pvs_online files exist for all PVs in the VG
# 3. vgs_online file exists for the VG
# 4. lvm-activate-<vgname> service ran
#
verify_autoactivation() {
    local vgname=$1; shift

    for lv in "$@"; do
        node1 dmsetup info ${vgname}-${lv} > /dev/null 2>&1
        echo "  LV ${vgname}/${lv} is active"
    done

    local pvids
    pvids=$(node1 pvs -qq --noheadings -o pv_uuid -S vg_name=${vgname} 2>/dev/null)
    for pvid in $pvids; do
        pvid=$(echo $pvid | tr -d ' -')
        node1 test -f /run/lvm/pvs_online/${pvid}
        echo "  pvs_online/${pvid} exists"
    done

    node1 test -f /run/lvm/vgs_online/${vgname}
    echo "  vgs_online/${vgname} exists"

    node1 journalctl -u lvm-activate-${vgname} --no-pager -b 2>&1 | grep -q "vgchange"
    echo "  lvm-activate-${vgname} service ran"
}

# verify_autoactivation_count <vgname> <num_lvs>
# Verify that dmsetup lists <num_lvs> devices starting with <vgname>-

verify_autoactivation_count() {
    local vgname=$1
    local expected=$2

    local num_active
    num_active=$(node1 dmsetup ls 2>/dev/null | grep -c "^${vgname}-" || true)
    echo "  $num_active of $expected LVs active"
    [ "$num_active" -eq "$expected" ]

    local pvids
    pvids=$(node1 pvs -qq --noheadings -o pv_uuid -S vg_name=${vgname} 2>/dev/null)
    for pvid in $pvids; do
        pvid=$(echo $pvid | tr -d ' -')
        node1 test -f /run/lvm/pvs_online/${pvid}
    done
    echo "  all pvs_online files exist"

    node1 test -f /run/lvm/vgs_online/${vgname}
    echo "  vgs_online/${vgname} exists"

    node1 journalctl -u lvm-activate-${vgname} --no-pager -b 2>&1 | grep -q "vgchange"
    echo "  lvm-activate-${vgname} service ran"
}

#
# cleanup_test <vgname> [vgname2] ...
#
cleanup_test() {
    for vg in "$@"; do
        node1 vgchange -an $vg 2>/dev/null || true
        node1 vgremove -f $vg 2>/dev/null || true
    done
}

create_md_dev() {
    local mddev=$1; shift
    node1 wipefs -a "$@"
    node1 mdadm --create $mddev --level=1 --raid-devices=$# --metadata=1.2 --run "$@"
    node1 udevadm settle
    sleep 2
}

remove_md_dev() {
    local mddev=$1; shift
    node1 mdadm --stop $mddev 2>/dev/null || true
    for d in "$@"; do
        node1 mdadm --zero-superblock $d 2>/dev/null || true
    done
    node1 udevadm settle
}

create_crypt_dev() {
    local name=$1
    local dev=$2
    local keyfile=/root/crypt-key-${name}
    node1 dd if=/dev/urandom of=$keyfile bs=32 count=1 2>/dev/null
    node1 cryptsetup luksFormat --batch-mode --key-file $keyfile $dev
    node1 cryptsetup luksOpen --key-file $keyfile $dev $name
    local luks_uuid
    luks_uuid=$(node1 cryptsetup luksUUID $dev)
    node1 "echo '${name} UUID=${luks_uuid} ${keyfile} _netdev' >> /etc/crypttab"
    node1 udevadm settle
}

remove_crypt_dev() {
    local name=$1
    local dev=$2
    node1 cryptsetup close $name 2>/dev/null || true
    node1 "sed -i '/${name}/d' /etc/crypttab"
    node1 rm -f /root/crypt-key-${name}
    node1 wipefs -a $dev 2>/dev/null || true
}

# -------------------------------------------------------------------
# Count available devices
# -------------------------------------------------------------------

scsi_count=0
for i in $(seq 1 50); do
    eval "d=\${scsi${i}:-}"
    [ -n "$d" ] && scsi_count=$((scsi_count + 1)) || break
done

nvme_count=0
for i in $(seq 1 50); do
    eval "d=\${nvme${i}:-}"
    [ -n "$d" ] && nvme_count=$((nvme_count + 1)) || break
done

mpath_count=0
for i in $(seq 1 50); do
    eval "d=\${mpath${i}:-}"
    [ -n "$d" ] && mpath_count=$((mpath_count + 1)) || break
done

dev_count=0
for i in $(seq 1 50); do
    eval "d=\${dev${i}:-}"
    [ -n "$d" ] && dev_count=$((dev_count + 1)) || break
done

echo "Devices: scsi=$scsi_count nvme=$nvme_count mpath=$mpath_count total=$dev_count"

# Configure persistent NVMe-oF so connections survive reboot
setup_persistent_nvme


# ===================================================================
# Test 1: Single VG, many LVs (scsi devices)
# ===================================================================

if [ $scsi_count -ge 1 ]; then
    echo "== Test 1: single VG, many LVs on scsi =="

    scsi_devs=""
    for i in $(seq 1 $scsi_count); do
        eval "scsi_devs=\"\$scsi_devs \$scsi${i}\""
    done

    node1 vgcreate testvg $scsi_devs

    lv_names=""
    for i in $(seq 1 $scsi_count); do
        node1 lvcreate -l1 -n lv${i} testvg
        lv_names="$lv_names lv${i}"
    done

    reboot_and_wait
    verify_autoactivation testvg $lv_names
    cleanup_test testvg

    echo "== Test 1 passed =="
else
    echo "== Test 1: SKIP (no scsi devices) =="
fi


# ===================================================================
# Test 2: Many VGs, 1 LV each (scsi devices)
# ===================================================================

if [ $scsi_count -ge 2 ]; then
    echo "== Test 2: many VGs, 1 LV each on scsi =="

    vg_names=""
    for i in $(seq 1 $scsi_count); do
        eval "d=\$scsi${i}"
        node1 vgcreate testvg${i} $d
        node1 lvcreate -l1 -n lv1 testvg${i}
        vg_names="$vg_names testvg${i}"
    done

    reboot_and_wait

    for i in $(seq 1 $scsi_count); do
        verify_autoactivation testvg${i} lv1
    done

    cleanup_test $vg_names

    echo "== Test 2 passed =="
else
    echo "== Test 2: SKIP (need 2+ scsi devices, have $scsi_count) =="
fi


# ===================================================================
# Test 3: Single VG, many LVs (nvme devices)
# ===================================================================

if [ $nvme_count -ge 1 ]; then
    echo "== Test 3: single VG, many LVs on nvme =="

    nvme_devs=""
    for i in $(seq 1 $nvme_count); do
        eval "nvme_devs=\"\$nvme_devs \$nvme${i}\""
    done

    node1 vgcreate testvg $nvme_devs

    lv_names=""
    for i in $(seq 1 $nvme_count); do
        node1 lvcreate -l1 -n lv${i} testvg
        lv_names="$lv_names lv${i}"
    done

    reboot_and_wait
    verify_autoactivation testvg $lv_names
    cleanup_test testvg

    echo "== Test 3 passed =="
else
    echo "== Test 3: SKIP (no nvme devices) =="
fi


# ===================================================================
# Test 4: Many VGs, 1 LV each (nvme devices)
# ===================================================================

if [ $nvme_count -ge 2 ]; then
    echo "== Test 4: many VGs, 1 LV each on nvme =="

    vg_names=""
    for i in $(seq 1 $nvme_count); do
        eval "d=\$nvme${i}"
        node1 vgcreate testvg${i} $d
        node1 lvcreate -l1 -n lv1 testvg${i}
        vg_names="$vg_names testvg${i}"
    done

    reboot_and_wait

    for i in $(seq 1 $nvme_count); do
        verify_autoactivation testvg${i} lv1
    done

    cleanup_test $vg_names

    echo "== Test 4 passed =="
else
    echo "== Test 4: SKIP (need 2+ nvme devices, have $nvme_count) =="
fi


# ===================================================================
# Test 5: Single VG, many LVs (multipath devices)
# ===================================================================

if [ $mpath_count -ge 1 ]; then
    echo "== Test 5: single VG, many LVs on multipath =="

    mpath_devs=""
    for i in $(seq 1 $mpath_count); do
        eval "mpath_devs=\"\$mpath_devs \$mpath${i}\""
    done

    node1 vgcreate testvg $mpath_devs

    lv_names=""
    for i in $(seq 1 $mpath_count); do
        node1 lvcreate -l1 -n lv${i} testvg
        lv_names="$lv_names lv${i}"
    done

    reboot_and_wait
    verify_autoactivation testvg $lv_names
    cleanup_test testvg

    echo "== Test 5 passed =="
else
    echo "== Test 5: SKIP (no multipath devices) =="
fi


# ===================================================================
# Test 6: Many VGs, 1 LV each (multipath devices)
# ===================================================================

if [ $mpath_count -ge 2 ]; then
    echo "== Test 6: many VGs, 1 LV each on multipath =="

    vg_names=""
    for i in $(seq 1 $mpath_count); do
        eval "d=\$mpath${i}"
        node1 vgcreate testvg${i} $d
        node1 lvcreate -l1 -n lv1 testvg${i}
        vg_names="$vg_names testvg${i}"
    done

    reboot_and_wait

    for i in $(seq 1 $mpath_count); do
        verify_autoactivation testvg${i} lv1
    done

    cleanup_test $vg_names

    echo "== Test 6 passed =="
else
    echo "== Test 6: SKIP (need 2+ multipath devices, have $mpath_count) =="
fi


# ===================================================================
# Test 7: One VG with mixed device types
# ===================================================================

type_count=0
mixed_devs=""
[ $scsi_count -ge 1 ] && { mixed_devs="$mixed_devs $scsi1"; type_count=$((type_count + 1)); }
[ $nvme_count -ge 1 ] && { mixed_devs="$mixed_devs $nvme1"; type_count=$((type_count + 1)); }
[ $mpath_count -ge 1 ] && { mixed_devs="$mixed_devs $mpath1"; type_count=$((type_count + 1)); }

if [ $type_count -ge 2 ]; then
    echo "== Test 7: one VG with mixed device types =="

    node1 vgcreate testvg $mixed_devs
    node1 lvcreate -l1 -n lv1 testvg

    reboot_and_wait
    verify_autoactivation testvg lv1
    cleanup_test testvg

    echo "== Test 7 passed =="
else
    echo "== Test 7: SKIP (need 2+ device types, have $type_count) =="
fi


# ===================================================================
# Test 8: VG on 1 md raid1 device
# ===================================================================

if [ $dev_count -ge 2 ]; then
    echo "== Test 8: VG on 1 md raid1 device =="

    create_md_dev /dev/md127 $dev1 $dev2

    node1 wipefs -a /dev/md127
    node1 pvcreate /dev/md127
    node1 vgcreate testvg /dev/md127
    node1 lvcreate -l1 -n lv1 testvg

    reboot_and_wait
    verify_autoactivation testvg lv1
    cleanup_test testvg

    remove_md_dev /dev/md127 $dev1 $dev2

    echo "== Test 8 passed =="
else
    echo "== Test 8: SKIP (need 2+ devices, have $dev_count) =="
fi


# ===================================================================
# Test 9: VG on 2 md raid1 devices
# ===================================================================

if [ $dev_count -ge 4 ]; then
    echo "== Test 9: VG on 2 md raid1 devices =="

    create_md_dev /dev/md127 $dev1 $dev2
    create_md_dev /dev/md126 $dev3 $dev4

    node1 wipefs -a /dev/md127
    node1 wipefs -a /dev/md126

    node1 vgcreate testvg /dev/md127 /dev/md126
    node1 lvcreate -l1 -n lv1 testvg

    reboot_and_wait
    verify_autoactivation testvg lv1
    cleanup_test testvg

    remove_md_dev /dev/md127 $dev1 $dev2
    remove_md_dev /dev/md126 $dev3 $dev4

    echo "== Test 9 passed =="
else
    echo "== Test 9: SKIP (need 4+ devices, have $dev_count) =="
fi


# ===================================================================
# Test 10: VG on dm-crypt device
# ===================================================================

if [ $dev_count -ge 1 ]; then
    echo "== Test 10: VG on dm-crypt device =="

    create_crypt_dev testcrypt $dev1

    node1 pvcreate -y /dev/mapper/testcrypt
    node1 vgcreate testvg /dev/mapper/testcrypt
    node1 lvcreate -l1 -n lv1 testvg

    reboot_and_wait
    verify_autoactivation testvg lv1
    cleanup_test testvg

    remove_crypt_dev testcrypt $dev1

    echo "== Test 10 passed =="
else
    echo "== Test 10: SKIP (need 1+ devices, have $dev_count) =="
fi


# ===================================================================
# Test 11: One VG using all devices, 1 LV per device
# ===================================================================

if [ $dev_count -ge 1 ]; then
    echo "== Test 11: one VG, all devices, 1 LV per device =="

    all_devs=""
    for i in $(seq 1 $dev_count); do
        eval "all_devs=\"\$all_devs \$dev${i}\""
    done

    node1 vgcreate testvg $all_devs

    lv_names=""
    for i in $(seq 1 $dev_count); do
        eval "d=\$dev${i}"
        node1 lvcreate -l1 -n lv${i} testvg $d
        lv_names="$lv_names lv${i}"
    done

    reboot_and_wait
    verify_autoactivation testvg $lv_names
    cleanup_test testvg

    echo "== Test 11 passed =="
else
    echo "== Test 11: SKIP (no devices) =="
fi


# ===================================================================
# Test 12: Single VG on scsi devices, many LV types
# ===================================================================

if [ $scsi_count -ge 2 ]; then
    echo "== Test 12: single VG, many LV types on scsi =="

    scsi_devs=""
    for i in $(seq 1 $scsi_count); do
        eval "scsi_devs=\"\$scsi_devs \$scsi${i}\""
    done

    node1 vgcreate testvg $scsi_devs

    lv_names=""

    # linear
    node1 lvcreate -l1 -n lv_linear testvg
    lv_names="$lv_names lv_linear"

    # striped
    node1 lvcreate --type striped -i2 -l2 -n lv_striped testvg
    lv_names="$lv_names lv_striped"

    # raid1
    node1 lvcreate --type raid1 -m1 -l1 -n lv_raid1 testvg
    lv_names="$lv_names lv_raid1"

    # thin-pool + thin volume
    node1 lvcreate --type thin-pool -l4 -n tpool testvg
    node1 lvcreate --type thin -V4m --thinpool tpool -n lv_thin testvg
    lv_names="$lv_names lv_thin"

    # cache: create origin, create cache-pool, convert to cached LV
    node1 lvcreate -l1 -n lv_cached testvg
    node1 lvcreate --type cache-pool -l1 -n cpool testvg
    node1 lvconvert --yes --type cache --cachepool cpool testvg/lv_cached
    lv_names="$lv_names lv_cached"

    node1 lvcreate -l1 -n lv_wcmain testvg
    node1 lvcreate -l1 -n lv_wcfast -an testvg
    node1 lvconvert --yes --type writecache --cachevol lv_wcfast testvg/lv_wcmain
    lv_names="$lv_names lv_wcmain"

#    if node1 modprobe kvdo 2>/dev/null || node1 test -d /sys/module/kvdo 2>/dev/null || \
#       node1 modprobe dm-vdo 2>/dev/null || node1 test -d /sys/module/dm_vdo 2>/dev/null; then
#        node1 lvcreate --type vdo -l8 -n lv_vdo -V1g --yes testvg
#        lv_names="$lv_names lv_vdo"
#        echo "  vdo: included"
#    else
#        echo "  vdo: skipped (no kernel module)"
#    fi

    reboot_and_wait
    verify_autoactivation testvg $lv_names
    cleanup_test testvg

    echo "== Test 12 passed =="
else
    echo "== Test 12: SKIP (need 2+ scsi devices, have $scsi_count) =="
fi


# ===================================================================
# Test 13: One VG, all devices, 1024 LVs (or as many as fit)
# ===================================================================

if [ $dev_count -ge 1 ]; then
    echo "== Test 13: one VG, all devices, 1024 LVs =="

    all_devs=""
    for i in $(seq 1 $dev_count); do
        eval "all_devs=\"\$all_devs \$dev${i}\""
    done

    node1 vgcreate -s 128K testvg $all_devs

    vg_free=$(node1 vgs --noheadings -o vg_free_count testvg 2>/dev/null | tr -d ' ')
    max_lvs=1024
    [ "$vg_free" -lt "$max_lvs" ] && max_lvs=$vg_free

    echo "  Creating $max_lvs LVs via vgcfgrestore ($vg_free extents free)"

    node1 vgcfgbackup -f /tmp/vgdata testvg

    node1 "awk -v NUM=$max_lvs '
/^\t\}/ {
    printf(\"\t}\n\tlogical_volumes {\n\");
    cnt=0;
    for (i = 0; i < NUM; i++) {
        printf(\"\t\tlv%06d  {\n\", i);
        printf(\"\t\t\tid = \\\"%06d-1111-2222-3333-2222-1111-%06d\\\"\n\", i, i);
        print \"\t\t\tstatus = [\\\"READ\\\", \\\"WRITE\\\", \\\"VISIBLE\\\"]\";
        print \"\t\t\tsegment_count = 1\";
        print \"\t\t\tsegment1 {\";
        print \"\t\t\t\tstart_extent = 0\";
        print \"\t\t\t\textent_count = 1\";
        print \"\t\t\t\ttype = \\\"striped\\\"\";
        print \"\t\t\t\tstripe_count = 1\";
        print \"\t\t\t\tstripes = [\";
        print \"\t\t\t\t\t\\\"pv0\\\", \" cnt++;
        printf(\"\t\t\t\t]\n\t\t\t}\n\t\t}\n\");
    }
}
{print}
' /tmp/vgdata > /tmp/vgdata_new"

    node1 vgcfgrestore -f /tmp/vgdata_new testvg
    node1 rm -f /tmp/vgdata /tmp/vgdata_new

    reboot_and_wait
    verify_autoactivation_count testvg $max_lvs
    cleanup_test testvg

    echo "== Test 13 passed =="
else
    echo "== Test 13: SKIP (no devices) =="
fi


# ===================================================================
# Test 14: VG with metadata on only 1 PV
# ===================================================================

if [ $dev_count -ge 3 ]; then
    echo "== Test 14: VG with metadata on only 1 PV =="

    node1 pvcreate -y --metadatacopies 0 $dev1
    node1 pvcreate -y --metadatacopies 0 $dev2
    node1 pvcreate -y $dev3

    node1 vgcreate testvg $dev1 $dev2 $dev3
    node1 lvcreate -l1 -n lv1 testvg $dev1
    node1 lvcreate -l1 -n lv2 testvg $dev2
    node1 lvcreate -l1 -n lv3 testvg $dev3
    node1 lvcreate -l8 -n lv4 -i2 testvg $dev1 $dev2

    reboot_and_wait
    verify_autoactivation testvg lv1 lv2 lv3 lv4

    node1 test -f /run/lvm/pvs_lookup/testvg
    echo "  pvs_lookup/testvg exists"

    cleanup_test testvg

    node1 wipefs -a $dev1
    node1 wipefs -a $dev2
    node1 wipefs -a $dev3

    echo "== Test 14 passed =="
else
    echo "== Test 14: SKIP (need 3+ devices, have $dev_count) =="
fi


# ===================================================================
# Test 15: autoactivation with filter using PV UUID symlink
# ===================================================================

if [ $dev_count -ge 1 ]; then
    echo "== Test 15: filter with PV UUID symlink =="

    node1 vgcreate testvg $dev1
    node1 lvcreate -l1 -n lv1 testvg

    pv_uuid=$(node1 pvs --noheadings -o uuid $dev1 2>/dev/null | awk '{print $1}')
    node1 test -L /dev/disk/by-id/lvm-pv-uuid-${pv_uuid}
    echo "  test PV symlink lvm-pv-uuid-${pv_uuid} exists"

    filter_entries="\"a|/dev/disk/by-id/lvm-pv-uuid-${pv_uuid}|\""

    root_pv_uuid=$(node1 "root_lv=\$(findmnt -n -o SOURCE / 2>/dev/null);
        if [ -n \"\$root_lv\" ]; then
            root_vg=\$(lvs --noheadings -o vg_name \$root_lv 2>/dev/null | awk '{print \$1}');
            if [ -n \"\$root_vg\" ]; then
                pvs --noheadings -o uuid -S vg_name=\$root_vg 2>/dev/null | awk '{print \$1}';
            fi;
        fi" || true)

    for rpv in $root_pv_uuid; do
        rpv=$(echo $rpv | tr -d ' ')
        [ -z "$rpv" ] && continue
        filter_entries="${filter_entries}, \"a|/dev/disk/by-id/lvm-pv-uuid-${rpv}|\""
        echo "  root PV symlink lvm-pv-uuid-${rpv} included in filter"
    done

    filter_entries="${filter_entries}, \"r|.*|\""

    node1 cp /etc/lvm/lvm.conf /etc/lvm/lvm.conf.bak
    node1 "sed -i 's/use_devicesfile = 1/use_devicesfile = 0/' /etc/lvm/lvm.conf"
    node1 "sed -i '/^[[:space:]]*filter/d' /etc/lvm/lvm.conf"
    node1 "sed -i '/devices {/a\\    filter = [ ${filter_entries} ]' /etc/lvm/lvm.conf"

    reboot_and_wait
    verify_autoactivation testvg lv1

    node1 mv /etc/lvm/lvm.conf.bak /etc/lvm/lvm.conf
    cleanup_test testvg

    echo "== Test 15 passed =="
else
    echo "== Test 15: SKIP (no devices) =="
fi


# ===================================================================
# Test 16: autoactivation with corrupted device IDs in system.devices
# ===================================================================

if [ $dev_count -ge 1 ]; then
    echo "== Test 16: corrupted device IDs in system.devices =="

    node1 vgcreate testvg $dev1
    node1 lvcreate -l1 -n lv1 testvg

    pv_uuid=$(node1 pvs --noheadings -o uuid $dev1 2>/dev/null | tr -d ' -')

    node1 cp /etc/lvm/devices/system.devices /etc/lvm/devices/system.devices.bak
    node1 cp /etc/lvm/lvm.conf /etc/lvm/lvm.conf.bak

    real_idname=$(node1 "grep $pv_uuid /etc/lvm/devices/system.devices" | grep -oP 'IDNAME=\K\S+')
    real_product_uuid=$(node1 "grep PRODUCT_UUID /etc/lvm/devices/system.devices | head -1" | grep -oP 'PRODUCT_UUID=\K\S+')

    echo "  real IDNAME=$real_idname"
    echo "  real PRODUCT_UUID=$real_product_uuid"

    node1 "sed -i 's/IDNAME=${real_idname}/IDNAME=11111111-0000-0000-0000-000000000000/' /etc/lvm/devices/system.devices"
    node1 "sed -i 's/PRODUCT_UUID=${real_product_uuid}/PRODUCT_UUID=00000000-0000-0000-0000-000000000000/' /etc/lvm/devices/system.devices"

    node1 "cat /etc/lvm/devices/system.devices"

    # Force reboot to prevent shutdown services (lvm2-monitor) from
    # reading the corrupted system.devices and consuming the refresh trigger.
    reboot_and_wait_force
    verify_autoactivation testvg lv1

    node1 pvs --noheadings -o uuid $dev1

    node1 "grep $pv_uuid /etc/lvm/devices/system.devices" | grep -q "$real_idname"
    echo "  IDNAME restored to $real_idname"

    updated_product_uuid=$(node1 "grep PRODUCT_UUID /etc/lvm/devices/system.devices | head -1" | grep -oP 'PRODUCT_UUID=\K\S+')
    [ "$updated_product_uuid" != "00000000-0000-0000-0000-000000000000" ]
    echo "  PRODUCT_UUID restored to $updated_product_uuid"

    node1 cp /etc/lvm/lvm.conf.bak /etc/lvm/lvm.conf
    cleanup_test testvg

    echo "== Test 16 passed =="
else
    echo "== Test 16: SKIP (need device_ids_refresh support and 1+ devices) =="
fi


echo "All tests completed"
exit 0
