# Development Utilities

`BackupLinuxScripts.sh` archives this repository as a timestamped ZIP file. Its destination is a personal OneDrive mount configured by `DEST_DIR`; change that path before using the script on another machine or account.

The archive excludes `node_modules`, `*.tmp`, and `.git` and is staged in `/tmp` before it is moved to the destination.