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
