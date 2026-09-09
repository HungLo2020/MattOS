# MattOS COSMIC session integration

MattOS uses upstream `greetd`, `cosmic-greeter`, `start-cosmic`, and
`cosmic-session`.  The system display-manager alias starts greetd on VT1;
greetd authenticates through the MattOS PAM stack and creates the logind
session, then starts the selected upstream Wayland session.  COSMIC itself
owns the compositor and its panel, launcher, settings daemon, notifications,
OSD, background, and workspace processes.

`multi-user.target` and the regular virtual consoles remain independent of
the display manager, so a failed graphical login can be recovered from a
different VT or by booting with `systemd.unit=multi-user.target`.

## Live graphics recovery

Live boots automatically capture `/run/mattos-graphics/report.txt`, `journal.txt`
and `dmesg.txt`; these are root-readable and ephemeral. `before-cosmic.txt`
records PCI identities, DRM/connector state, AMDGPU parameters and kernel logs
before greetd starts. A missing initialization line at that instant may simply
mean asynchronous probing; it does not block the compositor.

The live-only startup watchdog gives COSMIC 120 seconds to answer a read-only
`cosmic-randr list --kdl` request as the compositor's UID, with an enabled output
and positive current mode, and requires the compositor to own a DRM card FD.
A process/socket alone is not success. This proves a responsive output-control
path, **not** correct visible scanout on physical hardware. The check stops once
startup succeeds; it does not police later user monitor/session changes.

On timeout it preserves diagnostics, stops the live graphics entrypoints and
starts the existing normal live tty1 getty. It activates the text logind session
without selecting/resetting/unloading any GPU. Serial/network access remains
available. No first-boot installed-system behavior or graphics preferences change.
Users with exceptionally slow hardware can restart the display manager manually
after capturing evidence. A kernel/hardware lockup can prevent userspace recovery;
serial output or another boot in CLI mode is then required.

Choose **MattOS AMD graphics diagnostics (CLI)** in GRUB for `drm.debug=0x1ff`,
an 8 MiB kernel log ring and AMDGPU dynamic-debug logging. It retains AMDGPU/DC
and native acceleration, but does not start COSMIC automatically. `loglevel=4`
keeps verbose DRM records in dmesg/journal rather than feeding framebuffer-console
updates back into DRM debug logging; kernel errors remain visible. Wait for the
capture service, then preserve the logs before rebooting:

```sh
sudo systemctl status mattos-graphics-capture.service --no-pager
sudo mattos-graphics-startup --capture
sudo cp -r /run/mattos-graphics /path/to/writable/external/destination/
```

To compare CLI initialization with graphical startup from that same boot:
`sudo systemctl start cosmic-greeter.service`. The startup watchdog applies.
Capture again after a failure. Send all reports, the ISO SHA256, GPU model,
monitor/cable topology and any last-screen photograph. Firmware declarations are
not all mandatory; actual request failures plus PCI IDs determine relevance.
