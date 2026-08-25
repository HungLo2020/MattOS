# MattOS COSMIC defaults provenance

This directory is the MattOS-owned policy layer for COSMIC defaults.  The
vendored upstream trees under `src/desktop/cosmic/` remain authoritative
source inputs and are not modified by this snapshot.

The imported files were copied from these pinned upstream trees on 2026-08-24:

| Resource | Upstream repository | Imported commit | Source path | Purpose |
| --- | --- | --- | --- | --- |
| Panel and dock defaults | `https://github.com/pop-os/cosmic-panel.git` | `d6699ffc423a3830bf4cab7e2c7f08a173e998f0` | `data/default_schema/` | System COSMIC panel/dock configuration, with user overrides taking precedence |
| Settings accent palettes | `https://github.com/pop-os/cosmic-settings.git` | `7287257ec9f2ca301642bd4800f391ad9079d3e9` | `resources/accent_palette_{dark,light}.ron` | System appearance palette defaults |
| Initial Setup layouts and themes | `https://github.com/pop-os/cosmic-initial-setup.git` | `b5ac4182bb00bc774ca86febadf0369e362bc031` | `res/layouts/`, `res/themes/` | Layout and appearance choices shown by Initial Setup |

The commits above are the exact MattOS-imported revisions recorded in
`upstream/state/`.  This snapshot is intentionally limited to upstream
resources that COSMIC reads through documented runtime contracts; it does not
copy a developer home directory or generated build output.

## Runtime contracts

`resources/defaults/` preserves the COSMIC config hierarchy and is installed
under `/usr/share/cosmic/`.  COSMIC config lookup uses that system location as
the fallback and `~/.config/cosmic/` as the user layer, so users can override
individual defaults without modifying MattOS resources.

`resources/layouts/` is installed under `/usr/share/cosmic-layouts/`, and
`resources/themes/` under `/usr/share/cosmic-themes/`, matching the paths read
by the pinned `cosmic-initial-setup` source.
