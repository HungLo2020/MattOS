# Default ZIP Backup Configurations

The Zip Backup Manager discovers every `*.env` file in this directory when it
runs. A template can be selected during setup or imported non-interactively:

```text
python3 src/server/zip_backups.py --setup-default mattmc --yes
```

Despite the historical manager name, new archives use `tar.zst`. Existing
managed `.zip` archives remain discoverable so normal retention can age them
out safely.
