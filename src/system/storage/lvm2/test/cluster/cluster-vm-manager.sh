#!/bin/bash
#
# cluster-vm-manager.sh - VM lifecycle management for LVM cluster testing
#
# Provides:
# - VM creation and destruction
# - IP address discovery
# - SSH setup
# - Package installation
# - Source deployment and compilation
# - Lock manager configuration
#

# Source the core library
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck disable=SC1091
source "$SCRIPT_DIR/cluster-test-lib.sh"

#
# VM naming and identification
#

cluster_vm_get_name() {
    local cluster_id="$1"
    local node_num="$2"

    echo "${cluster_id}-node${node_num}"
}

#
# VM creation
#

cluster_vm_create() {
    local node_num="$1"
    local cluster_id="$2"
    local backing_image="${3:-$CLUSTER_NODE_OS_IMAGE}"

    if [ -z "$node_num" ] || [ -z "$cluster_id" ]; then
        cluster_die "cluster_vm_create: node_num and cluster_id are required"
    fi

    local vm_name=$(cluster_vm_get_name "$cluster_id" "$node_num")

    cluster_log "Creating VM: $vm_name (node $node_num)"

    # Check if backing image exists
    if [ -z "$backing_image" ]; then
        cluster_die "No backing image specified (CLUSTER_NODE_OS_IMAGE is not set)"
    fi

    if [ ! -f "$backing_image" ]; then
        cluster_die "Backing image not found: $backing_image"
    fi

    # Create disk image directory
    local disk_dir
    disk_dir=$(cluster_get_image_dir)
    local disk_image="${disk_dir}/${vm_name}.qcow2"

    # Create a copy-on-write disk from the backing image
    cluster_log "Creating disk image: $disk_image"
    qemu-img create -f qcow2 -F qcow2 -b "$backing_image" "$disk_image" "${CLUSTER_NODE_DISK_SIZE}G" || \
        cluster_die "Failed to create disk image: $disk_image"

    # Generate cloud-init configuration
    local cloudinit_dir
    cloudinit_dir=$(cluster_get_cloudinit_dir "$vm_name")
    mkdir -p "$cloudinit_dir"

    # Create meta-data
    cat > "$cloudinit_dir/meta-data" <<EOF
instance-id: ${vm_name}
local-hostname: ${vm_name}
EOF

    # Create user-data with SSH key
    local ssh_key_file="${CLUSTER_SSH_KEY_DIR}/cluster_test_rsa.pub"
    if [ ! -f "$ssh_key_file" ]; then
        cluster_log "Generating SSH key for cluster testing"
        ssh-keygen -t rsa -b 4096 -f "${CLUSTER_SSH_KEY_DIR}/cluster_test_rsa" -N "" -C "cluster-test" || \
            cluster_die "Failed to generate SSH key"
    fi

    local ssh_pubkey=$(cat "$ssh_key_file")

    cat > "$cloudinit_dir/user-data" <<EOF
#cloud-config
users:
  - name: ${CLUSTER_SSH_USER}
    ssh_authorized_keys:
      - ${ssh_pubkey}
    sudo: ALL=(ALL) NOPASSWD:ALL
    lock_passwd: false

# Allow root login
ssh_pwauth: false
disable_root: false

# Set root password (for console access if needed)
chpasswd:
  expire: false
  list: |
    root:cluster123

# Expand root filesystem
growpart:
  mode: auto
  devices: ['/']
resize_rootfs: true

runcmd:
  - systemctl enable sshd
  - systemctl start sshd
EOF

    # Create cloud-init ISO
    local cloudinit_iso="${disk_dir}/${vm_name}-cloudinit.iso"
    cluster_log "Creating cloud-init ISO: $cloudinit_iso"

    if command -v genisoimage &>/dev/null; then
        genisoimage -output "$cloudinit_iso" -volid cidata -joliet -rock \
            "$cloudinit_dir/user-data" "$cloudinit_dir/meta-data" || \
            cluster_die "Failed to create cloud-init ISO"
    elif command -v mkisofs &>/dev/null; then
        mkisofs -output "$cloudinit_iso" -volid cidata -joliet -rock \
            "$cloudinit_dir/user-data" "$cloudinit_dir/meta-data" || \
            cluster_die "Failed to create cloud-init ISO"
    else
        cluster_die "Neither genisoimage nor mkisofs found - required for cloud-init"
    fi

    # Clean up cloud-init directory
    rm -rf "$cloudinit_dir"

    # Create VM with virt-install
    cluster_log "Installing VM with virt-install"
    cluster_virt_install \
        --name "$vm_name" \
        --memory "$CLUSTER_NODE_MEMORY" \
        --vcpus "$CLUSTER_NODE_VCPUS" \
        --disk path="$disk_image",format=qcow2,bus=virtio \
        --disk path="$cloudinit_iso",device=cdrom \
        --network network="${CLUSTER_NETWORK_NAME}",model=virtio \
        --osinfo "${CLUSTER_NODE_OS_VARIANT:-linux2024}" \
        --graphics none \
        --noautoconsole \
        --import || cluster_die "Failed to create VM: $vm_name"

    cluster_log "VM created successfully: $vm_name"
}

#
# VM IP address discovery
#

cluster_vm_get_ip() {
    local vm_name="$1"
    local max_retries=5
    local retry_delay=2

    if [ -z "$vm_name" ]; then
        cluster_die "cluster_vm_get_ip: vm_name is required"
    fi

    local attempt
    for attempt in $(seq 1 "$max_retries"); do
        # Try cluster_virsh domifaddr first (works with DHCP)
        local ip=$(cluster_virsh domifaddr "$vm_name" 2>/dev/null | awk '/ipv4/ {print $4}' | cut -d'/' -f1 | head -n1)

        if [ -n "$ip" ]; then
            echo "$ip"
            return 0
        fi

        # Fallback: try getting IP from ARP table
        local mac=$(cluster_virsh domiflist "$vm_name" 2>/dev/null | awk '/network/ {print $5}' | head -n1)
        if [ -n "$mac" ]; then
            ip=$(arp -an | grep -i "$mac" | awk '{print $2}' | tr -d '()')
            if [ -n "$ip" ]; then
                echo "$ip"
                return 0
            fi
        fi

        if [ "$attempt" -lt "$max_retries" ]; then
            sleep "$retry_delay"
        fi
    done

    return 1
}

#
# VM boot waiting
#

cluster_vm_wait_boot() {
    local vm_name="$1"
    local timeout="${CLUSTER_VM_BOOT_TIMEOUT:-300}"

    if [ -z "$vm_name" ]; then
        cluster_die "cluster_vm_wait_boot: vm_name is required"
    fi

    cluster_log "Waiting for VM to boot: $vm_name"

    # Wait for VM to be running
    cluster_wait_with_timeout "$timeout" 5 \
        "cluster_virsh domstate '$vm_name' 2>/dev/null | grep -q running" \
        "VM running state" || return 1

    # Wait for IP address
    cluster_log "Waiting for IP address assignment"
    local ip=""
    local elapsed=0

    while [ $elapsed -lt "$timeout" ]; do
        ip=$(cluster_vm_get_ip "$vm_name")
        if [ -n "$ip" ]; then
            cluster_log "VM IP address: $ip"
            echo "$ip"
            return 0
        fi

        sleep 5
        elapsed=$((elapsed + 5))
    done

    cluster_error "Failed to get IP address for VM: $vm_name"
    return 1
}

#
# SSH setup and connectivity
#

cluster_vm_setup_ssh() {
    local vm_ip="$1"
    local timeout="${CLUSTER_SSH_READY_TIMEOUT:-180}"

    if [ -z "$vm_ip" ]; then
        cluster_die "cluster_vm_setup_ssh: vm_ip is required"
    fi

    local ssh_key="${CLUSTER_SSH_KEY_DIR}/cluster_test_rsa"
    local ssh_opts="-o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o ConnectTimeout=5 -o LogLevel=ERROR"

    cluster_log "Waiting for SSH to become available on $vm_ip"

    local elapsed=0
    while [ $elapsed -lt "$timeout" ]; do
        if ssh $ssh_opts -i "$ssh_key" "${CLUSTER_SSH_USER}@${vm_ip}" "true" 2>/dev/null; then
            cluster_log "SSH is ready on $vm_ip"
            return 0
        fi

        sleep 5
        elapsed=$((elapsed + 5))

        if [ $((elapsed % 30)) -eq 0 ]; then
            cluster_debug "Still waiting for SSH... (${elapsed}s/${timeout}s)"
        fi
    done

    cluster_error "SSH not available on $vm_ip after ${timeout}s"
    return 1
}

cluster_vm_ssh() {
    local vm_ip="$1"
    shift
    local cmd="$*"

    local ssh_key="${CLUSTER_SSH_KEY_DIR}/cluster_test_rsa"
    local ssh_opts="-o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o LogLevel=ERROR -o ConnectTimeout=10 -o ServerAliveInterval=5 -o ServerAliveCountMax=3"

    ssh $ssh_opts -i "$ssh_key" "${CLUSTER_SSH_USER}@${vm_ip}" "$cmd"
}

cluster_vm_scp() {
    local src="$1"
    local dst="$2"

    local ssh_key="${CLUSTER_SSH_KEY_DIR}/cluster_test_rsa"
    local scp_opts="-o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o LogLevel=ERROR -o ConnectTimeout=10 -o ServerAliveInterval=5 -o ServerAliveCountMax=3"

    scp $scp_opts -i "$ssh_key" -r "$src" "$dst"
}

#
# Root filesystem expansion for LVM-based images (e.g. Fedora Server)
#
# Cloud-init growpart expands the partition but the LVM PV/LV/filesystem
# remain at their original size.  This runs the resize steps explicitly
# via SSH so there is no dependency on cloud-init runcmd timing.
#

cluster_vm_expand_rootfs() {
    local vm_ip="$1"

    cluster_log "Expanding root filesystem on $vm_ip"
    cluster_vm_ssh "$vm_ip" \
        "ROOT_PART=\$(findmnt -n -o SOURCE / 2>/dev/null);
         if lvs \$ROOT_PART >/dev/null 2>&1; then
             PV=\$(pvs --noheadings -o pv_name 2>/dev/null | head -1 | tr -d ' ');
             DISK=\$(echo \$PV | sed 's/[0-9]*$//');
             PARTNUM=\$(echo \$PV | grep -o '[0-9]*$');
             growpart \$DISK \$PARTNUM 2>/dev/null;
             pvresize \$PV 2>/dev/null;
             lvextend -l +100%FREE \$ROOT_PART 2>/dev/null;
             xfs_growfs / 2>/dev/null || resize2fs \$ROOT_PART 2>/dev/null;
         fi;
         true"
}

#
# Package installation
#

cluster_vm_install_packages() {
    local vm_ip="$1"
    local node_num="$2"

    if [ -z "$vm_ip" ] || [ -z "$node_num" ]; then
        cluster_die "cluster_vm_install_packages: vm_ip and node_num are required"
    fi

    cluster_log "Installing packages on node $node_num ($vm_ip)"

    # Detect package manager
    local pkg_mgr=""
    if cluster_vm_ssh "$vm_ip" "command -v dnf" &>/dev/null; then
        pkg_mgr="dnf"
    elif cluster_vm_ssh "$vm_ip" "command -v yum" &>/dev/null; then
        pkg_mgr="yum"
    elif cluster_vm_ssh "$vm_ip" "command -v apt-get" &>/dev/null; then
        pkg_mgr="apt-get"
    else
        cluster_die "No supported package manager found on node $node_num"
    fi

    cluster_debug "Detected package manager: $pkg_mgr"

    local packages=()

    # All nodes need chrony for clock synchronization
    packages+=(chrony)

    if [ "$node_num" = "0" ]; then
        # Node 0: Storage exporter packages
        cluster_log "Installing storage exporter packages on node 0"
        if [[ "$pkg_mgr" =~ ^(dnf|yum)$ ]]; then
            packages+=(targetcli python3-rtslib nvme-cli sg3_utils jq)
        else
            packages+=(targetcli-fb nvme-cli sg3-utils jq)
        fi
    else
        # Test nodes (1..N): Install based on configuration

        # Always install initiator tools
        if [[ "$pkg_mgr" =~ ^(dnf|yum)$ ]]; then
            packages+=(iscsi-initiator-utils nvme-cli sg3_utils jq cryptsetup binutils mdadm)
            # Add multipath if multipath devices configured
            if [ "${CLUSTER_NUM_MULTIPATH:-0}" -gt 0 ]; then
                packages+=(device-mapper-multipath)
            fi
        else
            packages+=(open-iscsi nvme-cli sg3-utils jq cryptsetup binutils mdadm)
            # Add multipath if multipath devices configured
            if [ "${CLUSTER_NUM_MULTIPATH:-0}" -gt 0 ]; then
                packages+=(multipath-tools)
            fi
        fi

        # LVM packages or build dependencies
        if [ -n "${LVM_SOURCE_DIR:-}" ]; then
            cluster_log "Installing LVM build dependencies on node $node_num"
            if [[ "$pkg_mgr" =~ ^(dnf|yum)$ ]]; then
                packages+=(gcc gcc-c++ make autoconf automake libtool pkgconfig)
                packages+=(libaio-devel libudev-devel libblkid-devel device-mapper-devel)
                packages+=(readline-devel ncurses-devel systemd-devel libstdc++-devel gdb)
                packages+=(libnvme-devel xfsprogs-devel vdo)
            else
                packages+=(build-essential g++ autoconf automake libtool pkg-config)
                packages+=(libaio-dev libudev-dev libblkid-dev libdevmapper-dev)
                packages+=(libreadline-dev libncurses-dev libsystemd-dev gdb)
                packages+=(libnvme-dev xfsprogs-dev vdo)
            fi
        else
            cluster_log "Installing LVM packages on node $node_num"
            if [[ "$pkg_mgr" =~ ^(dnf|yum)$ ]]; then
                packages+=(lvm2 lvm2-lockd)
            else
                packages+=(lvm2)
            fi
        fi

        # Lock manager packages or build dependencies
        if [ "$CLUSTER_LOCK_TYPE" = "sanlock" ]; then
            if [ -n "${SANLOCK_SOURCE_DIR:-}" ]; then
                cluster_log "Installing sanlock build dependencies on node $node_num"
                if [[ "$pkg_mgr" =~ ^(dnf|yum)$ ]]; then
                    packages+=(libaio-devel libblkid-devel systemd-devel libuuid-devel device-mapper-devel)
                else
                    packages+=(libaio-dev libblkid-dev libsystemd-dev uuid-dev libdevmapper-dev)
                fi
            else
                cluster_log "Installing sanlock packages on node $node_num"
                if [[ "$pkg_mgr" =~ ^(dnf|yum)$ ]]; then
                    packages+=(sanlock sanlock-lib sanlock-devel)
                else
                    packages+=(sanlock libsanlock-dev)
                fi
            fi
        elif [ "$CLUSTER_LOCK_TYPE" = "dlm" ]; then
            cluster_log "Installing DLM packages on node $node_num"
            if [[ "$pkg_mgr" =~ ^(dnf|yum)$ ]]; then
                packages+=(corosync dlm dlm-lib dlm-devel)
                # Install kernel-modules-extra matching the running kernel
                # This will be done separately after package install to ensure version match
            else
                packages+=(corosync)
            fi
        elif [ "$CLUSTER_LOCK_TYPE" = "none" ]; then
            cluster_log "Installing sanlock packages (lock type: none) on node $node_num"
            if [ -n "${SANLOCK_SOURCE_DIR:-}" ]; then
                if [[ "$pkg_mgr" =~ ^(dnf|yum)$ ]]; then
                    packages+=(libaio-devel libblkid-devel systemd-devel libuuid-devel device-mapper-devel)
                else
                    packages+=(libaio-dev libblkid-dev libsystemd-dev uuid-dev libdevmapper-dev)
                fi
            else
                if [[ "$pkg_mgr" =~ ^(dnf|yum)$ ]]; then
                    packages+=(sanlock sanlock-lib sanlock-devel)
                else
                    packages+=(sanlock libsanlock-dev)
                fi
            fi
        fi

        # Add base packages
        if [ -n "${CLUSTER_BASE_PACKAGES:-}" ]; then
            read -ra base_pkgs <<< "$CLUSTER_BASE_PACKAGES"
            packages+=("${base_pkgs[@]}")
        fi
    fi

    # Install packages
    if [ ${#packages[@]} -gt 0 ]; then
        cluster_log "Installing: ${packages[*]}"

        if [[ "$pkg_mgr" =~ ^(dnf|yum)$ ]]; then
            cluster_vm_ssh "$vm_ip" "$pkg_mgr install -y ${packages[*]}" || \
                cluster_die "Package installation failed on node $node_num"
        else
            cluster_vm_ssh "$vm_ip" "apt-get update && apt-get install -y ${packages[*]}" || \
                cluster_die "Package installation failed on node $node_num"
        fi

        cluster_log "Packages installed successfully on node $node_num"
    fi

    # Install kernel-modules-extra for DLM (must match running kernel version)
    if [ "$node_num" != "0" ] && [ "$CLUSTER_LOCK_TYPE" = "dlm" ]; then
        cluster_log "Installing kernel-modules-extra for DLM on node $node_num"
        if [[ "$pkg_mgr" =~ ^(dnf|yum)$ ]]; then
            cluster_vm_ssh "$vm_ip" "$pkg_mgr install -y \"kernel-modules-extra-\$(uname -r)\"" || \
                cluster_warn "Failed to install kernel-modules-extra for current kernel"
        else
            cluster_vm_ssh "$vm_ip" "apt-get install -y linux-modules-extra-\$(uname -r)" || \
                cluster_warn "Failed to install linux-modules-extra for current kernel"
        fi
    fi

    # Load extra packages from file if specified
    if [ -n "${CLUSTER_EXTRA_PACKAGES_FILE:-}" ] && [ -f "$CLUSTER_EXTRA_PACKAGES_FILE" ]; then
        cluster_log "Installing extra packages from: $CLUSTER_EXTRA_PACKAGES_FILE"
        local extra_packages=$(cat "$CLUSTER_EXTRA_PACKAGES_FILE" | tr '\n' ' ')

        if [[ "$pkg_mgr" =~ ^(dnf|yum)$ ]]; then
            cluster_vm_ssh "$vm_ip" "$pkg_mgr install -y $extra_packages" || \
                cluster_warn "Some extra packages failed to install"
        else
            cluster_vm_ssh "$vm_ip" "apt-get install -y $extra_packages" || \
                cluster_warn "Some extra packages failed to install"
        fi
    fi
}

cluster_vm_setup_chrony() {
    local vm_ip="$1"
    local node_num="$2"

    cluster_log "Configuring chrony on node $node_num ($vm_ip)"

    # Use the KVM PTP clock as the primary reference if available,
    # with NTP pool servers as fallback.  Persist the ptp_kvm module
    # so it loads on boot (needed for golden-image-derived VMs and
    # snapshot restores).
    cluster_vm_ssh "$vm_ip" "
        if [ -e /dev/ptp_kvm ] || modprobe ptp_kvm 2>/dev/null; then
            echo ptp_kvm > /etc/modules-load.d/ptp_kvm.conf
            cat > /etc/chrony.conf <<'EOF'
refclock PHC /dev/ptp_kvm poll 3 dpoll -2 offset 0 stratum 2
pool 2.fedora.pool.ntp.org iburst
makestep 1 -1
maxdistance 16
rtcsync
EOF
        fi
        systemctl enable chronyd
        systemctl restart chronyd
        chronyc makestep 1>/dev/null 2>&1 || true
    " || cluster_warn "chrony setup failed on node $node_num"
}

#
# Source deployment
#

cluster_vm_deploy_sanlock_source() {
    local vm_ip="$1"
    local node_num="$2"

    if [ "$node_num" = "0" ]; then
        cluster_debug "Skipping sanlock source deployment on node 0 (storage exporter)"
        return 0
    fi

    if [ -z "${SANLOCK_SOURCE_DIR:-}" ]; then
        return 0
    fi

    if [ "$CLUSTER_LOCK_TYPE" != "sanlock" ] && [ "$CLUSTER_LOCK_TYPE" != "none" ]; then
        return 0
    fi

    cluster_log "Deploying sanlock source to node $node_num ($vm_ip)"

    if [ ! -d "$SANLOCK_SOURCE_DIR" ]; then
        cluster_die "Sanlock source directory not found: $SANLOCK_SOURCE_DIR"
    fi

    local remote_dir="/root/sanlock-source"

    # Create remote directory
    cluster_vm_ssh "$vm_ip" "mkdir -p $remote_dir"

    # Copy source tree (exclude .git and build artifacts).
    # Pre-built binaries/libs must be excluded (not just *.o/*.a): if a
    # linked target is copied over, rsync -a preserves its mtime and make
    # sees it as up to date, skipping the rebuild and installing a binary
    # linked against the host's glibc instead of the VM's.
    cluster_log "Copying sanlock source tree to $vm_ip:$remote_dir"
    rsync -az --exclude='.git' --exclude='*.o' --exclude='*.a' \
        --exclude='*.so' --exclude='*.so.*' --exclude='*.pc' \
        --exclude='/src/sanlock' --exclude='/wdmd/wdmd' \
        -e "ssh -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o LogLevel=ERROR -i ${CLUSTER_SSH_KEY_DIR}/cluster_test_rsa" \
        "$SANLOCK_SOURCE_DIR/" "${CLUSTER_SSH_USER}@${vm_ip}:${remote_dir}/" || \
        cluster_die "Failed to copy sanlock source to node $node_num"

    # Build sanlock
    cluster_log "Building sanlock on node $node_num"
    cluster_vm_ssh "$vm_ip" "cd $remote_dir && make LIBDIR=/usr/lib64 ${SANLOCK_BUILD_OPTS:-}" || \
        cluster_die "Failed to build sanlock on node $node_num"

    # Install sanlock
    cluster_log "Installing sanlock on node $node_num"
    cluster_vm_ssh "$vm_ip" "cd $remote_dir && make install LIBDIR=/usr/lib64" || \
        cluster_die "Failed to install sanlock on node $node_num"

    # Install systemd-wdmd helper
    cluster_log "Installing systemd-wdmd helper on node $node_num"
    cluster_vm_ssh "$vm_ip" "
        cd $remote_dir
        # systemd-wdmd is in init.d/ subdirectory
        if [ -f init.d/systemd-wdmd ]; then
            install -D -m 755 init.d/systemd-wdmd /lib/systemd/systemd-wdmd
        elif [ -f wdmd/systemd-wdmd ]; then
            install -D -m 755 wdmd/systemd-wdmd /lib/systemd/systemd-wdmd
        elif [ -f systemd-wdmd ]; then
            install -D -m 755 systemd-wdmd /lib/systemd/systemd-wdmd
        else
            echo 'WARNING: systemd-wdmd helper not found in source tree'
        fi
    "

    # Run ldconfig to update library cache
    cluster_vm_ssh "$vm_ip" "ldconfig"

    # Install systemd service files from source tree if they exist
    cluster_log "Installing systemd service files for sanlock on node $node_num"
    cluster_vm_ssh "$vm_ip" "
        cd $remote_dir
        # Try various locations where service files might be
        for svc_file in init.d/wdmd.service src/wdmd.service wdmd/wdmd.service; do
            if [ -f \"\$svc_file\" ]; then
                cp \"\$svc_file\" /etc/systemd/system/wdmd.service
                break
            fi
        done
        for svc_file in init.d/sanlock.service.native init.d/sanlock.service src/sanlock.service; do
            if [ -f \"\$svc_file\" ]; then
                cp \"\$svc_file\" /etc/systemd/system/sanlock.service
                break
            fi
        done
        systemctl daemon-reload
    "

    # Verify installation
    cluster_log "Verifying sanlock installation on node $node_num"
    cluster_vm_ssh "$vm_ip" "sanlock version" || \
        cluster_die "Sanlock installation verification failed on node $node_num"

    cluster_log "Sanlock source deployed successfully on node $node_num"
}

cluster_vm_deploy_lvm_source() {
    local vm_ip="$1"
    local node_num="$2"

    if [ "$node_num" = "0" ]; then
        cluster_debug "Skipping LVM source deployment on node 0 (storage exporter)"
        return 0
    fi

    if [ -z "${LVM_SOURCE_DIR:-}" ]; then
        return 0
    fi

    cluster_log "Deploying LVM source to node $node_num ($vm_ip)"

    if [ ! -d "$LVM_SOURCE_DIR" ]; then
        cluster_die "LVM source directory not found: $LVM_SOURCE_DIR"
    fi

    local remote_dir="/root/lvm-source"

    # Create remote directory
    cluster_vm_ssh "$vm_ip" "mkdir -p $remote_dir"

    # Copy source tree (exclude .git and build artifacts)
    cluster_log "Copying LVM source tree to $vm_ip:$remote_dir"
    rsync -az --exclude='.git' --exclude='*.o' --exclude='*.a' --exclude='test/cluster' \
        -e "ssh -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o LogLevel=ERROR -i ${CLUSTER_SSH_KEY_DIR}/cluster_test_rsa" \
        "$LVM_SOURCE_DIR/" "${CLUSTER_SSH_USER}@${vm_ip}:${remote_dir}/" || \
        cluster_die "Failed to copy LVM source to node $node_num"

    # Configure and build
    cluster_log "Configuring LVM on node $node_num"

    # If sanlock was built from source, set PKG_CONFIG_PATH
    local configure_env=""
    if [ -n "${SANLOCK_SOURCE_DIR:-}" ] && { [ "$CLUSTER_LOCK_TYPE" = "sanlock" ] || [ "$CLUSTER_LOCK_TYPE" = "none" ]; }; then
        configure_env="PKG_CONFIG_PATH=/usr/lib/pkgconfig:/usr/lib64/pkgconfig:$PKG_CONFIG_PATH"
    fi

    # Build configure options to match production build
    local configure_opts="${LVM_BUILD_OPTS:-}"

    # Base paths (use standard RPM paths)
    configure_opts="$configure_opts --prefix=/usr"
    configure_opts="$configure_opts --sbindir=/usr/sbin"
    configure_opts="$configure_opts --libdir=/usr/lib64"
    configure_opts="$configure_opts --with-usrlibdir=/usr/lib64"

    # Runtime directories (match production paths under /run)
    configure_opts="$configure_opts --with-default-dm-run-dir=/run"
    configure_opts="$configure_opts --with-default-run-dir=/run/lvm"
    configure_opts="$configure_opts --with-default-pid-dir=/run"
    configure_opts="$configure_opts --with-default-locking-dir=/run/lock/lvm"

    # Lock manager (specific to configured type)
    if [ "$CLUSTER_LOCK_TYPE" = "sanlock" ] || [ "$CLUSTER_LOCK_TYPE" = "none" ]; then
        configure_opts="$configure_opts --enable-lvmlockd-sanlock"
    elif [ "$CLUSTER_LOCK_TYPE" = "dlm" ]; then
        configure_opts="$configure_opts --enable-lvmlockd-dlm"
    fi

    # Core features (match production build)
    configure_opts="$configure_opts --enable-debug"
    configure_opts="$configure_opts --enable-lvmpolld"
    configure_opts="$configure_opts --enable-dmeventd"
    configure_opts="$configure_opts --enable-udev_sync"
    configure_opts="$configure_opts --with-thin=internal"
    configure_opts="$configure_opts --enable-blkid_wiping"
    configure_opts="$configure_opts --with-default-use-devices-file=1"

    # Detect systemd unit directory (use system default, typically /usr/lib/systemd/system)
    local systemd_unit_dir
    systemd_unit_dir=$(cluster_vm_ssh "$vm_ip" "pkg-config --variable=systemdsystemunitdir systemd 2>/dev/null" || echo "/usr/lib/systemd/system")
    configure_opts="$configure_opts --with-systemdsystemunitdir=$systemd_unit_dir"

    cluster_vm_ssh "$vm_ip" "cd $remote_dir && $configure_env ./configure $configure_opts" || \
        cluster_die "Failed to configure LVM on node $node_num"

    cluster_log "Building LVM on node $node_num"
    cluster_vm_ssh "$vm_ip" "cd $remote_dir && make -j\$(nproc)" || \
        cluster_die "Failed to build LVM on node $node_num"

    # Install
    cluster_log "Installing LVM on node $node_num"
    cluster_vm_ssh "$vm_ip" "cd $remote_dir && make install" || \
        cluster_die "Failed to install LVM on node $node_num"

    # Run ldconfig to update library cache
    cluster_vm_ssh "$vm_ip" "ldconfig"

    # install_system_dirs and install_tmpfiles_configuration are separate
    # make targets (normally run by the RPM spec, not by "make install").
    # install_system_dirs creates /var/lib/lvm (needed for persist key files).
    # install_tmpfiles_configuration creates /run/lvm and /run/lock/lvm
    # (needed for lvmlockd socket).
    cluster_log "Installing system dirs and tmpfiles configuration for LVM on node $node_num"
    cluster_vm_ssh "$vm_ip" "cd $remote_dir && make install_system_dirs" || \
        cluster_die "Failed to install system dirs on node $node_num"
    cluster_vm_ssh "$vm_ip" "cd $remote_dir && make install_tmpfiles_configuration" || \
        cluster_die "Failed to install tmpfiles configuration on node $node_num"
    cluster_vm_ssh "$vm_ip" "systemd-tmpfiles --create" || \
        cluster_die "Failed to create runtime dirs from tmpfiles configuration on node $node_num"

    # Install systemd service files if they exist but weren't installed by make install
    cluster_log "Checking systemd service files for lvmlockd on node $node_num"
    cluster_vm_ssh "$vm_ip" "
        cd $remote_dir
        # If lvmlockd.service doesn't exist in systemd, try to install it
        if [ ! -f $systemd_unit_dir/lvmlockd.service ]; then
            # Check if configure created the service file in the build tree
            if [ -f scripts/lvmlockd.service ]; then
                cp scripts/lvmlockd.service $systemd_unit_dir/
            fi
        fi
        systemctl daemon-reload
    "

    # Verify installation
    cluster_log "Verifying LVM installation on node $node_num"
    cluster_vm_ssh "$vm_ip" "lvm version" || \
        cluster_die "LVM installation verification failed on node $node_num"

    cluster_log "LVM source deployed successfully on node $node_num"
}

#
# Lock manager setup
#

cluster_vm_setup_lock_manager() {
    local vm_ip="$1"
    local node_num="$2"

    if [ "$node_num" = "0" ]; then
        cluster_debug "Skipping lock manager setup on node 0 (storage exporter)"
        return 0
    fi

    cluster_log "Setting up lock manager on node $node_num ($vm_ip)"

    if [ "$CLUSTER_LOCK_TYPE" = "none" ]; then
        cluster_log "Lock type is 'none' - skipping lock manager configuration on node $node_num"
        cluster_log "Packages for sanlock and DLM are installed but not configured"
        return 0
    elif [ "$CLUSTER_LOCK_TYPE" = "sanlock" ]; then
        # Configure sanlock with unique host_id per node
        cluster_log "Configuring sanlock with host_id=$node_num on node $node_num"

        # Create lvmlocal.conf with host_id
        cluster_vm_ssh "$vm_ip" "mkdir -p /etc/lvm"
        cluster_vm_ssh "$vm_ip" "cat > /etc/lvm/lvmlocal.conf <<'EOF'
local {
    host_id = $node_num
}
EOF"

        # Configure sanlock.conf if custom settings are specified
        if [ "${#SANLOCK_CONF_SETTINGS[@]}" -gt 0 ]; then
            cluster_log "Deploying custom sanlock.conf settings to node $node_num"
            cluster_vm_ssh "$vm_ip" "mkdir -p /etc/sanlock"

            # Join array elements with newlines and deploy
            local sanlock_conf_content
            sanlock_conf_content=$(printf '%s\n' "${SANLOCK_CONF_SETTINGS[@]}")

            cluster_vm_ssh "$vm_ip" "cat > /etc/sanlock/sanlock.conf <<'SANLOCK_CONF_EOF'
${sanlock_conf_content}
SANLOCK_CONF_EOF"
            cluster_debug "Deployed sanlock.conf to node $node_num"
        fi

        # Enable and start services
        cluster_log "Starting sanlock on node $node_num"
        cluster_vm_ssh "$vm_ip" "systemctl daemon-reload"
        cluster_vm_ssh "$vm_ip" "systemctl enable wdmd sanlock"
        cluster_vm_ssh "$vm_ip" "systemctl start wdmd"
        cluster_vm_ssh "$vm_ip" "systemctl start sanlock"

        # Verify sanlock is running
        if ! cluster_vm_ssh "$vm_ip" "systemctl is-active sanlock" &>/dev/null; then
            # Show status for debugging
            cluster_vm_ssh "$vm_ip" "systemctl status sanlock" || true
            cluster_die "Sanlock failed to start on node $node_num"
        fi

        cluster_log "Sanlock configured and running on node $node_num"

    elif [ "$CLUSTER_LOCK_TYPE" = "dlm" ]; then
        cluster_log "Configuring DLM/Corosync on node $node_num"

        # Generate corosync.conf with all test nodes
        local corosync_conf="/tmp/corosync.conf.$$"

        cat > "$corosync_conf" <<EOF
totem {
        version: 2
        cluster_name: ${CLUSTER_ID}
        crypto_cipher: none
        crypto_hash: none
}

logging {
        fileline: off
        to_stderr: yes
        to_logfile: yes
        logfile: /var/log/cluster/corosync.log
        to_syslog: yes
        debug: off
        logger_subsys {
                subsys: QUORUM
                debug: off
        }
}

quorum {
        provider: corosync_votequorum
}

nodelist {
EOF

        # Add all test nodes to nodelist
        for i in $(seq 1 "$CLUSTER_NUM_NODES"); do
            local node_ip="${CLUSTER_NODE_IPS[$i]}"
            local node_name="${CLUSTER_ID}-node${i}"

            cluster_debug "Adding node $i to corosync.conf: name=${node_name}, nodeid=${i}, ring0_addr=${node_ip}"

            if [ -z "$node_ip" ]; then
                cluster_die "Node IP is empty for node $i - CLUSTER_NODE_IPS array may not be set correctly"
            fi

            cat >> "$corosync_conf" <<EOF
        node {
                name: ${node_name}
                nodeid: ${i}
                ring0_addr: ${node_ip}
        }
EOF
        done

        cat >> "$corosync_conf" <<EOF
}
EOF

        # Debug: show generated config
        cluster_debug "Generated corosync.conf:"
        cluster_debug "$(cat "$corosync_conf")"

        # Deploy corosync.conf to this node
        cluster_vm_ssh "$vm_ip" "mkdir -p /etc/corosync /var/log/cluster"
        cluster_vm_scp "$corosync_conf" "${CLUSTER_SSH_USER}@${vm_ip}:/etc/corosync/corosync.conf"

        # Verify deployed config
        cluster_log "Verifying corosync.conf on node $node_num"
        if ! cluster_vm_ssh "$vm_ip" "grep -q 'ring0_addr:' /etc/corosync/corosync.conf"; then
            cluster_error "corosync.conf is missing ring0_addr entries!"
            cluster_vm_ssh "$vm_ip" "cat /etc/corosync/corosync.conf" || true
            cluster_die "Invalid corosync.conf generated"
        fi

        rm -f "$corosync_conf"

        # Enable and start corosync
        cluster_log "Starting corosync on node $node_num"
        cluster_vm_ssh "$vm_ip" "systemctl enable corosync"
        cluster_vm_ssh "$vm_ip" "systemctl start corosync"

        # Verify corosync is running
        if ! cluster_vm_ssh "$vm_ip" "systemctl is-active corosync" &>/dev/null; then
            cluster_vm_ssh "$vm_ip" "systemctl status corosync" || true
            cluster_die "Corosync failed to start on node $node_num"
        fi

        # Load DLM kernel module
        cluster_log "Loading DLM kernel module on node $node_num"
        cluster_vm_ssh "$vm_ip" "mkdir -p /etc/modules-load.d && echo 'dlm' > /etc/modules-load.d/dlm.conf"

        # Verify module exists before trying to load
        if ! cluster_vm_ssh "$vm_ip" "ls /lib/modules/\$(uname -r)/kernel/fs/dlm/dlm.ko* 2>/dev/null" &>/dev/null; then
            cluster_error "DLM kernel module not found"
            cluster_error "Kernel version: $(cluster_vm_ssh "$vm_ip" "uname -r")"
            cluster_error "Installed kernel packages:"
            cluster_vm_ssh "$vm_ip" "rpm -qa | grep kernel" || true
            cluster_die "DLM kernel module not available - kernel-modules-extra may not be installed or kernel mismatch"
        fi

        if ! cluster_vm_ssh "$vm_ip" "modprobe dlm"; then
            cluster_error "Failed to load dlm kernel module"
            cluster_vm_ssh "$vm_ip" "dmesg | tail -20" || true
            cluster_die "DLM kernel module failed to load on node $node_num"
        fi

        # Enable and start DLM
        cluster_log "Starting DLM on node $node_num"
        cluster_vm_ssh "$vm_ip" "systemctl enable dlm"
        cluster_vm_ssh "$vm_ip" "systemctl start dlm"

        # Verify DLM is running
        if ! cluster_vm_ssh "$vm_ip" "systemctl is-active dlm" &>/dev/null; then
            cluster_vm_ssh "$vm_ip" "systemctl status dlm" || true
            cluster_die "DLM failed to start on node $node_num"
        fi

        cluster_log "DLM/Corosync configured and running on node $node_num"
    fi

    # Configure LVM (always set system_id_source, optionally enable lvmlockd)
    cluster_log "Configuring LVM on node $node_num"
    cluster_vm_ssh "$vm_ip" "mkdir -p /etc/lvm"

    # Back up existing config if present
    cluster_vm_ssh "$vm_ip" "
        if [ -f /etc/lvm/lvm.conf ]; then
            cp /etc/lvm/lvm.conf /etc/lvm/lvm.conf.backup
        fi
    "

    # Build lvm.conf content
    if [ "$CLUSTER_LOCK_TYPE" = "sanlock" ] || [ "$CLUSTER_LOCK_TYPE" = "dlm" ]; then
        cluster_log "Configuring lvm.conf with system_id_source and use_lvmlockd on node $node_num"
        cluster_vm_ssh "$vm_ip" "cat > /etc/lvm/lvm.conf <<'LVMCONF'
global {
    use_lvmlockd = 1
    system_id_source = \"uname\"
}
LVMCONF
"
        cluster_debug "Configured use_lvmlockd=1 and system_id_source=\"uname\" in lvm.conf on node $node_num"
    else
        cluster_log "Configuring lvm.conf with system_id_source on node $node_num"
        cluster_vm_ssh "$vm_ip" "cat > /etc/lvm/lvm.conf <<'LVMCONF'
global {
    system_id_source = \"uname\"
}
LVMCONF
"
        cluster_debug "Configured system_id_source=\"uname\" in lvm.conf on node $node_num"
    fi

    # Start lvmlockd (only for sanlock and dlm)
    if [ "$CLUSTER_LOCK_TYPE" = "sanlock" ] || [ "$CLUSTER_LOCK_TYPE" = "dlm" ]; then
        cluster_log "Starting lvmlockd on node $node_num"
        cluster_vm_ssh "$vm_ip" "systemctl daemon-reload"
        cluster_vm_ssh "$vm_ip" "systemctl enable lvmlockd"
        cluster_vm_ssh "$vm_ip" "systemctl start lvmlockd"

        # Verify lvmlockd is running
        if ! cluster_vm_ssh "$vm_ip" "systemctl is-active lvmlockd" &>/dev/null; then
            # Show status for debugging
            cluster_vm_ssh "$vm_ip" "systemctl status lvmlockd" || true
            cluster_die "lvmlockd failed to start on node $node_num"
        fi
    fi

    cluster_log "Lock manager setup complete on node $node_num"
}

#
# VM destruction
#

cluster_vm_destroy() {
    local vm_name="$1"

    if [ -z "$vm_name" ]; then
        cluster_die "cluster_vm_destroy: vm_name is required"
    fi

    cluster_log "Destroying VM: $vm_name"

    # Stop VM if running
    if cluster_virsh domstate "$vm_name" 2>/dev/null | grep -q running; then
        cluster_log "Stopping VM: $vm_name"
        cluster_virsh destroy "$vm_name" 2>/dev/null || true
    fi

    # Undefine VM
    if cluster_virsh dominfo "$vm_name" &>/dev/null; then
        cluster_log "Undefining VM: $vm_name"
        cluster_virsh undefine "$vm_name" --remove-all-storage 2>/dev/null || true
    fi

    # Clean up cloud-init ISO
    local disk_dir
    disk_dir=$(cluster_get_image_dir)
    local cloudinit_iso="${disk_dir}/${vm_name}-cloudinit.iso"
    if [ -f "$cloudinit_iso" ]; then
        rm -f "$cloudinit_iso"
    fi

    cluster_log "VM destroyed: $vm_name"
}

#
# Storage export setup
#

cluster_vm_setup_storage_export() {
    local node0_ip="$1"
    local cluster_id="$2"

    if [ -z "$node0_ip" ] || [ -z "$cluster_id" ]; then
        cluster_die "cluster_vm_setup_storage_export: node0_ip and cluster_id are required"
    fi

    cluster_log "Setting up storage export on node 0 ($node0_ip)"

    # Copy required scripts to node 0
    local remote_dir="/root/cluster-scripts"
    cluster_vm_ssh "$node0_ip" "mkdir -p $remote_dir"

    cluster_log "Copying cluster scripts to node 0"
    cluster_vm_scp "$SCRIPT_DIR/cluster-test-lib.sh" "${CLUSTER_SSH_USER}@${node0_ip}:${remote_dir}/"
    cluster_vm_scp "$SCRIPT_DIR/cluster-storage-exporter.sh" "${CLUSTER_SSH_USER}@${node0_ip}:${remote_dir}/"

    # Make scripts executable
    cluster_vm_ssh "$node0_ip" "chmod +x ${remote_dir}/*.sh"

    # Execute storage setup on node 0 with required environment variables
    cluster_log "Executing storage export setup on node 0"

    # Build environment variables based on what's configured
    local export_env="
        export CLUSTER_NUM_SCSI='${CLUSTER_NUM_SCSI:-0}'
        export CLUSTER_NUM_NVME='${CLUSTER_NUM_NVME:-0}'
        export CLUSTER_NUM_MULTIPATH='${CLUSTER_NUM_MULTIPATH:-0}'
        export NODE0_IP='${node0_ip}'
        export CLUSTER_DEBUG='${CLUSTER_DEBUG:-0}'
    "

    # Add iSCSI configuration if enabled
    if [ "${CLUSTER_NUM_SCSI:-0}" -gt 0 ]; then
        cluster_log "DEBUG: Exporting iSCSI config to node 0:"
        cluster_log "  CLUSTER_SCSI_SIZE='${CLUSTER_SCSI_SIZE}'"
        cluster_log "  CLUSTER_SCSI_SECTOR_SIZE='${CLUSTER_SCSI_SECTOR_SIZE:-512}'"
        cluster_log "  CLUSTER_SCSI_BACKING_TYPE='${CLUSTER_SCSI_BACKING_TYPE}'"
        cluster_log "  CLUSTER_SCSI_OPTIMAL_IO_SIZE='${CLUSTER_SCSI_OPTIMAL_IO_SIZE:-}'"

        export_env+="
        export CLUSTER_SCSI_SIZE='${CLUSTER_SCSI_SIZE}'
        export CLUSTER_SCSI_SECTOR_SIZE='${CLUSTER_SCSI_SECTOR_SIZE:-512}'
        export CLUSTER_SCSI_BACKING_TYPE='${CLUSTER_SCSI_BACKING_TYPE}'
        export CLUSTER_SCSI_OPTIMAL_IO_SIZE='${CLUSTER_SCSI_OPTIMAL_IO_SIZE:-}'
        "
    fi

    # Add NVMe configuration if enabled
    if [ "${CLUSTER_NUM_NVME:-0}" -gt 0 ]; then
        export_env+="
        export CLUSTER_NVME_SIZE='${CLUSTER_NVME_SIZE}'
        export CLUSTER_NVME_SECTOR_SIZE='${CLUSTER_NVME_SECTOR_SIZE:-512}'
        export CLUSTER_NVME_BACKING_TYPE='${CLUSTER_NVME_BACKING_TYPE}'
        "
    fi

    # Add multipath configuration if enabled
    if [ "${CLUSTER_NUM_MULTIPATH:-0}" -gt 0 ]; then
        export_env+="
        export CLUSTER_MULTIPATH_PATHS='${CLUSTER_MULTIPATH_PATHS:-2}'
        export CLUSTER_MULTIPATH_SIZE='${CLUSTER_MULTIPATH_SIZE}'
        export CLUSTER_MULTIPATH_SECTOR_SIZE='${CLUSTER_MULTIPATH_SECTOR_SIZE:-512}'
        export CLUSTER_MULTIPATH_BACKING_TYPE='${CLUSTER_MULTIPATH_BACKING_TYPE}'
        export CLUSTER_MULTIPATH_OPTIMAL_IO_SIZE='${CLUSTER_MULTIPATH_OPTIMAL_IO_SIZE:-}'
        "
    fi

    cluster_vm_ssh "$node0_ip" "
        $export_env
        cd ${remote_dir}
        ./cluster-storage-exporter.sh '${cluster_id}'
    " || {
        cluster_error "Failed to setup storage export on node 0"
        return 1
    }

    cluster_log "Storage export setup complete on node 0"
    if [ "${CLUSTER_NUM_SCSI:-0}" -gt 0 ]; then
        cluster_log "  iSCSI: ${CLUSTER_NUM_SCSI} devices, ${CLUSTER_SCSI_SIZE}MB, ${CLUSTER_SCSI_SECTOR_SIZE}-byte sectors, ${CLUSTER_SCSI_BACKING_TYPE}"
    fi
    if [ "${CLUSTER_NUM_NVME:-0}" -gt 0 ]; then
        cluster_log "  NVMe: ${CLUSTER_NUM_NVME} devices, ${CLUSTER_NVME_SIZE}MB, ${CLUSTER_NVME_SECTOR_SIZE}-byte sectors, ${CLUSTER_NVME_BACKING_TYPE}"
    fi
    if [ "${CLUSTER_NUM_MULTIPATH:-0}" -gt 0 ]; then
        cluster_log "  Multipath: ${CLUSTER_NUM_MULTIPATH} devices, ${CLUSTER_MULTIPATH_PATHS} paths/device, ${CLUSTER_MULTIPATH_SIZE}MB, ${CLUSTER_MULTIPATH_SECTOR_SIZE}-byte sectors, ${CLUSTER_MULTIPATH_BACKING_TYPE}"
    fi

    return 0
}

#
# Storage import setup
#

cluster_vm_setup_storage_import() {
    local node_ip="$1"
    local node_num="$2"
    local node0_ip="$3"
    local cluster_id="$4"

    if [ -z "$node_ip" ] || [ -z "$node_num" ] || [ -z "$node0_ip" ] || [ -z "$cluster_id" ]; then
        cluster_die "cluster_vm_setup_storage_import: all arguments are required"
    fi

    cluster_log "Setting up storage import on node $node_num ($node_ip)"

    # Copy storage importer script to test node
    local remote_dir="/root/cluster-scripts"
    cluster_vm_ssh "$node_ip" "mkdir -p $remote_dir"

    cluster_debug "Copying cluster-storage-importer.sh to node $node_num"
    cluster_vm_scp "$SCRIPT_DIR/cluster-storage-importer.sh" "${CLUSTER_SSH_USER}@${node_ip}:${remote_dir}/"

    # Copy cluster-test-lib.sh (needed by storage importer)
    cluster_debug "Copying cluster-test-lib.sh to node $node_num"
    cluster_vm_scp "$SCRIPT_DIR/cluster-test-lib.sh" "${CLUSTER_SSH_USER}@${node_ip}:${remote_dir}/"

    # Make scripts executable
    cluster_vm_ssh "$node_ip" "chmod +x ${remote_dir}/*.sh"

    # Execute storage import on test node with required environment variables
    cluster_log "Executing storage import on node $node_num"
    cluster_vm_ssh "$node_ip" "
        export CLUSTER_NUM_SCSI='${CLUSTER_NUM_SCSI:-0}'
        export CLUSTER_NUM_NVME='${CLUSTER_NUM_NVME:-0}'
        export CLUSTER_NUM_MULTIPATH='${CLUSTER_NUM_MULTIPATH:-0}'
        export CLUSTER_MULTIPATH_PATHS='${CLUSTER_MULTIPATH_PATHS:-2}'
        export CLUSTER_MULTIPATH_SIZE='${CLUSTER_MULTIPATH_SIZE:-1024}'
        export CLUSTER_MULTIPATH_SECTOR_SIZE='${CLUSTER_MULTIPATH_SECTOR_SIZE:-512}'
        export CLUSTER_MULTIPATH_BACKING_TYPE='${CLUSTER_MULTIPATH_BACKING_TYPE:-sparsefile}'
        export NODE0_IP='${node0_ip}'
        export THIS_NODE_IP='${node_ip}'
        export CLUSTER_DEBUG='${CLUSTER_DEBUG:-0}'

        cd ${remote_dir}
        ./cluster-storage-importer.sh '${cluster_id}' '${node_num}'
    " || {
        cluster_error "Failed to import storage on node $node_num"
        return 1
    }

    cluster_log "Storage import complete on node $node_num"
    return 0
}

#
# Golden image -- build a test-node image with packages pre-installed
# so that multiple test nodes can COW from it instead of each doing
# their own package install.
#

cluster_golden_image_build() {
    local cluster_id="$1"

    if [ -z "$cluster_id" ]; then
        cluster_die "cluster_golden_image_build: cluster_id is required"
    fi

    local disk_dir
    disk_dir=$(cluster_get_image_dir)
    local golden_image="${disk_dir}/${cluster_id}-golden-testnode.qcow2"

    cluster_log "Building golden image for test nodes"

    # Create a temporary builder VM using the standard create function.
    # Use cluster_id "golden-builder" so the VM name becomes
    # "${cluster_id}-golden-builder" (via get_name appending "-node1").
    local builder_cluster_id="${cluster_id}-golden"
    cluster_vm_create 1 "$builder_cluster_id" "$CLUSTER_NODE_OS_IMAGE" >&2

    local tmp_vm_name
    tmp_vm_name=$(cluster_vm_get_name "$builder_cluster_id" 1)

    local builder_ip
    builder_ip=$(cluster_vm_wait_boot "$tmp_vm_name")
    if [ -z "$builder_ip" ]; then
        cluster_warn "Golden image build failed: could not get builder IP"
        cluster_virsh destroy "$tmp_vm_name" >/dev/null 2>&1 || true
        cluster_virsh undefine "$tmp_vm_name" --remove-all-storage >/dev/null 2>&1 || true
        return 1
    fi

    cluster_vm_setup_ssh "$builder_ip"
    cluster_vm_expand_rootfs "$builder_ip" >&2

    # Install the same packages a test node (node_num=1) would get
    if ! cluster_vm_install_packages "$builder_ip" 1 >&2; then
        cluster_warn "Golden image build failed: package installation failed"
        cluster_virsh destroy "$tmp_vm_name" >/dev/null 2>&1 || true
        cluster_virsh undefine "$tmp_vm_name" --remove-all-storage >/dev/null 2>&1 || true
        return 1
    fi

    cluster_vm_setup_chrony "$builder_ip" 1 >&2

    # Clean cloud-init state so it re-runs on derived VMs
    cluster_vm_ssh "$builder_ip" "
        cloud-init clean --logs 2>/dev/null ||
        rm -rf /var/lib/cloud/instances /var/lib/cloud/instance \
               /var/lib/cloud/data /var/log/cloud-init*
    " >/dev/null 2>&1 || cluster_warn "cloud-init cleanup may have failed"

    # Clean machine-id so each derived VM gets a unique one
    cluster_vm_ssh "$builder_ip" "
        truncate -s 0 /etc/machine-id 2>/dev/null || true
        rm -f /var/lib/dbus/machine-id 2>/dev/null || true
    " >/dev/null 2>&1 || true

    # Clean NVMe host identity so each derived VM gets a unique one.
    # Without this, all VMs cloned from the golden image share the same
    # hostnqn/hostid, which breaks NVMe persistent reservations (the
    # nvmet target identifies PR registrants by hostid).
    cluster_vm_ssh "$builder_ip" "
        rm -f /etc/nvme/hostnqn /etc/nvme/hostid 2>/dev/null || true
    " >/dev/null 2>&1 || true

    # Shut down the builder VM cleanly
    cluster_vm_ssh "$builder_ip" "shutdown -h now" 2>/dev/null || true

    # Wait for the VM to reach shut-off state
    local wait_count=0
    while [ $wait_count -lt 60 ]; do
        local state
        state=$(cluster_virsh domstate "$tmp_vm_name" 2>/dev/null || echo "unknown")
        if [ "$state" = "shut off" ]; then
            break
        fi
        sleep 2
        wait_count=$((wait_count + 1))
    done

    if [ $wait_count -ge 60 ]; then
        cluster_warn "Builder VM did not shut down cleanly, forcing"
        cluster_virsh destroy "$tmp_vm_name" >/dev/null 2>&1 || true
    fi

    # Convert the COW overlay to a standalone qcow2 (no backing file dependency)
    local tmp_disk="${disk_dir}/${tmp_vm_name}.qcow2"
    cluster_log "Converting builder disk to standalone golden image"
    if ! qemu-img convert -f qcow2 -O qcow2 "$tmp_disk" "$golden_image"; then
        cluster_warn "Golden image build failed: qemu-img convert failed"
        cluster_virsh undefine "$tmp_vm_name" --remove-all-storage 2>/dev/null || true
        rm -f "$golden_image"
        return 1
    fi

    # Clean up builder VM and its disk/ISO files
    cluster_virsh undefine "$tmp_vm_name" --remove-all-storage >/dev/null 2>&1 || true
    rm -f "${disk_dir}/${tmp_vm_name}-cloudinit.iso" 2>/dev/null || true

    cluster_log "Golden image built: $golden_image"
    echo "$golden_image"
}

cluster_golden_image_delete() {
    local cluster_id="$1"
    local disk_dir
    disk_dir=$(cluster_get_image_dir)
    local golden_image="${disk_dir}/${cluster_id}-golden-testnode.qcow2"

    if [ -f "$golden_image" ]; then
        cluster_log "Removing golden image: $golden_image"
        rm -f "$golden_image"
    fi
}

#
# Batch operations
#

cluster_vms_create_all() {
    local cluster_id="$1"

    if [ -z "$cluster_id" ]; then
        cluster_die "cluster_vms_create_all: cluster_id is required"
    fi

    cluster_session_prepare

    local num_nodes="${CLUSTER_NUM_NODES:-3}"
    local node_ips=()

    # Create node 0 (storage exporter)
    cluster_log "Creating node 0 (storage exporter)"
    cluster_vm_create 0 "$cluster_id"

    local node0_ip=$(cluster_vm_wait_boot "$(cluster_vm_get_name "$cluster_id" 0)")
    if [ -z "$node0_ip" ]; then
        cluster_die "Failed to get IP for node 0"
    fi

    cluster_vm_setup_ssh "$node0_ip"
    cluster_vm_expand_rootfs "$node0_ip"
    cluster_vm_install_packages "$node0_ip" 0
    cluster_vm_setup_chrony "$node0_ip" 0

    # Set up storage export on node 0
    cluster_vm_setup_storage_export "$node0_ip" "$cluster_id"

    node_ips+=("$node0_ip")

    # Build a golden image with packages pre-installed for test nodes.
    # All test nodes will COW from this image, avoiding repeated package installs.
    local golden_image=""
    if [ "$num_nodes" -gt 1 ]; then
        golden_image=$(cluster_golden_image_build "$cluster_id")
        if [ -n "$golden_image" ] && [ -f "$golden_image" ]; then
            cluster_log "Using golden image for test nodes: $golden_image"
        else
            cluster_warn "Golden image build failed, falling back to per-node package install"
            golden_image=""
        fi
    fi

    # Create test nodes 1..N and build IP array
    for node_num in $(seq 1 "$num_nodes"); do
        cluster_log "Creating node $node_num (test node)"

        if [ -n "$golden_image" ]; then
            # Fast path: create VM from golden image, skip package install
            cluster_vm_create "$node_num" "$cluster_id" "$golden_image"

            local node_ip=$(cluster_vm_wait_boot "$(cluster_vm_get_name "$cluster_id" "$node_num")")
            if [ -z "$node_ip" ]; then
                cluster_die "Failed to get IP for node $node_num"
            fi

            cluster_vm_setup_ssh "$node_ip"
        else
            # Slow path: create VM from base image, install packages individually
            cluster_vm_create "$node_num" "$cluster_id"

            local node_ip=$(cluster_vm_wait_boot "$(cluster_vm_get_name "$cluster_id" "$node_num")")
            if [ -z "$node_ip" ]; then
                cluster_die "Failed to get IP for node $node_num"
            fi

            cluster_vm_setup_ssh "$node_ip"
            cluster_vm_expand_rootfs "$node_ip"
            cluster_vm_install_packages "$node_ip" "$node_num"
            cluster_vm_setup_chrony "$node_ip" "$node_num"
        fi

        # Deploy sanlock source first (if enabled)
        cluster_vm_deploy_sanlock_source "$node_ip" "$node_num"

        # Deploy LVM source (if enabled)
        cluster_vm_deploy_lvm_source "$node_ip" "$node_num"

        node_ips+=("$node_ip")
    done

    # Export node IPs for later use (needed for corosync config generation)
    export CLUSTER_NODE_IPS=("${node_ips[@]}")

    cluster_log "All VMs created, node IPs: ${CLUSTER_NODE_IPS[*]}"

    # Import storage on test nodes from node 0
    cluster_log "Setting up storage import on test nodes"
    for node_num in $(seq 1 "$num_nodes"); do
        local node_ip="${CLUSTER_NODE_IPS[$node_num]}"
        cluster_vm_setup_storage_import "$node_ip" "$node_num" "$node0_ip" "$cluster_id" || {
            cluster_error "Failed to setup storage import on node $node_num"
            return 1
        }
    done

    # Now set up lock managers (requires all node IPs for DLM/corosync)
    cluster_log "Setting up lock managers on test nodes"
    for node_num in $(seq 1 "$num_nodes"); do
        local node_ip="${CLUSTER_NODE_IPS[$node_num]}"
        cluster_vm_setup_lock_manager "$node_ip" "$node_num"
    done

    cluster_log "All VMs created successfully"
    cluster_log "Node IPs: ${CLUSTER_NODE_IPS[*]}"
}

cluster_vms_destroy_all() {
    local cluster_id="$1"

    if [ -z "$cluster_id" ]; then
        cluster_die "cluster_vms_destroy_all: cluster_id is required"
    fi

    # Discover all VMs matching this cluster ID
    # This works even if state file is missing or corrupt
    local vm_pattern="${cluster_id}"
    local discovered_vms=()

    cluster_log "Discovering VMs matching pattern: ${vm_pattern}*"

    # Find all VMs (running and shut off) matching the cluster ID
    while IFS= read -r vm_name; do
        if [ -n "$vm_name" ]; then
            discovered_vms+=("$vm_name")
            cluster_debug "Found VM: $vm_name"
        fi
    done < <(cluster_virsh list --all --name 2>/dev/null | grep "^${vm_pattern}" || true)

    if [ ${#discovered_vms[@]} -eq 0 ]; then
        cluster_warn "No VMs found matching pattern: ${vm_pattern}*"
        cluster_log "Cluster may have already been destroyed"
        return 0
    fi

    cluster_log "Found ${#discovered_vms[@]} VM(s) to destroy"

    # Destroy test nodes (node1+) first so they drop iSCSI/NVMe sessions
    # before we ask node0 to clean up storage targets.
    local node0_vm_name=$(cluster_vm_get_name "$cluster_id" 0)
    for vm_name in "${discovered_vms[@]}"; do
        [ "$vm_name" = "$node0_vm_name" ] && continue
        cluster_log "Destroying test VM: $vm_name"

        local vm_state=$(cluster_virsh domstate "$vm_name" 2>/dev/null || echo "unknown")
        if [ "$vm_state" = "running" ]; then
            cluster_debug "Force stopping VM: $vm_name"
            cluster_virsh destroy "$vm_name" 2>/dev/null || cluster_warn "Failed to force stop $vm_name"
        fi

        cluster_debug "Undefining VM and removing storage: $vm_name"
        cluster_virsh undefine "$vm_name" --remove-all-storage 2>/dev/null || \
        cluster_virsh undefine "$vm_name" --storage vda 2>/dev/null || \
        cluster_virsh undefine "$vm_name" 2>/dev/null || \
        cluster_warn "Failed to undefine $vm_name (may already be removed)"
    done

    # Now clean up storage on node0 (iSCSI/NVMe targets).
    # Test nodes are gone so targetcli delete won't block on active sessions.
    if cluster_virsh dominfo "$node0_vm_name" &>/dev/null 2>&1; then
        cluster_log "Attempting storage cleanup on node 0"
        local node0_ip=$(cluster_vm_get_ip "$node0_vm_name" 2>/dev/null || true)
        if [ -n "$node0_ip" ]; then
            cluster_debug "Cleaning up storage on node 0 ($node0_ip)"
            timeout 60 \
            cluster_vm_ssh "$node0_ip" "
                export CLUSTER_NUM_SCSI='${CLUSTER_NUM_SCSI:-0}'
                export CLUSTER_NUM_NVME='${CLUSTER_NUM_NVME:-0}'
                export CLUSTER_DEBUG='${CLUSTER_DEBUG:-0}'
                cd /root/cluster-scripts 2>/dev/null || cd /tmp
                if [ -f cluster-storage-exporter.sh ]; then
                    source ./cluster-test-lib.sh
                    source ./cluster-storage-exporter.sh
                    cluster_storage_cleanup '${cluster_id}'
                fi
            " 2>/dev/null || cluster_debug "Storage cleanup failed or timed out (VM may be down)"
        fi
    fi

    # Destroy node0
    if [ -n "$node0_vm_name" ]; then
        cluster_log "Destroying node0 VM: $node0_vm_name"

        local vm_state=$(cluster_virsh domstate "$node0_vm_name" 2>/dev/null || echo "unknown")
        if [ "$vm_state" = "running" ]; then
            cluster_debug "Force stopping VM: $node0_vm_name"
            cluster_virsh destroy "$node0_vm_name" 2>/dev/null || cluster_warn "Failed to force stop $node0_vm_name"
        fi

        cluster_debug "Undefining VM and removing storage: $node0_vm_name"
        cluster_virsh undefine "$node0_vm_name" --remove-all-storage 2>/dev/null || \
        cluster_virsh undefine "$node0_vm_name" --storage vda 2>/dev/null || \
        cluster_virsh undefine "$node0_vm_name" 2>/dev/null || \
        cluster_warn "Failed to undefine $node0_vm_name (may already be removed)"
    fi

    # Clean up any leftover image files from a failed create.
    # If create failed after creating disk/ISO files but before defining the VM,
    # those files won't be removed by cluster_virsh undefine --remove-all-storage.
    # Also catches golden builder VM leftovers (${cluster_id}-golden-*).
    local disk_dir
    disk_dir=$(cluster_get_image_dir)
    local leftover_files=$(ls "${disk_dir}/${cluster_id}"-node* "${disk_dir}/${cluster_id}"-golden-* 2>/dev/null || true)
    if [ -n "$leftover_files" ]; then
        cluster_log "Cleaning up leftover image files"
        echo "$leftover_files" | while read -r f; do
            cluster_debug "Removing leftover file: $f"
            rm -f "$f"
        done
    fi

    cluster_log "All VMs destroyed"
}

#
# VM Stop/Start (Hibernate/Resume) functions
#

cluster_vm_save() {
    local vm_name="$1"
    local save_path="$2"

    if [ -z "$vm_name" ] || [ -z "$save_path" ]; then
        cluster_die "cluster_vm_save: vm_name and save_path are required"
    fi

    cluster_debug "Saving VM $vm_name to $save_path"

    # Check if VM is running
    local vm_state=$(cluster_virsh domstate "$vm_name" 2>/dev/null || echo "unknown")
    if [ "$vm_state" != "running" ]; then
        cluster_warn "VM $vm_name is not running (state: $vm_state), cannot save"
        return 1
    fi

    # Save VM to disk (hibernate)
    cluster_virsh save "$vm_name" "$save_path" || {
        cluster_error "Failed to save VM $vm_name to $save_path"
        return 1
    }

    cluster_debug "VM $vm_name saved successfully"
    return 0
}

cluster_vm_restore_from_file() {
    local save_path="$1"

    if [ -z "$save_path" ]; then
        cluster_die "cluster_vm_restore_from_file: save_path is required"
    fi

    if [ ! -f "$save_path" ]; then
        cluster_error "Save file not found: $save_path"
        return 1
    fi

    cluster_debug "Restoring VM from $save_path"

    # Restore VM from save file
    cluster_virsh restore "$save_path" || {
        cluster_error "Failed to restore VM from $save_path"
        return 1
    }

    cluster_debug "VM restored successfully from $save_path"
    return 0
}

cluster_vms_pause_all() {
    local cluster_id="$1"

    if [ -z "$cluster_id" ]; then
        cluster_die "cluster_vms_pause_all: cluster_id is required"
    fi

    cluster_log "Pausing all VMs for cluster: $cluster_id"

    # Determine number of VMs to pause (node 0 + test nodes 1..N)
    local num_nodes=${CLUSTER_NUM_NODES:-3}
    local save_dir
    save_dir=$(cluster_get_image_dir)
    local all_saved=0
    local saved_count=0

    # Pause all VMs (node 0 through N)
    for node_num in $(seq 0 "$num_nodes"); do
        local vm_name=$(cluster_vm_get_name "$cluster_id" "$node_num")
        local save_path="${save_dir}/${vm_name}.save"

        # Check if VM exists
        if ! cluster_virsh dominfo "$vm_name" &>/dev/null; then
            cluster_debug "VM $vm_name does not exist, skipping"
            continue
        fi

        # Check if VM is running
        local vm_state=$(cluster_virsh domstate "$vm_name" 2>/dev/null || echo "unknown")
        if [ "$vm_state" != "running" ]; then
            cluster_debug "VM $vm_name is not running (state: $vm_state), skipping"
            continue
        fi

        cluster_log "Saving VM: $vm_name"
        if cluster_vm_save "$vm_name" "$save_path"; then
            saved_count=$((saved_count + 1))
        else
            cluster_error "Failed to save VM: $vm_name"
            all_saved=1
        fi
    done

    if [ $all_saved -ne 0 ]; then
        cluster_error "Some VMs failed to save"
        return 1
    fi

    if [ $saved_count -eq 0 ]; then
        cluster_warn "No running VMs found to pause"
        return 1
    fi

    cluster_log "Successfully paused $saved_count VM(s)"
    return 0
}

cluster_vms_resume_all() {
    local cluster_id="$1"

    if [ -z "$cluster_id" ]; then
        cluster_die "cluster_vms_resume_all: cluster_id is required"
    fi

    cluster_log "Resuming all VMs for cluster: $cluster_id"

    # Determine number of VMs to start (node 0 + test nodes 1..N)
    local num_nodes=${CLUSTER_NUM_NODES:-3}
    local save_dir
    save_dir=$(cluster_get_image_dir)
    local all_restored=0
    local restored_count=0

    # Restore all VMs (node 0 through N)
    for node_num in $(seq 0 "$num_nodes"); do
        local vm_name=$(cluster_vm_get_name "$cluster_id" "$node_num")
        local save_path="${save_dir}/${vm_name}.save"

        # Check if save file exists
        if [ ! -f "$save_path" ]; then
            cluster_debug "Save file not found for $vm_name: $save_path"
            continue
        fi

        cluster_log "Restoring VM: $vm_name"
        if cluster_vm_restore_from_file "$save_path"; then
            restored_count=$((restored_count + 1))
            # Remove save file after successful restore
            rm -f "$save_path" || cluster_warn "Failed to remove save file: $save_path"
        else
            cluster_error "Failed to restore VM: $vm_name"
            all_restored=1
        fi
    done

    if [ $all_restored -ne 0 ]; then
        cluster_error "Some VMs failed to restore"
        return 1
    fi

    if [ $restored_count -eq 0 ]; then
        cluster_error "No save files found to restore"
        return 1
    fi

    cluster_log "Successfully restored $restored_count VM(s)"

    # Wait for all VMs to be accessible via SSH
    cluster_log "Waiting for VMs to be accessible"
    for node_num in $(seq 0 "$num_nodes"); do
        local vm_name=$(cluster_vm_get_name "$cluster_id" "$node_num")

        # Skip if VM doesn't exist
        if ! cluster_virsh dominfo "$vm_name" &>/dev/null; then
            continue
        fi

        # Get IP address (retry — DHCP lease may be stale after restore)
        local node_ip=""
        local ip_attempt=0
        while [ $ip_attempt -lt 10 ]; do
            node_ip=$(cluster_vm_get_ip "$vm_name" 2>/dev/null || true)
            [ -n "$node_ip" ] && break
            ip_attempt=$((ip_attempt + 1))
            sleep 2
        done
        if [ -z "$node_ip" ]; then
            cluster_warn "Could not get IP for $vm_name after retries, skipping SSH check"
            continue
        fi

        cluster_debug "Waiting for SSH on $vm_name ($node_ip)"

        # Wait for SSH with timeout
        local max_attempts=30
        local attempt=0
        while [ $attempt -lt $max_attempts ]; do
            if cluster_vm_ssh "$node_ip" "echo 'SSH ready'" &>/dev/null; then
                cluster_debug "SSH ready on $vm_name"
                break
            fi
            attempt=$((attempt + 1))
            sleep 2
        done

        if [ $attempt -eq $max_attempts ]; then
            cluster_warn "SSH not ready on $vm_name after $max_attempts attempts"
        fi
    done

    cluster_log "All VMs started successfully"
    return 0
}

#
# VM Snapshot functions
#

cluster_vms_snapshot_all() {
    local cluster_id="$1"
    local snapshot_name="$2"

    if [ -z "$cluster_id" ] || [ -z "$snapshot_name" ]; then
        cluster_die "cluster_vms_snapshot_all: cluster_id and snapshot_name are required"
    fi

    cluster_log "Creating snapshot for cluster: $cluster_id"
    cluster_log "Snapshot name: $snapshot_name"

    # Determine number of VMs to snapshot (node 0 + test nodes 1..N)
    local num_nodes=${CLUSTER_NUM_NODES:-3}
    local save_dir
    save_dir=$(cluster_get_image_dir)
    local was_running=0
    local all_saved=0
    local saved_count=0

    # Check if any VMs are running
    for node_num in $(seq 0 "$num_nodes"); do
        local vm_name=$(cluster_vm_get_name "$cluster_id" "$node_num")

        if ! cluster_virsh dominfo "$vm_name" &>/dev/null; then
            continue
        fi

        local vm_state=$(cluster_virsh domstate "$vm_name" 2>/dev/null || echo "unknown")
        if [ "$vm_state" = "running" ]; then
            was_running=1
            break
        fi
    done

    # Snapshot all VMs (node 0 through N)
    for node_num in $(seq 0 "$num_nodes"); do
        local vm_name=$(cluster_vm_get_name "$cluster_id" "$node_num")
        local snapshot_path="${save_dir}/${vm_name}-snapshot-${snapshot_name}.save"

        # Check if VM exists
        if ! cluster_virsh dominfo "$vm_name" &>/dev/null; then
            cluster_debug "VM $vm_name does not exist, skipping"
            continue
        fi

        # Check if VM is running
        local vm_state=$(cluster_virsh domstate "$vm_name" 2>/dev/null || echo "unknown")
        local disk_path="${save_dir}/${vm_name}.qcow2"

        if [ "$vm_state" = "running" ]; then
            cluster_log "Snapshotting running VM: $vm_name (memory + disk)"
            # Save memory state (this also shuts down the VM)
            if cluster_vm_save "$vm_name" "$snapshot_path"; then
                # Also copy the disk for a complete snapshot
                if [ -f "$disk_path" ]; then
                    cp -a "$disk_path" "${disk_path}.snapshot-${snapshot_name}" || {
                        cluster_error "Failed to copy disk for $vm_name"
                        all_saved=1
                        continue
                    }
                    saved_count=$((saved_count + 1))
                else
                    cluster_warn "Disk not found for $vm_name: $disk_path"
                    all_saved=1
                fi
            else
                cluster_error "Failed to snapshot VM: $vm_name"
                all_saved=1
            fi
        elif [ "$vm_state" = "shut off" ]; then
            # For shut off VMs, copy the disk image only
            cluster_log "Snapshotting shut off VM: $vm_name (disk only)"
            if [ -f "$disk_path" ]; then
                cp -a "$disk_path" "${disk_path}.snapshot-${snapshot_name}" || {
                    cluster_error "Failed to copy disk for $vm_name"
                    all_saved=1
                    continue
                }
                # Create a marker file to indicate this is a disk-only snapshot
                touch "${save_dir}/${vm_name}-snapshot-${snapshot_name}.disk"
                saved_count=$((saved_count + 1))
            else
                cluster_warn "Disk not found for $vm_name: $disk_path"
            fi
        else
            cluster_debug "VM $vm_name is in state $vm_state, skipping"
        fi
    done

    if [ $all_saved -ne 0 ]; then
        cluster_error "Some VMs failed to snapshot"
        return 1
    fi

    if [ $saved_count -eq 0 ]; then
        cluster_error "No VMs found to snapshot"
        return 1
    fi

    cluster_log "Successfully created snapshot for $saved_count VM(s)"

    # If cluster was running before snapshot, restore it
    if [ $was_running -eq 1 ]; then
        cluster_log "Restoring cluster after snapshot"
        for node_num in $(seq 0 "$num_nodes"); do
            local vm_name=$(cluster_vm_get_name "$cluster_id" "$node_num")
            local snapshot_path="${save_dir}/${vm_name}-snapshot-${snapshot_name}.save"

            # Restore VM if it was saved
            if [ -f "$snapshot_path" ]; then
                cluster_debug "Restoring VM: $vm_name"
                if ! cluster_vm_restore_from_file "$snapshot_path"; then
                    cluster_error "Failed to restore VM after snapshot: $vm_name"
                fi
                # Keep the snapshot file (don't delete it)
            fi
        done

        # Wait for all restored VMs to be accessible via SSH
        cluster_log "Waiting for VMs to be accessible after snapshot"
        for node_num in $(seq 0 "$num_nodes"); do
            local vm_name=$(cluster_vm_get_name "$cluster_id" "$node_num")

            if ! cluster_virsh dominfo "$vm_name" &>/dev/null; then
                continue
            fi

            local vm_state=$(cluster_virsh domstate "$vm_name" 2>/dev/null || echo "unknown")
            if [ "$vm_state" != "running" ]; then
                continue
            fi

            # Retry — DHCP lease may be stale after snapshot restore
            local node_ip=""
            local ip_attempt=0
            while [ $ip_attempt -lt 10 ]; do
                node_ip=$(cluster_vm_get_ip "$vm_name" 2>/dev/null || true)
                [ -n "$node_ip" ] && break
                ip_attempt=$((ip_attempt + 1))
                sleep 2
            done
            if [ -z "$node_ip" ]; then
                cluster_warn "Could not get IP for $vm_name after retries, skipping SSH check"
                continue
            fi

            cluster_debug "Waiting for SSH on $vm_name ($node_ip)"
            local max_attempts=30
            local attempt=0
            while [ $attempt -lt $max_attempts ]; do
                if cluster_vm_ssh "$node_ip" "echo 'SSH ready'" &>/dev/null; then
                    cluster_debug "SSH ready on $vm_name"
                    break
                fi
                attempt=$((attempt + 1))
                sleep 2
            done

            if [ $attempt -eq $max_attempts ]; then
                cluster_warn "SSH not ready on $vm_name after $max_attempts attempts"
            fi
        done

        cluster_vms_reconnect_iscsi "$cluster_id"
        cluster_vms_reconnect_nvme "$cluster_id"
        cluster_log "Cluster restored after snapshot"
    fi

    return 0
}

cluster_vms_restart_from_snapshot() {
    local cluster_id="$1"
    local snapshot_name="$2"

    if [ -z "$cluster_id" ]; then
        cluster_die "cluster_vms_restart_from_snapshot: cluster_id is required"
    fi

    if [ -z "$snapshot_name" ]; then
        cluster_die "cluster_vms_restart_from_snapshot: snapshot_name is required"
    fi

    cluster_log "Restarting cluster from snapshot: $snapshot_name"

    # Verify snapshot exists
    local num_nodes=${CLUSTER_NUM_NODES:-3}
    local save_dir
    save_dir=$(cluster_get_image_dir)
    local snapshot_exists=0

    for node_num in $(seq 0 "$num_nodes"); do
        local vm_name=$(cluster_vm_get_name "$cluster_id" "$node_num")
        local disk_snapshot_path="${save_dir}/${vm_name}.qcow2.snapshot-${snapshot_name}"

        if [ -f "$disk_snapshot_path" ]; then
            snapshot_exists=1
            break
        fi
    done

    if [ $snapshot_exists -eq 0 ]; then
        cluster_error "No snapshot '$snapshot_name' found for cluster: $cluster_id"
        return 1
    fi

    # Step 1: Clean up current VM state
    cluster_log "Cleaning up current VM state"
    for node_num in $(seq 0 "$num_nodes"); do
        local vm_name=$(cluster_vm_get_name "$cluster_id" "$node_num")

        if ! cluster_virsh dominfo "$vm_name" &>/dev/null; then
            continue
        fi

        # Destroy running VMs
        local vm_state=$(cluster_virsh domstate "$vm_name" 2>/dev/null || echo "unknown")
        if [ "$vm_state" = "running" ] || [ "$vm_state" = "paused" ]; then
            cluster_debug "Destroying VM: $vm_name"
            cluster_virsh destroy "$vm_name" 2>/dev/null || cluster_warn "Failed to destroy $vm_name"
        fi

        # Delete any .save files from stop command
        local save_path="${save_dir}/${vm_name}.save"
        if [ -f "$save_path" ]; then
            cluster_debug "Deleting save file: $save_path"
            rm -f "$save_path" || cluster_warn "Failed to delete $save_path"
        fi
    done

    # Step 2: Replace current disk with snapshot and start VMs
    local all_restarted=0
    local restarted_count=0

    for node_num in $(seq 0 "$num_nodes"); do
        local vm_name=$(cluster_vm_get_name "$cluster_id" "$node_num")
        local snapshot_memory_path="${save_dir}/${vm_name}-snapshot-${snapshot_name}.save"
        local disk_path="${save_dir}/${vm_name}.qcow2"
        local disk_snapshot_path="${disk_path}.snapshot-${snapshot_name}"
        local disk_snapshot_marker="${save_dir}/${vm_name}-snapshot-${snapshot_name}.disk"

        # Check if disk snapshot exists
        if [ ! -f "$disk_snapshot_path" ]; then
            cluster_debug "No disk snapshot for $vm_name, skipping"
            continue
        fi

        cluster_log "Restarting VM from snapshot: $vm_name"

        # Delete current disk and replace with snapshot
        cluster_debug "Replacing disk with snapshot for $vm_name"
        rm -f "$disk_path" || {
            cluster_error "Failed to delete current disk for $vm_name"
            all_restarted=1
            continue
        }

        cp -a "$disk_snapshot_path" "$disk_path" || {
            cluster_error "Failed to copy snapshot disk for $vm_name"
            all_restarted=1
            continue
        }

        # Start the VM (either from memory snapshot or fresh boot)
        if [ -f "$snapshot_memory_path" ]; then
            # This was a running VM when snapshotted, restore memory state
            cluster_debug "Restoring VM from memory snapshot: $vm_name"
            if cluster_vm_restore_from_file "$snapshot_memory_path"; then
                restarted_count=$((restarted_count + 1))
            else
                cluster_error "Failed to restore VM from memory: $vm_name"
                all_restarted=1
            fi
        else
            # This was a shut off VM when snapshotted, just start it
            cluster_debug "Starting VM from disk snapshot: $vm_name"
            if cluster_virsh start "$vm_name" 2>/dev/null; then
                restarted_count=$((restarted_count + 1))
            else
                cluster_error "Failed to start VM: $vm_name"
                all_restarted=1
            fi
        fi
    done

    if [ $all_restarted -ne 0 ]; then
        cluster_error "Some VMs failed to restart"
        return 1
    fi

    if [ $restarted_count -eq 0 ]; then
        cluster_error "No VMs were restarted from snapshot"
        return 1
    fi

    cluster_log "Successfully restarted $restarted_count VM(s) from snapshot"

    # Wait for all VMs to be accessible via SSH
    cluster_log "Waiting for VMs to be accessible"
    for node_num in $(seq 0 "$num_nodes"); do
        local vm_name=$(cluster_vm_get_name "$cluster_id" "$node_num")

        # Skip if VM doesn't exist
        if ! cluster_virsh dominfo "$vm_name" &>/dev/null; then
            continue
        fi

        # Check if VM is running
        local vm_state=$(cluster_virsh domstate "$vm_name" 2>/dev/null || echo "unknown")
        if [ "$vm_state" != "running" ]; then
            cluster_debug "VM $vm_name is not running, skipping SSH check"
            continue
        fi

        # Get IP address (retry — DHCP lease may be stale after snapshot restore)
        local node_ip=""
        local ip_attempt=0
        while [ $ip_attempt -lt 10 ]; do
            node_ip=$(cluster_vm_get_ip "$vm_name" 2>/dev/null || true)
            [ -n "$node_ip" ] && break
            ip_attempt=$((ip_attempt + 1))
            sleep 2
        done
        if [ -z "$node_ip" ]; then
            cluster_warn "Could not get IP for $vm_name after retries, skipping SSH check"
            continue
        fi

        cluster_debug "Waiting for SSH on $vm_name ($node_ip)"

        # Wait for SSH with timeout
        local max_attempts=30
        local attempt=0
        while [ $attempt -lt $max_attempts ]; do
            if cluster_vm_ssh "$node_ip" "echo 'SSH ready'" &>/dev/null; then
                cluster_debug "SSH ready on $vm_name"
                break
            fi
            attempt=$((attempt + 1))
            sleep 2
        done

        if [ $attempt -eq $max_attempts ]; then
            cluster_warn "SSH not ready on $vm_name after $max_attempts attempts"
        fi
    done

    # Set VM clocks from the host's clock and restart chronyd so it
    # re-syncs from the corrected time rather than the stale snapshot time.
    cluster_log "Resyncing clocks after snapshot restore"
    local host_epoch=$(date +%s)
    for node_num in $(seq 0 "$num_nodes"); do
        local vm_name=$(cluster_vm_get_name "$cluster_id" "$node_num")
        local node_ip=$(cluster_vm_get_ip "$vm_name" 2>/dev/null || true)
        if [ -n "$node_ip" ]; then
            cluster_vm_ssh "$node_ip" "
                modprobe ptp_kvm 2>/dev/null || true
                date -s @${host_epoch} &>/dev/null
                systemctl restart chronyd
                chronyc makestep 1>/dev/null 2>&1 || true
            " &>/dev/null || true
        fi
    done

    cluster_vms_reconnect_iscsi "$cluster_id"
    cluster_vms_reconnect_nvme "$cluster_id"
    cluster_log "Cluster restarted from snapshot successfully"
    return 0
}

cluster_snapshot_delete() {
    local cluster_id="$1"
    local snapshot_name="$2"

    if [ -z "$cluster_id" ]; then
        cluster_die "cluster_snapshot_delete: cluster_id is required"
    fi

    if [ -z "$snapshot_name" ]; then
        cluster_die "cluster_snapshot_delete: snapshot_name is required"
    fi

    cluster_log "Deleting snapshot '$snapshot_name' for cluster: $cluster_id"

    # Determine number of VMs (node 0 + test nodes 1..N)
    local num_nodes=${CLUSTER_NUM_NODES:-3}
    local save_dir
    save_dir=$(cluster_get_image_dir)
    local deleted_count=0

    # Delete all snapshot files
    for node_num in $(seq 0 "$num_nodes"); do
        local vm_name=$(cluster_vm_get_name "$cluster_id" "$node_num")
        local snapshot_path="${save_dir}/${vm_name}-snapshot-${snapshot_name}.save"
        local disk_snapshot_marker="${save_dir}/${vm_name}-snapshot-${snapshot_name}.disk"
        local disk_path="${save_dir}/${vm_name}.qcow2"
        local disk_snapshot_path="${disk_path}.snapshot-${snapshot_name}"

        # Delete running VM snapshot file
        if [ -f "$snapshot_path" ]; then
            cluster_debug "Deleting snapshot file: $snapshot_path"
            rm -f "$snapshot_path" || cluster_warn "Failed to delete $snapshot_path"
            deleted_count=$((deleted_count + 1))
        fi

        # Delete disk snapshot files
        if [ -f "$disk_snapshot_marker" ]; then
            cluster_debug "Deleting disk snapshot marker: $disk_snapshot_marker"
            rm -f "$disk_snapshot_marker" || cluster_warn "Failed to delete $disk_snapshot_marker"
        fi

        if [ -f "$disk_snapshot_path" ]; then
            cluster_debug "Deleting disk snapshot: $disk_snapshot_path"
            rm -f "$disk_snapshot_path" || cluster_warn "Failed to delete $disk_snapshot_path"
            deleted_count=$((deleted_count + 1))
        fi
    done

    if [ $deleted_count -eq 0 ]; then
        cluster_warn "No snapshot files found to delete"
        return 1
    fi

    cluster_log "Deleted $deleted_count snapshot file(s)"
    return 0
}

#
# Reconnect NVMe-oF on test nodes after snapshot restore.
#
# Saving node0 first during snapshot kills the NVMe-oF target, breaking
# TCP connections on test nodes.  After restore, the initiator may not
# auto-reconnect in time, so explicitly disconnect and reconnect here.
#
cluster_vms_reconnect_nvme() {
    local cluster_id="$1"
    local num_nvme="${CLUSTER_NUM_NVME:-0}"

    if [ "$num_nvme" -eq 0 ]; then
        return 0
    fi

    local num_nodes=${CLUSTER_NUM_NODES:-3}
    local node0_vm=$(cluster_vm_get_name "$cluster_id" 0)
    local node0_ip=$(cluster_vm_get_ip "$node0_vm" 2>/dev/null || true)

    if [ -z "$node0_ip" ]; then
        cluster_warn "Cannot reconnect NVMe: node0 IP not available"
        return 0
    fi

    local subsystem_nqn="nqn.2025-03.lvm.cluster:${cluster_id}"

    cluster_log "Reconnecting NVMe-oF on test nodes"

    for node_num in $(seq 1 "$num_nodes"); do
        local vm_name=$(cluster_vm_get_name "$cluster_id" "$node_num")
        local node_ip=$(cluster_vm_get_ip "$vm_name" 2>/dev/null || true)

        if [ -z "$node_ip" ]; then
            continue
        fi

        cluster_vm_ssh "$node_ip" "
            nvme disconnect -n '$subsystem_nqn' 2>/dev/null || true
            nvme connect -t tcp -n '$subsystem_nqn' -a '$node0_ip' -s 4420 2>/dev/null || true
            # wait for devices to appear
            for i in \$(seq 1 10); do
                if nvme list 2>/dev/null | grep -q '/dev/nvme'; then
                    break
                fi
                sleep 1
            done
        " &>/dev/null || cluster_warn "NVMe reconnect failed on node $node_num"

        cluster_debug "Reconnected NVMe-oF on node $node_num"
    done
}

#
# cluster_vms_reconnect_iscsi - Re-export iSCSI storage after snapshot restore
#
# After a VM snapshot restore that involved a cold boot (no memory restore),
# loop device associations on node0 are lost.  The saved targetcli config
# references /dev/loopN devices that no longer exist, so target.service
# fails to restore backstores and LUNs.
#
# This function re-creates the loop devices from the existing backing
# files, restarts target.service so it can restore successfully, then
# rescans iSCSI sessions on all test nodes so the LUNs reappear.
#
cluster_vms_reconnect_iscsi() {
    local cluster_id="$1"
    local num_scsi="${CLUSTER_NUM_SCSI:-0}"
    local num_multipath="${CLUSTER_NUM_MULTIPATH:-0}"

    if [ "$num_scsi" -eq 0 ] && [ "$num_multipath" -eq 0 ]; then
        return 0
    fi

    local num_nodes=${CLUSTER_NUM_NODES:-3}
    local node0_vm=$(cluster_vm_get_name "$cluster_id" 0)
    local node0_ip=$(cluster_vm_get_ip "$node0_vm" 2>/dev/null || true)

    if [ -z "$node0_ip" ]; then
        cluster_warn "Cannot reconnect iSCSI: node0 IP not available"
        return 0
    fi

    # Check if LIO target already has LUNs (memory-restored VMs keep loop
    # devices intact, so no reconnect is needed)
    local lun_count
    lun_count=$(cluster_vm_ssh "$node0_ip" "ls /sys/kernel/config/target/iscsi/*/tpgt_1/lun/ 2>/dev/null | wc -l" 2>/dev/null || echo "0")
    if [ "${lun_count:-0}" -gt 0 ]; then
        cluster_debug "iSCSI target already has $lun_count LUN(s), skipping reconnect"
        return 0
    fi

    cluster_log "Reconnecting iSCSI storage on node0"

    local scsi_backing="${CLUSTER_SCSI_BACKING_TYPE:-loop_prealloc}"
    local mpath_backing="${CLUSTER_MULTIPATH_BACKING_TYPE:-loop_prealloc}"

    # Re-create loop devices and restart target.service on node0
    cluster_vm_ssh "$node0_ip" "
        # Re-attach SCSI backing files to loop devices
        if [ $num_scsi -gt 0 ]; then
            scsi_dir='/var/tmp/lvm-cluster-storage-scsi'
            for i in \$(seq 1 $num_scsi); do
                img=\"\${scsi_dir}/scsi-disk-${cluster_id}-\${i}.img\"
                if [ -f \"\$img\" ]; then
                    case '$scsi_backing' in
                        loop_prealloc|loop_sparse)
                            losetup -f \"\$img\" 2>/dev/null || true
                            ;;
                    esac
                fi
            done
        fi

        # Re-attach multipath backing files to loop devices
        if [ $num_multipath -gt 0 ]; then
            mp_dir='/var/tmp/lvm-cluster-storage-multipath'
            for i in \$(seq 1 $num_multipath); do
                img=\"\${mp_dir}/multipath-${cluster_id}-\${i}.img\"
                if [ -f \"\$img\" ]; then
                    case '$mpath_backing' in
                        loop_prealloc|loop_sparse)
                            losetup -f \"\$img\" 2>/dev/null || true
                            ;;
                    esac
                fi
            done
        fi

        # Restart target.service so it can restore the config with loop devices present
        systemctl restart target 2>/dev/null || true

        # Verify LUNs appeared
        sleep 1
        luns=\$(ls /sys/kernel/config/target/iscsi/*/tpgt_1/lun/ 2>/dev/null | wc -l)
        if [ \"\$luns\" -eq 0 ]; then
            echo 'WARNING: target.service restarted but no LUNs found' >&2
            exit 1
        fi
    " 2>/dev/null || cluster_warn "iSCSI reconnect on node0 may have failed"

    # Rescan iSCSI sessions on all test nodes so LUNs reappear
    cluster_log "Rescanning iSCSI sessions on test nodes"
    for node_num in $(seq 1 "$num_nodes"); do
        local vm_name=$(cluster_vm_get_name "$cluster_id" "$node_num")
        local node_ip=$(cluster_vm_get_ip "$vm_name" 2>/dev/null || true)

        if [ -z "$node_ip" ]; then
            continue
        fi

        cluster_vm_ssh "$node_ip" "
            iscsiadm -m session --rescan 2>/dev/null || true
            # Wait for SCSI devices to appear
            for i in \$(seq 1 10); do
                if lsblk -d -n -o VENDOR 2>/dev/null | grep -qi 'LIO-ORG'; then
                    break
                fi
                sleep 1
            done
        " &>/dev/null || cluster_warn "iSCSI rescan failed on node $node_num"

        cluster_debug "Rescanned iSCSI on node $node_num"
    done
}

# Export functions
export -f cluster_vm_get_name
export -f cluster_vm_create cluster_vm_destroy
export -f cluster_vm_get_ip cluster_vm_wait_boot
export -f cluster_vm_setup_ssh cluster_vm_ssh cluster_vm_scp
export -f cluster_vm_expand_rootfs cluster_vm_install_packages
export -f cluster_vm_deploy_lvm_source cluster_vm_deploy_sanlock_source
export -f cluster_vm_setup_lock_manager
export -f cluster_vm_setup_storage_export cluster_vm_setup_storage_import
export -f cluster_golden_image_build cluster_golden_image_delete
export -f cluster_vms_create_all cluster_vms_destroy_all
export -f cluster_vm_save cluster_vm_restore_from_file
export -f cluster_vms_pause_all cluster_vms_resume_all
export -f cluster_vms_snapshot_all cluster_vms_restart_from_snapshot cluster_snapshot_delete
export -f cluster_vms_reconnect_nvme cluster_vms_reconnect_iscsi
