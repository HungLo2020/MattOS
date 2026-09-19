#!/bin/bash
#
# cluster-storage-exporter.sh - Storage export layer for LVM cluster testing
#
# This script runs on node 0 (storage exporter node) to:
# - Create backing storage for iSCSI and/or NVMe devices
# - Configure iSCSI targets using targetcli
# - Configure NVMe-oF targets using configfs
#
# Node 0 acts as a SAN simulator, exporting shared storage to test nodes (1..N)
#

# Source the core library for logging functions
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck disable=SC1091
source "$SCRIPT_DIR/cluster-test-lib.sh" || exit 1

# Storage directories on node 0
CLUSTER_SCSI_STORAGE_DIR="${CLUSTER_SCSI_STORAGE_DIR:-/var/tmp/lvm-cluster-storage-scsi}"
CLUSTER_NVME_STORAGE_DIR="${CLUSTER_NVME_STORAGE_DIR:-/var/tmp/lvm-cluster-storage-nvme}"
CLUSTER_MULTIPATH_STORAGE_DIR="${CLUSTER_MULTIPATH_STORAGE_DIR:-/var/tmp/lvm-cluster-storage-multipath}"

# Global arrays to track backing devices per protocol
declare -a SCSI_BACKING_DEVICES
declare -a NVME_BACKING_DEVICES
declare -a MULTIPATH_BACKING_DEVICES

#
# cluster_storage_create_scsi_backing - Create backing storage for iSCSI devices
#
# Arguments:
#   $1 - backing_type (file_prealloc, file_sparse, loop_prealloc, loop_sparse, or memory)
#   $2 - num_devices (number of backing devices to create)
#   $3 - device_size (size in MB)
#   $4 - sector_size (512 or 4096)
#   $5 - cluster_id (for unique naming)
#
# Returns:
#   0 on success, 1 on failure
#
# Sets global array:
#   SCSI_BACKING_DEVICES - array of backing device paths/names
#
cluster_storage_create_scsi_backing() {
    local backing_type="$1"
    local num_devices="$2"
    local device_size="$3"
    local sector_size="$4"
    local cluster_id="$5"

    cluster_log "Creating $num_devices iSCSI backing devices (type=$backing_type, size=${device_size}MB, sector=$sector_size)"

    SCSI_BACKING_DEVICES=()

    case "$backing_type" in
        file_prealloc)
            # Create directory for backing files
            mkdir -p "$CLUSTER_SCSI_STORAGE_DIR" || {
                cluster_error "Failed to create iSCSI storage directory: $CLUSTER_SCSI_STORAGE_DIR"
                return 1
            }

            # Create preallocated files with dd
            for i in $(seq 1 "$num_devices"); do
                local file_path="$CLUSTER_SCSI_STORAGE_DIR/scsi-disk-${cluster_id}-${i}.img"

                cluster_debug "Creating preallocated file: $file_path"
                dd if=/dev/zero of="$file_path" bs=1M count="$device_size" 2>&1 | grep -v records || {
                    cluster_error "Failed to create backing file: $file_path"
                    return 1
                }

                chmod 600 "$file_path"
                SCSI_BACKING_DEVICES+=("$file_path")
                cluster_debug "iSCSI backing device $i: $file_path"
            done
            ;;

        file_sparse)
            # Create directory for sparse files
            mkdir -p "$CLUSTER_SCSI_STORAGE_DIR" || {
                cluster_error "Failed to create iSCSI storage directory: $CLUSTER_SCSI_STORAGE_DIR"
                return 1
            }

            # Create sparse files with truncate (much faster than dd)
            for i in $(seq 1 "$num_devices"); do
                local file_path="$CLUSTER_SCSI_STORAGE_DIR/scsi-disk-${cluster_id}-${i}.img"

                cluster_debug "Creating sparse file: $file_path (${device_size}MB)"
                truncate -s "${device_size}M" "$file_path" || {
                    cluster_error "Failed to create sparse file: $file_path"
                    return 1
                }

                chmod 600 "$file_path"
                SCSI_BACKING_DEVICES+=("$file_path")
                cluster_debug "iSCSI backing device $i: $file_path (sparse, ${device_size}MB)"
            done
            ;;

        loop_prealloc)
            # Create directory for backing files
            mkdir -p "$CLUSTER_SCSI_STORAGE_DIR" || {
                cluster_error "Failed to create iSCSI storage directory: $CLUSTER_SCSI_STORAGE_DIR"
                return 1
            }

            # Create preallocated files with dd and set up loop devices
            for i in $(seq 1 "$num_devices"); do
                local file_path="$CLUSTER_SCSI_STORAGE_DIR/scsi-disk-${cluster_id}-${i}.img"

                cluster_debug "Creating preallocated file: $file_path"
                dd if=/dev/zero of="$file_path" bs=1M count="$device_size" 2>&1 | grep -v records || {
                    cluster_error "Failed to create backing file: $file_path"
                    return 1
                }

                chmod 600 "$file_path"

                # Set up loop device
                cluster_debug "Setting up loop device for: $file_path"
                local loop_dev=$(losetup -f --show "$file_path") || {
                    cluster_error "Failed to set up loop device for: $file_path"
                    return 1
                }

                SCSI_BACKING_DEVICES+=("$loop_dev")
                cluster_debug "iSCSI backing device $i: $loop_dev (loop on preallocated file)"
            done
            ;;

        loop_sparse)
            # Create directory for sparse files
            mkdir -p "$CLUSTER_SCSI_STORAGE_DIR" || {
                cluster_error "Failed to create iSCSI storage directory: $CLUSTER_SCSI_STORAGE_DIR"
                return 1
            }

            # Create sparse files with truncate and set up loop devices
            for i in $(seq 1 "$num_devices"); do
                local file_path="$CLUSTER_SCSI_STORAGE_DIR/scsi-disk-${cluster_id}-${i}.img"

                cluster_debug "Creating sparse file: $file_path (${device_size}MB)"
                truncate -s "${device_size}M" "$file_path" || {
                    cluster_error "Failed to create sparse file: $file_path"
                    return 1
                }

                chmod 600 "$file_path"

                # Set up loop device
                cluster_debug "Setting up loop device for: $file_path"
                local loop_dev=$(losetup -f --show "$file_path") || {
                    cluster_error "Failed to set up loop device for: $file_path"
                    return 1
                }

                SCSI_BACKING_DEVICES+=("$loop_dev")
                cluster_debug "iSCSI backing device $i: $loop_dev (loop on sparse file)"
            done
            ;;

        memory)
            # For ramdisk, targetcli rd_mcp handles it - just track names
            for i in $(seq 1 "$num_devices"); do
                local ramdisk_name="ramdisk-scsi-${cluster_id}-${i}"
                SCSI_BACKING_DEVICES+=("$ramdisk_name")
                cluster_debug "iSCSI backing device $i: $ramdisk_name (ramdisk)"
            done
            ;;

        *)
            cluster_error "Unknown iSCSI backing type: $backing_type"
            return 1
            ;;
    esac

    cluster_log "Created ${#SCSI_BACKING_DEVICES[@]} iSCSI backing devices"
    return 0
}

#
# cluster_storage_create_nvme_backing - Create backing storage for NVMe devices
#
# Arguments:
#   $1 - backing_type (loop_prealloc, loop_sparse, or loop_memory)
#   $2 - num_devices (number of backing devices to create)
#   $3 - device_size (size in MB)
#   $4 - sector_size (512 or 4096)
#   $5 - cluster_id (for unique naming)
#
# Returns:
#   0 on success, 1 on failure
#
# Sets global array:
#   NVME_BACKING_DEVICES - array of backing device paths
#
cluster_storage_create_nvme_backing() {
    local backing_type="$1"
    local num_devices="$2"
    local device_size="$3"
    local sector_size="$4"
    local cluster_id="$5"

    cluster_log "Creating $num_devices NVMe backing devices (type=$backing_type, size=${device_size}MB, sector=$sector_size)"

    NVME_BACKING_DEVICES=()

    case "$backing_type" in
        loop_prealloc)
            # Create directory for backing files
            mkdir -p "$CLUSTER_NVME_STORAGE_DIR" || {
                cluster_error "Failed to create NVMe storage directory: $CLUSTER_NVME_STORAGE_DIR"
                return 1
            }

            # Create preallocated files with dd
            for i in $(seq 1 "$num_devices"); do
                local file_path="$CLUSTER_NVME_STORAGE_DIR/nvme-disk-${cluster_id}-${i}.img"

                cluster_debug "Creating preallocated file: $file_path"
                dd if=/dev/zero of="$file_path" bs=1M count="$device_size" 2>&1 | grep -v records || {
                    cluster_error "Failed to create backing file: $file_path"
                    return 1
                }

                chmod 600 "$file_path"
                NVME_BACKING_DEVICES+=("$file_path")
                cluster_debug "NVMe backing device $i: $file_path"
            done
            ;;

        loop_sparse)
            # Create directory for sparse files
            mkdir -p "$CLUSTER_NVME_STORAGE_DIR" || {
                cluster_error "Failed to create NVMe storage directory: $CLUSTER_NVME_STORAGE_DIR"
                return 1
            }

            # Create sparse files with truncate
            for i in $(seq 1 "$num_devices"); do
                local file_path="$CLUSTER_NVME_STORAGE_DIR/nvme-disk-${cluster_id}-${i}.img"

                cluster_debug "Creating sparse file: $file_path"
                truncate -s "${device_size}M" "$file_path" || {
                    cluster_error "Failed to create sparse file: $file_path"
                    return 1
                }

                chmod 600 "$file_path"
                NVME_BACKING_DEVICES+=("$file_path")
                cluster_debug "NVMe backing device $i: $file_path (sparse)"
            done
            ;;

        loop_memory)
            # For NVMe, create tmpfs files (NVMe requires file backing)
            for i in $(seq 1 "$num_devices"); do
                local tmpfs_path="/dev/shm/nvme-${cluster_id}-${i}.img"

                cluster_debug "Creating tmpfs file: $tmpfs_path"
                dd if=/dev/zero of="$tmpfs_path" bs=1M count="$device_size" 2>&1 | grep -v records || {
                    cluster_error "Failed to create tmpfs backing file"
                    return 1
                }

                chmod 600 "$tmpfs_path"
                NVME_BACKING_DEVICES+=("$tmpfs_path")
                cluster_debug "NVMe backing device $i: $tmpfs_path (tmpfs)"
            done
            ;;

        *)
            cluster_error "Unknown NVMe backing type: $backing_type"
            return 1
            ;;
    esac

    cluster_log "Created ${#NVME_BACKING_DEVICES[@]} NVMe backing devices"
    return 0
}

#
# cluster_iscsi_setup_target - Configure iSCSI target using targetcli
#
# Arguments:
#   $1 - exporter_ip
#   $2 - num_devices (number of iSCSI devices to export)
#   $3 - backing_type (file, sparsefile, ramdisk)
#   $4 - device_size (MB)
#   $5 - sector_size (512 or 4096)
#   $6 - cluster_id
#
# Returns:
#   0 on success, 1 on failure
#
#
# cluster_storage_create_multipath_backing - Create backing storage for multipath devices
#
# Arguments:
#   $1 - backing_type (file_prealloc, file_sparse, loop_prealloc, loop_sparse, or memory)
#   $2 - num_devices (number of multipath devices)
#   $3 - device_size (size in MB)
#   $4 - sector_size (512 or 4096)
#   $5 - cluster_id (for unique naming)
#
# Returns:
#   0 on success, 1 on failure
#
# Sets global array:
#   MULTIPATH_BACKING_DEVICES - array of backing device paths
#
cluster_storage_create_multipath_backing() {
    local backing_type="$1"
    local num_devices="$2"
    local device_size="$3"
    local sector_size="$4"
    local cluster_id="$5"

    cluster_log "Creating $num_devices multipath backing devices (type=$backing_type, size=${device_size}MB, sector=$sector_size)"

    MULTIPATH_BACKING_DEVICES=()

    case "$backing_type" in
        file_prealloc)
            # Create directory for backing files
            mkdir -p "$CLUSTER_MULTIPATH_STORAGE_DIR" || {
                cluster_error "Failed to create multipath storage directory: $CLUSTER_MULTIPATH_STORAGE_DIR"
                return 1
            }

            # Create preallocated files with dd
            for i in $(seq 1 "$num_devices"); do
                local file_path="$CLUSTER_MULTIPATH_STORAGE_DIR/multipath-${cluster_id}-${i}.img"

                cluster_debug "Creating preallocated file: $file_path"
                dd if=/dev/zero of="$file_path" bs=1M count="$device_size" 2>&1 | grep -v records || {
                    cluster_error "Failed to create backing file: $file_path"
                    return 1
                }

                chmod 600 "$file_path"
                MULTIPATH_BACKING_DEVICES+=("$file_path")
                cluster_debug "Multipath backing device $i: $file_path"
            done
            ;;

        file_sparse)
            # Create directory for sparse files
            mkdir -p "$CLUSTER_MULTIPATH_STORAGE_DIR" || {
                cluster_error "Failed to create multipath storage directory: $CLUSTER_MULTIPATH_STORAGE_DIR"
                return 1
            }

            # Create sparse files with truncate (much faster than dd)
            for i in $(seq 1 "$num_devices"); do
                local file_path="$CLUSTER_MULTIPATH_STORAGE_DIR/multipath-${cluster_id}-${i}.img"

                cluster_debug "Creating sparse file: $file_path (${device_size}MB)"
                truncate -s "${device_size}M" "$file_path" || {
                    cluster_error "Failed to create sparse file: $file_path"
                    return 1
                }

                chmod 600 "$file_path"
                MULTIPATH_BACKING_DEVICES+=("$file_path")
                cluster_debug "Multipath backing device $i: $file_path (sparse, ${device_size}MB)"
            done
            ;;

        loop_prealloc)
            # Create directory for backing files
            mkdir -p "$CLUSTER_MULTIPATH_STORAGE_DIR" || {
                cluster_error "Failed to create multipath storage directory: $CLUSTER_MULTIPATH_STORAGE_DIR"
                return 1
            }

            # Create preallocated files with dd and set up loop devices
            for i in $(seq 1 "$num_devices"); do
                local file_path="$CLUSTER_MULTIPATH_STORAGE_DIR/multipath-${cluster_id}-${i}.img"

                cluster_debug "Creating preallocated file: $file_path"
                dd if=/dev/zero of="$file_path" bs=1M count="$device_size" 2>&1 | grep -v records || {
                    cluster_error "Failed to create backing file: $file_path"
                    return 1
                }

                chmod 600 "$file_path"

                # Set up loop device
                cluster_debug "Setting up loop device for: $file_path"
                local loop_dev=$(losetup -f --show "$file_path") || {
                    cluster_error "Failed to set up loop device for: $file_path"
                    return 1
                }

                MULTIPATH_BACKING_DEVICES+=("$loop_dev")
                cluster_debug "Multipath backing device $i: $loop_dev (loop on preallocated file)"
            done
            ;;

        loop_sparse)
            # Create directory for sparse files
            mkdir -p "$CLUSTER_MULTIPATH_STORAGE_DIR" || {
                cluster_error "Failed to create multipath storage directory: $CLUSTER_MULTIPATH_STORAGE_DIR"
                return 1
            }

            # Create sparse files with truncate and set up loop devices
            for i in $(seq 1 "$num_devices"); do
                local file_path="$CLUSTER_MULTIPATH_STORAGE_DIR/multipath-${cluster_id}-${i}.img"

                cluster_debug "Creating sparse file: $file_path (${device_size}MB)"
                truncate -s "${device_size}M" "$file_path" || {
                    cluster_error "Failed to create sparse file: $file_path"
                    return 1
                }

                chmod 600 "$file_path"

                # Set up loop device
                cluster_debug "Setting up loop device for: $file_path"
                local loop_dev=$(losetup -f --show "$file_path") || {
                    cluster_error "Failed to set up loop device for: $file_path"
                    return 1
                }

                MULTIPATH_BACKING_DEVICES+=("$loop_dev")
                cluster_debug "Multipath backing device $i: $loop_dev (loop on sparse file)"
            done
            ;;

        memory)
            # For ramdisk, targetcli rd_mcp handles it - just track names
            for i in $(seq 1 "$num_devices"); do
                local ramdisk_name="ramdisk-multipath-${cluster_id}-${i}"
                MULTIPATH_BACKING_DEVICES+=("$ramdisk_name")
                cluster_debug "Multipath backing device $i: $ramdisk_name (ramdisk)"
            done
            ;;

        *)
            cluster_error "Unknown multipath backing type: $backing_type"
            return 1
            ;;
    esac

    cluster_log "Created ${#MULTIPATH_BACKING_DEVICES[@]} multipath backing devices"
    return 0
}

#
# cluster_multipath_setup_targets - Configure multiple iSCSI targets for multipath
#
# Creates multiple iSCSI targets (one per path) where each target's LUNs
# point to the same backing storage, enabling true multipath with redundant paths.
#
# Arguments:
#   $1 - exporter_ip (IP address for iSCSI targets)
#   $2 - num_devices (number of multipath devices)
#   $3 - num_paths (number of paths per device, default 2)
#   $4 - backing_type (file_prealloc, file_sparse, loop_prealloc, loop_sparse, or memory)
#   $5 - device_size (size in MB)
#   $6 - sector_size (512 or 4096)
#   $7 - cluster_id
#
# Returns:
#   0 on success, 1 on failure
#
cluster_multipath_setup_targets() {
    local exporter_ip="$1"
    local num_devices="$2"
    local num_paths="$3"
    local backing_type="$4"
    local device_size="$5"
    local sector_size="$6"
    local cluster_id="$7"

    cluster_log "Configuring multipath iSCSI targets (devices=$num_devices, paths=$num_paths, type=$backing_type, size=${device_size}MB, sector=$sector_size)"

    # Check if targetcli is available
    if ! command -v targetcli &>/dev/null; then
        cluster_error "targetcli command not found - is targetcli package installed?"
        return 1
    fi

    # Enable persistent reservations
    cluster_log "Enabling persistent reservations support"
    mkdir -p /etc/target/pr || {
        cluster_warn "Failed to create /etc/target/pr directory"
    }

    # Step 1: Create backstores once (shared across all paths)
    cluster_log "Creating shared backstores for multipath devices"
    for dev_num in $(seq 1 "$num_devices"); do
        local backstore_name="lvmdisk-mp-${dev_num}"
        local backing_device="${MULTIPATH_BACKING_DEVICES[$((dev_num-1))]}"

        cluster_debug "Creating shared backstore: $backstore_name -> $backing_device"

        case "$backing_type" in
            file_prealloc|file_sparse)
                # Create fileio backstore
                local sparse_opt=""
                if [ "$backing_type" = "file_sparse" ]; then
                    sparse_opt="sparse=true"
                fi

                targetcli /backstores/fileio create "$backstore_name" "$backing_device" ${device_size}M write_back=false $sparse_opt || {
                    cluster_error "Failed to create fileio backstore: $backstore_name"
                    return 1
                }
                ;;

            loop_prealloc|loop_sparse)
                # Create block backstore with loop device
                targetcli /backstores/block create "$backstore_name" "$backing_device" || {
                    cluster_error "Failed to create block backstore: $backstore_name"
                    return 1
                }
                ;;

            memory)
                # Create ramdisk backstore
                targetcli /backstores/ramdisk create "$backstore_name" ${device_size}M || {
                    cluster_error "Failed to create ramdisk backstore: $backstore_name"
                    return 1
                }
                ;;

            *)
                cluster_error "Unknown backing type: $backing_type"
                return 1
                ;;
        esac

        # Set optimal_io_size reported to SCSI initiators
        if [ -n "${CLUSTER_MULTIPATH_OPTIMAL_IO_SIZE:-}" ]; then
            local mp_backstore_path
            if [ "$backing_type" = "memory" ]; then
                mp_backstore_path="/backstores/ramdisk/$backstore_name"
            elif [ "$backing_type" = "loop_prealloc" ] || [ "$backing_type" = "loop_sparse" ]; then
                mp_backstore_path="/backstores/block/$backstore_name"
            else
                mp_backstore_path="/backstores/fileio/$backstore_name"
            fi
            local optimal_sectors
            if [ "$CLUSTER_MULTIPATH_OPTIMAL_IO_SIZE" -eq 0 ]; then
                optimal_sectors=0
            else
                optimal_sectors=$(( (CLUSTER_MULTIPATH_OPTIMAL_IO_SIZE * 1024) / sector_size ))
            fi
            cluster_log "Setting optimal_sectors=$optimal_sectors on $backstore_name (${CLUSTER_MULTIPATH_OPTIMAL_IO_SIZE}KB, ${sector_size}-byte sectors)"
            targetcli "$mp_backstore_path" set attribute optimal_sectors=$optimal_sectors || {
                cluster_warn "Failed to set optimal_sectors on $backstore_name"
            }
        fi
    done

    # Step 2: Create a separate target for each path, with LUNs pointing to shared backstores
    for path_num in $(seq 0 $((num_paths - 1))); do
        local target_iqn="iqn.2025-03.com.lvm:cluster.${cluster_id}.mp.path-${path_num}"
        cluster_log "Creating multipath target path $path_num: $target_iqn"

        # Clear any existing configuration for this target
        targetcli /iscsi delete "$target_iqn" 2>/dev/null || true

        # Create target
        targetcli /iscsi create "$target_iqn" || {
            cluster_error "Failed to create multipath iSCSI target: $target_iqn"
            return 1
        }

        # Create LUNs for each device, pointing to the shared backstores
        for dev_num in $(seq 1 "$num_devices"); do
            local backstore_name="lvmdisk-mp-${dev_num}"
            local lun_num=$((dev_num - 1))

            cluster_debug "Creating LUN $lun_num in path $path_num -> backstore $backstore_name"

            # Determine backstore path based on type
            local backstore_path
            if [ "$backing_type" = "memory" ]; then
                backstore_path="/backstores/ramdisk/$backstore_name"
            elif [ "$backing_type" = "loop_prealloc" ] || [ "$backing_type" = "loop_sparse" ]; then
                backstore_path="/backstores/block/$backstore_name"
            else
                backstore_path="/backstores/fileio/$backstore_name"
            fi

            targetcli /iscsi/"$target_iqn"/tpg1/luns create "$backstore_path" "$lun_num" 2>/dev/null || \
            targetcli /iscsi/"$target_iqn"/tpg1/luns create "$backstore_path" || {
                cluster_error "Failed to create LUN: $lun_num"
                return 1
            }
        done

        # Configure ACLs (demo mode - allow any initiator)
        cluster_log "Configuring ACLs for $target_iqn (demo mode enabled)"
        targetcli /iscsi/"$target_iqn"/tpg1/ set attribute authentication=0 demo_mode_write_protect=0 generate_node_acls=1 cache_dynamic_acls=1 enforce_pr_isids=1 login_timeout=30 || {
            cluster_warn "Failed to set ACL attributes (may not be critical)"
        }
    done

    # Save configuration
    cluster_log "Saving multipath iSCSI target configuration"
    targetcli saveconfig || {
        cluster_warn "Failed to save targetcli configuration"
    }

    # Enable and start target service
    systemctl enable target 2>/dev/null || true
    systemctl restart target || {
        cluster_error "Failed to restart target service"
        return 1
    }

    # Configure firewall (same as regular iSCSI)
    cluster_log "Ensuring firewall allows iSCSI (port 3260)"
    if command -v firewall-cmd &>/dev/null; then
        firewall-cmd --permanent --add-service=iscsi-target 2>/dev/null || \
        firewall-cmd --permanent --add-port=3260/tcp 2>/dev/null || true
        firewall-cmd --reload 2>/dev/null || true
    elif command -v ufw &>/dev/null; then
        ufw allow 3260/tcp 2>/dev/null || true
    fi

    cluster_log "Multipath iSCSI target configuration complete"
    cluster_log "  Devices: $num_devices"
    cluster_log "  Paths per device: $num_paths"
    cluster_log "  Total targets: $num_paths"

    return 0
}

cluster_iscsi_setup_target() {
    local exporter_ip="$1"
    local num_devices="$2"
    local backing_type="$3"
    local device_size="$4"
    local sector_size="$5"
    local cluster_id="$6"

    # DEBUG: Log all parameters
    cluster_log "cluster_iscsi_setup_target called with:"
    cluster_log "  exporter_ip=$exporter_ip"
    cluster_log "  num_devices=$num_devices"
    cluster_log "  backing_type=$backing_type"
    cluster_log "  device_size=$device_size"
    cluster_log "  sector_size=$sector_size"
    cluster_log "  cluster_id=$cluster_id"

    # Validate required parameters
    if [ -z "$device_size" ] || [ "$device_size" -le 0 ]; then
        cluster_error "Invalid device_size: '$device_size' (must be > 0)"
        return 1
    fi

    cluster_log "Configuring iSCSI target (devices=$num_devices, type=$backing_type, size=${device_size}MB, sector=$sector_size)"

    # Check if targetcli is available
    if ! command -v targetcli &>/dev/null; then
        cluster_error "targetcli command not found - is targetcli package installed?"
        return 1
    fi

    # Enable persistent reservations
    cluster_log "Enabling persistent reservations support"
    mkdir -p /etc/target/pr || {
        cluster_warn "Failed to create /etc/target/pr directory"
    }

    # Target IQN
    local target_iqn="iqn.2025-03.com.lvm:cluster.${cluster_id}"
    cluster_debug "Target IQN: $target_iqn"

    # Clear any existing configuration for this cluster
    targetcli /iscsi delete "$target_iqn" 2>/dev/null || true

    # Create target
    cluster_log "Creating iSCSI target: $target_iqn"
    targetcli /iscsi create "$target_iqn" || {
        cluster_error "Failed to create iSCSI target"
        return 1
    }

    # Create backstores and LUNs
    for i in $(seq 1 "$num_devices"); do
        local backstore_name="lvmdisk-scsi-${i}"
        local backing_device="${SCSI_BACKING_DEVICES[$((i-1))]}"

        cluster_log "Creating iSCSI backstore $i/$num_devices: $backstore_name"

        case "$backing_type" in
            file_prealloc)
                # Create fileio backstore with preallocated file
                targetcli /backstores/fileio create "$backstore_name" "$backing_device" ${device_size}M write_back=false || {
                    cluster_error "Failed to create fileio backstore: $backstore_name"
                    return 1
                }
                ;;

            file_sparse)
                # Create fileio backstore with sparse file
                targetcli /backstores/fileio create "$backstore_name" "$backing_device" ${device_size}M write_back=false sparse=true || {
                    cluster_error "Failed to create sparse fileio backstore: $backstore_name"
                    return 1
                }
                ;;

            loop_prealloc|loop_sparse)
                # Create block backstore with loop device
                targetcli /backstores/block create "$backstore_name" "$backing_device" || {
                    cluster_error "Failed to create block backstore: $backstore_name"
                    return 1
                }
                ;;

            memory)
                # Create ramdisk backstore
                targetcli /backstores/ramdisk create "$backstore_name" ${device_size}M || {
                    cluster_error "Failed to create ramdisk backstore: $backstore_name"
                    return 1
                }
                ;;

            *)
                cluster_error "Unknown backing type: $backing_type"
                return 1
                ;;
        esac

        # Determine backstore path based on type
        local backstore_path
        if [ "$backing_type" = "memory" ]; then
            backstore_path="/backstores/ramdisk/$backstore_name"
        elif [ "$backing_type" = "loop_prealloc" ] || [ "$backing_type" = "loop_sparse" ]; then
            backstore_path="/backstores/block/$backstore_name"
        else
            backstore_path="/backstores/fileio/$backstore_name"
        fi

        # Set optimal_io_size reported to SCSI initiators
        if [ -n "${CLUSTER_SCSI_OPTIMAL_IO_SIZE:-}" ]; then
            local optimal_sectors
            if [ "$CLUSTER_SCSI_OPTIMAL_IO_SIZE" -eq 0 ]; then
                optimal_sectors=0
            else
                optimal_sectors=$(( (CLUSTER_SCSI_OPTIMAL_IO_SIZE * 1024) / sector_size ))
            fi
            cluster_log "Setting optimal_sectors=$optimal_sectors on $backstore_name (${CLUSTER_SCSI_OPTIMAL_IO_SIZE}KB, ${sector_size}-byte sectors)"
            targetcli "$backstore_path" set attribute optimal_sectors=$optimal_sectors || {
                cluster_warn "Failed to set optimal_sectors on $backstore_name"
            }
        fi

        # Create LUN mapped to backstore
        local lun_num=$((i - 1))
        cluster_log "Creating LUN $lun_num for backstore: $backstore_name"

        targetcli /iscsi/"$target_iqn"/tpg1/luns create "$backstore_path" "$lun_num" 2>/dev/null || \
        targetcli /iscsi/"$target_iqn"/tpg1/luns create "$backstore_path" || {
            cluster_error "Failed to create LUN: $lun_num"
            return 1
        }
    done

    # Configure ACLs (demo mode - allow any initiator)
    cluster_log "Configuring ACLs (demo mode enabled)"
    targetcli /iscsi/"$target_iqn"/tpg1/ set attribute authentication=0 demo_mode_write_protect=0 generate_node_acls=1 cache_dynamic_acls=1 enforce_pr_isids=1 login_timeout=30 || {
        cluster_warn "Failed to set ACL attributes (may not be critical)"
    }

    # Save configuration
    cluster_log "Saving iSCSI target configuration"
    targetcli saveconfig || {
        cluster_warn "Failed to save targetcli configuration"
    }

    # Enable and start target service
    systemctl enable target 2>/dev/null || true
    systemctl restart target || {
        cluster_error "Failed to restart target service"
        return 1
    }

    # Configure firewall to allow iSCSI traffic (port 3260)
    cluster_log "Configuring firewall for iSCSI (port 3260)"
    if command -v firewall-cmd &>/dev/null; then
        # Using firewalld
        firewall-cmd --permanent --add-service=iscsi-target 2>/dev/null || \
        firewall-cmd --permanent --add-port=3260/tcp 2>/dev/null || true
        firewall-cmd --reload 2>/dev/null || true
    elif command -v ufw &>/dev/null; then
        # Using ufw (Ubuntu/Debian)
        ufw allow 3260/tcp 2>/dev/null || true
    else
        cluster_warn "No supported firewall found (firewalld or ufw) - you may need to configure manually"
    fi

    cluster_log "iSCSI target configuration complete"
    cluster_log "  Target IQN: $target_iqn"
    cluster_log "  LUNs: $num_devices"
    cluster_log "  Sector size: $sector_size"

    return 0
}

#
# cluster_nvme_setup_target - Configure NVMe-oF target using configfs
#
# Arguments:
#   $1 - exporter_ip
#   $2 - num_devices (number of NVMe devices to export)
#   $3 - backing_type (file, sparsefile, ramdisk)
#   $4 - device_size (MB)
#   $5 - sector_size (512 or 4096)
#   $6 - cluster_id
#
# Returns:
#   0 on success, 1 on failure
#
cluster_nvme_setup_target() {
    local exporter_ip="$1"
    local num_devices="$2"
    local backing_type="$3"
    local device_size="$4"
    local sector_size="$5"
    local cluster_id="$6"

    # Validate required parameters
    if [ -z "$device_size" ] || [ "$device_size" -le 0 ]; then
        cluster_error "Invalid device_size: '$device_size' (must be > 0)"
        return 1
    fi

    cluster_log "Configuring NVMe-oF target (devices=$num_devices, type=$backing_type, size=${device_size}MB, sector=$sector_size)"

    # Check if nvmet module is loaded
    if ! lsmod | grep -q nvmet; then
        cluster_log "Loading nvmet kernel module"
        modprobe nvmet || {
            cluster_error "Failed to load nvmet module"
            return 1
        }
    fi

    if ! lsmod | grep -q nvme_tcp; then
        cluster_log "Loading nvme-tcp kernel module"
        modprobe nvme-tcp || {
            cluster_error "Failed to load nvme-tcp module"
            return 1
        }
    fi

    # NVMe subsystem NQN
    local subsystem_nqn="nqn.2025-03.lvm.cluster:${cluster_id}"
    local nvmet_base="/sys/kernel/config/nvmet"
    local subsystem_path="$nvmet_base/subsystems/$subsystem_nqn"
    local port_path="$nvmet_base/ports/1"

    cluster_debug "Subsystem NQN: $subsystem_nqn"

    # Create subsystem
    cluster_log "Creating NVMe subsystem: $subsystem_nqn"
    mkdir -p "$subsystem_path" || {
        cluster_error "Failed to create subsystem directory"
        return 1
    }

    # Allow any host to connect
    echo 1 > "$subsystem_path/attr_allow_any_host" || {
        cluster_error "Failed to set allow_any_host"
        return 1
    }

    # Create namespaces for each backing device
    for i in $(seq 1 "$num_devices"); do
        local ns_id="$i"
        local ns_path="$subsystem_path/namespaces/$ns_id"
        local backing_device="${NVME_BACKING_DEVICES[$((i-1))]}"

        cluster_log "Creating NVMe namespace $i/$num_devices (nsid=$ns_id)"

        # Create namespace directory
        mkdir -p "$ns_path" || {
            cluster_error "Failed to create namespace directory: $ns_path"
            return 1
        }

        # Create loop device for block device backing
        cluster_debug "Creating loop device for NVMe namespace backing: $backing_device"
        local loop_dev
        loop_dev=$(losetup -f --show "$backing_device") || {
            cluster_error "Failed to create loop device for: $backing_device"
            return 1
        }

        # Set namespace device path
        echo "$loop_dev" > "$ns_path/device_path" || {
            cluster_error "Failed to set device_path for namespace $ns_id"
            losetup -d "$loop_dev" 2>/dev/null || true
            return 1
        }

        # Enable NVMe reservations
        echo 1 > "$ns_path/resv_enable" || {
            cluster_warn "Failed to enable reservations for namespace $ns_id (may not be supported)"
        }

        # Enable namespace
        echo 1 > "$ns_path/enable" || {
            cluster_error "Failed to enable namespace $ns_id"
            return 1
        }

        cluster_debug "NVMe namespace $ns_id enabled with backing: $loop_dev"
    done

    # Create port for TCP transport
    cluster_log "Configuring NVMe TCP port"
    mkdir -p "$port_path" || {
        cluster_error "Failed to create port directory"
        return 1
    }

    echo "tcp" > "$port_path/addr_trtype" || {
        cluster_error "Failed to set transport type"
        return 1
    }

    echo "ipv4" > "$port_path/addr_adrfam" || {
        cluster_error "Failed to set address family"
        return 1
    }

    echo "$exporter_ip" > "$port_path/addr_traddr" || {
        cluster_error "Failed to set transport address"
        return 1
    }

    echo "4420" > "$port_path/addr_trsvcid" || {
        cluster_error "Failed to set transport service ID"
        return 1
    }

    # Link subsystem to port
    cluster_log "Linking subsystem to port"
    ln -s "$subsystem_path" "$port_path/subsystems/$subsystem_nqn" 2>/dev/null || {
        cluster_warn "Subsystem already linked to port (may be ok)"
    }

    # Configure firewall to allow NVMe-oF traffic (port 4420)
    cluster_log "Configuring firewall for NVMe-oF (port 4420)"
    if command -v firewall-cmd &>/dev/null; then
        # Using firewalld
        firewall-cmd --permanent --add-port=4420/tcp 2>/dev/null || true
        firewall-cmd --reload 2>/dev/null || true
    elif command -v ufw &>/dev/null; then
        # Using ufw (Ubuntu/Debian)
        ufw allow 4420/tcp 2>/dev/null || true
    else
        cluster_warn "No supported firewall found (firewalld or ufw) - you may need to configure manually"
    fi

    cluster_log "NVMe-oF target configuration complete"
    cluster_log "  Subsystem NQN: $subsystem_nqn"
    cluster_log "  Namespaces: $num_devices"
    cluster_log "  Port: ${exporter_ip}:4420"
    cluster_log "  Sector size: $sector_size"

    return 0
}

#
# cluster_storage_setup - Main storage export setup function
#
# Environment variables required:
#   CLUSTER_NUM_SCSI
#   CLUSTER_NUM_NVME
#   CLUSTER_SCSI_SIZE (if CLUSTER_NUM_SCSI > 0)
#   CLUSTER_SCSI_SECTOR_SIZE (if CLUSTER_NUM_SCSI > 0)
#   CLUSTER_SCSI_BACKING_TYPE (if CLUSTER_NUM_SCSI > 0)
#   CLUSTER_NVME_SIZE (if CLUSTER_NUM_NVME > 0)
#   CLUSTER_NVME_SECTOR_SIZE (if CLUSTER_NUM_NVME > 0)
#   CLUSTER_NVME_BACKING_TYPE (if CLUSTER_NUM_NVME > 0)
#   NODE0_IP (IP address of node 0)
#
# Returns:
#   0 on success, 1 on failure
#
cluster_storage_setup() {
    local cluster_id="$1"

    if [ -z "$cluster_id" ]; then
        cluster_error "cluster_id is required"
        return 1
    fi

    # Validate required environment variables
    if [ -z "${CLUSTER_NUM_SCSI:-}" ]; then
        cluster_error "CLUSTER_NUM_SCSI not set"
        return 1
    fi

    if [ -z "${CLUSTER_NUM_NVME:-}" ]; then
        cluster_error "CLUSTER_NUM_NVME not set"
        return 1
    fi

    if [ -z "${CLUSTER_NUM_MULTIPATH:-}" ]; then
        cluster_error "CLUSTER_NUM_MULTIPATH not set"
        return 1
    fi

    if [ -z "${NODE0_IP:-}" ]; then
        cluster_error "NODE0_IP not set"
        return 1
    fi

    local num_scsi="${CLUSTER_NUM_SCSI}"
    local num_nvme="${CLUSTER_NUM_NVME}"
    local num_multipath="${CLUSTER_NUM_MULTIPATH}"
    local total_devices=$((num_scsi + num_nvme + num_multipath))
    local exporter_ip="${NODE0_IP}"

    # Validate at least one protocol is enabled
    if [ $total_devices -eq 0 ]; then
        cluster_error "At least one of CLUSTER_NUM_SCSI, CLUSTER_NUM_NVME, or CLUSTER_NUM_MULTIPATH must be > 0"
        return 1
    fi

    cluster_log "=========================================="
    cluster_log "Storage Export Setup on Node 0"
    cluster_log "=========================================="
    cluster_log "Cluster ID: $cluster_id"
    cluster_log "iSCSI Devices: $num_scsi"
    cluster_log "NVMe Devices: $num_nvme"
    cluster_log "Multipath Devices: $num_multipath"
    cluster_log "Total Devices: $total_devices"
    cluster_log "Exporter IP: $exporter_ip"
    cluster_log "=========================================="

    # Create iSCSI backing storage and configure target
    if [ $num_scsi -gt 0 ]; then
        local scsi_size="${CLUSTER_SCSI_SIZE}"
        local scsi_sector="${CLUSTER_SCSI_SECTOR_SIZE}"
        local scsi_backing="${CLUSTER_SCSI_BACKING_TYPE}"

        # DEBUG: Log what we got from environment
        cluster_log "DEBUG: iSCSI configuration from environment:"
        cluster_log "  CLUSTER_SCSI_SIZE='${CLUSTER_SCSI_SIZE}'"
        cluster_log "  CLUSTER_SCSI_SECTOR_SIZE='${CLUSTER_SCSI_SECTOR_SIZE}'"
        cluster_log "  CLUSTER_SCSI_BACKING_TYPE='${CLUSTER_SCSI_BACKING_TYPE}'"
        cluster_log "  Local variables:"
        cluster_log "    scsi_size='$scsi_size'"
        cluster_log "    scsi_sector='$scsi_sector'"
        cluster_log "    scsi_backing='$scsi_backing'"

        cluster_log "iSCSI Configuration:"
        cluster_log "  Size: ${scsi_size}MB"
        cluster_log "  Sector Size: $scsi_sector"
        cluster_log "  Backing Type: $scsi_backing"

        # Create backing storage
        cluster_storage_create_scsi_backing "$scsi_backing" "$num_scsi" "$scsi_size" "$scsi_sector" "$cluster_id" || {
            cluster_error "Failed to create iSCSI backing storage"
            return 1
        }

        # Setup iSCSI target
        cluster_iscsi_setup_target "$exporter_ip" "$num_scsi" "$scsi_backing" "$scsi_size" "$scsi_sector" "$cluster_id" || {
            cluster_error "Failed to setup iSCSI target"
            return 1
        }
    else
        cluster_log "Skipping iSCSI export (CLUSTER_NUM_SCSI=0)"
    fi

    # Create NVMe backing storage and configure target
    if [ $num_nvme -gt 0 ]; then
        local nvme_size="${CLUSTER_NVME_SIZE}"
        local nvme_sector="${CLUSTER_NVME_SECTOR_SIZE}"
        local nvme_backing="${CLUSTER_NVME_BACKING_TYPE}"

        cluster_log "NVMe Configuration:"
        cluster_log "  Size: ${nvme_size}MB"
        cluster_log "  Sector Size: $nvme_sector"
        cluster_log "  Backing Type: $nvme_backing"

        # Create backing storage
        cluster_storage_create_nvme_backing "$nvme_backing" "$num_nvme" "$nvme_size" "$nvme_sector" "$cluster_id" || {
            cluster_error "Failed to create NVMe backing storage"
            return 1
        }

        # Setup NVMe target
        cluster_nvme_setup_target "$exporter_ip" "$num_nvme" "$nvme_backing" "$nvme_size" "$nvme_sector" "$cluster_id" || {
            cluster_error "Failed to setup NVMe-oF target"
            return 1
        }
    else
        cluster_log "Skipping NVMe-oF export (CLUSTER_NUM_NVME=0)"
    fi

    # Create multipath backing storage and configure targets
    local num_multipath="${CLUSTER_NUM_MULTIPATH:-0}"
    if [ $num_multipath -gt 0 ]; then
        local mp_size="${CLUSTER_MULTIPATH_SIZE}"
        local mp_sector="${CLUSTER_MULTIPATH_SECTOR_SIZE}"
        local mp_backing="${CLUSTER_MULTIPATH_BACKING_TYPE}"
        local mp_paths="${CLUSTER_MULTIPATH_PATHS:-2}"

        cluster_log "Multipath Configuration:"
        cluster_log "  Devices: $num_multipath"
        cluster_log "  Paths per device: $mp_paths"
        cluster_log "  Size: ${mp_size}MB"
        cluster_log "  Sector Size: $mp_sector"
        cluster_log "  Backing Type: $mp_backing"

        # Create backing storage
        cluster_storage_create_multipath_backing "$mp_backing" "$num_multipath" "$mp_size" "$mp_sector" "$cluster_id" || {
            cluster_error "Failed to create multipath backing storage"
            return 1
        }

        # Setup multipath targets
        cluster_multipath_setup_targets "$exporter_ip" "$num_multipath" "$mp_paths" "$mp_backing" "$mp_size" "$mp_sector" "$cluster_id" || {
            cluster_error "Failed to setup multipath iSCSI targets"
            return 1
        }
    else
        cluster_log "Skipping multipath export (CLUSTER_NUM_MULTIPATH=0)"
    fi

    cluster_log "Storage export setup complete"
    return 0
}

#
# cluster_storage_cleanup - Cleanup storage export configuration
#
# Arguments:
#   $1 - cluster_id
#
# Returns:
#   0 on success, 1 on failure
#
cluster_storage_cleanup() {
    local cluster_id="$1"

    if [ -z "$cluster_id" ]; then
        cluster_error "cluster_id is required"
        return 1
    fi

    cluster_log "Cleaning up storage export for cluster: $cluster_id"

    local num_scsi="${CLUSTER_NUM_SCSI:-0}"
    local num_nvme="${CLUSTER_NUM_NVME:-0}"

    # Cleanup iSCSI
    if [ $num_scsi -gt 0 ]; then
        local target_iqn="iqn.2025-03.com.lvm:cluster.${cluster_id}"
        local backing_type="${CLUSTER_SCSI_BACKING_TYPE:-file_sparse}"

        if command -v targetcli &>/dev/null; then
            cluster_debug "Removing iSCSI target: $target_iqn"
            targetcli /iscsi delete "$target_iqn" 2>/dev/null || true

            # Remove backstores and detach loop devices if needed
            for i in $(seq 1 "$num_scsi"); do
                local backstore_name="lvmdisk-scsi-${i}"

                # For loop-based backing, get loop device before removing backstore
                if [ "$backing_type" = "loop_prealloc" ] || [ "$backing_type" = "loop_sparse" ]; then
                    # Try to get loop device from block backstore
                    local loop_dev=$(targetcli /backstores/block ls 2>/dev/null | grep "$backstore_name" | awk '{print $2}' || true)

                    targetcli /backstores/block delete "$backstore_name" 2>/dev/null || true

                    # Detach loop device
                    if [ -n "$loop_dev" ] && [ -b "$loop_dev" ]; then
                        cluster_debug "Detaching loop device: $loop_dev"
                        losetup -d "$loop_dev" 2>/dev/null || true
                    fi
                else
                    targetcli /backstores/fileio delete "$backstore_name" 2>/dev/null || true
                    targetcli /backstores/ramdisk delete "$backstore_name" 2>/dev/null || true
                fi
            done

            targetcli saveconfig 2>/dev/null || true
        fi

        # Detach any remaining loop devices for this cluster (cleanup fallback)
        if [ "$backing_type" = "loop_prealloc" ] || [ "$backing_type" = "loop_sparse" ]; then
            for loop_dev in $(losetup -l -n -O NAME,BACK-FILE 2>/dev/null | grep "scsi-disk-${cluster_id}-" | awk '{print $1}'); do
                if [ -b "$loop_dev" ]; then
                    cluster_debug "Detaching remaining loop device: $loop_dev"
                    losetup -d "$loop_dev" 2>/dev/null || true
                fi
            done
        fi

        # Remove backing storage files
        if [ -d "$CLUSTER_SCSI_STORAGE_DIR" ]; then
            cluster_debug "Removing iSCSI backing storage directory"
            rm -rf "$CLUSTER_SCSI_STORAGE_DIR"
        fi
    fi

    # Cleanup NVMe-oF
    if [ $num_nvme -gt 0 ]; then
        local subsystem_nqn="nqn.2025-03.lvm.cluster:${cluster_id}"
        local nvmet_base="/sys/kernel/config/nvmet"
        local subsystem_path="$nvmet_base/subsystems/$subsystem_nqn"
        local port_path="$nvmet_base/ports/1"

        # Unlink subsystem from port
        if [ -L "$port_path/subsystems/$subsystem_nqn" ]; then
            cluster_debug "Unlinking NVMe subsystem from port"
            rm -f "$port_path/subsystems/$subsystem_nqn" 2>/dev/null || true
        fi

        # Disable and remove namespaces
        if [ -d "$subsystem_path" ]; then
            for ns_path in "$subsystem_path/namespaces"/*; do
                [ -d "$ns_path" ] || continue

                # Get loop device before disabling
                local loop_dev=""
                if [ -f "$ns_path/device_path" ]; then
                    loop_dev=$(cat "$ns_path/device_path" 2>/dev/null || true)
                fi

                # Disable namespace
                echo 0 > "$ns_path/enable" 2>/dev/null || true

                # Detach loop device if it exists
                if [ -n "$loop_dev" ] && [ -b "$loop_dev" ]; then
                    cluster_debug "Detaching loop device: $loop_dev"
                    losetup -d "$loop_dev" 2>/dev/null || true
                fi

                # Remove namespace
                rmdir "$ns_path" 2>/dev/null || true
            done

            # Remove subsystem
            cluster_debug "Removing NVMe subsystem: $subsystem_nqn"
            rmdir "$subsystem_path" 2>/dev/null || true
        fi

        # Remove port (if no other subsystems)
        if [ -d "$port_path/subsystems" ]; then
            local subsys_count
            subsys_count=$(find "$port_path/subsystems" -mindepth 1 -maxdepth 1 2>/dev/null | wc -l)
            if [ "$subsys_count" -eq 0 ]; then
                cluster_debug "Removing NVMe port (no subsystems remaining)"
                rmdir "$port_path" 2>/dev/null || true
            fi
        fi

        # Remove backing storage files
        if [ -d "$CLUSTER_NVME_STORAGE_DIR" ]; then
            cluster_debug "Removing NVMe backing storage directory"
            rm -rf "$CLUSTER_NVME_STORAGE_DIR"
        fi

        # Remove tmpfs files (ramdisk backing)
        rm -f /dev/shm/nvme-${cluster_id}-*.img 2>/dev/null || true
    fi

    # Cleanup multipath
    local num_multipath="${CLUSTER_NUM_MULTIPATH:-0}"
    local num_paths="${CLUSTER_MULTIPATH_PATHS:-2}"
    if [ $num_multipath -gt 0 ]; then
        local mp_backing_type="${CLUSTER_MULTIPATH_BACKING_TYPE:-file_sparse}"

        if command -v targetcli &>/dev/null; then
            # Remove each path target
            for path_num in $(seq 0 $((num_paths - 1))); do
                local target_iqn="iqn.2025-03.com.lvm:cluster.${cluster_id}.mp.path-${path_num}"
                cluster_debug "Removing multipath iSCSI target: $target_iqn"
                targetcli /iscsi delete "$target_iqn" 2>/dev/null || true
            done

            # Remove shared backstores (once, not per path) and detach loop devices if needed
            for dev_num in $(seq 1 "$num_multipath"); do
                local backstore_name="lvmdisk-mp-${dev_num}"
                cluster_debug "Removing shared multipath backstore: $backstore_name"

                # For loop-based backing, get loop device before removing backstore
                if [ "$mp_backing_type" = "loop_prealloc" ] || [ "$mp_backing_type" = "loop_sparse" ]; then
                    # Try to get loop device from block backstore
                    local loop_dev=$(targetcli /backstores/block ls 2>/dev/null | grep "$backstore_name" | awk '{print $2}' || true)

                    targetcli /backstores/block delete "$backstore_name" 2>/dev/null || true

                    # Detach loop device
                    if [ -n "$loop_dev" ] && [ -b "$loop_dev" ]; then
                        cluster_debug "Detaching loop device: $loop_dev"
                        losetup -d "$loop_dev" 2>/dev/null || true
                    fi
                else
                    targetcli /backstores/fileio delete "$backstore_name" 2>/dev/null || true
                    targetcli /backstores/ramdisk delete "$backstore_name" 2>/dev/null || true
                fi
            done

            targetcli saveconfig 2>/dev/null || true
        fi

        # Detach any remaining loop devices for this cluster (cleanup fallback)
        if [ "$mp_backing_type" = "loop_prealloc" ] || [ "$mp_backing_type" = "loop_sparse" ]; then
            for loop_dev in $(losetup -l -n -O NAME,BACK-FILE 2>/dev/null | grep "multipath-${cluster_id}-" | awk '{print $1}'); do
                if [ -b "$loop_dev" ]; then
                    cluster_debug "Detaching remaining loop device: $loop_dev"
                    losetup -d "$loop_dev" 2>/dev/null || true
                fi
            done
        fi

        # Remove backing storage files
        if [ -d "$CLUSTER_MULTIPATH_STORAGE_DIR" ]; then
            cluster_debug "Removing multipath backing storage directory"
            rm -rf "$CLUSTER_MULTIPATH_STORAGE_DIR"
        fi
    fi

    cluster_log "Storage export cleanup complete"
    return 0
}

# If script is executed directly (not sourced), run main function if arguments provided
if [ "${BASH_SOURCE[0]}" = "$0" ]; then
    if [ $# -lt 1 ]; then
        echo "Usage: $0 <cluster_id>" >&2
        echo "" >&2
        echo "This script is normally called via cluster-vm-manager.sh" >&2
        echo "" >&2
        echo "Required environment variables:" >&2
        echo "  CLUSTER_NUM_SCSI - Number of iSCSI devices (0 to disable)" >&2
        echo "  CLUSTER_NUM_NVME - Number of NVMe devices (0 to disable)" >&2
        echo "  CLUSTER_NUM_MULTIPATH - Number of multipath devices (0 to disable)" >&2
        echo "  CLUSTER_SCSI_SIZE - Size in MB (if SCSI enabled)" >&2
        echo "  CLUSTER_SCSI_SECTOR_SIZE - 512 or 4096 (if SCSI enabled)" >&2
        echo "  CLUSTER_SCSI_BACKING_TYPE - file_prealloc, file_sparse, loop_prealloc, loop_sparse, or memory (if SCSI enabled)" >&2
        echo "  CLUSTER_NVME_SIZE - Size in MB (if NVMe enabled)" >&2
        echo "  CLUSTER_NVME_SECTOR_SIZE - 512 or 4096 (if NVMe enabled)" >&2
        echo "  CLUSTER_NVME_BACKING_TYPE - loop_prealloc, loop_sparse, or loop_memory (if NVMe enabled)" >&2
        echo "  CLUSTER_MULTIPATH_SIZE - Size in MB (if multipath enabled)" >&2
        echo "  CLUSTER_MULTIPATH_SECTOR_SIZE - 512 or 4096 (if multipath enabled)" >&2
        echo "  CLUSTER_MULTIPATH_BACKING_TYPE - file_prealloc, file_sparse, loop_prealloc, loop_sparse, or memory (if multipath enabled)" >&2
        echo "  CLUSTER_MULTIPATH_PATHS - Number of paths per device (default: 2, if multipath enabled)" >&2
        echo "  NODE0_IP - IP address of node 0 (this host)" >&2
        exit 1
    fi

    cluster_storage_setup "$1"
fi
