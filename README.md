# MattOS

A source-controlled Linux-based operating system project built and orchestrated with ProjectTaskforge.

## Quick start (Windows host)

1. Bootstrap WSL and Linux build dependencies:

```
cargo run -p mattos-build -- bootstrap-wsl
```

2. Validate host + WSL requirements:

```
cargo run -p mattos-build -- doctor
```

3. Build ISO in WSL Linux filesystem and copy artifact back to Windows:

```
cargo run -p mattos-build -- build-wsl-iso
```

4. Optional explicit ISO copyback command:

```
cargo run -p mattos-build -- copy-iso-from-wsl
```

5. If WSL distro install is blocked by policy, run this exact elevated command:

```
wsl --install -d Ubuntu
```

Expected ISO artifact:

```
out/images/mattos-x86_64.iso
```
