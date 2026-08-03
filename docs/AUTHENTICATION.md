# Authentication

MattOS provides a local, PAM-backed authentication stack for the live image. The normal boot path is:

```text
systemd -> agetty --autologin mattos -> /bin/login -f mattos -> PAM -> Brush
```

The separate GRUB rescue entry runs `/usr/libexec/mattos/rescue-init` as PID 1 and launches Brush directly. It does not depend on systemd, PAM, Shadow, login, su, or sudo-rs.

## Imported sources

The authentication projects are editable source trees, not submodules. `upstream/state/` records the imported revisions:

| Component | Repository | Branch | Commit |
| --- | --- | --- | --- |
| Linux-PAM | `https://github.com/linux-pam/linux-pam.git` | `master` | `dd74fc113a9ba1f94d5469f6f7857a1884b3f550` |
| Shadow | `https://github.com/shadow-maint/shadow.git` | `master` | `855d15a04625818fa28a94e693dd4dc7acfb5af3` |
| sudo-rs | `https://github.com/trifectatechfoundation/sudo-rs.git` | `main` | `e5a01fd7a20a5bfe5da6bc1b5cf7628e721c35c8` |
| libxcrypt | `https://github.com/besser82/libxcrypt.git` | `develop` (`v4.4.38`) | `55ea777e8d567e5e86ffac917c28815ac54cc341` |

MattOS-owned policy remains outside those imported trees in `src/system/auth/config/` and `src/system/profiles/live/`.

## Build configuration

Linux-PAM is built with Meson under `/usr`, with `/etc` configuration, an x86-64 library directory, and `/usr/lib/x86_64-linux-gnu/security` as the module directory. Documentation, i18n, audit, SELinux, logind/elogind, examples, and extended tests are disabled for the image build. Systemd separately builds `pam_systemd.so` for session registration against this PAM stack.

Only the local-authentication runtime is staged:

- `libpam.so.0` and `libpam_misc.so.0`;
- `pam_unix`, `pam_env`, `pam_nologin`, `pam_rootok`, `pam_permit`, `pam_deny`, `pam_shells`, and `pam_securetty`;
- systemd's locally built `pam_systemd` session module;
- `unix_chkpwd` for `pam_unix` password verification.

Traditional util-linux builds `agetty`, `login`, and `su` with local PAM, plus the libblkid/libmount/libsmartcols and mount/umount closure that the exact rootfs graph requires. SELinux compatibility is compiled against the staged MattOS libselinux/PCRE2 build, while systemd integration, NLS, Python bindings, completions, and unrelated tools remain disabled.

Shadow is configured with PAM and yescrypt, and without NLS, SELinux, logind, Btrfs, nscd, or sssd. `/etc/login.defs` specifies `ENCRYPT_METHOD YESCRYPT`; QEMU validation confirmed newly assigned passwords use the `$y$` yescrypt format.

`mattos-libcrypt1` comes from libxcrypt `v4.4.38`, configured with all hashing algorithms and glibc-compatible obsolete APIs. The upstream suite passes its yescrypt generation/verification cases. The installed `libcrypt.so.1` exports `GLIBC_2.2.5`, `XCRYPT_2.0`, `XCRYPT_4.3`, and `XCRYPT_4.4`; PAM requires `crypt_checksalt@XCRYPT_4.3` plus `crypt_r` and `crypt_gensalt_rn` at `XCRYPT_2.0`, while Shadow requires `crypt` and `crypt_gensalt` at `XCRYPT_2.0`. PAM, Shadow, and their helpers are rebuilt with staged include, pkg-config, linker, and runtime paths so host libcrypt fallback fails the build.

The source-built `libselinux.so.1` is compatibility runtime only. MattOS installs no SELinux policy, boot mode, relabeling tools, enforcing/permissive configuration, or policy compiler.

sudo-rs builds the `sudo` and `visudo` binaries in release mode and links against the local Linux-PAM build. MattOS does not include C sudo.

## PAM policy

PAM service files are installed from `src/system/auth/config/pam.d/`:

- `login`: nologin and secure-TTY checks, environment setup, Unix authentication/account/session handling, and optional `pam_systemd` registration;
- `su` and `su-l`: root short-circuit plus Unix authentication/account/session handling; the login form additionally registers the session with `pam_systemd`;
- `sudo`: environment setup plus Unix authentication/account/session handling;
- `passwd`: Unix password updates;
- `systemd-user`: the minimal account/session stack used when systemd starts a per-user manager, including optional `pam_systemd` environment setup;
- `other`: deny by default.

`su-l` is separate because util-linux selects that PAM service for `su --login` and `su -`. `pam_systemd` is deliberately absent from `sudo`, plain `su`, `passwd`, and `other`; it is a session hook, not an authentication or password policy.

## Accounts and authorization

The live profile supplies coherent `passwd`, `group`, `shadow`, `gshadow`, and `shells` files.

| Account | UID:GID | Groups | Home | Shell | Live password state |
| --- | --- | --- | --- | --- | --- |
| `root` | `0:0` | `root` | `/root` | `/bin/brush` | locked |
| `mattos` | `1000:1000` | `mattos`, `sudo` | `/home/mattos` | `/bin/brush` | locked; autologin only until changed at runtime |

The permanent `/etc/sudoers` permits root and members of `%sudo`, and requires ordinary administrative users to authenticate. The live overlay alone adds `/etc/sudoers.d/00-mattos-live`, which grants `mattos` `NOPASSWD: ALL`. Live passwordless sudo is intentional, temporary, and profile-specific.

The tty1 and ttyS0 live overrides both run `agetty --autologin mattos`; agetty invokes `/bin/login`, which uses the forced-login path and still establishes the PAM-backed login session. Brush uses its full Reedline backend on tty1 and automatically selects its basic backend on PC serial devices such as ttyS0.

## Security-sensitive paths

The build enforces and validates these image modes. Initramfs assembly forces archive ownership to `0:0`; systemd-tmpfiles changes `/home/mattos` to `1000:1000` at boot.

| Path | Owner at runtime | Mode |
| --- | --- | --- |
| `/etc/shadow` | `root:root` | `0600` |
| `/etc/gshadow` | `root:root` | `0600` |
| `/etc/sudoers` | `root:root` | `0440` |
| `/etc/sudoers.d` | `root:root` | `0750` |
| `/etc/sudoers.d/00-mattos-live` | `root:root` | `0440` |
| `/usr/bin/login` | `root:root` | `04755` |
| `/usr/bin/su` | `root:root` | `04755` |
| `/usr/bin/passwd` | `root:root` | `04755` |
| `/usr/bin/sudo` | `root:root` | `04755` |
| `/root` | `root:root` | `0700` |
| `/home/mattos` | `mattos:mattos` | `0750` |

Only the four authentication programs that require an effective root identity are setuid. PAM modules and `unix_chkpwd` are not setuid.

The authentication binaries and modules use the staged ELF loader plus `libc.so.6`, `libpam.so.0`, `libpam_misc.so.0`, `libcrypt.so.1`, `libbsd.so.0`, `libmd.so.0`, and `libgcc_s.so.1` as required by their `DT_NEEDED` entries.

## Confirmed QEMU behavior

Both tty1 and ttyS0 autologin as the non-root `mattos` user with the prompt `mattos@MattOS:~$`, `/home/mattos` as the home and working directory, `/bin/brush` as the shell, and systemd as PID 1. `sudo --version` reports sudo-rs and the live-only `sudo id` succeeds without a password. Exiting Brush causes getty to start a fresh live session.

The session milestone adds the optional `pam_systemd` hook without changing authentication decisions. `loginctl` lists both live consoles: tty1 is associated with `seat0`, while ttyS0 correctly has no seat. Each login receives `/run/user/1000`, a session-bound `user@1000.service`, and a per-user D-Bus connection. See `SESSIONS.md` for lifecycle and bus details.

Runtime validation also confirmed:

- `useradd -m` creates a user, group, account-database entry, and mode-`0700` home;
- interactive `passwd` sets and changes passwords using PAM and yescrypt;
- normal username/password login works after disabling autologin on the test console;
- `su - USER` authenticates and establishes a login shell in that user's home;
- a locked root account rejects `su -`;
- an administrative user is rejected after incorrect sudo passwords and succeeds with the correct password;
- a user outside `%sudo` is denied by sudoers policy;
- the rescue entry runs `rescue-init` as PID 1 and opens a root Brush shell on tty1 independently of the normal authentication stack.

## Persistence boundary

The current root filesystem is an initramfs. Accounts, groups, password hashes, homes, and policy changes made at runtime disappear at reboot. A future installed-system profile must create persistent users and omit the live account database, both autologin overrides, the live MOTD, and `00-mattos-live`; installation is intentionally outside this milestone.
