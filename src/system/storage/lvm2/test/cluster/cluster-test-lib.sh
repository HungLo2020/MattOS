#!/bin/bash
#
# cluster-test-lib.sh - Core utility functions for LVM cluster testing
#
# Provides:
# - Logging utilities (cluster_log, cluster_warn, cluster_die)
# - Configuration loading and validation (cluster_load_config)
# - Cluster ID generation (cluster_generate_id)
# - State persistence (cluster_state_save, cluster_state_load)
#

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Paths and libvirt connection are set by cluster_init_privileges()
CLUSTER_STATE_DIR="${CLUSTER_STATE_DIR:-}"
CLUSTER_IMAGE_DIR="${CLUSTER_IMAGE_DIR:-}"
CLUSTER_PRIVILEGE_MODE="${CLUSTER_PRIVILEGE_MODE:-}"
LIBVIRT_DEFAULT_URI="${LIBVIRT_DEFAULT_URI:-}"

#
# Logging functions
#

cluster_log() {
    echo -e "${GREEN}[INFO]${NC} $*" >&2
}

cluster_warn() {
    echo -e "${YELLOW}[WARN]${NC} $*" >&2
}

cluster_error() {
    echo -e "${RED}[ERROR]${NC} $*" >&2
}

cluster_die() {
    cluster_error "$@"
    exit 1
}

cluster_debug() {
    if [ "${CLUSTER_DEBUG:-0}" = "1" ]; then
        echo -e "${BLUE}[DEBUG]${NC} $*" >&2
    fi
}

#
# Configuration management
#

cluster_load_config() {
    local config_file="$1"

    if [ -z "$config_file" ]; then
        cluster_die "No configuration file specified"
    fi

    if [ ! -f "$config_file" ]; then
        cluster_die "Configuration file not found: $config_file"
    fi

    # Get the directory where this script lives to find default config
    local script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    local default_config="$script_dir/configs/default-cluster.conf"

    # Reset arrays that configs populate with += to prevent accumulation
    SANLOCK_CONF_SETTINGS=()

    # Load default config first (unless we're already loading it)
    if [ "$config_file" != "$default_config" ] && [ -f "$default_config" ]; then
        cluster_debug "Loading default configuration from: $default_config"
        # shellcheck disable=SC1090
        source "$default_config" || cluster_die "Failed to load default config: $default_config"
    fi

    cluster_log "Loading configuration from: $config_file"

    # Source the user config file (overwrites defaults)
    # shellcheck disable=SC1090
    source "$config_file" || cluster_die "Failed to load config file: $config_file"

    # Validate required configuration
    cluster_validate_config

    # Extract sanlock io_timeout from SANLOCK_CONF_SETTINGS for test scripts
    CLUSTER_SANLOCK_IO_TIMEOUT=10
    local _setting
    for _setting in "${SANLOCK_CONF_SETTINGS[@]}"; do
        case "$_setting" in
            io_timeout=*) CLUSTER_SANLOCK_IO_TIMEOUT="${_setting#io_timeout=}" ;;
        esac
    done
    export CLUSTER_SANLOCK_IO_TIMEOUT

    cluster_debug "Configuration loaded successfully"
}

cluster_validate_config() {
    local errors=0

    # Validate CLUSTER_NUM_NODES
    if [ -z "${CLUSTER_NUM_NODES:-}" ]; then
        cluster_error "CLUSTER_NUM_NODES is not set"
        errors=$((errors + 1))
    elif [ "$CLUSTER_NUM_NODES" -lt 1 ]; then
        cluster_error "CLUSTER_NUM_NODES must be at least 1 (got: $CLUSTER_NUM_NODES)"
        errors=$((errors + 1))
    fi

    # Validate CLUSTER_NODE_MEMORY
    if [ -z "${CLUSTER_NODE_MEMORY:-}" ]; then
        cluster_error "CLUSTER_NODE_MEMORY is not set"
        errors=$((errors + 1))
    fi

    # Validate CLUSTER_NODE_VCPUS
    if [ -z "${CLUSTER_NODE_VCPUS:-}" ]; then
        cluster_error "CLUSTER_NODE_VCPUS is not set"
        errors=$((errors + 1))
    fi

    # Validate CLUSTER_NUM_SCSI
    if [ -z "${CLUSTER_NUM_SCSI:-}" ]; then
        cluster_error "CLUSTER_NUM_SCSI is not set"
        errors=$((errors + 1))
    elif ! [[ "$CLUSTER_NUM_SCSI" =~ ^[0-9]+$ ]]; then
        cluster_error "CLUSTER_NUM_SCSI must be a non-negative integer (got: $CLUSTER_NUM_SCSI)"
        errors=$((errors + 1))
    fi

    # Validate CLUSTER_NUM_NVME
    if [ -z "${CLUSTER_NUM_NVME:-}" ]; then
        cluster_error "CLUSTER_NUM_NVME is not set"
        errors=$((errors + 1))
    elif ! [[ "$CLUSTER_NUM_NVME" =~ ^[0-9]+$ ]]; then
        cluster_error "CLUSTER_NUM_NVME must be a non-negative integer (got: $CLUSTER_NUM_NVME)"
        errors=$((errors + 1))
    fi

    # At least one storage type must export devices
    if [ "${CLUSTER_NUM_SCSI:-0}" -eq 0 ] && [ "${CLUSTER_NUM_NVME:-0}" -eq 0 ] && [ "${CLUSTER_NUM_MULTIPATH:-0}" -eq 0 ]; then
        cluster_error "At least one of CLUSTER_NUM_SCSI, CLUSTER_NUM_NVME, or CLUSTER_NUM_MULTIPATH must be > 0"
        errors=$((errors + 1))
    fi

    # Validate iSCSI configuration (if iSCSI is enabled)
    if [ "${CLUSTER_NUM_SCSI:-0}" -gt 0 ]; then
        if [ -z "${CLUSTER_SCSI_SIZE:-}" ]; then
            cluster_error "CLUSTER_SCSI_SIZE is not set (required when CLUSTER_NUM_SCSI > 0)"
            errors=$((errors + 1))
        elif ! [[ "$CLUSTER_SCSI_SIZE" =~ ^[0-9]+$ ]] || [ "$CLUSTER_SCSI_SIZE" -lt 1 ]; then
            cluster_error "CLUSTER_SCSI_SIZE must be a positive integer (got: $CLUSTER_SCSI_SIZE)"
            errors=$((errors + 1))
        fi

        if [ -z "${CLUSTER_SCSI_SECTOR_SIZE:-}" ]; then
            cluster_error "CLUSTER_SCSI_SECTOR_SIZE is not set (required when CLUSTER_NUM_SCSI > 0)"
            errors=$((errors + 1))
        elif [[ ! "$CLUSTER_SCSI_SECTOR_SIZE" =~ ^(512|4096)$ ]]; then
            cluster_error "CLUSTER_SCSI_SECTOR_SIZE must be 512 or 4096 (got: $CLUSTER_SCSI_SECTOR_SIZE)"
            errors=$((errors + 1))
        fi

        if [ -z "${CLUSTER_SCSI_BACKING_TYPE:-}" ]; then
            cluster_error "CLUSTER_SCSI_BACKING_TYPE is not set (required when CLUSTER_NUM_SCSI > 0)"
            errors=$((errors + 1))
        elif [[ ! "$CLUSTER_SCSI_BACKING_TYPE" =~ ^(file_prealloc|file_sparse|loop_prealloc|loop_sparse|memory)$ ]]; then
            cluster_error "CLUSTER_SCSI_BACKING_TYPE must be 'file_prealloc', 'file_sparse', 'loop_prealloc', 'loop_sparse', or 'memory' (got: $CLUSTER_SCSI_BACKING_TYPE)"
            errors=$((errors + 1))
        fi

        if [ -n "${CLUSTER_SCSI_OPTIMAL_IO_SIZE:-}" ]; then
            if ! [[ "$CLUSTER_SCSI_OPTIMAL_IO_SIZE" =~ ^[0-9]+$ ]]; then
                cluster_error "CLUSTER_SCSI_OPTIMAL_IO_SIZE must be a non-negative integer in KB (got: $CLUSTER_SCSI_OPTIMAL_IO_SIZE)"
                errors=$((errors + 1))
            fi
        fi
    fi

    # Validate NVMe configuration (if NVMe is enabled)
    if [ "${CLUSTER_NUM_NVME:-0}" -gt 0 ]; then
        if [ -z "${CLUSTER_NVME_SIZE:-}" ]; then
            cluster_error "CLUSTER_NVME_SIZE is not set (required when CLUSTER_NUM_NVME > 0)"
            errors=$((errors + 1))
        elif ! [[ "$CLUSTER_NVME_SIZE" =~ ^[0-9]+$ ]] || [ "$CLUSTER_NVME_SIZE" -lt 1 ]; then
            cluster_error "CLUSTER_NVME_SIZE must be a positive integer (got: $CLUSTER_NVME_SIZE)"
            errors=$((errors + 1))
        fi

        if [ -z "${CLUSTER_NVME_SECTOR_SIZE:-}" ]; then
            cluster_error "CLUSTER_NVME_SECTOR_SIZE is not set (required when CLUSTER_NUM_NVME > 0)"
            errors=$((errors + 1))
        elif [[ ! "$CLUSTER_NVME_SECTOR_SIZE" =~ ^(512|4096)$ ]]; then
            cluster_error "CLUSTER_NVME_SECTOR_SIZE must be 512 or 4096 (got: $CLUSTER_NVME_SECTOR_SIZE)"
            errors=$((errors + 1))
        fi

        if [ -z "${CLUSTER_NVME_BACKING_TYPE:-}" ]; then
            cluster_error "CLUSTER_NVME_BACKING_TYPE is not set (required when CLUSTER_NUM_NVME > 0)"
            errors=$((errors + 1))
        elif [[ ! "$CLUSTER_NVME_BACKING_TYPE" =~ ^(loop_prealloc|loop_sparse|loop_memory)$ ]]; then
            cluster_error "CLUSTER_NVME_BACKING_TYPE must be 'loop_prealloc', 'loop_sparse', or 'loop_memory' (got: $CLUSTER_NVME_BACKING_TYPE)"
            errors=$((errors + 1))
        fi
    fi

    # Validate multipath configuration (if multipath is enabled)
    if [ "${CLUSTER_NUM_MULTIPATH:-0}" -gt 0 ]; then
        if [ -z "${CLUSTER_MULTIPATH_SIZE:-}" ]; then
            cluster_error "CLUSTER_MULTIPATH_SIZE is not set (required when CLUSTER_NUM_MULTIPATH > 0)"
            errors=$((errors + 1))
        elif ! [[ "$CLUSTER_MULTIPATH_SIZE" =~ ^[0-9]+$ ]] || [ "$CLUSTER_MULTIPATH_SIZE" -lt 1 ]; then
            cluster_error "CLUSTER_MULTIPATH_SIZE must be a positive integer (got: $CLUSTER_MULTIPATH_SIZE)"
            errors=$((errors + 1))
        fi

        if [ -z "${CLUSTER_MULTIPATH_SECTOR_SIZE:-}" ]; then
            cluster_error "CLUSTER_MULTIPATH_SECTOR_SIZE is not set (required when CLUSTER_NUM_MULTIPATH > 0)"
            errors=$((errors + 1))
        elif [[ ! "$CLUSTER_MULTIPATH_SECTOR_SIZE" =~ ^(512|4096)$ ]]; then
            cluster_error "CLUSTER_MULTIPATH_SECTOR_SIZE must be 512 or 4096 (got: $CLUSTER_MULTIPATH_SECTOR_SIZE)"
            errors=$((errors + 1))
        fi

        if [ -z "${CLUSTER_MULTIPATH_BACKING_TYPE:-}" ]; then
            cluster_error "CLUSTER_MULTIPATH_BACKING_TYPE is not set (required when CLUSTER_NUM_MULTIPATH > 0)"
            errors=$((errors + 1))
        elif [[ ! "$CLUSTER_MULTIPATH_BACKING_TYPE" =~ ^(file_prealloc|file_sparse|loop_prealloc|loop_sparse|memory)$ ]]; then
            cluster_error "CLUSTER_MULTIPATH_BACKING_TYPE must be 'file_prealloc', 'file_sparse', 'loop_prealloc', 'loop_sparse', or 'memory' (got: $CLUSTER_MULTIPATH_BACKING_TYPE)"
            errors=$((errors + 1))
        fi

        if [ -n "${CLUSTER_MULTIPATH_OPTIMAL_IO_SIZE:-}" ]; then
            if ! [[ "$CLUSTER_MULTIPATH_OPTIMAL_IO_SIZE" =~ ^[0-9]+$ ]]; then
                cluster_error "CLUSTER_MULTIPATH_OPTIMAL_IO_SIZE must be a non-negative integer in KB (got: $CLUSTER_MULTIPATH_OPTIMAL_IO_SIZE)"
                errors=$((errors + 1))
            fi
        fi

        if [ -z "${CLUSTER_MULTIPATH_PATHS:-}" ]; then
            cluster_error "CLUSTER_MULTIPATH_PATHS is not set (required when CLUSTER_NUM_MULTIPATH > 0)"
            errors=$((errors + 1))
        elif ! [[ "$CLUSTER_MULTIPATH_PATHS" =~ ^[0-9]+$ ]] || [ "$CLUSTER_MULTIPATH_PATHS" -lt 2 ]; then
            cluster_error "CLUSTER_MULTIPATH_PATHS must be at least 2 (got: $CLUSTER_MULTIPATH_PATHS)"
            errors=$((errors + 1))
        fi
    fi

    # Calculate total devices for logging
    local total_devices=$((CLUSTER_NUM_SCSI + CLUSTER_NUM_NVME + CLUSTER_NUM_MULTIPATH))
    cluster_debug "Total storage devices: $total_devices (SCSI: $CLUSTER_NUM_SCSI, NVMe: $CLUSTER_NUM_NVME, Multipath: $CLUSTER_NUM_MULTIPATH)"

    # Validate CLUSTER_LOCK_TYPE
    if [ -z "${CLUSTER_LOCK_TYPE:-}" ]; then
        cluster_error "CLUSTER_LOCK_TYPE is not set"
        errors=$((errors + 1))
    elif [[ ! "$CLUSTER_LOCK_TYPE" =~ ^(sanlock|dlm|none)$ ]]; then
        cluster_error "CLUSTER_LOCK_TYPE must be 'sanlock', 'dlm', or 'none' (got: $CLUSTER_LOCK_TYPE)"
        errors=$((errors + 1))
    fi

    # Validate source deployment settings
    if [ -n "${LVM_SOURCE_DIR:-}" ] && [ ! -d "$LVM_SOURCE_DIR" ]; then
        cluster_error "LVM_SOURCE_DIR does not exist: $LVM_SOURCE_DIR"
        errors=$((errors + 1))
    fi

    if [ -n "${SANLOCK_SOURCE_DIR:-}" ]; then
        if [ "$CLUSTER_LOCK_TYPE" != "sanlock" ] && [ "$CLUSTER_LOCK_TYPE" != "none" ]; then
            cluster_error "SANLOCK_SOURCE_DIR requires CLUSTER_LOCK_TYPE=sanlock or CLUSTER_LOCK_TYPE=none"
            errors=$((errors + 1))
        fi

        if [ ! -d "$SANLOCK_SOURCE_DIR" ]; then
            cluster_error "SANLOCK_SOURCE_DIR does not exist: $SANLOCK_SOURCE_DIR"
            errors=$((errors + 1))
        fi
    fi

    if [ $errors -gt 0 ]; then
        cluster_die "Configuration validation failed with $errors error(s)"
    fi

    return 0
}

#
# Cluster ID management
#

cluster_generate_id() {
    local config_name="${1:-}"
    local timestamp=$(date +%m%d%H%M%S)

    # VM names become hostnames: ${cluster_id}-nodeN
    # Linux hostnames are limited to 63 characters.
    # Fixed parts: "lvmtest-" (8) + "-" (1) + timestamp (10) + "-nodeNN" (7) = 26
    # Truncate config_name to 37 characters to stay within the limit.
    if [ -n "$config_name" ]; then
        config_name="${config_name:0:37}"
        echo "lvmtest-${config_name}-${timestamp}"
    else
        echo "lvmtest-${timestamp}"
    fi
}

cluster_validate_cluster_id() {
    local cluster_id="$1"
    local check_exists="${2:-1}"  # Default: check for existing cluster (for create)
    local normalized_id=""

    if [ -z "$cluster_id" ]; then
        cluster_error "Cluster ID cannot be empty"
        return 1
    fi

    # Auto-add "lvmtest-" prefix if not present
    if [[ ! "$cluster_id" =~ ^lvmtest- ]]; then
        normalized_id="lvmtest-${cluster_id}"
        cluster_debug "Auto-adding prefix: $cluster_id -> $normalized_id"
    else
        normalized_id="$cluster_id"
    fi

    # Check for invalid characters (allow alphanumeric, dash, underscore)
    if [[ ! "$normalized_id" =~ ^[a-zA-Z0-9_-]+$ ]]; then
        cluster_error "Invalid cluster ID format: $normalized_id"
        cluster_error "Cluster ID can only contain letters, numbers, dashes, and underscores"
        return 1
    fi

    # Only check for uniqueness when creating a new cluster
    if [ "$check_exists" = "1" ]; then
        # Check for uniqueness (no existing state file)
        local state_file=$(cluster_state_get_file "$normalized_id")
        if [ -f "$state_file" ]; then
            cluster_error "Cluster ID already exists: $normalized_id"
            cluster_error "State file found: $state_file"
            cluster_error ""
            cluster_error "Choose a different cluster ID or destroy the existing cluster with:"
            cluster_error "  lvmtest -i $normalized_id destroy"
            return 1
        fi

        # Check for existing VMs with this ID
        local existing_vms=$(cluster_virsh list --all --name 2>/dev/null | grep "^${normalized_id}-" || true)
        if [ -n "$existing_vms" ]; then
            cluster_error "VMs already exist with cluster ID: $normalized_id"
            cluster_error "Found VMs:"
            echo "$existing_vms" | while read -r vm_name; do
                cluster_error "  $vm_name"
            done
            cluster_error ""
            cluster_error "Please destroy these VMs first or choose a different cluster ID"
            return 1
        fi
    fi

    # Return the normalized ID via stdout
    echo "$normalized_id"
    return 0
}

#
# State persistence
#

cluster_state_get_file() {
    local cluster_id="$1"
    echo "${CLUSTER_STATE_DIR}/${cluster_id}.state"
}

cluster_state_save() {
    local cluster_id="$1"

    if [ -z "$cluster_id" ]; then
        cluster_die "cluster_state_save: cluster_id is required"
    fi

    # Create state directory if it doesn't exist
    mkdir -p "$CLUSTER_STATE_DIR" || cluster_die "Failed to create state directory: $CLUSTER_STATE_DIR"
    chmod 777 "$CLUSTER_STATE_DIR" 2>/dev/null || true

    local state_file=$(cluster_state_get_file "$cluster_id")

    cluster_debug "Saving cluster state to: $state_file"

    # Save all CLUSTER_* variables to state file
    {
        echo "# Cluster state file"
        echo "# Generated: $(date)"
        echo "CLUSTER_ID=\"$cluster_id\""

        # Explicitly save important configuration variables
        # (using explicit list to ensure they're saved even if not exported)
        [ -n "${CLUSTER_NUM_NODES:-}" ] && echo "CLUSTER_NUM_NODES=$CLUSTER_NUM_NODES"
        [ -n "${CLUSTER_NUM_SCSI:-}" ] && echo "CLUSTER_NUM_SCSI=$CLUSTER_NUM_SCSI"
        [ -n "${CLUSTER_NUM_NVME:-}" ] && echo "CLUSTER_NUM_NVME=$CLUSTER_NUM_NVME"
        [ -n "${CLUSTER_NUM_MULTIPATH:-}" ] && echo "CLUSTER_NUM_MULTIPATH=$CLUSTER_NUM_MULTIPATH"

        # iSCSI configuration (if enabled)
        if [ "${CLUSTER_NUM_SCSI:-0}" -gt 0 ]; then
            [ -n "${CLUSTER_SCSI_SIZE:-}" ] && echo "CLUSTER_SCSI_SIZE=$CLUSTER_SCSI_SIZE"
            [ -n "${CLUSTER_SCSI_SECTOR_SIZE:-}" ] && echo "CLUSTER_SCSI_SECTOR_SIZE=$CLUSTER_SCSI_SECTOR_SIZE"
            [ -n "${CLUSTER_SCSI_BACKING_TYPE:-}" ] && echo "CLUSTER_SCSI_BACKING_TYPE=\"$CLUSTER_SCSI_BACKING_TYPE\""
        fi

        # NVMe configuration (if enabled)
        if [ "${CLUSTER_NUM_NVME:-0}" -gt 0 ]; then
            [ -n "${CLUSTER_NVME_SIZE:-}" ] && echo "CLUSTER_NVME_SIZE=$CLUSTER_NVME_SIZE"
            [ -n "${CLUSTER_NVME_SECTOR_SIZE:-}" ] && echo "CLUSTER_NVME_SECTOR_SIZE=$CLUSTER_NVME_SECTOR_SIZE"
            [ -n "${CLUSTER_NVME_BACKING_TYPE:-}" ] && echo "CLUSTER_NVME_BACKING_TYPE=\"$CLUSTER_NVME_BACKING_TYPE\""
        fi

        # Multipath configuration (if enabled)
        if [ "${CLUSTER_NUM_MULTIPATH:-0}" -gt 0 ]; then
            [ -n "${CLUSTER_MULTIPATH_PATHS:-}" ] && echo "CLUSTER_MULTIPATH_PATHS=$CLUSTER_MULTIPATH_PATHS"
            [ -n "${CLUSTER_MULTIPATH_SIZE:-}" ] && echo "CLUSTER_MULTIPATH_SIZE=$CLUSTER_MULTIPATH_SIZE"
            [ -n "${CLUSTER_MULTIPATH_SECTOR_SIZE:-}" ] && echo "CLUSTER_MULTIPATH_SECTOR_SIZE=$CLUSTER_MULTIPATH_SECTOR_SIZE"
            [ -n "${CLUSTER_MULTIPATH_BACKING_TYPE:-}" ] && echo "CLUSTER_MULTIPATH_BACKING_TYPE=\"$CLUSTER_MULTIPATH_BACKING_TYPE\""
        fi

        [ -n "${CLUSTER_LOCK_TYPE:-}" ] && echo "CLUSTER_LOCK_TYPE=\"$CLUSTER_LOCK_TYPE\""
        [ -n "${CLUSTER_SSH_KEY_DIR:-}" ] && echo "CLUSTER_SSH_KEY_DIR=\"$CLUSTER_SSH_KEY_DIR\""
        [ -n "${CLUSTER_SSH_USER:-}" ] && echo "CLUSTER_SSH_USER=\"$CLUSTER_SSH_USER\""
        [ -n "${LVM_SOURCE_DIR:-}" ] && echo "LVM_SOURCE_DIR=\"$LVM_SOURCE_DIR\""

        # Also save any other CLUSTER_* variables that are exported
        env | grep '^CLUSTER_' | while IFS='=' read -r key value; do
            # Skip ones we already saved explicitly
            case "$key" in
                CLUSTER_ID|CLUSTER_NUM_NODES|CLUSTER_NUM_SCSI|CLUSTER_NUM_NVME|CLUSTER_NUM_MULTIPATH|\
                CLUSTER_SCSI_SIZE|CLUSTER_SCSI_SECTOR_SIZE|CLUSTER_SCSI_BACKING_TYPE|\
                CLUSTER_NVME_SIZE|CLUSTER_NVME_SECTOR_SIZE|CLUSTER_NVME_BACKING_TYPE|\
                CLUSTER_MULTIPATH_PATHS|CLUSTER_MULTIPATH_SIZE|CLUSTER_MULTIPATH_SECTOR_SIZE|CLUSTER_MULTIPATH_BACKING_TYPE|\
                CLUSTER_LOCK_TYPE|CLUSTER_SSH_KEY_DIR|CLUSTER_SSH_USER|LVM_SOURCE_DIR)
                    continue
                    ;;
            esac
            printf '%s=%q\n' "$key" "$value"
        done

        # Export SANLOCK_* variables
        env | grep '^SANLOCK_' | while IFS='=' read -r key value; do
            case "$key" in
                SANLOCK_CONF_SETTINGS|SANLOCK_CONF_EOF*)
                    continue
                    ;;
            esac
            printf '%s=%q\n' "$key" "$value"
        done

        # Save SANLOCK_CONF_SETTINGS array if it exists
        if [ "${#SANLOCK_CONF_SETTINGS[@]}" -gt 0 ]; then
            echo 'SANLOCK_CONF_SETTINGS=()'
            for setting in "${SANLOCK_CONF_SETTINGS[@]}"; do
                printf 'SANLOCK_CONF_SETTINGS+=(%q)\n' "$setting"
            done
        fi

        # Save node IPs if they exist
        if [ "${#CLUSTER_NODE_IPS[@]}" -gt 0 ]; then
            echo "CLUSTER_NODE_IPS=(${CLUSTER_NODE_IPS[*]})"
        fi

        # Initialize paused state tracking
        echo "CLUSTER_IS_PAUSED=${CLUSTER_IS_PAUSED:-0}"
        echo "CLUSTER_PAUSED_TIMESTAMP=\"${CLUSTER_PAUSED_TIMESTAMP:-}\""

        # Initialize snapshot tracking (supports multiple named snapshots)
        if [ "${#CLUSTER_SNAPSHOTS[@]}" -gt 0 ]; then
            printf 'CLUSTER_SNAPSHOTS=(%s)\n' "${CLUSTER_SNAPSHOTS[*]}"
        else
            echo "CLUSTER_SNAPSHOTS=()"
        fi

        # Save snapshot metadata (timestamps and paused states)
        for snap in "${CLUSTER_SNAPSHOTS[@]}"; do
            local ts="${CLUSTER_SNAPSHOT_TIMESTAMPS[$snap]:-}"
            local paused="${CLUSTER_SNAPSHOT_PAUSED_STATES[$snap]:-0}"
            [ -n "$ts" ] && printf 'CLUSTER_SNAPSHOT_TIMESTAMPS[%q]=%q\n' "$snap" "$ts"
            printf 'CLUSTER_SNAPSHOT_PAUSED_STATES[%q]=%q\n' "$snap" "$paused"
        done

        echo "CLUSTER_IMAGE_DIR=\"${CLUSTER_IMAGE_DIR:-$(cluster_get_image_dir)}\""
        echo "CLUSTER_SNAPSHOT_DIR=\"${CLUSTER_SNAPSHOT_DIR:-$(cluster_get_image_dir)}\""
        echo "LIBVIRT_DEFAULT_URI=\"${LIBVIRT_DEFAULT_URI:-qemu:///system}\""
        echo "CLUSTER_PRIVILEGE_MODE=\"${CLUSTER_PRIVILEGE_MODE:-system}\""

    } > "$state_file" || cluster_die "Failed to write state file: $state_file"

    # Make state file readable and writable by everyone
    chmod 666 "$state_file" 2>/dev/null || true

    cluster_debug "State saved successfully"
}

cluster_state_load() {
    local cluster_id="$1"

    if [ -z "$cluster_id" ]; then
        cluster_die "cluster_state_load: cluster_id is required"
    fi

    local state_file=$(cluster_state_get_file "$cluster_id")

    if [ ! -f "$state_file" ]; then
        cluster_error "Cluster state file not found: $state_file"
        return 1
    fi

    cluster_debug "Loading cluster state from: $state_file"

    # Initialize associative arrays for snapshot metadata
    declare -gA CLUSTER_SNAPSHOT_TIMESTAMPS
    declare -gA CLUSTER_SNAPSHOT_PAUSED_STATES

    # Source the state file - errors will go to stderr
    # Caller can use 2>/dev/null to suppress error messages
    # shellcheck disable=SC1090
    if source "$state_file"; then
        cluster_debug "State loaded successfully for cluster: $cluster_id"

        # Migrate old format to new format if needed
        cluster_state_migrate_old_format "$cluster_id"

        # Reconnect to the libvirt URI used when the cluster was created
        if [ -n "${LIBVIRT_DEFAULT_URI:-}" ]; then
            export LIBVIRT_DEFAULT_URI
        fi
        if [ -n "${CLUSTER_IMAGE_DIR:-}" ]; then
            export CLUSTER_IMAGE_DIR
            export CLUSTER_SNAPSHOT_DIR="${CLUSTER_SNAPSHOT_DIR:-$CLUSTER_IMAGE_DIR}"
        fi
        if [ -n "${CLUSTER_PRIVILEGE_MODE:-}" ]; then
            export CLUSTER_PRIVILEGE_MODE
        fi

        return 0
    else
        # Don't print error message - let source error speak for itself
        # or let caller suppress with 2>/dev/null
        return 1
    fi
}

cluster_state_migrate_old_format() {
    local cluster_id="$1"
    local needs_migration=0

    # Migrate CLUSTER_IS_STOPPED to CLUSTER_IS_PAUSED
    if [ -n "${CLUSTER_IS_STOPPED:-}" ] && [ -z "${CLUSTER_IS_PAUSED:-}" ]; then
        CLUSTER_IS_PAUSED="$CLUSTER_IS_STOPPED"
        CLUSTER_PAUSED_TIMESTAMP="${CLUSTER_STOPPED_TIMESTAMP:-}"
        needs_migration=1
        cluster_debug "Migrated CLUSTER_IS_STOPPED to CLUSTER_IS_PAUSED"
    fi

    # Migrate old single snapshot format to new multi-snapshot format
    if [ -n "${CLUSTER_SNAPSHOT_NAME:-}" ] && [ "${#CLUSTER_SNAPSHOTS[@]}" -eq 0 ]; then
        CLUSTER_SNAPSHOTS=("${CLUSTER_SNAPSHOT_NAME}")
        CLUSTER_SNAPSHOT_TIMESTAMPS["${CLUSTER_SNAPSHOT_NAME}"]="${CLUSTER_SNAPSHOT_TIMESTAMP:-}"
        # Old snapshots default to "running" state (paused=0)
        CLUSTER_SNAPSHOT_PAUSED_STATES["${CLUSTER_SNAPSHOT_NAME}"]=0
        needs_migration=1
        cluster_debug "Migrated single snapshot to multi-snapshot format"
    fi

    # Save migrated state
    if [ "$needs_migration" -eq 1 ]; then
        cluster_debug "Saving migrated state for cluster: $cluster_id"
        cluster_state_save "$cluster_id"
    fi
}

cluster_state_delete() {
    local cluster_id="$1"

    if [ -z "$cluster_id" ]; then
        cluster_die "cluster_state_delete: cluster_id is required"
    fi

    local state_file=$(cluster_state_get_file "$cluster_id")

    if [ -f "$state_file" ]; then
        cluster_debug "Deleting state file: $state_file"
        rm -f "$state_file" || cluster_warn "Failed to delete state file: $state_file"
    fi
}

cluster_state_list() {
    if [ ! -d "$CLUSTER_STATE_DIR" ]; then
        return
    fi

    find "$CLUSTER_STATE_DIR" -name "*.state" -type f 2>/dev/null | while read -r state_file; do
        local cluster_id=$(basename "$state_file" .state)
        echo "$cluster_id"
    done
}

cluster_state_set_paused() {
    local cluster_id="$1"
    local is_paused="$2"

    if [ -z "$cluster_id" ]; then
        cluster_die "cluster_state_set_paused: cluster_id is required"
    fi

    if [ -z "$is_paused" ]; then
        cluster_die "cluster_state_set_paused: is_paused is required (0 or 1)"
    fi

    local state_file=$(cluster_state_get_file "$cluster_id")

    if [ ! -f "$state_file" ]; then
        cluster_die "Cluster state file not found: $state_file"
    fi

    # Update or add CLUSTER_IS_PAUSED variable in state file
    if grep -q "^CLUSTER_IS_PAUSED=" "$state_file" 2>/dev/null; then
        # Update existing value
        sed -i "s/^CLUSTER_IS_PAUSED=.*/CLUSTER_IS_PAUSED=$is_paused/" "$state_file"
    else
        # Add new value
        echo "CLUSTER_IS_PAUSED=$is_paused" >> "$state_file"
    fi

    # Update or add timestamp
    local timestamp=$(date +%s)
    if grep -q "^CLUSTER_PAUSED_TIMESTAMP=" "$state_file" 2>/dev/null; then
        if [ "$is_paused" -eq 1 ]; then
            sed -i "s/^CLUSTER_PAUSED_TIMESTAMP=.*/CLUSTER_PAUSED_TIMESTAMP=$timestamp/" "$state_file"
        else
            # Clear timestamp when resuming
            sed -i "s/^CLUSTER_PAUSED_TIMESTAMP=.*/CLUSTER_PAUSED_TIMESTAMP=\"\"/" "$state_file"
        fi
    else
        if [ "$is_paused" -eq 1 ]; then
            echo "CLUSTER_PAUSED_TIMESTAMP=$timestamp" >> "$state_file"
        else
            echo "CLUSTER_PAUSED_TIMESTAMP=\"\"" >> "$state_file"
        fi
    fi

    cluster_debug "Updated paused state: is_paused=$is_paused"
}

cluster_state_is_paused() {
    local cluster_id="$1"

    if [ -z "$cluster_id" ]; then
        cluster_die "cluster_state_is_paused: cluster_id is required"
    fi

    local state_file=$(cluster_state_get_file "$cluster_id")

    if [ ! -f "$state_file" ]; then
        cluster_die "Cluster state file not found: $state_file"
    fi

    # Load the state and check CLUSTER_IS_PAUSED
    local is_paused=$(grep "^CLUSTER_IS_PAUSED=" "$state_file" 2>/dev/null | cut -d= -f2)

    if [ "${is_paused:-0}" -eq 1 ]; then
        return 0  # Cluster is paused
    else
        return 1  # Cluster is running
    fi
}

cluster_state_add_snapshot() {
    local cluster_id="$1"
    local snapshot_name="$2"
    local timestamp="$3"
    local is_paused="${4:-0}"  # Default to running if not specified

    if [ -z "$cluster_id" ] || [ -z "$snapshot_name" ]; then
        cluster_die "cluster_state_add_snapshot: cluster_id and snapshot_name are required"
    fi

    # Add to CLUSTER_SNAPSHOTS array if not already present
    local found=0
    for snap in "${CLUSTER_SNAPSHOTS[@]}"; do
        if [ "$snap" = "$snapshot_name" ]; then
            found=1
            break
        fi
    done

    if [ "$found" -eq 0 ]; then
        CLUSTER_SNAPSHOTS+=("$snapshot_name")
    fi

    # Set snapshot metadata
    CLUSTER_SNAPSHOT_TIMESTAMPS["$snapshot_name"]="$timestamp"
    CLUSTER_SNAPSHOT_PAUSED_STATES["$snapshot_name"]="$is_paused"

    # Save updated state
    cluster_state_save "$cluster_id"

    cluster_debug "Added snapshot: name=$snapshot_name, timestamp=$timestamp, paused=$is_paused"
}

cluster_state_remove_snapshot() {
    local cluster_id="$1"
    local snapshot_name="$2"

    if [ -z "$cluster_id" ] || [ -z "$snapshot_name" ]; then
        cluster_die "cluster_state_remove_snapshot: cluster_id and snapshot_name are required"
    fi

    # Remove from CLUSTER_SNAPSHOTS array
    local new_snapshots=()
    for snap in "${CLUSTER_SNAPSHOTS[@]}"; do
        if [ "$snap" != "$snapshot_name" ]; then
            new_snapshots+=("$snap")
        fi
    done
    CLUSTER_SNAPSHOTS=("${new_snapshots[@]}")

    # Remove metadata
    unset "CLUSTER_SNAPSHOT_TIMESTAMPS[$snapshot_name]"
    unset "CLUSTER_SNAPSHOT_PAUSED_STATES[$snapshot_name]"

    # Save updated state
    cluster_state_save "$cluster_id"

    cluster_debug "Removed snapshot: $snapshot_name"
}

cluster_state_list_snapshots() {
    local cluster_id="$1"

    if [ -z "$cluster_id" ]; then
        cluster_die "cluster_state_list_snapshots: cluster_id is required"
    fi

    # Load state if not already loaded
    if [ -z "${CLUSTER_ID:-}" ] || [ "$CLUSTER_ID" != "$cluster_id" ]; then
        cluster_state_load "$cluster_id" || return 1
    fi

    # Print each snapshot name
    for snap in "${CLUSTER_SNAPSHOTS[@]}"; do
        echo "$snap"
    done
}

cluster_state_snapshot_exists() {
    local cluster_id="$1"
    local snapshot_name="$2"

    if [ -z "$cluster_id" ] || [ -z "$snapshot_name" ]; then
        cluster_die "cluster_state_snapshot_exists: cluster_id and snapshot_name are required"
    fi

    # Check if snapshot is in array
    for snap in "${CLUSTER_SNAPSHOTS[@]}"; do
        if [ "$snap" = "$snapshot_name" ]; then
            return 0  # Snapshot exists
        fi
    done

    return 1  # Snapshot not found
}

cluster_state_get_snapshot_paused_state() {
    local cluster_id="$1"
    local snapshot_name="$2"

    if [ -z "$cluster_id" ] || [ -z "$snapshot_name" ]; then
        cluster_die "cluster_state_get_snapshot_paused_state: cluster_id and snapshot_name are required"
    fi

    # Return paused state (default to 0/running if not set)
    echo "${CLUSTER_SNAPSHOT_PAUSED_STATES[$snapshot_name]:-0}"
}

#
# Helper functions
#

cluster_user_in_group() {
    local group="$1"
    id -nG 2>/dev/null | tr ' ' '\n' | grep -qx "$group"
}

cluster_get_image_dir() {
    echo "${CLUSTER_IMAGE_DIR:-/var/lib/libvirt/images}"
}

cluster_get_cloudinit_dir() {
    local vm_name="$1"
    echo "${CLUSTER_CLOUDINIT_DIR:-${TMPDIR:-/tmp}}/cloudinit-${vm_name}"
}

cluster_virsh() {
    virsh "$@"
}

cluster_virt_install() {
    virt-install --connect "${LIBVIRT_DEFAULT_URI:-qemu:///system}" "$@"
}

cluster_init_privileges() {
    if [ -z "${CLUSTER_PRIVILEGE_MODE:-}" ]; then
        if [ "$EUID" -eq 0 ] || cluster_user_in_group libvirt; then
            CLUSTER_PRIVILEGE_MODE=system
        else
            CLUSTER_PRIVILEGE_MODE=session
        fi
    fi

    case "$CLUSTER_PRIVILEGE_MODE" in
        system)
            export LIBVIRT_DEFAULT_URI="${LIBVIRT_DEFAULT_URI:-qemu:///system}"
            export CLUSTER_IMAGE_DIR="${CLUSTER_IMAGE_DIR:-/var/lib/libvirt/images}"
            export CLUSTER_STATE_DIR="${CLUSTER_STATE_DIR:-/var/tmp/lvm-cluster-state}"
            export CLUSTER_CLOUDINIT_DIR="${CLUSTER_CLOUDINIT_DIR:-/var/tmp}"
            ;;
        session)
            export LIBVIRT_DEFAULT_URI="${LIBVIRT_DEFAULT_URI:-qemu:///session}"
            export CLUSTER_IMAGE_DIR="${CLUSTER_IMAGE_DIR:-$HOME/.local/share/libvirt/images}"
            export CLUSTER_STATE_DIR="${CLUSTER_STATE_DIR:-$HOME/.cache/lvm-cluster-state}"
            export CLUSTER_CLOUDINIT_DIR="${CLUSTER_CLOUDINIT_DIR:-${TMPDIR:-/tmp}}"
            ;;
        *)
            cluster_die "Unknown CLUSTER_PRIVILEGE_MODE: $CLUSTER_PRIVILEGE_MODE"
            ;;
    esac

    export CLUSTER_SNAPSHOT_DIR="${CLUSTER_SNAPSHOT_DIR:-$CLUSTER_IMAGE_DIR}"
    export CLUSTER_PRIVILEGE_MODE
    cluster_debug "Privilege mode: $CLUSTER_PRIVILEGE_MODE ($LIBVIRT_DEFAULT_URI)"
}

cluster_check_host_prerequisites() {
    if [ ! -r /dev/kvm ]; then
        cluster_warn "No access to /dev/kvm — VMs will use slow software emulation."
        cluster_warn "Add user to kvm group for hardware acceleration: sudo usermod -aG kvm $USER"
    fi
}

cluster_session_prepare() {
    local disk_dir
    disk_dir=$(cluster_get_image_dir)

    mkdir -p "$disk_dir" "$CLUSTER_STATE_DIR" || \
        cluster_die "Failed to create image/state directories"

    local net_name="${CLUSTER_NETWORK_NAME:-default}"
    if ! cluster_virsh net-info "$net_name" &>/dev/null; then
        cluster_warn "Network '$net_name' not found on $LIBVIRT_DEFAULT_URI"
        cluster_warn "Define and start a user NAT network, or run: virsh net-start default"
    else
        if ! cluster_virsh net-info "$net_name" 2>/dev/null | grep -qi 'Active:.*yes'; then
            cluster_virsh net-start "$net_name" 2>/dev/null || \
                cluster_warn "Could not start network '$net_name'"
        fi
        cluster_virsh net-autostart "$net_name" 2>/dev/null || true
    fi

    if [ -n "${CLUSTER_NODE_OS_IMAGE:-}" ] && [ ! -r "${CLUSTER_NODE_OS_IMAGE}" ]; then
        cluster_die "OS image not readable: ${CLUSTER_NODE_OS_IMAGE}"
    fi

    cluster_check_host_prerequisites

    local avail_kb num_nodes disk_gb needed_gb needed_kb
    avail_kb=$(df -k "$disk_dir" 2>/dev/null | awk 'NR==2 {print $4}')
    num_nodes="${CLUSTER_NUM_NODES:-3}"
    disk_gb="${CLUSTER_NODE_DISK_SIZE:-20}"
    needed_gb=$(( (num_nodes + 2) * disk_gb ))
    needed_kb=$(( needed_gb * 1024 * 1024 ))
    if [ -n "$avail_kb" ] && [ "$avail_kb" -lt "$needed_kb" ]; then
        cluster_warn "Low disk space in $disk_dir (~${needed_gb}GB recommended for this cluster)"
    fi
}

cluster_check_deps() {
    local deps=(virsh virt-install qemu-img ssh-keygen)
    local missing=()

    for dep in "${deps[@]}"; do
        if ! command -v "$dep" &>/dev/null; then
            missing+=("$dep")
        fi
    done

    if [ ${#missing[@]} -gt 0 ]; then
        cluster_error "Missing required dependencies: ${missing[*]}"
        cluster_die "Please install the required packages"
    fi

    if ! command -v genisoimage &>/dev/null && ! command -v mkisofs &>/dev/null; then
        cluster_error "Missing required dependency: genisoimage or mkisofs"
        cluster_die "Please install the required packages"
    fi

    if ! cluster_virsh list &>/dev/null; then
        cluster_die "Cannot connect to libvirt at ${LIBVIRT_DEFAULT_URI}"
    fi
}

# Backward compatibility alias
cluster_check_root() {
    cluster_init_privileges
}

#
# Utility functions
#

cluster_wait_with_timeout() {
    local timeout="$1"
    local interval="${2:-5}"
    local check_cmd="$3"
    local description="${4:-condition}"

    local elapsed=0

    cluster_log "Waiting for $description (timeout: ${timeout}s)"

    while [ $elapsed -lt "$timeout" ]; do
        if eval "$check_cmd"; then
            cluster_log "$description ready after ${elapsed}s"
            return 0
        fi

        sleep "$interval"
        elapsed=$((elapsed + interval))

        if [ $((elapsed % 30)) -eq 0 ]; then
            cluster_debug "Still waiting... (${elapsed}s/${timeout}s)"
        fi
    done

    cluster_error "Timeout waiting for $description after ${timeout}s"
    return 1
}

#
# Group file parsing
#

cluster_parse_group_file() {
    local group_file="$1"

    if [ -z "$group_file" ]; then
        cluster_error "cluster_parse_group_file: group_file is required"
        return 1
    fi

    if [ ! -f "$group_file" ]; then
        cluster_error "Group file not found: $group_file"
        return 1
    fi

    # Return arrays (use global variables for simplicity)
    local config_name=""
    local -a test_files=()
    local config_seen=0
    local line_num=0

    while IFS= read -r line; do
        line_num=$((line_num + 1))

        # Strip leading/trailing whitespace
        line=$(echo "$line" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')

        # Skip empty lines and comments
        [[ -z "$line" || "$line" =~ ^# ]] && continue

        if [[ "$line" =~ ^config[[:space:]]+ ]]; then
            if [ $config_seen -eq 1 ]; then
                cluster_error "Multiple 'config' lines at line $line_num: $group_file"
                return 1
            fi
            config_name="${line#config }"
            # Trim whitespace from config name
            config_name="${config_name#"${config_name%%[![:space:]]*}"}"
            config_name="${config_name%"${config_name##*[![:space:]]}"}"
            config_seen=1

        elif [[ "$line" =~ ^run[[:space:]]+ ]]; then
            local test_name="${line#run }"
            # Trim whitespace from test name
            test_name="${test_name#"${test_name%%[![:space:]]*}"}"
            test_name="${test_name%"${test_name##*[![:space:]]}"}"
            test_files+=("$test_name")

        else
            cluster_error "Invalid line $line_num: $line"
            return 1
        fi
    done < "$group_file"

    # Validate: must have at least one test
    if [ ${#test_files[@]} -eq 0 ]; then
        cluster_error "No 'run' lines found in: $group_file"
        return 1
    fi

    # Export results via global variables
    GROUP_CONFIG_NAME="$config_name"
    GROUP_TEST_FILES=("${test_files[@]}")

    return 0
}

# cluster_version_lt - Compare two dotted version strings
# Returns 0 if ver1 < ver2, 1 otherwise
cluster_version_lt() {
    local ver1="$1"
    local ver2="$2"

    if [ "$ver1" = "$ver2" ]; then
        return 1
    fi

    local IFS='.'
    local -a parts1=($ver1)
    local -a parts2=($ver2)

    local max=${#parts1[@]}
    if [ ${#parts2[@]} -gt $max ]; then
        max=${#parts2[@]}
    fi

    local i
    for (( i=0; i<max; i++ )); do
        local p1=${parts1[i]:-0}
        local p2=${parts2[i]:-0}
        if [ "$p1" -lt "$p2" ] 2>/dev/null; then
            return 0
        elif [ "$p1" -gt "$p2" ] 2>/dev/null; then
            return 1
        fi
    done

    return 1
}

# cluster_is_known_failure - Check if a test is a known failure
# Uses exported CLUSTER_KNOWN_FAILURES_* and CLUSTER_VERSION_* variables.
# Returns 0 if the test is a known failure, 1 otherwise.
cluster_is_known_failure() {
    local test_name="$1"

    local cond_var="CLUSTER_KNOWN_FAILURES_${test_name//-/_}"
    local condition="${!cond_var}"

    if [ -z "$condition" ]; then
        return 1
    fi

    # Unconditional known failure (no version condition)
    if [ "$condition" = "always" ]; then
        return 0
    fi

    # Parse condition: program<version
    local program="${condition%%<*}"
    local threshold="${condition#*<}"

    local ver_var="CLUSTER_VERSION_${program}"
    local running="${!ver_var}"

    if [ -z "$running" ]; then
        return 0
    fi

    cluster_version_lt "$running" "$threshold"
    return $?
}

# cluster_load_known_failures - Parse KNOWN_FAILURES file and version info
# Sets CLUSTER_KNOWN_FAILURES_<testname> and CLUSTER_VERSION_<program> variables.
cluster_load_known_failures() {
    local script_dir="$1"
    local results_dir="$2"

    local kf_file="$script_dir/KNOWN_FAILURES"
    if [ ! -f "$kf_file" ]; then
        return 0
    fi

    while IFS= read -r line; do
        line=$(echo "$line" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')
        [[ -z "$line" || "$line" =~ ^# ]] && continue

        # Strip trailing comment
        local content="${line%%#*}"
        content=$(echo "$content" | sed 's/[[:space:]]*$//')

        # First field is test name
        local test_name="${content%% *}"
        local rest="${content#"$test_name"}"
        rest=$(echo "$rest" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')

        local var_name="CLUSTER_KNOWN_FAILURES_${test_name//-/_}"
        if [ -z "$rest" ]; then
            export "$var_name"="always"
        else
            export "$var_name"="$rest"
        fi
    done < "$kf_file"

    # Load versions from most recent log_create file
    local log_create
    log_create=$(ls -t "$results_dir"/cluster/log_create_*.txt 2>/dev/null | head -1)
    if [ -n "$log_create" ]; then
        while IFS= read -r line; do
            local program=$(echo "$line" | awk '{print $2}')
            local version=$(echo "$line" | awk '{print $3}')
            if [ -n "$program" ] && [ -n "$version" ]; then
                export "CLUSTER_VERSION_${program}"="$version"
            fi
        done < <(grep '^VERSION ' "$log_create")
    fi

    return 0
}

# Export functions for use in other scripts
export -f cluster_log cluster_warn cluster_error cluster_die cluster_debug
export -f cluster_load_config cluster_validate_config
export -f cluster_generate_id
export -f cluster_state_save cluster_state_load cluster_state_delete cluster_state_list
export -f cluster_state_get_file
export -f cluster_state_set_paused cluster_state_is_paused
export -f cluster_state_add_snapshot cluster_state_remove_snapshot cluster_state_list_snapshots
export -f cluster_state_snapshot_exists cluster_state_get_snapshot_paused_state
export -f cluster_state_migrate_old_format
export -f cluster_user_in_group cluster_get_image_dir cluster_get_cloudinit_dir
export -f cluster_virsh cluster_virt_install cluster_init_privileges
export -f cluster_check_host_prerequisites cluster_session_prepare
export -f cluster_check_root cluster_check_deps
export -f cluster_wait_with_timeout
export -f cluster_parse_group_file
export -f cluster_version_lt cluster_is_known_failure cluster_load_known_failures
