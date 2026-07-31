# Building MattOS

The build orchestration tool is implemented in Rust and the first bootable ISO flow must run in a Linux filesystem inside WSL (not from `/mnt/c`).

## Windows -> WSL-native workflow

1. Bootstrap WSL distro, packages, and Linux-side repo mirror (`~/src/MattOS` by default):

```
cargo run -p mattos-build -- bootstrap-wsl
```

2. If distro installation is blocked by policy, run this exact elevated command:

```
wsl --install -d Ubuntu
```

3. Optional script wrapper:

```
powershell -ExecutionPolicy Bypass -File tools/bootstrap-wsl.ps1
```

## Validate prerequisites

Run:

```
cargo run -p mattos-build -- doctor
```

Doctor distinguishes:

- Windows host requirements (git/cargo/rustc/wsl)
- WSL/Linux build requirements (kernel + ISO toolchain)
- Optional QEMU validation requirements

## Full WSL build

Run the complete import + build + optional boot validation pipeline in WSL:

```
cargo run -p mattos-build -- build-wsl-iso
```

This performs:

1. Linux-side upstream re-import (`linux`, `brush`, `coreutils`) on case-sensitive filesystem.
2. Build stages: kernel, mattos-init, brush, coreutils, rootfs, initramfs, bootloader, ISO.
3. QEMU serial boot test for interactive shell commands unless disabled.
4. Copy completed ISO from WSL tree back to Windows-visible path.

To skip the QEMU boot test in constrained environments:

```
cargo run -p mattos-build -- build-wsl-iso --skip-boot-test
```

To copy ISO again later:

```
cargo run -p mattos-build -- copy-iso-from-wsl
```

Expected ISO output path:

```
out/images/mattos-x86_64.iso
```

## Boot validation

Automatic validation in `build-wsl-iso` checks:

```
pwd
ls /
echo MattOS
uname -a
cat /proc/version
mkdir /tmp/test
touch /tmp/test/file
ls /tmp/test
```

## Milestone completion criteria

1. Upstream trees imported as tracked files (no git submodules)
2. `upstream/state/*.toml` contains repo/branch/imported commit metadata
3. `out/images/mattos-x86_64.iso` generated
4. ISO boot-tested in QEMU
