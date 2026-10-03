# Handoff: follow-up fixes in progress (2026-10-03)

This is a working note for whoever continues this session's work. Read
`AGENTS.md` and the wiki (`docs/index.md`, `docs/project/rules.md`,
`docs/project/goals.md`) first. This file is not part of the wiki; delete it
once the work below is finished.

## Where things stand

- `main` is at `1388d93427` (pushed by the owner). Everything described under
  "Uncommitted work" is on top of it, **unstaged and uncommitted**.
- No build, QEMU test or publish is running. The last full build was stopped
  on purpose right after the `grub` stage finished, so the ISO in
  `out/images/` is from **before** the GRUB fix below. The next build rebuilds
  only the ISO-side stages.
- Last published state: all MattOS packages and the third-party codex 0.160.0
  and fastfetch 2.69.0 are published. Nothing in the uncommitted work changes a
  package payload yet, except the ISO's `efi.img`.

## Uncommitted work (4 items, implemented and unit-tested)

All tests pass: `cargo test -p mattos-build` (583), `cargo test -p mattos-installer`,
`python3 -m unittest discover -s DevUtils/tests` (160),
`python3 -m pytest third-party-packages/tests`, and
`python3 DevUtils/check_wiki_structure.py`.

### 1. Reproducible ISO (`efi.img`)

GRUB's `grub-mkrescue` runs the host's `mformat` without a volume serial, and
mtools then picks a random FAT serial. That serial was the only difference
between two builds of the ISO. `mcopy` already honors `SOURCE_DATE_EPOCH`.

- Fix: `upstream/patches/grub/0004-mkrescue-efi-image-serial.patch`. With
  `SOURCE_DATE_EPOCH` set, mkrescue passes `mformat -N <low 32 bits of the
  epoch>`. It is recorded in `upstream/patches/grub/manifest.toml`, and the
  manifest SHA-256 is updated in `upstream/sources.toml` and
  `upstream/state/grub.toml`. It is documented in
  `docs/system/boot/wifi-and-grub.md`.
- **Verified**: the rebuilt `grub` stage's `grub-mkrescue`, run twice a few
  seconds apart on the same tree, produced byte-identical ISOs.
- Not yet done: building the real ISO twice and comparing. This is optional;
  the stage-level check above is direct evidence.

### 2. Cache keys now cover shared helpers (`stages/helpers/*.rs`)

Background, already committed in `1388d93427`: recipe projection v4 splits
Rust items lexically and adds implicit recipe inputs. Every `stages/*.rs` file
a stage's recipe reaches is part of its key, even if `stage_inputs.rs` doesn't
list it.

- Uncommitted change: implicit coverage now also includes `stages/helpers/*.rs`
  (coverage marker `recipe-coverage:implicit` = `v2`).
  - `src/tools/mattos-build/src/recipe_projection.rs`: `implicit_recipe_inputs`
    and `IMPLICIT_RECIPE_COVERAGE_VERSION`.
  - `src/tools/mattos-build/src/stage_cache.rs`: manifests recorded under an
    older or missing marker adopt the newly covered files once, without a
    rebuild.
- `cache impact all` before the stopped build showed 302 MIGRATE and 1 MISS
  (`grub`, from the patch above).
- The test is `stage_keys_cover_recipe_files_their_recipe_calls_into_without_listing`
  in `main_tests.rs`. The docs are in `docs/build-system/architecture.md`.
- Only code outside `stages/` (`main.rs`, `performance.rs`, the scheduler)
  still needs a manual `recipe_revision` bump when it changes stage outputs.
  When bumping, check the stored manifest's `recipe=N` first
  (`out/state/stages/<id>.json`). The bump must exceed it: `LayerShellQt` was
  already at 1, so it had to go to 2.

### 3. Disk tooling: `mattos-build disk report` / `disk prune [--dry-run]`

- `src/tools/mattos-build/src/commands/disk.rs` (new), wired in `main.rs`.
  Tests are in `disk_tests` inside that file. Docs are in
  `docs/build-system/commands.md`.
- `disk report` is read-only. `disk prune` removes only:
  - `out/build/<name>` directories whose name the build tool's source never
    mentions;
  - `.<name>.building-<pid>` temporaries whose process is gone;
  - all pkg-config overlays, which are regenerated on demand;
  - `.deb` files `out/packages/inventory.toml` doesn't reference;
  - staging directories of removed packages;
  - manifests of removed stages;
  - `out/tmp` entries older than a day.
- It keeps the ISO, the upgrade baseline, QEMU disks and `out/cache`.
- It refuses to run (except with `--dry-run`) while a mattos-build
  build/package/image command, third-party recipe build, `run_qemu.py` or
  `PublishPackages.py` is running.
- Report on 2026-10-03: `out/` is 164 GB, of which about 32 GB is
  reclaimable:
  - 16 GB of old `out/tmp` experiments;
  - 11 GB of `.rootfs/.iso.building-*` leftovers;
  - 1.1 GB of overlays;
  - 1.8 GB of old `.deb` files.
- **Not yet run for real.** Run `disk prune --dry-run`, review the list, then
  `disk prune` when nothing is building. `out/tmp/provenance-cache` (a clone
  cache for the provenance audit, re-cloned on demand) is included as old
  scratch.

### 4. Discover end-to-end update check (installed-system test)

- `DevUtils/run_qemu.py`: for the Plasma profile (skipped with
  `--no-network`), `_verify_installed_disk_boot` now does the following:
  1. `prepare_update_probe_repository` repacks the installed `kcalc` `.deb`
     with version `<version>+updatetest1`, in pure Python (zstd from the 3.14
     stdlib). It writes a flat unsigned repository (`Packages`, `Release`) to
     `out/tmp/update-probe-repository`.
  2. `serve_flat_repository` serves it on host loopback; the guest reaches it
     at `10.0.2.2`.
  3. It appends `installed_discover_update_probes` to the checks.
- The probes, as framed serial commands:
  1. add an APT source;
  2. wait for the `DiscoverNotifier` process;
  3. `sudo pkgcli -y refresh`;
  4. `pkgcli list-updates` must show the new version;
  5. Discover's status-notifier item (`Id` contains `DiscoverNotifier`) must
     reach `Status` `Active`;
  6. `sudo pkgcli -y update kcalc`, after which dpkg must show the new
     version;
  7. remove the source.
- Tests are in `DevUtils/tests/test_run_qemu_update_probe.py`. The docs are in
  `docs/system/installer.md` ("Validation and initial package discovery").
- **Never run in a VM yet.** Expect possible first-run issues:
  - the notifier's SNI `Status` may need longer than 120 s (its autostart uses
    `--check-delay 20`);
  - `pkgcli` output format.

  The repacked package itself is verified with host `dpkg-deb`.

## What to do next

1. `cargo run -p mattos-build -- build`. It should rebuild only `grub`-dependent
   image stages (`iso`).
2. **Upgrade test**:
   `python3 DevUtils/run_qemu.py --upgrade-test --install-profile plasma`.
   The baseline `out/images/upgrade-baseline/mattos-x86_64.iso` is from
   2026-10-02 15:37, before the KDE-apps batch. This is the only check on that
   batch's package moves for existing installs:
   - GStreamer moved out of `xdg-desktop-portal` into `libgstreamer1.0-0` and
     `libgstreamer-plugins-base1.0-0`, which declare
     `Replaces: xdg-desktop-portal`;
   - `libjpeg62-turbo` and `libjansson4` became MattOS packages, replacing the
     `-0mattos` third-party builds.

   The report is `out/logs/upgrade-test.json`.
3. **Install test with the Discover probe**:
   `python3 DevUtils/run_qemu.py --install --install-profile plasma`. Don't
   pass `--no-build`: `--install` always builds and rejects it. Expect
   54 + 7 checks. The 14 `app-*` probes come from the earlier batch.
4. `cargo run -p mattos-build -- disk prune --dry-run`, then `disk prune`.
5. Commit and push **only if the owner asks**.

## Working rules from the owner (beyond AGENTS.md)

- Never stage, commit, push or otherwise touch git history/index unless asked
  in the current session. Never use or store the user's password. Never touch
  LinuxScripts.
- Post a status update at least every 10 minutes during long builds/tests.
- **Disk space is a major concern.** Do not add on-disk caching for
  third-party package builds (no persistent Cargo home/target, no sccache),
  even though codex takes about an hour to rebuild.
- `/tmp` is RAM-backed (tmpfs); keep big scratch under `out/tmp`.
- Prefer `cargo run -p mattos-build -- cache impact all` (read-only) before
  big builds to avoid surprise full rebuilds.
- Never `pkill -f` with a pattern that matches your own shell.
- In the original agent sandbox, network, podman and QEMU commands needed the
  sandbox disabled. "newuidmap" errors from podman mean the editor was
  launched with `NoNewPrivs`.
- The owner may play games on this machine. QEMU tests take screenshots of the
  QEMU window, so a covering fullscreen window can fail the
  `rejected-password` screenshot step. That is environmental; rerun.

## Known follow-ups (not started; owner said media gaps are fine for now)

- No hardware video decoding (VA-API) in FFmpeg/mpv. Haruna can't stream
  (no FFmpeg network protocols, no yt-dlp). mpv has no Lua.
- NASM man pages are not built (they need AsciiDoc).
- Haruna carries two harmless `$ORIGIN` RUNPATH entries from
  `qt_standard_project_setup` (Qt adds them unconditionally).
