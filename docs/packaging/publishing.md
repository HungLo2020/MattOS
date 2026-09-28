# Publishing Packages

Build and upload every generated package through the vendored repository
manager:

```
python3 DevUtils/PublishPackages.py
```

Use `--dry-run` to validate the discovered packages and print the upload
command without changing the remote repository. The build itself never
publishes; see [MattOS remote repository integration](remote-repository.md).
