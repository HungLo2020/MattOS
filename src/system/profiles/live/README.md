# MattOS Live Profile

This profile overlays the live ISO with:

- automatic console login for `mattos` on tty1 and ttyS0;
- an automatic native COSMIC session for the graphical live boot mode, opened
  by the production greetd/PAM/logind display-manager service;
- temporary account database entries for `mattos`;
- a live MOTD and ephemeral home-directory policy;
- live-only passwordless sudo policy at `/etc/sudoers.d/00-mattos-live`.

The live account is UID/GID 1000, uses `/home/mattos` and `/bin/brush`, and is a member of the `sudo` group. Its password is locked in the image because login is performed by the intentional agetty or greetd initial-session paths. Password login remains available for accounts created at runtime.

Installed profiles omit the live account database, console/display-manager
autologin overrides, MOTD, and `00-mattos-live` so persistent users receive
normal password login and sudo authentication.
