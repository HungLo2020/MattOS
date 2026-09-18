# MattOS Calamares integration

`upstream/` is a pinned, unmodified Calamares 3.4 source import. `mattos/` is
MattOS policy and branding, installed outside the upstream tree.

The initial source-owned closure is Qt 6 Widgets/SVG/Wayland and KF6
CoreAddons/I18n/WidgetsAddons, Polkit-Qt6, YAML-CPP and KPMCore. QML,
Kirigami, Python, webview, package-manager and desktop-specific Calamares
modules are deliberately excluded. This keeps the first installer UI limited
to welcome, locale, keyboard, partitioning, users, summary, progress and
completion.

Guided storage policy is GPT/UEFI with an EFI system partition and Btrfs root.
The existing MattOS installer policy creates `@`, `@home` and `@snapshots`.
Manual ext4 remains an existing Rust installer capability until the Calamares
adapter has equivalent policy coverage. Calamares must not decide package or
profile composition; the `mattos-executor` boundary delegates that to the
existing Rust installer engine.

## Closure classification

| Component | Classification | Runtime evidence |
| --- | --- | --- |
| QtBase | both | `Qt6::Core`, `Gui`, `Widgets`, `Network`, `DBus` and platform/image plugins are loaded by Calamares; `moc`, `rcc`, `uic`, `qmake` and `qtpaths` are build tools. |
| QtSvg | both | Calamares requires `Qt6::Svg`; its SVG library/plugin is runtime material, while its CMake helpers are build-only. |
| QtWayland | both | the `wayland` QPA platform plugin is runtime-loaded under a Wayland session; its generators are build-only. |
| KCoreAddons, KI18n, KWidgetsAddons | both | KPMCore and Calamares link/load the framework libraries and translations; their CMake metadata is consumed while building. |
| Polkit-Qt6 | both | KPMCore's external-command authorization path links the library and installs a policy; its CMake metadata is build-only. |
| yaml-cpp | both | Calamares links YAML-CPP to parse settings/modules; headers and CMake metadata are downstream build inputs. |
| KPMCore | both | Calamares loads its partition backend and KPMCore loads the sfdisk plugin; headers/CMake metadata are needed by Calamares. |
| Calamares | both | the executable, `libcalamares`, modules, configuration and translations are runtime material; module-build CMake helpers are build-only. |
| Extra CMake Modules | build-tool-only | ECM supplies CMake macros only. It is intentionally not source-owned or packaged for runtime closure. |

Host `cmake`, `ninja`, `pkg-config`, compiler drivers and the Qt code generators
are permitted only as build tools. They must never provide a target header,
library, CMake package, plugin, translation or executable in the MattOS image.
