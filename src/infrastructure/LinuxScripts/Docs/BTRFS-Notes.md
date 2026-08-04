# BTRFS-Notes
## Header
- This document is purely here for me to keep track of how i setup BTRFS and what i did on my HungLoSVR. the gist is that i had two 8tb drives. 1 had data that i did not care about, and the other my media servers were using as storage. i wiped the drive i didnt care about. set it up with btrfs and then cloned the data too it. i then wiped the first drive. then i joined it to the other drive in BTRFS RAID1.

## Steps I Took

### 1: Identify disks and mounts

```bash
lsblk
```

You are looking for:
- Drive A: The drive you care about.
- Drive B: The drive you DONT care about.

### 2: Stop anything touching the drives.
- I have no specific command i ran here. however i had containers running accessing my drive A. i dont believe anything was using drive B at all. but regardless i stopped all my containers.

### 4: Wipe drive B. (I skipped the step 3)
- I had to manually unmount the drive in dolphin

- WARNING THE FOLLOWING COMMAND IS FOR THE DRIVE ID FOR MY DRIVE B (THE ONE I WIPED).

```bash
sudo wipefs -a /dev/sdb
```
- The drive should show up in lsblk as empty now.

### 5: Create BTRFS on the whole disk.

```bash
sudo mkfs.btrfs -f -L tank /dev/sdb
```

- remember /dev/sdb is the drive I wiped for this. it will probably be different on other runs or for other people.

### 6: Mount it.

```bash
sudo mkdir -p /mnt/tank
sudo mount /dev/sdb /mnt/tank
```

### 7: Create a sobvolume.
This makes management much cleaner and allows use of features such as snapshots. do not skip this.

```bash
sudo btrfs subvolume create/mnt/tank/@data
```

Now remount use that subvolume.

```bash
sudo umount /mnt/tank
sudo mkdir -p /srv/storage
sudo mount -o subvol=@data /dev/sdb /srv/storage
```

Keep note:
- Source: important drive: (for me) is /mnt/Storage01
- Destination: New BTRFS: /mnt/storage

### Step 8: Mount your important drive (if not already).
- Just in case. it will need to be mounted.

### Step 9: Copy the Data.

```bash
sudo rsync -aAXH --info=progress2 /mnt/Storage01/ /srv/storage/
```

This will take a while...

### Step 10: Verify the Copy (Critical).

```bash
sudo rsync -aAXHn --delete /mnt/Storage01/ /srv/storage/
```
If this print is empty go to the next step.

If this prints anything than something is different between the two directories. something failed.

### Step 11: Make the BTRFS permanent in fstab.

Get UUID:
```bash
sudo blkid /dev/sdb
```

edit fstab
```bash
sudo nano /etc/fstab
```

add the following:
```bash
UUID=YOUR-UUID-HERE  /srv/storage  btrfs  defaults,noatime,compress=zstd,subvol=@data  0  0
```

Test:
```bash
sudo umount /srv/storage
sudo mount -a
```

confirm:
```bash
df -hT | grep storage
```

### Step 12: Now add the original important drive to create RAID1.
Once you are 110% sure your data is safe on the new BTRFS drive...

unmount old drive (sda for me)
```bash
sudo umount /sev/sda
```

then wipe the drive:
```bash
sudo wipefs -a /dev/sda
sudo sgdisk --zap-all /dev/sda
```

Addit to BTRFS:
```bash
sudo btrfs device add /dev/sda /srv/storage
```

Now convert to RAID1:
```bash
sudo btrfs balance start -dconvert=raid1 -mconvert=raid1 /srv/storage
```

Check Status:
```bash
sudo btrfs balance status /srv/storage
```

When done verify:
```bash
sudo btrfs filesystem df /srv/storage
sudo btrfs filesystem show /srv/storage
```

you should see:
```bash
Data, RAID1
Metadata, RAID1
```

### DONE!!!

- you now have copied data from 1 drive onto a BTRFS Raid 1!.
- it can survive one disk failure.
- 8tb useable (assuming 2 8tb drives were used)
- can expand anytime with any amount of any capacity drives.
- can take snapshots (basically git for FS)
- can even change raid type