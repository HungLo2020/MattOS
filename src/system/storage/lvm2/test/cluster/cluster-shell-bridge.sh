#!/usr/bin/env bash
# cluster-shell-bridge.sh -- Bridge for running test/shell/ scripts on cluster VMs
#
# Deploys a synthetic lib/ and the test script to node1, then executes
# the test remotely. The test script runs unmodified, using real cluster
# devices instead of DM-backed virtual devices.

BRIDGE_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SHELL_BRIDGE_DIR="$BRIDGE_DIR/shell-bridge"
LVM_TEST_LIB_DIR="$BRIDGE_DIR/../lib"
SHELL_WORK_DIR="/root/shell-test-work"

#
# cluster_build_shell_lib -- Assemble synthetic lib/ directory
#
# Creates a local temporary directory with:
# - Original helpers from test/lib/: utils.sh, check.sh, get.sh, lvm-wrapper.sh, paths
# - Original aux_t.sh (base aux functions)
# - Cluster-adapted files from shell-bridge/: inittest, aux-overrides.sh, aux
# - Binaries: not (and symlinks: should, fail, invalid -> not)
# - check (the executable wrapper)
#
cluster_build_shell_lib() {
    local dest_dir="$1"
    local lib_dest="$dest_dir/lib"

    mkdir -p "$lib_dest"

    # Copy original helper scripts from test/lib/
    for f in utils.sh check.sh get.sh lvm-wrapper.sh aux_t.sh; do
        if [[ -f "$LVM_TEST_LIB_DIR/$f" ]]; then
            cp "$LVM_TEST_LIB_DIR/$f" "$lib_dest/$f"
        else
            cluster_warn "Missing test/lib/$f"
        fi
    done

    # Copy executable scripts/binaries from test/lib/
    for f in utils check get not; do
        if [[ -f "$LVM_TEST_LIB_DIR/$f" ]]; then
            cp "$LVM_TEST_LIB_DIR/$f" "$lib_dest/$f"
            chmod +x "$lib_dest/$f"
        else
            cluster_warn "Missing test/lib/$f"
        fi
    done

    # Copy 'paths' if it exists
    if [[ -f "$LVM_TEST_LIB_DIR/paths" ]]; then
        cp "$LVM_TEST_LIB_DIR/paths" "$lib_dest/paths"
    fi

    # Create symlinks: should, fail, invalid -> not
    (cd "$lib_dest" && ln -sf not should && ln -sf not fail && ln -sf not invalid)

    # Create aux_t_defs.sh: aux_t.sh with the dispatch block removed.
    # The dispatch block starts with the LVM_TEST_AUX_TRACE line near the end.
    # Our aux wrapper sources this for function definitions only, then applies
    # overrides before dispatching.
    sed '/^\[\[.*LVM_TEST_AUX_TRACE/,$ d' "$lib_dest/aux_t.sh" > "$lib_dest/aux_t_defs.sh"

    # Copy cluster-adapted files from shell-bridge/
    cp "$SHELL_BRIDGE_DIR/inittest" "$lib_dest/inittest"
    chmod +x "$lib_dest/inittest"
    cp "$SHELL_BRIDGE_DIR/inittest" "$lib_dest/inittest.sh"

    cp "$SHELL_BRIDGE_DIR/aux-overrides.sh" "$lib_dest/aux-overrides.sh"

    cp "$SHELL_BRIDGE_DIR/aux" "$lib_dest/aux"
    chmod +x "$lib_dest/aux"

    # Copy profile files that tests may need
    for f in "$LVM_TEST_LIB_DIR"/*.profile; do
        [[ -f "$f" ]] && cp "$f" "$lib_dest/" || true
    done
}

#
# cluster_run_shell_test -- Main entry point for running a test/shell/ script
#
cluster_run_shell_test() {
    local test_script="$1"

    if [[ -z "$test_script" ]]; then
        cluster_error "cluster_run_shell_test: test_script is required"
        return 1
    fi

    if [[ ! -f "$test_script" ]]; then
        cluster_error "Test script not found: $test_script"
        return 1
    fi

    local test_name="$(basename "$test_script" .sh)"
    local timestamp="$(date +%m%d%H%M%S)"
    local results_dir="$BRIDGE_DIR/results"
    local log_file="$results_dir/log_${test_name}_${timestamp}.txt"

    export CLUSTER_TEST_TIMESTAMP="$timestamp"
    export CLUSTER_TEST_NAME="$test_name"
    export CLUSTER_RESULTS_DIR="$results_dir"

    mkdir -p "$results_dir" 2>/dev/null || true
    chmod 777 "$results_dir" 2>/dev/null || true

    cluster_log "=========================================="
    cluster_log "Running shell test on cluster: $(basename "$test_script")"
    cluster_log "=========================================="
    cluster_log "Cluster ID: ${CLUSTER_ID}"
    cluster_log "Target node: node1"
    cluster_log "=========================================="

    # Initialize log file
    {
        echo "=========================================="
        echo "Shell Test Log (cluster bridge)"
        echo "=========================================="
        echo "Test: $(basename "$test_script")"
        echo "Cluster ID: ${CLUSTER_ID}"
        echo "Target node: node1"
        echo "Date: $(date)"
        echo "=========================================="
        echo ""
    } > "$log_file"
    chmod 666 "$log_file" 2>/dev/null || true

    # Step 1: Discover devices on node1
    cluster_log "Discovering devices on node1"
    cluster_setup_device_vars || {
        cluster_error "Failed to discover devices"
        return 1
    }

    # Get node1 IP for scp/ssh
    local vm_name=$(cluster_vm_get_name "$CLUSTER_ID" 1)
    local node1_ip
    if ! node1_ip=$(cluster_vm_get_ip "$vm_name" 2>/dev/null); then
        cluster_error "Failed to get IP for node1 ($vm_name)"
        return 1
    fi

    # Step 2: Build synthetic lib/ locally
    cluster_log "Building synthetic lib/ directory"
    local local_work_dir=$(mktemp -d /tmp/cluster-shell-bridge.XXXXXXXXXX)
    cluster_build_shell_lib "$local_work_dir"

    # Copy the test script
    cp "$test_script" "$local_work_dir/test_script.sh"

    # Deploy any shell/ scripts that the test sources (e.g. ". ./shell/foo.sh"
    # or ". shell/foo.sh").  These wrapper tests don't call inittest themselves;
    # the sourced parent does.
    local shell_src_dir="$BRIDGE_DIR/../shell"
    local need_shell_dir=0
    while IFS= read -r dep; do
        if [[ -f "$shell_src_dir/$dep" ]]; then
            if [[ $need_shell_dir -eq 0 ]]; then
                mkdir -p "$local_work_dir/shell"
                need_shell_dir=1
            fi
            cp "$shell_src_dir/$dep" "$local_work_dir/shell/$dep"
            cluster_log "Including sourced script: shell/$dep"
        else
            cluster_warn "Test sources shell/$dep but file not found"
        fi
    done < <(grep -oP '^\.\s+\.?/?shell/\K\S+' "$test_script" || true)

    # Step 3: Write device list file
    local device_list_file="$local_work_dir/device_list"
    for i in "${!DEVICES[@]}"; do
        echo "${DEVICES[$i]}"
    done > "$device_list_file"

    # Step 4: Build global_filter from device list
    local filter_str="[ "
    for dev in "${DEVICES[@]}"; do
        filter_str+="\"a|${dev}|\", "
    done
    filter_str+="\"r|.*|\" ]"

    # Step 5: Deploy to node1
    cluster_log "Deploying test environment to node1"
    cluster_vm_ssh "$node1_ip" "rm -rf $SHELL_WORK_DIR; mkdir -p $SHELL_WORK_DIR" || {
        cluster_error "Failed to create work directory on node1"
        rm -rf "$local_work_dir"
        return 1
    }

    cluster_vm_scp "$local_work_dir/lib" "${CLUSTER_SSH_USER}@${node1_ip}:${SHELL_WORK_DIR}/" || {
        cluster_error "Failed to deploy lib/ to node1"
        rm -rf "$local_work_dir"
        return 1
    }

    if [[ -d "$local_work_dir/shell" ]]; then
        cluster_vm_scp "$local_work_dir/shell" "${CLUSTER_SSH_USER}@${node1_ip}:${SHELL_WORK_DIR}/" || {
            cluster_error "Failed to deploy shell/ to node1"
            rm -rf "$local_work_dir"
            return 1
        }
    fi

    cluster_vm_scp "$local_work_dir/test_script.sh" "${CLUSTER_SSH_USER}@${node1_ip}:${SHELL_WORK_DIR}/" || {
        cluster_error "Failed to deploy test script to node1"
        rm -rf "$local_work_dir"
        return 1
    }

    cluster_vm_scp "$local_work_dir/device_list" "${CLUSTER_SSH_USER}@${node1_ip}:${SHELL_WORK_DIR}/" || {
        cluster_error "Failed to deploy device list to node1"
        rm -rf "$local_work_dir"
        return 1
    }

    rm -rf "$local_work_dir"

    # Step 6: Clear old debug logs on node1
    cluster_vm_ssh "$node1_ip" "rm -rf /var/log/lvm-test/* 2>/dev/null; dmesg -C 2>/dev/null; date '+%Y-%m-%d %H:%M:%S' > /tmp/cluster_test_start_time" || true

    # Step 7: Execute test on node1
    cluster_log "Executing test: $test_name"

    local remote_cmd="cd $SHELL_WORK_DIR && \
export LVM_TEST_DEVICE_LIST=$SHELL_WORK_DIR/device_list && \
export CLUSTER_DEVICE_FILTER='$filter_str' && \
export PATH=\"$SHELL_WORK_DIR/lib:\$PATH\" && \
export installed_testsuite=1 && \
bash -c 'exec 0<&- ; exec bash test_script.sh'"

    cluster_vm_ssh "$node1_ip" "$remote_cmd" 2>&1 | tee -a "$log_file"
    local exit_code=${PIPESTATUS[0]}

    # Step 8: Handle skip (exit code 200)
    if [[ "$exit_code" -eq 200 ]]; then
        cluster_log "=========================================="
        cluster_log "Test SKIPPED: $test_name"
        cluster_log "=========================================="

        echo "" >> "$log_file"
        echo "Test SKIPPED: $test_name" >> "$log_file"
        echo "Date: $(date)" >> "$log_file"

        mkdir -p "$results_dir/skipped" 2>/dev/null || true
        chmod 777 "$results_dir/skipped" 2>/dev/null || true
        local final_log_file="$results_dir/skipped/log_${test_name}_${timestamp}.txt"
        mv "$log_file" "$final_log_file" 2>/dev/null || true
        cluster_log "Test log saved to: $final_log_file"

        # Clean up remote work directory
        cluster_vm_ssh "$node1_ip" "rm -rf $SHELL_WORK_DIR" 2>/dev/null || true
        return 0
    fi

    # Step 9: Report results
    local fail_prefix="FAILED_"
    if [[ "$exit_code" -ne 0 ]] && cluster_is_known_failure "$test_name"; then
        fail_prefix="KNOWN_"
    fi

    cluster_log "=========================================="
    if [[ "$exit_code" -eq 0 ]]; then
        cluster_log "Test PASSED: $test_name"
    else
        cluster_error "Test ${fail_prefix%_}: $test_name (exit code: $exit_code)"

        # Collect debug logs on failure
        cluster_log "Collecting debug logs from node1"
        cluster_collect_debug_logs "$test_name" "$fail_prefix" 2>/dev/null || {
            cluster_warn "Some debug logs could not be collected"
        }

        # Collect dmesg and journal from node1 into the debug directory
        local debug_dir="$results_dir/${fail_prefix}${test_name}_${timestamp}_debug"
        mkdir -p "$debug_dir" 2>/dev/null || true
        chmod 777 "$debug_dir" 2>/dev/null || true
        cluster_log "Collecting dmesg from node1"
        cluster_vm_ssh "$node1_ip" "dmesg" > "$debug_dir/dmesg.txt" 2>/dev/null || true
        cluster_log "Collecting journal from node1"
        cluster_vm_ssh "$node1_ip" "journalctl -b --no-pager" > "$debug_dir/journal.txt" 2>/dev/null || true
    fi
    cluster_log "=========================================="

    # Log final result and rename
    {
        echo ""
        echo "=========================================="
        if [[ "$exit_code" -eq 0 ]]; then
            echo "Test PASSED: $test_name"
        else
            echo "Test ${fail_prefix%_}: $test_name (exit code: $exit_code)"
        fi
        echo "Date: $(date)"
        echo "=========================================="
    } >> "$log_file"

    if [[ "$exit_code" -eq 0 ]]; then
        mkdir -p "$results_dir/passed" 2>/dev/null || true
        chmod 777 "$results_dir/passed" 2>/dev/null || true
        local final_log_file="$results_dir/passed/log_${test_name}_${timestamp}.txt"
        mv "$log_file" "$final_log_file" 2>/dev/null || final_log_file="$log_file"
    else
        local final_log_file="$results_dir/${fail_prefix}${test_name}_${timestamp}.txt"
        mv "$log_file" "$final_log_file" 2>/dev/null || final_log_file="$log_file"
        chmod 666 "$final_log_file" 2>/dev/null || true
    fi

    cluster_log "Test log saved to: $final_log_file"

    # Clean up remote work directory
    cluster_vm_ssh "$node1_ip" "rm -rf $SHELL_WORK_DIR" 2>/dev/null || true

    return $exit_code
}
