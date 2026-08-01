# MattOS Live Profile

This profile overlays the live ISO with:
- automatic console login for `mattos` on tty1 and ttyS0;
- temporary account database entries for `mattos`;
- live-only passwordless sudo policy at `/etc/sudoers.d/00-mattos-live`.

Future installed profiles should omit these files to require normal login and sudo authentication.
