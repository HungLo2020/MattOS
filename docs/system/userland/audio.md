# Desktop audio

The Plasma profile uses PipeWire, its PulseAudio compatibility server, and
WirePlumber as the session manager. Hardware audio requires the native ALSA
library, ALSA UCM hardware profiles, and PipeWire's ALSA SPA plugin.
The native ALSA utilities provide playback/mixer diagnostics and the upstream
`alsa-restore.service` to initialize, restore and save sound-card mixer state.
`libncursesw6` carries the form, menu and panel runtimes required by `alsamixer`,
with matching linker files and pkg-config metadata in `libncurses-dev`. Bluetooth
audio uses BlueZ and the native SBC codec. Optional AAC, aptX, LDAC and LE Audio
codecs are not enabled.

ALSA and SBC are ordinary pinned imports under `src/system/multimedia`, built
by the canonical Rust graph and available in the offline package repository.
ALSA utilities uses the matching checksum-pinned official release archive in
its output mirror for generated Autotools/gettext build infrastructure; its
vendored Git snapshot remains pristine.
The PipeWire package owns its ALSA/BlueZ plugins, ACP mixer profiles and device
quirk rules. WirePlumber starts with PipeWire; the PipeWire and PulseAudio
sockets are enabled for user sessions.

Systemd is built with ACL support against the native `libacl1`. Its udev
`70-uaccess.rules` tags sound devices, and logind grants the active local
session access to `/dev/snd`. Adding every user permanently to the `audio`
group is unnecessary. This also preserves seat-based device access for other
desktop peripherals.

The 26.10.1 image had ALSA and Bluetooth backends disabled and omitted udev's
ACL support. A kernel could detect sound cards while PipeWire showed no audio
devices. Those gaps are fixed for 26.10.2. Reboot after upgrading so udev and
logind run the ACL-enabled binaries.

For diagnostics, run `wpctl status`, `pactl info`, and inspect the user services
`pipewire`, `pipewire-pulse` and `wireplumber`. Hardware devices must produce an
`alsa_output` sink rather than only a dummy output.

The installed Plasma test attaches an Intel HDA virtual sound card backed by a
WAV capture. As the unprivileged graphical user it verifies sound-card access,
an ALSA sink, real playback, and PulseAudio compatibility through
`/usr/share/doc/pipewire/examples/audio-smoke.py`. The host then verifies that
the virtual card emitted non-silent PCM. A daemon reporting active or a null
sink alone cannot pass. This verifies the software path; actual Bluetooth
pairing and physical speaker output still need matching hardware.
