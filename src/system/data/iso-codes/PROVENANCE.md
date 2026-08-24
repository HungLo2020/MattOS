# ISO-codes runtime data

These JSON files are selected from the official `iso-codes` project at the
immutable Salsa commit `078b9578822fcd68386d92347089e7e45a8bf47d`, tagged
`v4.20.1`:

`https://salsa.debian.org/iso-codes-team/iso-codes`

MattOS ships only the three registries required by the pinned
`locales-rs` dependency:

| File | SHA-256 |
| --- | --- |
| `iso_3166-1.json` | `f01b812b57fba9f31ff621bf33e7c7570a01964dbeb5be2167e94decf538c89f` |
| `iso_639-2.json` | `0e9c25b3860cec74ce9367b0434c538caac4a3128a4c91c5fc7824fdc648d479` |
| `iso_639-3.json` | `2c61a9bb90a8c50c46bfbab484838863a12335bfdd0a92b4809f3faf1756b22d` |

The upstream project is licensed under LGPL-2.1-or-later. This directory is
runtime data, not a host-generated locale database.
