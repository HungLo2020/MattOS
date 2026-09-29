# Kernel Module Signing

Every kernel module MattOS ships, including the out-of-tree NVIDIA open
modules, is signed with one key committed to the repository:
`src/kernel/config/module-signing-key.pem`, a 4096-bit RSA private key and its
self-signed certificate in one PEM file.

## Why a committed key

The kernel configuration enables module signature checking
(`CONFIG_MODULE_SIG=y`) without enforcing it (`CONFIG_MODULE_SIG_FORCE` is
off). With no key supplied, Kbuild generates a random key in the build tree and
embeds its certificate in the kernel image, and nothing signs the modules. That
made the kernel image differ between otherwise identical builds whenever the
build tree was recreated (every kernel update), and every module load logged
"module verification failed" and tainted the kernel.

A fixed key keeps the kernel reproducible and lets each module carry a valid
signature. The key is deliberately public: a signature shows only that a module
was signed with it, not who built it, so it is not a trust anchor. If MattOS
ever enforces signatures (for Secure Boot, for example), it needs a private key
kept outside the repository instead.

## How modules are signed

- The `linux` stage copies the key into the kernel build tree as
  `certs/mattos-module-signing-key.pem`, the path `CONFIG_MODULE_SIG_KEY`
  names, and Kbuild embeds its certificate in the kernel. Kbuild's default
  `certs/signing_key.pem` is avoided because Kbuild would replace it with a
  generated key. The copy is rewritten only when the key changes, so an
  unchanged key does not relink the kernel.
- `CONFIG_MODULE_SIG_ALL` stays off. After `modules_install`, the stage rewrites
  checkout paths inside modules (`normalize_kernel_module_paths`), which would
  invalidate a signature applied during installation. `sign_kernel_modules`
  signs afterwards instead: it decompresses each `.ko.zst`, appends a SHA-512
  signature with the kernel build's `scripts/sign-file`, and recompresses it.
  Already signed modules are skipped.
- The `nvidia-driver` stage signs its modules the same way against the same
  kernel build tree.
- RSA PKCS#1 v1.5 signatures without signed attributes are deterministic, so
  signed modules stay reproducible.

The build refuses a resolved kernel configuration that does not sign with the
committed key or that sets `CONFIG_MODULE_SIG_ALL`, and the key is a source
input of both stages. The installed-system QEMU checks
`no-unsigned-module-taint` and `kernel-modules-signed` verify that the kernel
has no unsigned-module taint and that a loaded module carries a signature. The
kernel trusts only the embedded MattOS certificate, so a loaded signed module
without that taint was verified against it. MattOS builds kmod without OpenSSL,
so `modinfo` detects a signature but cannot name its signer (`sig_hashalgo`
reads `unknown`).
