# LVM Test Groups

This directory contains test group definitions for the LVM cluster testing system.

## What are Test Groups?

Test groups define a collection of tests to run together on a specific
cluster configuration.  Each group file specifies a cluster configuration
and a list of test scripts to run.

## Group File Format

```
# Comments start with #
config <config-name>
run <test-path>
run <test-path>
...
```

### Example

```
config shared-sanlock-3node-4scsi-caw-io2-512.conf
run shell/shared-vg-blocked-operations.sh
run shell/shared-vg-exclusive-activation.sh
```

### Rules

- **One `config` line** (optional): Specifies which cluster configuration to use
- **One or more `run` lines** (required): Specifies tests to execute
- **Comments**: Lines starting with `#` are ignored
- **Empty lines**: Ignored
- **Path normalization**:
  - Config: `foo` expands to `configs/foo.conf`, `foo.conf` expands to `configs/foo.conf`
  - Tests: `foo` expands to `shell/foo.sh`; paths containing `/` are relative to `test/cluster/`;
    paths starting with `test/shell/` are relative to the repo root (for bridge mode tests)

## How Groups Work

When you execute a group, lvmtest:

1. **Parses** the group file and validates all files exist
2. **Creates** a cluster with the specified configuration (or uses existing with -i)
3. **Creates** a 'new' snapshot for clean state
4. **For each test**:
   - Runs the test
   - Records pass/fail/skip status
   - **Reverts** cluster to 'new' snapshot (except after last test)
5. **Destroys** the cluster (unless -i was used)
6. **Reports** summary (total/passed/failed/skipped)

### Failure Behavior

- If a **test fails**: The group continues to the next test
- If **revert fails**: The group stops (cannot guarantee clean state)

## Host Prerequisites

lvmtest supports non-root usage via session libvirt. See
[test/cluster/CLAUDE.md](../CLAUDE.md) for full details.

- **Required:** session libvirt packages, readable OS image, disk space under `$HOME`
- **Recommended:** `kvm` group membership for hardware acceleration

One-time non-root setup:

```bash
mkdir -p ~/.local/share/libvirt/images ~/.cache/lvm-cluster-state
virsh net-start default
sudo usermod -aG kvm $USER   # optional but recommended; re-login required
```

## Directory Layout

Group files are organized into category subdirectories:

```
groups/
  basic/       Quick smoke-test subset
  standard/    Full test suite (remaining groups)
  bridge/      Bridge mode tests (single-node shell tests on a VM)
  my/          Personal groups (.gitignored)
```

- **basic/** - Small set of groups for quick validation
- **standard/** - All other groups for comprehensive testing
- **bridge/** - Run single-node shell tests on a VM (formerly `bridge-*.txt`)
- **my/** - Personal/custom groups, not tracked in git (formerly `my-*.txt`)

## Running Groups

### By Category

```bash
./lvmtest group -g groups/basic                    # all basic groups
./lvmtest group -g groups/basic -g groups/standard # both categories
```

### Via Makefile

```bash
make check_cluster G=basic                  # groups/basic/*.txt
make check_cluster G=basic,standard         # both directories
make check_cluster G=bridge                 # bridge groups only
```

### Single Group File

```bash
./lvmtest group -g groups/standard/shared-vg-3node-4scsi-caw-io2-512.txt
make check_cluster G=standard/shared-vg-3node-4scsi-caw-io2-512.txt
```

### Run on Existing Cluster

```bash
./lvmtest -i <cluster_id> group -g groups/basic
```

When -i is specified, the cluster is not created or destroyed by the group.

## Group Naming Convention

- `shared-vg-<nodes>-<devices>-<settings>.txt` - shared VG tests with sanlock,
  where settings encode sanlock parameters (caw/nocaw, io timeout, sector size)
- `local-vg-*.txt` - local VG tests (PR, lvmpersist, multipath, device-id, boot-activation)

## Related Commands

- `./lvmtest create -c <config>` - Create a cluster manually
- `./lvmtest run -t <test>` - Run a single test
- `./lvmtest revert` - Revert cluster to clean state
- `./lvmtest destroy` - Destroy a cluster
- `./lvmtest list` - List existing clusters
