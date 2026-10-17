# Desktop Defaults

The Plasma profile supplies the native KDE Qt platform theme through
`plasma-integration`. The session selects `QT_QPA_PLATFORMTHEME=kde` and
exports the target plugin directories to its user manager. This provides
KDE palette, font, file-dialog and settings-change integration for Qt
applications, including Dolphin. The default color scheme is Nordic;
per-user KDE settings take precedence over the packaged defaults.

## Fonts and terminals

`fonts-fira` maps Fontconfig's `sans-serif` to Fira Sans and `monospace` to
Fira Mono. KDE defaults to Fira Sans at 10pt and Fira Mono at 11pt. Konsole's
packaged MattOS profile uses Fira Mono at 11pt with a Nordic terminal color
scheme. Users can select another profile or change these settings normally.

The Plasma profile also includes `fonts-noto-cjk` (the SC, TC, HK, JP and KR
Noto Sans families) and `fonts-noto-color-emoji`. These are pinned upstream
font assets installed as data, following the existing Fira stage. Explicit
source-selection policies retain their licenses and supported font families.

## Interactive shell defaults

`mattos-base-files` owns `/etc/bash.bashrc`. Brush loads this for interactive
non-login shells, including Konsole. `/etc/profile` also sources it for
interactive login shells. The default prompt shows a green user/hostname
and blue directory; root uses a red user/hostname. Dumb terminals and
`NO_COLOR=1` get a plain prompt. Noninteractive shells are unaffected.
User `.bashrc` and `.brushrc` files can override the system defaults.

## Desktop helpers and Bluetooth

`xdg-utils` supplies the standard `xdg-open`, `xdg-mime`, `xdg-settings` and
related desktop integration commands. Its shell programs and embedded help
are generated from the vendored templates and DocBook sources in an output
mirror. Host `xmlto`, `xsltproc`, DocBook and `w3m` are build-only prerequisites
installed by `DevUtils/setup.py`; none are copied into MattOS. KDE's helper
programs come from `kde-cli-tools`.

BlueDevil supplies the Bluetooth pairing wizard, System Settings module,
Plasma applet and session integration. `kf6-bluez-qt` supplies its Bluetooth
library and QML module; BlueZ remains the system daemon. A machine without
an adapter should show that state without a missing-module error. Real
pairing still requires Bluetooth hardware.

## Wallets and logs

See [Authentication](../services/authentication.md) for the wallet daemon,
PAM hooks and post-login environment handshake, and
[Systemd boot](../boot/systemd-boot.md) for persistent installed journals.
