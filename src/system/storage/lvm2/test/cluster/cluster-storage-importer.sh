#!/bin/bash
#
# cluster-storage-importer.sh - Storage import layer for LVM cluster testing
#
# This script runs on test nodes (nodes 1..N) to:
# - Configure iSCSI initiators to connect to node 0
# - Configure NVMe-oF initiators to connect to node 0
# - Discover and list imported devices
# - Configure multipath if enabled
#
# Test nodes import shared storage from node 0 (storage exporter)
#

# Source the core library for logging functions
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck disable=SC1091
source "$SCRIPT_DIR/cluster-test-lib.sh" || exit 1

#
# cluster_iscsi_setup_initiator - Configure iSCSI initiator on test node
#
# Uses iscsiadm to:
#   - Discover available targets from node 0
#   - Login to the target
#   - Establish iSCSI session
#
#   $1 - node_ip (IP address of this test node, for logging)
#   $2 - target_ip (IP address of node 0, the storage exporter)
#   $3 - target_iqn (iSCSI target IQN to connect to)
#
# Returns:
#   0 on success, 1 on failure
#
cluster_iscsi_setup_initiator() {
    local node_ip="$1"
    local target_ip="$2"
    local target_iqn="$3"

    cluster_log "Configuring iSCSI initiator to connect to $target_ip"

    # Check if iscsiadm is available
    if ! command -v iscsiadm &>/dev/null; then
        cluster_error "iscsiadm command not found - is iscsi-initiator-utils package installed?"
        return 1
    fi

    # Ensure iscsid service is running
    if command -v systemctl &>/dev/null; then
        cluster_debug "Starting iscsid service"
        systemctl start iscsid 2>/dev/null || true
        systemctl enable iscsid 2>/dev/null || true
    fi

    # Discover targets from node 0
    cluster_log "Discovering iSCSI targets from $target_ip"
    iscsiadm -m discovery -t sendtargets -p "$target_ip:3260" || {
        cluster_error "Failed to discover iSCSI targets from $target_ip"
        return 1
    }

    # The default iSCSI NOP-Out keepalive interval and timeout are both
    # 5 seconds, which is tight for test VMs on shared hosts.  The iSCSI
    # target (LIO on node0) can briefly stall under VM scheduling pressure,
    # causing NOP timeouts → connection drop → session recovery → device
    # offline → test failure.  Increase NOP interval/timeout to 30s each
    # (60s total tolerance) and replacement_timeout to 300s to give session
    # recovery more time to reconnect.
    iscsiadm -m node -T "$target_iqn" -p "$target_ip:3260" \
        -o update -n node.conn[0].timeo.noop_out_interval -v 30 2>/dev/null || true
    iscsiadm -m node -T "$target_iqn" -p "$target_ip:3260" \
        -o update -n node.conn[0].timeo.noop_out_timeout -v 30 2>/dev/null || true
    iscsiadm -m node -T "$target_iqn" -p "$target_ip:3260" \
        -o update -n node.session.timeo.replacement_timeout -v 300 2>/dev/null || true

    # Login to the target
    cluster_log "Logging in to iSCSI target: $target_iqn"
    iscsiadm -m node -T "$target_iqn" -p "$target_ip:3260" --login || {
        cluster_error "Failed to login to iSCSI target: $target_iqn"
        return 1
    }

    # Wait a moment for devices to appear
    cluster_debug "Waiting for iSCSI devices to appear"
    sleep 2

    # Verify session is active
    if ! iscsiadm -m session 2>/dev/null | grep -q "$target_iqn"; then
        cluster_error "iSCSI session not established for target: $target_iqn"
        return 1
    fi

    cluster_log "iSCSI initiator configuration complete"
    cluster_log "  Target: $target_iqn"
    cluster_log "  Portal: $target_ip:3260"

    # List discovered devices
    cluster_debug "Discovered iSCSI devices:"
    lsblk -d -n -o NAME,SIZE,VENDOR,MODEL 2>/dev/null | grep -i 'LIO-ORG' || true

    return 0
}

#
# cluster_nvme_setup_initiator - Configure NVMe-oF initiator on test node
#
# Uses nvme-cli to:
#   - Discover NVMe subsystems from node 0
#   - Connect to the NVMe-oF target
#   - Establish NVMe-oF connection
#
#   $1 - node_ip (IP address of this test node, for logging)
#   $2 - target_ip (IP address of node 0, the storage exporter)
#   $3 - subsystem_nqn (NVMe subsystem NQN to connect to)
#
# Returns:
#   0 on success, 1 on failure
#
cluster_nvme_setup_initiator() {
    local node_ip="$1"
    local target_ip="$2"
    local subsystem_nqn="$3"

    cluster_log "Configuring NVMe-oF initiator to connect to $target_ip"

    # Check if nvme command is available
    if ! command -v nvme &>/dev/null; then
        cluster_error "nvme command not found - is nvme-cli package installed?"
        return 1
    fi

    # Ensure this node has a unique NVMe hostnqn and hostid.
    # VMs cloned from a golden image may share the same identity,
    # which breaks persistent reservations (nvmet identifies PR
    # registrants by hostid).
    if [ ! -s /etc/nvme/hostnqn ]; then
        mkdir -p /etc/nvme
        nvme gen-hostnqn > /etc/nvme/hostnqn 2>/dev/null || true
        cluster_log "Generated new NVMe hostnqn"
    fi
    if [ ! -s /etc/nvme/hostid ]; then
        mkdir -p /etc/nvme
        uuidgen > /etc/nvme/hostid 2>/dev/null || true
        cluster_log "Generated new NVMe hostid"
    fi
    cluster_log "NVMe hostnqn: $(cat /etc/nvme/hostnqn 2>/dev/null)"
    cluster_log "NVMe hostid: $(cat /etc/nvme/hostid 2>/dev/null)"

    # Load nvme-tcp kernel module if needed
    if ! lsmod | grep -q nvme_tcp; then
        cluster_log "Loading nvme-tcp kernel module"
        modprobe nvme-tcp || {
            cluster_error "Failed to load nvme-tcp module"
            return 1
        }
    fi

    # Discover NVMe subsystems from node 0
    cluster_log "Discovering NVMe subsystems from $target_ip"
    nvme discover -t tcp -a "$target_ip" -s 4420 || {
        cluster_warn "NVMe discovery failed or returned no results"
    }

    # Connect to the NVMe-oF target
    # Use a longer keep-alive timeout (default is 5s which is tight for
    # test VMs on shared hosts — VM scheduling jitter can cause timeouts).
    cluster_log "Connecting to NVMe subsystem: $subsystem_nqn"
    local connect_output
    connect_output=$(nvme connect -t tcp -n "$subsystem_nqn" -a "$target_ip" -s 4420 --keep-alive-tmo=15 2>&1)
    local connect_status=$?

    echo "$connect_output"

    if [ $connect_status -ne 0 ]; then
        cluster_error "Failed to connect to NVMe subsystem: $subsystem_nqn (exit code: $connect_status)"
        cluster_error "Error output: $connect_output"
        return 1
    fi

    # Wait a moment for devices to appear
    cluster_debug "Waiting for NVMe devices to appear"
    sleep 3

    # Verify connection is active - check for nvme devices
    cluster_debug "Checking for NVMe controller devices"
    if ! ls /dev/nvme* >/dev/null 2>&1; then
        cluster_error "No NVMe devices found after connection attempt"
        cluster_debug "NVMe subsystem list:"
        nvme list-subsys 2>&1 || true
        return 1
    fi

    # Check if the controller is connected
    if ! nvme list-subsys 2>/dev/null | grep -q "$subsystem_nqn"; then
        cluster_warn "Subsystem NQN not found in list, but NVMe devices exist"
        cluster_debug "Available NVMe devices:"
        nvme list 2>&1 || true
    fi

    # Install a systemd service so nvme-of reconnects automatically on reboot
    cat > /etc/systemd/system/lvmtest-nvme-connect.service <<SVCEOF
[Unit]
Description=Reconnect NVMe-oF TCP for lvmtest
After=network-online.target
Wants=network-online.target

[Service]
Type=oneshot
RemainAfterExit=yes
ExecStart=/bin/bash -c 'modprobe nvme-tcp; nvme connect -t tcp -n $subsystem_nqn -a $target_ip -s 4420 2>/dev/null; for i in \$(seq 1 10); do ls /dev/nvme* &>/dev/null && break; sleep 1; done'

[Install]
WantedBy=multi-user.target
SVCEOF
    systemctl daemon-reload
    systemctl enable lvmtest-nvme-connect.service 2>/dev/null

    cluster_log "NVMe-oF initiator configuration complete"
    cluster_log "  Subsystem: $subsystem_nqn"
    cluster_log "  Transport: TCP"
    cluster_log "  Address: $target_ip:4420"

    # List discovered devices
    cluster_debug "Discovered NVMe devices:"
    nvme list 2>/dev/null | grep -E 'Node|nvme' || true

    return 0
}

#
# cluster_multipath_setup_initiator - Configure iSCSI initiators for multipath
#
# Logs in to multiple iSCSI targets (one per path) for multipath devices.
# Each target provides access to the same backing storage, creating redundant paths.
#
#   $1 - node_ip (IP address of this test node, for logging)
#   $2 - target_ip (IP address of node 0, the storage exporter)
#   $3 - cluster_id (for constructing target IQNs)
#   $4 - num_devices (number of multipath devices)
#   $5 - num_paths (number of paths per device, default 2)
#
# Returns:
#   0 on success, 1 on failure
#
cluster_multipath_setup_initiator() {
    local node_ip="$1"
    local target_ip="$2"
    local cluster_id="$3"
    local num_devices="$4"
    local num_paths="${5:-2}"

    cluster_log "Configuring multipath iSCSI initiators (devices=$num_devices, paths=$num_paths)"

    # Check if iscsiadm is available
    if ! command -v iscsiadm &>/dev/null; then
        cluster_error "iscsiadm command not found - is iscsi-initiator-utils package installed?"
        return 1
    fi

    # Ensure iscsid service is running
    if command -v systemctl &>/dev/null; then
        systemctl start iscsid || {
            cluster_error "Failed to start iscsid service"
            return 1
        }
        systemctl enable iscsid 2>/dev/null || true
    else
        service iscsid start || {
            cluster_error "Failed to start iscsid service"
            return 1
        }
    fi

    # Discover and login to each path target
    for path_num in $(seq 0 $((num_paths - 1))); do
        local target_iqn="iqn.2025-03.com.lvm:cluster.${cluster_id}.mp.path-${path_num}"

        cluster_log "Setting up multipath path $path_num: $target_iqn"

        # Discover targets
        cluster_debug "Discovering iSCSI target: $target_iqn"
        iscsiadm -m discovery -t sendtargets -p "$target_ip" 2>&1 | grep -q "$target_iqn" || {
            cluster_error "Failed to discover multipath target: $target_iqn"
            return 1
        }

        # Increase NOP-Out keepalive timeouts and replacement_timeout to
        # handle LIO target stalls from VM scheduling pressure (see comment
        # in cluster_setup_iscsi_initiator for details).
        iscsiadm -m node -T "$target_iqn" -p "$target_ip" \
            -o update -n node.conn[0].timeo.noop_out_interval -v 30 2>/dev/null || true
        iscsiadm -m node -T "$target_iqn" -p "$target_ip" \
            -o update -n node.conn[0].timeo.noop_out_timeout -v 30 2>/dev/null || true
        iscsiadm -m node -T "$target_iqn" -p "$target_ip" \
            -o update -n node.session.timeo.replacement_timeout -v 300 2>/dev/null || true

        # Login to target
        cluster_debug "Logging in to iSCSI target: $target_iqn"
        iscsiadm -m node -T "$target_iqn" -p "$target_ip" --login || {
            cluster_error "Failed to login to multipath target: $target_iqn"
            return 1
        }

        # Verify session is established
        if ! iscsiadm -m session 2>/dev/null | grep -q "$target_iqn"; then
            cluster_error "iSCSI session not established for: $target_iqn"
            return 1
        fi

        cluster_debug "Successfully logged in to path $path_num"
    done

    # Wait for all devices to appear
    cluster_log "Waiting for multipath devices to appear"
    sleep 3

    # Trigger multipath to scan for new devices
    if command -v multipath &>/dev/null; then
        multipath -r 2>/dev/null || true
        sleep 2
    fi

    cluster_log "Multipath iSCSI initiator configuration complete"
    cluster_log "  Logged in to $num_paths targets (paths)"
    cluster_log "  Expected multipath devices: $num_devices"

    return 0
}

#
# cluster_multipath_setup - Configure device-mapper multipath
#
# Sets up multipath configuration for redundant paths to storage.
# This is used when CLUSTER_NUM_MULTIPATH > 0.
#
#   $1 - node_ip (IP address of this test node)
#
# Returns:
#   0 on success, 1 on failure
#
cluster_multipath_setup() {
    local node_ip="$1"

    cluster_log "Configuring device-mapper multipath"

    # Check if multipath tools are available
    if ! command -v multipath &>/dev/null; then
        cluster_error "multipath command not found - is device-mapper-multipath package installed?"
        return 1
    fi

    # Create basic multipath configuration
    local multipath_conf="/etc/multipath.conf"
    local backup_conf="${multipath_conf}.backup-$(date +%s)"

    # Backup existing config if present
    if [ -f "$multipath_conf" ]; then
        cluster_debug "Backing up existing multipath config to: $backup_conf"
        cp "$multipath_conf" "$backup_conf"
    fi

    cluster_log "Creating multipath configuration"
    cat > "$multipath_conf" <<'EOF'
# LVM Cluster Testing Multipath Configuration
# Auto-generated by cluster-storage-importer.sh

defaults {
    user_friendly_names yes
    find_multipaths yes
    polling_interval 10
}

# Blacklist local devices
blacklist {
    devnode "^(ram|raw|loop|fd|md|dm-|sr|scd|st)[0-9]*"
    devnode "^hd[a-z]"
    devnode "^vd[a-z]"
}

# Allow iSCSI and NVMe devices
blacklist_exceptions {
    property "(SCSI_IDENT_|ID_WWN)"
}
EOF

    # Configure SCSI persistent reservations to use file-based keys
    cluster_log "Configuring SCSI persistent reservation keys"
    if command -v mpathconf &>/dev/null; then
        mpathconf --enable --with_multipathd y --user_friendly_names y --find_multipaths y
        mpathconf --option reservation_key:file || {
            cluster_warn "Failed to set reservation_key to file (may not be supported)"
        }
    else
        cluster_warn "mpathconf command not found - skipping reservation_key configuration"
    fi

    # Reload multipath configuration
    cluster_log "Reloading multipath configuration"
    if command -v systemctl &>/dev/null; then
        systemctl restart multipathd || {
            cluster_error "Failed to restart multipathd service"
            return 1
        }
        systemctl enable multipathd 2>/dev/null || true
    else
        service multipathd restart || {
            cluster_error "Failed to restart multipathd service"
            return 1
        }
    fi

    # Wait for multipath to scan devices
    sleep 2

    # Reconfigure multipath devices
    multipath -r || {
        cluster_warn "Failed to reconfigure multipath devices"
    }

    cluster_log "Multipath configuration complete"

    # Show multipath status
    cluster_debug "Multipath device status:"
    multipath -ll 2>/dev/null || true

    return 0
}

#
# cluster_storage_discover_devices - Discover imported storage devices
#
# Scans for devices that were imported via iSCSI or NVMe-oF and
# lists them for use in tests.
#
# Arguments:
#   $1 - num_scsi (number of expected iSCSI devices, 0 = none)
#   $2 - num_nvme (number of expected NVMe devices, 0 = none)
#   $3 - num_multipath (number of expected multipath devices, 0 = none)
#
# Outputs:
#   List of device paths (one per line) to stdout
#
# Returns:
#   0 on success, 1 on failure
#
cluster_storage_discover_devices() {
    local expected_scsi="${1:-0}"
    local expected_nvme="${2:-0}"
    local expected_multipath="${3:-0}"

    cluster_log "Discovering imported storage devices (SCSI=$expected_scsi, NVMe=$expected_nvme, multipath=$expected_multipath)"

    # Rescan SCSI bus if iSCSI or multipath is configured
    if [ $expected_scsi -gt 0 ] || [ $expected_multipath -gt 0 ]; then
        cluster_debug "Rescanning SCSI bus"
        if [ -f /usr/bin/rescan-scsi-bus.sh ]; then
            /usr/bin/rescan-scsi-bus.sh -a >/dev/null 2>&1 || true
        elif command -v iscsiadm &>/dev/null; then
            iscsiadm -m session --rescan >/dev/null 2>&1 || true
        fi
    fi

    # Rescan NVMe devices if NVMe is configured
    if [ $expected_nvme -gt 0 ]; then
        cluster_debug "Rescanning NVMe devices"
        if command -v nvme &>/dev/null; then
            # Force kernel to rescan namespaces
            for ctrl in /sys/class/nvme/nvme*/; do
                [ -d "$ctrl" ] || continue
                echo 1 > "${ctrl}/rescan_controller" 2>/dev/null || true
            done
        fi
    fi

    # Trigger multipath to scan if configured
    if [ $expected_multipath -gt 0 ]; then
        cluster_debug "Triggering multipath rescan"
        if command -v multipath &>/dev/null; then
            multipath -r 2>/dev/null || true
        fi
    fi

    # Wait for udev to settle
    if command -v udevadm &>/dev/null; then
        cluster_debug "Waiting for udev to settle"
        udevadm settle --timeout=10 2>/dev/null || true
    fi

    local devices=()

    # Discover iSCSI devices (if configured)
    # Note: If multipath is enabled, we need to filter out SCSI devices that are multipath slaves
    if [ $expected_scsi -gt 0 ]; then
        cluster_debug "Discovering iSCSI devices (looking for LIO-ORG vendor)"

        # Get all LIO-ORG SCSI devices
        local scsi_devices=()
        while IFS= read -r dev; do
            if [ -n "$dev" ] && [ -b "/dev/$dev" ]; then
                scsi_devices+=("$dev")
            fi
        done < <(lsblk -d -n -o NAME,VENDOR 2>/dev/null | grep -i 'LIO-ORG' | awk '{print $1}' || true)

        # If multipath is enabled, filter out devices that are multipath slaves
        if [ $expected_multipath -gt 0 ] && [ ${#scsi_devices[@]} -gt 0 ]; then
            cluster_debug "Filtering out multipath slave devices from SCSI device list"

            # Get list of multipath slave devices (base names like sdb, sdc)
            local mpath_slaves=()
            while IFS= read -r slave; do
                [ -n "$slave" ] && mpath_slaves+=("$slave")
            done < <(multipath -ll 2>/dev/null | grep -oP '(?<=`- )[0-9]+:[0-9]+:[0-9]+:[0-9]+\s+\K\w+' || true)

            # Add only non-slave SCSI devices
            for dev in "${scsi_devices[@]}"; do
                local is_slave=0
                for slave in "${mpath_slaves[@]}"; do
                    if [ "$dev" = "$slave" ]; then
                        is_slave=1
                        cluster_debug "  Filtering out $dev (multipath slave)"
                        break
                    fi
                done
                if [ $is_slave -eq 0 ]; then
                    devices+=("/dev/$dev")
                fi
            done
        else
            # No multipath or no SCSI devices, add all SCSI devices
            for dev in "${scsi_devices[@]}"; do
                devices+=("/dev/$dev")
            done
        fi
    fi

    # Discover NVMe devices (if configured)
    if [ $expected_nvme -gt 0 ]; then
        cluster_debug "Discovering NVMe devices"
        while IFS= read -r dev; do
            if [ -n "$dev" ] && [ -b "$dev" ]; then
                devices+=("$dev")
            fi
        done < <(nvme list 2>/dev/null | grep -oP '/dev/nvme[0-9]+n[0-9]+' || true)
    fi

    # Discover multipath devices (if configured)
    if [ $expected_multipath -gt 0 ]; then
        cluster_debug "Discovering multipath devices"
        while IFS= read -r mpath_dev; do
            if [ -n "$mpath_dev" ] && [ -b "/dev/mapper/$mpath_dev" ]; then
                devices+=("/dev/mapper/$mpath_dev")
            fi
        done < <(multipath -ll 2>/dev/null | grep -oP '^[a-z0-9]+(?= \()' || true)
    fi

    # Output discovered devices
    if [ ${#devices[@]} -eq 0 ]; then
        cluster_warn "No storage devices discovered"
        return 1
    fi

    # Validate count matches expectation
    local expected_total=$((expected_scsi + expected_nvme + expected_multipath))
    if [ $expected_total -gt 0 ] && [ ${#devices[@]} -ne $expected_total ]; then
        cluster_warn "Expected $expected_total devices (SCSI: $expected_scsi, NVMe: $expected_nvme, multipath: $expected_multipath), found ${#devices[@]}"
    fi

    cluster_log "Discovered ${#devices[@]} storage device(s):"
    for dev in "${devices[@]}"; do
        cluster_log "  - $dev"
        echo "$dev"
    done

    return 0
}

#
# cluster_storage_import_disconnect - Disconnect from imported storage
#
# Disconnects iSCSI sessions and NVMe-oF connections.
# Used during cluster cleanup.
#
# Arguments:
#   $1 - num_scsi (number of expected iSCSI devices, 0 = none)
#   $2 - num_nvme (number of expected NVMe devices, 0 = none)
# This function should be called via SSH on test nodes (1..N).
#
#   $1 - cluster_id
#   $2 - node_num (this node's number: 1, 2, 3, ...)
#
# Environment variables required:
#   CLUSTER_NUM_SCSI - Number of iSCSI devices (0 to disable)
#   CLUSTER_NUM_NVME - Number of NVMe devices (0 to disable)
#   CLUSTER_NUM_MULTIPATH (0 or 1)
#   NODE0_IP (IP address of node 0, the storage exporter)
#   THIS_NODE_IP (IP address of this test node)
#
# Returns:
#   0 on success, 1 on failure
#
cluster_storage_import() {
    local cluster_id="$1"
    local node_num="$2"

    if [ -z "$cluster_id" ]; then
        cluster_error "cluster_id is required"
        return 1
    fi

    if [ -z "$node_num" ]; then
        cluster_error "node_num is required"
        return 1
    fi

    # Validate required environment variables
    if [ -z "${CLUSTER_NUM_SCSI:-}" ] && [ -z "${CLUSTER_NUM_NVME:-}" ] && [ -z "${CLUSTER_NUM_MULTIPATH:-}" ]; then
        cluster_error "At least one of CLUSTER_NUM_SCSI, CLUSTER_NUM_NVME, or CLUSTER_NUM_MULTIPATH must be set"
        return 1
    fi

    if [ -z "${NODE0_IP:-}" ]; then
        cluster_error "NODE0_IP not set"
        return 1
    fi

    if [ -z "${THIS_NODE_IP:-}" ]; then
        cluster_error "THIS_NODE_IP not set"
        return 1
    fi

    local num_scsi="${CLUSTER_NUM_SCSI:-0}"
    local num_nvme="${CLUSTER_NUM_NVME:-0}"
    local num_multipath="${CLUSTER_NUM_MULTIPATH:-0}"
    local target_ip="${NODE0_IP}"
    local node_ip="${THIS_NODE_IP}"

    # Construct target identifiers
    local target_iqn="iqn.2025-03.com.lvm:cluster.${cluster_id}"
    local subsystem_nqn="nqn.2025-03.lvm.cluster:${cluster_id}"

    cluster_log "=========================================="
    cluster_log "Storage Import Setup on Node $node_num"
    cluster_log "=========================================="
    cluster_log "Cluster ID: $cluster_id"
    cluster_log "iSCSI Devices: $num_scsi"
    cluster_log "NVMe Devices: $num_nvme"
    cluster_log "Multipath Devices: $num_multipath"
    cluster_log "Node IP: $node_ip"
    cluster_log "Target IP (Node 0): $target_ip"
    cluster_log "=========================================="

    # Step 1: Configure iSCSI initiator if enabled
    if [ $num_scsi -gt 0 ]; then
        cluster_log "Importing iSCSI storage from node 0"
        cluster_iscsi_setup_initiator "$node_ip" "$target_ip" "$target_iqn" || {
            cluster_error "Failed to setup iSCSI initiator"
            return 1
        }
    else
        cluster_log "Skipping iSCSI import (CLUSTER_NUM_SCSI=0)"
    fi

    # Step 2: Configure NVMe initiator if enabled
    if [ $num_nvme -gt 0 ]; then
        cluster_log "Importing NVMe storage from node 0"
        cluster_nvme_setup_initiator "$node_ip" "$target_ip" "$subsystem_nqn" || {
            cluster_error "Failed to setup NVMe-oF initiator"
            return 1
        }
    else
        cluster_log "Skipping NVMe import (CLUSTER_NUM_NVME=0)"
    fi

    # Step 3: Configure multipath if enabled
    if [ $num_multipath -gt 0 ]; then
        cluster_log "Importing multipath storage from node 0"
        local num_paths="${CLUSTER_MULTIPATH_PATHS:-2}"

        # Setup multipathd before logging in to targets
        cluster_multipath_setup "$node_ip" || {
            cluster_error "Failed to setup multipathd"
            return 1
        }

        # Login to all multipath targets
        cluster_multipath_setup_initiator "$node_ip" "$target_ip" "$cluster_id" "$num_multipath" "$num_paths" || {
            cluster_error "Failed to setup multipath iSCSI initiators"
            return 1
        }
    else
        cluster_log "Skipping multipath import (CLUSTER_NUM_MULTIPATH=0)"
    fi

    # Step 4: Discover imported devices
    cluster_storage_discover_devices "$num_scsi" "$num_nvme" "$num_multipath" || {
        cluster_error "Failed to discover storage devices"
        return 1
    }

    cluster_log "=========================================="
    cluster_log "Storage import setup complete on node $node_num!"
    cluster_log "=========================================="

    return 0
}

# If script is executed directly (not sourced), run cluster_storage_import
if [ "${BASH_SOURCE[0]}" = "${0}" ]; then
    if [ $# -lt 2 ]; then
        echo "Usage: $0 <cluster_id> <node_num>" >&2
        echo "" >&2
        echo "Required environment variables:" >&2
        echo "  CLUSTER_NUM_SCSI - Number of iSCSI devices (0 to disable)" >&2
        echo "  CLUSTER_NUM_NVME - Number of NVMe devices (0 to disable)" >&2
        echo "  CLUSTER_NUM_MULTIPATH - 0 or 1" >&2
        echo "  NODE0_IP - IP address of node 0 (storage exporter)" >&2
        echo "  THIS_NODE_IP - IP address of this test node" >&2
        exit 1
    fi

    cluster_storage_import "$1" "$2"
fi
