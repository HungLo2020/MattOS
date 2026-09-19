# lvmtest: VM-Based Cluster Testing for LVM

## What It Is

lvmtest is a test framework that creates a cluster of VMs and runs
test scripts across them.  Instead of running LVM commands against
virtual devices on the local host, tests run against real devices on
real VMs with real shared storage.

- lives under `test/cluster/`
- `lvmtest` is the front-end program, run from the host machine
- lvmtest creates test VMs (the cluster nodes)
- node0 is a special storage-exporter VM (targetcli, nvmet)
- node1, node2, ... are the test nodes
- node0 exports the same devices to all test nodes (simulating a SAN)
- the host runs shell test commands on the VMs over ssh

This enables testing of shared VGs, sanlock, dlm, persistent
reservations, and multi-node locking — none of which are possible
with the virtual-device framework.


## Shell Tests

Test scripts live in `test/cluster/shell/*.sh`.  The format is similar to
traditional shell tests: a series of LVM commands with expected outcomes.
The key difference is that each command is preceded by a node directive
specifying which VM to run it on.

### Node Directives

| Directive | Meaning |
|-----------|---------|
| `node1` | run command on node 1 |
| `node2` | run command on node 2 |
| `noden N` | run command on node N (variable) |
| `nodep` | run command on all test nodes in parallel |
| `nodes` | run command on all test nodes sequentially |
| `not` | expect command to fail |
| `success_node` | run on the node that succeeded from a previous `nodep` |

### Example

```bash
# create a shared VG on node1, start it on all nodes
node1 vgcreate --shared testvg $dev1 $dev2
nodep vgchange --lockstart testvg

# each node creates its own LVs
for node_num in $(seq 1 $CLUSTER_NUM_NODES); do
    noden ${node_num} lvcreate -l1 -n lv_${node_num} testvg
done

# all nodes activate their own LVs in parallel
nodep 'lvchange -ay testvg/lv_${NODE_NUM}'
assert_all_success

# verify each node has its LV active
for node_num in $(seq 1 $CLUSTER_NUM_NODES); do
    verify_lv_active_on ${node_num} testvg lv_${node_num}
done

# cleanup
nodep vgchange -an testvg
cleanup_vg testvg
```

Assertion helpers (`assert_one_success`, `assert_all_success`,
`assert_all_fail`, `verify_lv_active_on`, etc.) are provided in
`test/cluster/shell/lib/cluster-test-helpers.sh`.


## Cluster Configs

A config file (`configs/*.conf`) describes the cluster to create.
Key parameters:

```bash
CLUSTER_NUM_NODES=2                   # number of test VMs

CLUSTER_NODE_MEMORY=2048              # MB per VM
CLUSTER_NODE_VCPUS=2
CLUSTER_NODE_DISK_SIZE=20             # GB per VM
CLUSTER_NODE_OS_IMAGE="/path/to/Fedora-Cloud.qcow2"

CLUSTER_NUM_SCSI=4                    # shared iSCSI devices
CLUSTER_NUM_NVME=0                    # shared NVMe devices
CLUSTER_NUM_MULTIPATH=0               # shared multipath devices
CLUSTER_SCSI_SIZE=1024                # MB per device
CLUSTER_SCSI_SECTOR_SIZE=4096         # 512 or 4096

CLUSTER_LOCK_TYPE="sanlock"           # sanlock or dlm
SANLOCK_CONF_SETTINGS+=("use_compare_and_write=no")
SANLOCK_CONF_SETTINGS+=("io_timeout=2")
```

Pre-defined configs cover common combinations of node counts,
device types, sector sizes, and sanlock parameters.

`lvmtest makeconfig` interactively creates new configs.

Configs can also specify a custom LVM or sanlock source tree
to build and install on the test VMs.

Config file names currently follow a convention (optional) in which
variables relevant to tests are abbreviated in the name, e.g. shared or
local vg, number of nodes, number and types of devices, sector size,
sanlock settings (compare and write, io timeout).

## Cluster Admin

```
lvmtest create -c <config>     create a new cluster
lvmtest destroy                destroy the current cluster
lvmtest revert                 revert VMs to original snapshot
lvmtest run -t <test>          run a test on the cluster
lvmtest group -g <group>       run a group of tests
lvmtest list                   list clusters
lvmtest status                 show cluster status
```

`lvmtestssh 1` opens an ssh session to node1.

`lvmtest revert` resets all VMs to a snapshot taken after initial
creation, restoring a clean state between tests.

lvmtest can be run by non-root.

Cluster names and node names are derived from the config name, with an
lvmtest prefix and a timestamp suffix.

```
config name    shared-sanlock-3node-2scsi-2nvme-2mpath.conf

cluster name   lvmtest-shared-sanlock-3node-2scsi-2nvme-2mpa-0623120321

VM names       lvmtest-shared-sanlock-3node-2scsi-2nvme-2mpa-0623120321-node0
               lvmtest-shared-sanlock-3node-2scsi-2nvme-2mpa-0623120321-node1
               lvmtest-shared-sanlock-3node-2scsi-2nvme-2mpa-0623120321-node2

```

## Test Results

Results are collected under `test/cluster/results/`.

### Per-test files

Each test run produces a log file named with the test and timestamp:

```
results/passed/log_shared-vg-independent_0623155739.txt
```

Passed test logs are moved to `results/passed/`.

A failed test produces a result file and a debug directory:

```
results/FAILED_shared-vg-adopt_0623163214.txt
results/FAILED_shared-vg-adopt_0623163214_debug/
```

Known failures (tests listed in a known-failures file) use `KNOWN_`
instead of `FAILED_`.

The log file records every command executed, which node it ran on,
its output, and exit code.  On failure, the debug directory collects
diagnostic information from each node:

```
FAILED_shared-vg-adopt_0623163214_debug/
    shared-vg-adopt-node0-dmesg.log
    shared-vg-adopt-node0-targetcli.log
    shared-vg-adopt-node0-journal.log
    shared-vg-adopt-node1.log
    shared-vg-adopt-node1-dmesg.log
    shared-vg-adopt-node1-sanlock.log
    shared-vg-adopt-steal-time.log
    shared-sanlock-3node-4scsi-caw-io2-512.conf
    shared-vg-3node-4scsi-caw-io2-512.txt
```

The debug directory also includes a copy of the config and group
files so the failure can be reproduced.

### Group summary files

When running a group, a summary file is written:

```
results/groups/group_local-vg-3node-2scsi-2nvme-2mpath_0623151141.txt
```

It lists each test with its runtime and outcome:

```
# Group Results: local-vg-3node-2scsi-2nvme-2mpath.txt
# Total: 6, Passed: 5, Failed: 0, Known: 1, Skipped: 0
#
local-vg-pr              409  success
local-vg-lvmpersist      217  success
local-vg-multipath        30  success
local-vg-device-id       252  success
local-vg-boot-activation 196  known
local-vg-devices-import   11  success
```

### Cluster creation logs

Cluster creation is also logged:

```
results/cluster/log_create_shared-sanlock-3node-4scsi-caw-io2-512_0623151143.txt
```


## Test Groups

A group file (`groups/*.txt`) pairs a cluster config with a list
of tests to run on it:

```
config shared-sanlock-2node-4mpath-caw-io2-512.conf
run shell/shared-vg-exclusive-activation.sh
run shell/shared-vg-independent.sh
run shell/shared-vg-raid1.sh
run shell/stress-shared-vg-activate-deactivate-same.sh
...
```

- **Amortize cluster creation.**  The cluster is created once for
  the group.  Between tests, `lvmtest revert` resets all VMs to the
  original snapshot — fast and reliable, without the cost of a full
  create/destroy cycle.

- **Define the test matrix.**   Define which tests should be run against
  which configurations.  If we want to run a given test on a machine with
  a given setting, create a new config with that setting, and create a
  new group listing that config and the test(s) to run against it.

### Group organization

| Directory | Purpose |
|-----------|---------|
| `basic/` | Quick smoke tests |
| `standard/` | Full test matrix |
| `bridge/` | Run old single-node shell tests on a VM |
| `my/` | Personal groups, not tracked in git |


## Makefile Interface

```bash
make check_cluster G=basic                    # run all basic groups
make check_cluster G=basic,standard           # run basic + standard
make check_cluster G=standard/shared-vg-2node-4scsi-nocaw-io2-4k.txt  # one group

make check_cluster C=<config> T=<test>        # run one test on one config
```


## Devices and Device Types

Node0 exports shared storage to all test nodes.  The config
determines how many devices of each type are exported:

| Config parameter | Export method | Device paths on test nodes |
|------------------|--------------|---------------------------|
| `CLUSTER_NUM_SCSI` | iSCSI (targetcli) | `/dev/sd[b-z]` |
| `CLUSTER_NUM_NVME` | NVMe-oF (nvmet) | `/dev/nvme*n1` |
| `CLUSTER_NUM_MULTIPATH` | iSCSI multi-path | `/dev/mapper/mpath*` |

Configs also control device size, sector size (512 or 4096), and
backing type.  A config can include any combination of device types.

### Device variables

The framework discovers devices on each test node by WWN, maps them
consistently across nodes, and exports two sets of shell variables:

- **Generic:** `$dev1`, `$dev2`, ..., `$devN` — numbered sequentially
  across all device types.  Most tests use only these.

- **Type-specific:** `$scsi1`, `$scsi2`, `$nvme1`, `$nvme2`,
  `$mpath1`, `$mpath2`, etc. — numbered within each type.

The same `$dev1` refers to the same physical disk on every node
(even though the device path may differ between nodes).

Additional variables: `$DEVICES` (array of all device paths),
`$CLUSTER_NUM_SCSI`, `$CLUSTER_NUM_NVME`, `$CLUSTER_NUM_MULTIPATH`
(counts from the config).

Tests can use the generic dev variables, or the type-specific variables,
depending on how they are structured, and how important the device type is
to the specific test.  When using generic variables, type-coverage comes
from having the same test in multiple groups configured with different
device types.  When using type-specific variables, type-coverage comes
from either multiple groups, or from a single config that supplies
multiple device types to each test node (the later is a more efficient way
of running a single test against multiple device types.)

