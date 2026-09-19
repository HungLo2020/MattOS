# OVERVIEW

lvmtest is an LVM cluster test framework that runs test scripts across multiple
VMs representing different cluster nodes.

- cluster/configs/ contains test cluster config specs, used to create
  clusters with specific properties (e.g. number of VMs, VM properties,
  OS image to use, number, size and types of devices.)
- "lvmtest" provides the front-end user interface for creating clusters
  and running test scripts on clusters.
- "lvmtest create -c configs/test-cluster.conf" creates a new cluster of
  VMs with shared storage.
- "lvmtest create" creates N nodes: node1, node2, ..., nodeN, plus a special "node0"
- node0 is the storage-exporter node; it exports devices (e.g. via iscsi) to the all
  the test nodes (node1..nodeN).  Because the same devices are exported from node0
  to all the test nodes, the devices act as shared storage (like a SAN -
  Storage Area Network), and can be used to test shared VGs.
- cluster/shell/, and subdirs, contain test scripts, e.g. shell/example.sh
- "lvmtest run -t shell/example.sh" runs the example.sh test on the running cluster.
- example.sh contains a series of lvm commands to run on test nodes.
- preceding each lvm command in example.sh is an indication of which node
  the command should be run on. e.g. "node1 <command>" runs <command> on node1.
  "node2 <command>" runs it on node2, etc.
- Special node designations are:
  "nodep <command>": run command on all nodes in parallel.
  "nodes <command>": run command on all nodes sequentially.
  "noden <num> <command>": run command on node <num>.
  "success_node <command>": used after nodep to run command on the
   single node which successfully completed a nodep command.
- "not <command>" indicates command is expected to fail.
- example.sh fails if a command, or other check, fails when it is expected to succeed.
- results of each test are stored under cluster/results/.  This
  includes a log of commands execuated and their output, and debug info collected
  from VMs after a test fails.
- lvmtestssh and lvmtestscp are command line tools to easily ssh and scp
  to test node VMs.
- "lvmtest revert" resets all the VMs to a snapshot that was taken after
  they were first created.  This can be used to reset the cluster nodes
  to a clean state after each test.
- When testing shared VGs, tests focus on ensuring that lvm locking, through
  lvmlockd and the lock manager (sanlock or dlm), is correctly doing mutual
  exclusion for lvm commands on multiple nodes using the same VG.
- lvmtest can use "bridge mode" to run old style lvm test scripts (from ../shell/)
  on just one node.  In this way, the old style test scripts run against a VM's
  standard devices rather than virtual devices that they traditionally use.

# HOST PREREQUISITES (NON-ROOT USAGE)

lvmtest can run as a non-root user via session libvirt (`qemu:///session`).
Root and `libvirt` group members continue to use system libvirt (`qemu:///system`).

## Required

- Session libvirt: `qemu-kvm`, `libvirt-client`, `virt-install`, `qemu-img`, `genisoimage`
- Writable directories (session mode defaults):
  - `~/.local/share/libvirt/images` (VM disk images and snapshots)
  - `~/.cache/lvm-cluster-state` (cluster state files)
- A readable base OS cloud image (e.g. Fedora cloud qcow2)
- Sufficient disk quota under `$HOME` for the cluster size

## Strongly recommended

- `kvm` group membership for `/dev/kvm` hardware acceleration:
  `sudo usermod -aG kvm $USER` (re-login required)
- Without KVM, VMs use slow software emulation and may hit boot timeouts

## One-time setup (non-root)

```bash
mkdir -p ~/.local/share/libvirt/images ~/.cache/lvm-cluster-state
# Copy or download a cloud image into the images directory
virsh net-start default   # session-scoped NAT network
```

## Example workflow (non-root)

```bash
./lvmtest -c my-cluster create
./lvmtest -i my-cluster group -g groups/shared-vg-3node-4scsi-caw-io2-512.txt
./lvmtest -i my-cluster destroy
```

# DEVICE TYPE TESTING

Tests exercise three device types: SCSI, NVMe, and multipath. There are
two approaches to covering multiple device types, and each test should use
the approach that fits its needs.

## Approach 1: Per-type loop in the test script

The test uses type-specific variables ($scsi1, $nvme1, $mpath1) and
CLUSTER_NUM_SCSI/NVME/MULTIPATH to iterate over available device types
within a single run on a mixed-device config. Each iteration runs the
full test body with devices of one type.

Use when test logic changes by device type: PR type selection (WE vs WEAR),
parallel-race eligibility, device rediscovery method, etc. Examples:
local-vg-pr.sh, shared-vg-pr.sh, local-vg-lvmpersist.sh.

Pattern:
  DEV_TYPES=""
  [ "${CLUSTER_NUM_SCSI:-0}" -ge 2 ]     && DEV_TYPES="$DEV_TYPES scsi"
  [ "${CLUSTER_NUM_NVME:-0}" -ge 2 ]     && DEV_TYPES="$DEV_TYPES nvme"
  [ "${CLUSTER_NUM_MULTIPATH:-0}" -ge 2 ] && DEV_TYPES="$DEV_TYPES mpath"
  for devtype in $DEV_TYPES; do
      case $devtype in
          scsi)  d1=$scsi1; d2=$scsi2 ;;
          nvme)  d1=$nvme1; d2=$nvme2 ;;
          mpath) d1=$mpath1; d2=$mpath2 ;;
      esac
      # ... test body using $d1, $d2, with $devtype for conditional logic
  done

## Approach 2: Generic $dev variables with single-type configs

The test uses only generic $dev1, $dev2, etc. and is device-type-agnostic.
Multi-type coverage comes from running the same test group against different
configs (e.g. 4-scsi, 4-nvme, 4-mpath), each creating a separate cluster.

Use when test logic is identical regardless of device type: LV creation,
activation, locking, snapshots, thin, cache, raid, etc. This keeps scripts
simple and lets the config matrix also vary sanlock parameters (CAW,
io_timeout, sector_size). Examples: most shared-vg-*.sh tests.
  
