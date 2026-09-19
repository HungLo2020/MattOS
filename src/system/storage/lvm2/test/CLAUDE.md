# OVERVIEW

The test directory contains two separate testing frameworks for LVM:

1. The virtual-device test framework, with the make/Makefile front end.
   Located primarily in directories: api/ lib/ shell/

   This framework runs test scripts from test/shell.  Those scripts run
   LVM commands on the local host machine, i.e. the machine running make.
   These LVM commands run against fake devices set up by each script.
   These fake devices are dm devices backed by ramdisk or loop devices set
   up on the local machine.  The LVM commands run by this framework are
   highly configured so that they use custom files, and run as isolated as
   possible from the local system.  It tries as much as possible to not
   interfere with the system running the tests, with the expectation that
   the system running tests may not be a disposable test system.

2. The virtual-machine test framework, with the lvmtest script front end.
   Located under directory: cluster/

   This framework creates a cluster of VMs, and runs test scripts from
   test/cluster/shell.  Those scripts run LVM commands on the test VMs
   (cluster nodes) via ssh.  The LVM commands use the VM's standard devices,
   i.e. scsi or nvme devices directly.  The test VM devices are imported
   from a speical storage-exported VM (node0) which exports scsi/nvme
   devices to all of the test VMs.
 
The test scripts for each framework are different, but use a similar
style.  The scripts are largely a series of LVM commands to execute.
The main diffference is that lvmtest scripts prefix all commands with
"nodeN" indicating which cluster node the command should run on.
The scripts use command exit codes to determine if each step in a test
script is successful.

