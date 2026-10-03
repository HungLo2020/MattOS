//! Qt, KDE, Plasma, graphics, multimedia and Flatpak package payloads.

use super::*;

/// Preserve one upstream Qt module's complete `/usr` prefix. Qt plugins,
/// tools and imported CMake targets reference sibling paths below that prefix,
/// so an arbitrary runtime/development split would make later KF6 consumers
/// fragile without reducing the required Qt foundation closure.
pub(super) fn stage_qt_module(
    repo_root: &Path,
    staging: &Path,
    component: &str,
    package: &str,
) -> Result<()> {
    let install = component_install(repo_root, component).join("usr");
    if !install.is_dir() {
        bail!(
            "Qt package {package} requires staged {component} output at {}",
            install.display()
        );
    }
    // A Qt module's install prefix is normally self-contained.  The Qt
    // module build can, however, leave absolute links to sibling module
    // outputs in the shared plugin tree while standalone module configure
    // views are being assembled.  Those links are build-tree plumbing, not
    // payload owned by this package.  Copy the prefix while rejecting links
    // whose lexical target escapes the producing module's install prefix;
    // otherwise the first package would claim a sibling plugin and the
    // ownership audit would (correctly) reject the package set.
    copy_qt_prefix_without_foreign_links(&install, &staging.join("usr"))?;
    if component == "qtdeclarative" {
        // Qt Declarative installs QML test fixtures/plugins alongside the
        // runtime. They are not part of the target runtime package and some
        // embed the build host path; keep them available in the stage output
        // but exclude them from the shipped package.
        let tests = staging.join("usr/qml/Qt/test");
        if tests.exists() {
            fs::remove_dir_all(tests)?;
        }
        // QuickTestUtilsPrivate is a static test-support library, not part of
        // the QtQuick runtime or the development interface consumed by the
        // Plasma closure.  Qt builds it as an archive containing CMake's
        // disposable object-directory path; unlike ELF payloads it is not
        // processed by the debug-strip pass, so retaining it would publish a
        // host checkout path and fail the package boundary audit.  Exclude
        // the test-only archives and their qmake sidecars together with the
        // already-excluded QML fixtures.
        for name in [
            "libQt6QuickTestUtils.a",
            "libQt6QuickTestUtils.prl",
            "libQt6QuickControlsTestUtils.a",
            "libQt6QuickControlsTestUtils.prl",
        ] {
            let path = staging.join("usr/lib/x86_64-linux-gnu").join(name);
            if path.exists() {
                fs::remove_file(path)?;
            }
        }
    }
    let source = repo_root.join("src/desktop/qt").join(component);
    let licenses = source.join("LICENSES");
    if !licenses.is_dir() {
        bail!(
            "Qt package {package} is missing retained upstream licenses at {}",
            licenses.display()
        );
    }
    copy_tree_preserving(
        &licenses,
        &staging.join("usr/share/doc").join(package).join("licenses"),
    )?;
    copy_preserving(
        &source.join("licenseRule.json"),
        &staging
            .join("usr/share/doc")
            .join(package)
            .join("licenseRule.json"),
    )
}

pub(super) fn copy_qt_prefix_without_foreign_links(
    source: &Path,
    destination: &Path,
) -> Result<()> {
    #[cfg(unix)]
    let mut hardlinks = BTreeMap::new();
    copy_qt_prefix_inner(source, destination, source, &mut hardlinks)
}

pub(super) fn copy_qt_prefix_inner(
    source: &Path,
    destination: &Path,
    owner_prefix: &Path,
    hardlinks: &mut HardlinkMap,
) -> Result<()> {
    if !source.is_dir() {
        bail!(
            "required package input directory missing at {}",
            source.display()
        );
    }
    fs::create_dir_all(destination)?;
    let mut entries = fs::read_dir(source)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let from = entry.path();
        let to = destination.join(entry.file_name());
        let metadata = fs::symlink_metadata(&from)?;
        if metadata.file_type().is_symlink() {
            let target = fs::read_link(&from)?;
            let resolved = if target.is_absolute() {
                target
            } else {
                from.parent().unwrap_or(owner_prefix).join(target)
            };
            if !resolved.starts_with(owner_prefix) {
                continue;
            }
        }
        if metadata.is_dir() {
            copy_qt_prefix_inner(&from, &to, owner_prefix, hardlinks)?;
        } else {
            copy_path_preserving_with_hardlinks(&from, &to, &metadata, hardlinks)?;
        }
    }
    Ok(())
}

pub(super) fn stage_plasma_session_integration(repo_root: &Path, staging: &Path) -> Result<()> {
    let integration = repo_root.join("src/system/session/plasma");
    for (source, destination) in [
        (
            "plasma-desktop.conf",
            "usr/lib/environment.d/90-plasma-desktop.conf",
        ),
        (
            "mattos-graphics-watchdog.service",
            "usr/lib/systemd/system/mattos-graphics-watchdog.service",
        ),
        (
            "mattos-graphics-recovery.service",
            "usr/lib/systemd/system/mattos-graphics-recovery.service",
        ),
        (
            "mattos-graphics-capture.service",
            "usr/lib/systemd/system/mattos-graphics-capture.service",
        ),
    ] {
        copy_preserving(&integration.join(source), &staging.join(destination))?;
    }
    for name in ["mattos-graphics-report", "mattos-graphics-startup"] {
        copy_preserving(&integration.join(name), &staging.join("usr/bin").join(name))?;
        set_mode(staging.join("usr/bin").join(name), 0o755)?;
    }

    let wants = staging.join("etc/systemd/system/multi-user.target.wants");
    fs::create_dir_all(&wants)?;
    #[cfg(unix)]
    std::os::unix::fs::symlink(
        "/usr/lib/systemd/system/mattos-graphics-capture.service",
        wants.join("mattos-graphics-capture.service"),
    )?;

    for required in [
        "usr/bin/mattos-graphics-report",
        "usr/bin/mattos-graphics-startup",
        "usr/lib/systemd/system/mattos-graphics-watchdog.service",
        "usr/lib/environment.d/90-plasma-desktop.conf",
    ] {
        if fs::symlink_metadata(staging.join(required)).is_err() {
            bail!("plasma-desktop package is missing /{required}");
        }
    }
    Ok(())
}

pub(super) fn stage_plasma_login_manager_integration(
    repo_root: &Path,
    staging: &Path,
) -> Result<()> {
    let policy = repo_root.join("src/system/session/plasma-login-manager");
    for (source, destination) in [
        ("plasmalogin.pam", "etc/pam.d/plasmalogin"),
        ("plasmalogin-greeter.pam", "etc/pam.d/plasmalogin-greeter"),
        (
            "plasmalogin-autologin.pam",
            "etc/pam.d/plasmalogin-autologin",
        ),
        (
            "mattos-plasma.desktop",
            "usr/share/wayland-sessions/mattos-plasma.desktop",
        ),
        ("../plasma/start-plasma", "usr/bin/start-plasma"),
    ] {
        copy_preserving(&policy.join(source), &staging.join(destination))?;
    }
    set_mode(staging.join("usr/bin/start-plasma"), 0o755)?;
    // Upstream PLM's KWin unit requests optional KWin features unconditionally.
    // MattOS deliberately builds KWin without its screen locker and Activities
    // integration, so those conditional CLI switches are not compiled into
    // our kwin_wayland binary. Remove only the switches for disabled features
    // from the package-owned unit; keep upstream source pristine and preserve
    // all supported options (notably the Wayland input method and locale1).
    let kwin_unit = staging.join("usr/lib/systemd/user/plasma-login-kwin_wayland.service");
    let unit_contents = fs::read_to_string(&kwin_unit).with_context(|| {
        format!(
            "Plasma Login Manager did not install its KWin user unit at {}",
            kwin_unit.display()
        )
    })?;
    let unit_contents = adapt_plasma_login_kwin_unit(&unit_contents)?;
    fs::write(&kwin_unit, unit_contents)?;
    // KWin opens the active seat's DRM card/render nodes as the dedicated
    // greeter account.  MattOS's installed users receive these groups from
    // the installer, but the system account generated by PLM otherwise has
    // only its private primary group and cannot open /dev/dri/card*.  Keep
    // the access scoped to the display-manager account and let systemd's
    // native sysusers mechanism apply it before the login-manager starts.
    let sysusers = staging.join("usr/lib/sysusers.d/plasmalogin.conf");
    let sysusers_contents = fs::read_to_string(&sysusers)?;
    fs::write(&sysusers, adapt_plasma_login_sysusers(&sysusers_contents)?)?;
    for required in [
        "usr/bin/plasmalogin",
        "usr/bin/start-plasma",
        "usr/lib/systemd/system/plasmalogin.service",
        "usr/lib/systemd/user/plasma-login-kwin_wayland.service",
        "usr/lib/sysusers.d/plasmalogin.conf",
        "usr/lib/tmpfiles.d/plasmalogin.conf",
        "etc/pam.d/plasmalogin",
        "etc/pam.d/plasmalogin-greeter",
        "etc/pam.d/plasmalogin-autologin",
        "usr/share/wayland-sessions/mattos-plasma.desktop",
    ] {
        if fs::symlink_metadata(staging.join(required)).is_err() {
            bail!("plasma-login-manager package is missing /{required}");
        }
    }
    let session =
        fs::read_to_string(staging.join("usr/share/wayland-sessions/mattos-plasma.desktop"))?;
    if !session.contains("Exec=/usr/bin/start-plasma") || session.contains("startplasma-x11") {
        bail!("MattOS Plasma Login Manager session must select start-plasma Wayland wrapper");
    }
    Ok(())
}

pub(super) fn adapt_plasma_login_kwin_unit(contents: &str) -> Result<String> {
    const DISABLED_FEATURE_OPTIONS: [&str; 2] = ["--no-lockscreen", "--no-kactivities"];
    let mut exec_start_count = 0;
    let mut adjusted = String::with_capacity(contents.len());
    for line in contents.lines() {
        if let Some(command) = line.strip_prefix("ExecStart=") {
            exec_start_count += 1;
            if !command.contains("/kwin_wayland ") {
                bail!("Plasma Login Manager KWin unit does not launch kwin_wayland");
            }
            let mut adapted = command.to_owned();
            for option in DISABLED_FEATURE_OPTIONS {
                let occurrences = adapted
                    .split_whitespace()
                    .filter(|part| *part == option)
                    .count();
                if occurrences != 1 {
                    bail!(
                        "expected exactly one {option} in upstream Plasma Login Manager KWin unit; found {occurrences}"
                    );
                }
                adapted = adapted.replace(&format!(" {option}"), "");
            }
            adjusted.push_str("ExecStart=");
            adjusted.push_str(&adapted);
        } else {
            adjusted.push_str(line);
        }
        adjusted.push('\n');
    }
    if exec_start_count != 1 {
        bail!(
            "expected exactly one ExecStart in Plasma Login Manager KWin unit; found {exec_start_count}"
        );
    }
    if !adjusted.contains("--no-global-shortcuts")
        || !adjusted.contains("--inputmethod plasma-keyboard --locale1")
        || adjusted.contains("--no-lockscreen")
        || adjusted.contains("--no-kactivities")
    {
        bail!("adapted Plasma Login Manager KWin unit does not match MattOS KWin feature policy");
    }
    Ok(adjusted)
}

pub(super) fn adapt_plasma_login_sysusers(contents: &str) -> Result<String> {
    let account_lines = contents
        .lines()
        .filter(|line| {
            let mut fields = line.split_whitespace();
            fields.next() == Some("u") && fields.next() == Some("plasmalogin")
        })
        .count();
    if account_lines != 1 {
        bail!("expected exactly one upstream plasmalogin sysusers account; found {account_lines}");
    }

    let mut output = contents.trim_end_matches('\n').to_owned();
    for group in ["video", "render"] {
        let expected = format!("m plasmalogin {group}");
        let memberships = contents
            .lines()
            .filter(|line| {
                let fields = line.split_whitespace().collect::<Vec<_>>();
                fields.as_slice() == ["m", "plasmalogin", group]
            })
            .count();
        if memberships > 1 {
            bail!("duplicate plasmalogin membership for {group} in upstream sysusers file");
        }
        if memberships == 0 {
            output.push('\n');
            output.push_str(&expected);
        }
    }
    output.push('\n');
    Ok(output)
}

#[cfg(test)]
#[test]
fn plasma_login_kwin_unit_omits_disabled_kwin_feature_options_only() {
    let upstream = "[Service]\nExecStart=/usr/bin/kwin_wayland --no-lockscreen --no-global-shortcuts --no-kactivities --inputmethod plasma-keyboard --locale1\n";
    let adapted = adapt_plasma_login_kwin_unit(upstream).unwrap();
    assert_eq!(
        adapted,
        "[Service]\nExecStart=/usr/bin/kwin_wayland --no-global-shortcuts --inputmethod plasma-keyboard --locale1\n"
    );
    assert!(
        adapt_plasma_login_kwin_unit("[Service]\nExecStart=/usr/bin/kwin_wayland --locale1\n")
            .is_err()
    );
    assert!(
        adapt_plasma_login_kwin_unit(
            "[Service]\nExecStart=/usr/bin/kwin --no-lockscreen --no-kactivities\n"
        )
        .is_err()
    );
}

#[cfg(test)]
#[test]
fn plasma_login_greeter_receives_only_required_drm_device_groups() {
    let upstream =
        "# Generated by Plasma Login Manager\nu plasmalogin - \"Greeter\" /var/lib/plasmalogin -\n";
    let adapted = adapt_plasma_login_sysusers(upstream).unwrap();
    assert_eq!(
        adapted,
        "# Generated by Plasma Login Manager\nu plasmalogin - \"Greeter\" /var/lib/plasmalogin -\nm plasmalogin video\nm plasmalogin render\n"
    );
    assert_eq!(adapt_plasma_login_sysusers(&adapted).unwrap(), adapted);
    assert!(adapt_plasma_login_sysusers("u other - \"Greeter\" - -\n").is_err());
    assert!(
        adapt_plasma_login_sysusers(
            "u plasmalogin - \"Greeter\" - -\nm plasmalogin video\nm plasmalogin video\n"
        )
        .is_err()
    );
}

pub(super) fn stage_plasma_live_session_integration(
    repo_root: &Path,
    staging: &Path,
) -> Result<()> {
    let integration = repo_root.join("src/system/session/plasma");
    for (source, destination) in [
        ("plasma-live.toml", "etc/greetd/plasma-live.toml"),
        ("plasma-greeter.pam", "etc/pam.d/plasma-greeter"),
        (
            "plasma-greeter.service",
            "usr/lib/systemd/system/plasma-greeter.service",
        ),
    ] {
        copy_preserving(&integration.join(source), &staging.join(destination))?;
    }
    for name in ["mattos-plasma-greeter"] {
        copy_preserving(&integration.join(name), &staging.join("usr/bin").join(name))?;
        set_mode(staging.join("usr/bin").join(name), 0o755)?;
    }
    let display_manager = staging.join("etc/systemd/system/display-manager.service");
    fs::create_dir_all(display_manager.parent().expect("display-manager parent"))?;
    #[cfg(unix)]
    std::os::unix::fs::symlink(
        "/usr/lib/systemd/system/plasma-greeter.service",
        &display_manager,
    )?;
    for required in [
        "etc/greetd/plasma-live.toml",
        "etc/pam.d/plasma-greeter",
        "etc/systemd/system/display-manager.service",
        "usr/bin/mattos-plasma-greeter",
        "usr/lib/systemd/system/plasma-greeter.service",
    ] {
        if fs::symlink_metadata(staging.join(required)).is_err() {
            bail!("mattos-plasma-live package is missing /{required}");
        }
    }
    Ok(())
}

pub(super) const MATTOS_THEME_CONFIG_DIR: &str = "src/system/desktop/branding/MattOS";
pub(super) const MATTOS_DESKTOP_DIRECTORY_OVERRIDES: &[&str] = &[
    "kf5-development.directory",
    "kf5-education.directory",
    "kf5-games.directory",
    "kf5-graphics.directory",
    "kf5-internet.directory",
    "kf5-multimedia.directory",
    "kf5-office.directory",
    "kf5-science.directory",
    "kf5-system.directory",
    "kf5-utilities.directory",
];

pub(super) fn is_mattos_desktop_directory_override(relative: &Path) -> bool {
    relative.starts_with("share/desktop-directories")
        && relative
            .file_name()
            .and_then(OsStr::to_str)
            .is_some_and(|name| MATTOS_DESKTOP_DIRECTORY_OVERRIDES.contains(&name))
}

pub(super) fn stage_mattos_plasma_theme(repo_root: &Path, staging: &Path) -> Result<()> {
    let branding = repo_root.join(MATTOS_THEME_CONFIG_DIR);
    let laf_root = staging.join("usr/share/plasma/look-and-feel/org.mattos.desktop");
    copy_preserving(
        &branding.join("metadata.json"),
        &laf_root.join("metadata.json"),
    )?;
    copy_tree_preserving(&branding.join("contents"), &laf_root.join("contents"))?;
    let layout_path = laf_root.join("contents/layouts/org.kde.plasma.desktop-layout.js");
    let layout_template = fs::read_to_string(&layout_path)?;
    let panel_config = fs::read_to_string(branding.join("panel-defaults.conf"))?;
    fs::write(
        &layout_path,
        render_mattos_panel_layout(&layout_template, &panel_config)?,
    )?;

    let theme_root = staging.join("usr/share/plasma/desktoptheme/Nordic");
    let nordic = repo_root.join("src/desktop/themes/nordic-kde");
    for relative in ["widgets", "icons", "dialogs"] {
        copy_tree_preserving(&nordic.join(relative), &theme_root.join(relative))?;
    }
    copy_preserving(
        &nordic.join("metadata.desktop"),
        &theme_root.join("metadata.desktop"),
    )?;
    copy_preserving(
        &nordic.join("colors"),
        &staging.join("usr/share/color-schemes/Nordic.colors"),
    )?;

    copy_tree_preserving(
        &repo_root.join("src/desktop/themes/papirus-icon-theme/Papirus"),
        &staging.join("usr/share/icons/Papirus"),
    )?;
    copy_tree_preserving(
        &repo_root.join("src/desktop/themes/papirus-icon-theme/Papirus-Dark"),
        &staging.join("usr/share/icons/Papirus-Dark"),
    )?;
    let papirus_dark_index = staging.join("usr/share/icons/Papirus-Dark/index.theme");
    let index_contents = fs::read_to_string(&papirus_dark_index)?;
    let patched_index = add_papirus_base_fallback(&index_contents)?;
    fs::write(&papirus_dark_index, patched_index)?;
    copy_tree_preserving(
        &repo_root.join("src/desktop/themes/utterly-round-aurorae/Utterly-Round-Dark"),
        &staging.join("usr/share/aurorae/themes/Utterly-Round-Dark"),
    )?;
    copy_tree_preserving(
        &repo_root
            .join("out/build/material-cursors/install/usr/share/icons/material_light_cursors"),
        &staging.join("usr/share/icons/material_light_cursors"),
    )?;

    for config in ["kdeglobals", "kcminputrc", "kwinrc", "plasmarc"] {
        copy_preserving(
            &branding.join("xdg").join(config),
            &staging.join("etc/xdg").join(config),
        )?;
    }
    copy_tree_preserving(
        &branding.join("desktop-directories"),
        &staging.join("usr/share/desktop-directories"),
    )?;

    let docs = staging.join("usr/share/doc/mattos-plasma-theme/upstream-licenses");
    for (source, destination, is_directory) in [
        ("src/desktop/themes/nordic-kde/LICENSE", "nordic-kde", true),
        (
            "src/desktop/themes/papirus-icon-theme/LICENSE",
            "papirus-icon-theme.LICENSE",
            false,
        ),
        (
            "src/desktop/themes/material-cursors/LICENSE",
            "material-cursors.LICENSE",
            false,
        ),
        (
            "src/desktop/themes/utterly-round-aurorae/Utterly-Round-Dark/LICENSE.md",
            "utterly-round-aurorae.LICENSE.md",
            false,
        ),
    ] {
        if is_directory {
            copy_tree_preserving(&repo_root.join(source), &docs.join(destination))?;
        } else {
            copy_preserving(&repo_root.join(source), &docs.join(destination))?;
        }
    }
    let notice = "MattOS-owned desktop defaults. Upstream source revisions are recorded in mattos-build-info.toml. Wallpaper image files are intentionally not included; the configured slideshow paths are stored in the Plasma layout.\n";
    fs::write(
        staging.join("usr/share/doc/mattos-plasma-theme/README.MattOS"),
        notice,
    )?;

    for required in [
        "usr/share/plasma/look-and-feel/org.mattos.desktop/metadata.json",
        "usr/share/plasma/look-and-feel/org.mattos.desktop/contents/layouts/org.kde.plasma.desktop-layout.js",
        "usr/share/plasma/desktoptheme/Nordic/metadata.desktop",
        "usr/share/color-schemes/Nordic.colors",
        "usr/share/icons/Papirus/index.theme",
        "usr/share/icons/Papirus-Dark/index.theme",
        "usr/share/icons/Papirus/24x24/apps/org.kde.dolphin.svg",
        "usr/share/icons/Papirus/24x24/apps/kate.svg",
        "usr/share/icons/Papirus/24x24/apps/utilities-terminal.svg",
        "usr/share/icons/material_light_cursors/index.theme",
        "usr/share/icons/material_light_cursors/cursors/left_ptr",
        "usr/share/aurorae/themes/Utterly-Round-Dark/metadata.json",
        "etc/xdg/kdeglobals",
        "etc/xdg/kcminputrc",
        "etc/xdg/kwinrc",
        "etc/xdg/plasmarc",
    ] {
        if !staging.join(required).is_file() {
            bail!("mattos-plasma-theme package is missing /{required}");
        }
    }
    if staging.join("usr/share/wallpapers").exists() {
        bail!("mattos-plasma-theme must not package wallpaper images");
    }
    Ok(())
}

pub(super) fn add_papirus_base_fallback(index: &str) -> Result<String> {
    let original = "Inherits=breeze-dark,hicolor";
    if index.matches(original).count() != 1 {
        bail!("Papirus-Dark index.theme must contain exactly one known inheritance declaration");
    }
    Ok(index.replacen(original, "Inherits=Papirus,breeze-dark,hicolor", 1))
}

pub(super) fn render_mattos_panel_layout(template: &str, config: &str) -> Result<String> {
    let mut values = BTreeMap::new();
    let mut section = String::new();
    for (line_number, line) in config.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            section = line[1..line.len() - 1].to_string();
            continue;
        }
        if section != "Panel" {
            bail!(
                "panel-defaults.conf has unsupported section on line {}",
                line_number + 1
            );
        }
        let (key, value) = line
            .split_once('=')
            .with_context(|| format!("invalid panel-defaults.conf line {}", line_number + 1))?;
        let key = key.trim();
        if !matches!(
            key,
            "floating" | "lengthMode" | "opacityMode" | "visibilityMode" | "thickness"
        ) {
            bail!("unknown MattOS panel default {key}");
        }
        if values
            .insert(key.to_string(), value.trim().to_string())
            .is_some()
        {
            bail!("duplicate MattOS panel default {key}");
        }
    }
    let get = |key: &str| {
        values
            .get(key)
            .with_context(|| format!("panel-defaults.conf is missing {key}"))
    };
    let floating = match get("floating")?.as_str() {
        "true" => "true",
        "false" => "false",
        other => bail!("invalid panel floating value {other:?}"),
    };
    let length_mode = match get("lengthMode")?.as_str() {
        "0" => "fill",
        "1" => "fit",
        "2" => "custom",
        other => bail!("invalid panel lengthMode value {other:?}"),
    };
    let opacity = match get("opacityMode")?.as_str() {
        "0" => "adaptive",
        "1" => "opaque",
        "2" => "translucent",
        other => bail!("invalid panel opacityMode value {other:?}"),
    };
    let hiding = match get("visibilityMode")?.as_str() {
        "0" => "none",
        "1" => "autohide",
        "2" => "dodgewindows",
        "3" => "windowsgobelow",
        other => bail!("invalid panel visibilityMode value {other:?}"),
    };
    let thickness = get("thickness")?
        .parse::<u16>()
        .context("panel thickness must be a positive integer")?;
    if !(24..=256).contains(&thickness) {
        bail!("panel thickness {thickness} is outside the supported 24..=256 range");
    }
    let rendered = template
        .replace("@@MATTOS_PANEL_FLOATING@@", floating)
        .replace("@@MATTOS_PANEL_LENGTH_MODE@@", length_mode)
        .replace("@@MATTOS_PANEL_OPACITY@@", opacity)
        .replace("@@MATTOS_PANEL_HIDING@@", hiding)
        .replace("@@MATTOS_PANEL_THICKNESS@@", &thickness.to_string());
    if rendered.contains("@@MATTOS_PANEL_") {
        bail!("unexpanded MattOS panel-defaults template token");
    }
    Ok(rendered)
}

pub(super) fn stage_kde_module(
    repo_root: &Path,
    staging: &Path,
    component: &str,
    package: &str,
) -> Result<()> {
    let install_root = component_install(repo_root, component);
    let install = install_root.join("usr");
    if !install.is_dir() {
        bail!(
            "KDE package {package} requires staged {component} output at {}",
            install.display()
        );
    }
    if component == "plasma-workspace" {
        // MattOS's Plasma appearance package owns these customized menu
        // categories. The upstream copy would otherwise claim the same paths.
        copy_tree_filtered(&install, &staging.join("usr"), &|relative, _| {
            !is_mattos_desktop_directory_override(relative)
        })?;
    } else {
        copy_tree_preserving(&install, &staging.join("usr"))?;
    }
    // ModemManagerQt's disposable SDK prefix is hydrated with the underlying
    // ModemManager C headers so isolated downstream CMake probes can resolve
    // its exported private include contract.  Those headers remain owned by
    // the modemmanager package and must not be duplicated in the Qt wrapper.
    if component == "modemmanager-qt" {
        let dependency_headers = staging.join("usr/include/ModemManager");
        if dependency_headers.exists() {
            fs::remove_dir_all(&dependency_headers)?;
        }
    }
    // Plasma's shell autostart desktop file is installed by KDE's
    // KDE_INSTALL_AUTOSTARTDIR, which is /etc/xdg/autostart in this target.
    // Keep it with the workspace package; omitting it leaves the classic
    // plasma_session fallback with KWin but no plasmashell.  The systemd-user
    // path remains the primary path, so this is also the correct fallback
    // payload rather than a package-specific launcher.
    // Discover's update notifier starts with the session the same way.
    if component == "discover" {
        let autostart = install_root.join("etc/xdg/autostart");
        if !autostart.join("org.kde.discover.notifier.desktop").is_file() {
            bail!("Discover output lacks etc/xdg/autostart/org.kde.discover.notifier.desktop");
        }
        copy_tree_preserving(&autostart, &staging.join("etc/xdg/autostart"))?;
    }
    if component == "plasma-workspace" {
        let autostart = install_root.join("etc/xdg/autostart");
        if autostart.is_dir() {
            copy_tree_preserving(&autostart, &staging.join("etc/xdg/autostart"))?;
        }
        // KService resolves the desktop application catalog through the
        // freedesktop menu definition selected by XDG_MENU_PREFIX=plasma-.
        // Without this non-/usr payload kbuildsycoca6 creates an effectively
        // empty cache and Kicker cannot discover installed applications.
        let menus = install_root.join("etc/xdg/menus");
        if !menus.join("plasma-applications.menu").is_file() {
            bail!("Plasma workspace output lacks etc/xdg/menus/plasma-applications.menu");
        }
        copy_tree_preserving(&menus, &staging.join("etc/xdg/menus"))?;
        // ECM's QtBinariesDir query is evaluated against the build prefix.
        // Normalize generated user units to the installed target path before
        // publication; a runtime unit must never retain a developer checkout
        // path or depend on a build-tree qdbus executable.
        let host_qdbus = repo_root
            .join("out/build/qtbase/install/usr/bin/qdbus")
            .display()
            .to_string();
        let user_units = staging.join("usr/lib/systemd/user");
        if user_units.is_dir() {
            for entry in fs::read_dir(&user_units)? {
                let path = entry?.path();
                if path.extension().and_then(|ext| ext.to_str()) != Some("service") {
                    continue;
                }
                let contents = fs::read_to_string(&path)?;
                let normalized = contents.replace(&host_qdbus, "/usr/bin/qdbus");
                if normalized != contents {
                    fs::write(path, normalized)?;
                }
            }
        }
    }
    // The calendar-event plugin is optional Plasma integration. It requires
    // the separately maintained KCalendarCore/libical stack, which is not in
    // the current shell closure; omit the complete optional family rather
    // than shipping an unusable ELF with an absent dependency.
    if matches!(component, "plasma-workspace" | "plasma-desktop") {
        remove_path_if_exists(
            &staging.join("usr/lib/x86_64-linux-gnu/plugins/plasmacalendarplugins"),
        )?;
    }
    if component == "breeze" {
        // The optional Breeze settings helper is a KDE Control Module host;
        // it is not part of the Wayland shell and would introduce a runtime
        // dependency on the not-yet-packaged KCMUtils closure.  Keep the
        // style/window-decoration payload while leaving that unrelated
        // utility out of the first-class Breeze runtime package.
        let settings = staging.join("usr/bin/breeze-settings6");
        if settings.exists() {
            fs::remove_file(settings)?;
        }
    }
    let source = match component {
        "kcoreaddons"
        | "ki18n"
        | "kwidgetsaddons"
        | "kconfig"
        | "kdbusaddons"
        | "kauth"
        | "layer-shell-qt"
        | "plasma-framework"
        | "krunner"
        | "kcrash"
        | "kwindowsystem"
        | "kpackage"
        | "karchive"
        | "kio"
        | "ksvg"
        | "knotifications"
        | "kguiaddons"
        | "kitemmodels"
        | "kglobalaccel"
        | "kiconthemes"
        | "kcolorscheme"
        | "kcompletion"
        | "kjobwidgets"
        | "kservice"
        | "solid"
        | "kcodecs"
        | "kdecoration"
        | "kidletime"
        | "kwayland"
        | "knighttime"
        | "kholidays"
        | "kstatusnotifieritem"
        | "kxmlgui"
        | "kconfigwidgets"
        | "kitemviews"
        | "kbookmarks"
        | "kirigami"
        | "breeze-icons"
        | "plasma-activities"
        | "kwin"
        | "plasma-workspace"
        | "plasma-desktop"
        | "breeze"
        | "plasma-activities-stats"
        | "kcmutils"
        | "ksysguard"
        | "knewstuff"
        | "attica"
        | "sonnet"
        | "kglobalacceld"
        | "kirigami-addons"
        | "kquickcharts" => repo_root.join("src/desktop/kde").join(component),
        "polkit-qt-1" => repo_root.join("src/system/security/polkit-qt-1"),
        "qca" => repo_root.join("src/system/security/qca"),
        "yaml-cpp" => repo_root.join("src/system/libraries/yaml-cpp"),
        "kpmcore" => repo_root.join("src/system/storage/kpmcore"),
        component if repo_root.join("src/desktop/kde").join(component).is_dir() => {
            // Keep the KDE source-root ownership rule generic for every
            // first-class KDE package.  The package registry still controls
            // which components can reach this helper; this guard only avoids
            // a second, drift-prone list of source directories.
            repo_root.join("src/desktop/kde").join(component)
        }
        _ => bail!("unknown KDE module source component {component}"),
    };
    let license = [
        "LICENSES",
        "LICENSE",
        "COPYING",
        "COPYING-ICONS",
        "COPYING.LIB",
    ]
    .iter()
    .map(|name| source.join(name))
    .find(|path| path.exists())
    // yaml-cpp's imported tree retains README.md while its ignored aggregate
    // LICENSE is omitted; use that retained upstream notice for the package.
    .or_else(|| {
        (component == "yaml-cpp")
            .then(|| source.join("README.md"))
            .filter(|path| path.exists())
    })
    .ok_or_else(|| anyhow!("KDE package {package} lacks a retained upstream license"))?;
    let destination = staging.join("usr/share/doc").join(package);
    if license.is_dir() {
        copy_tree_preserving(&license, &destination.join("licenses"))?;
    } else {
        copy_preserving(&license, &destination.join("copyright"))?;
    }
    Ok(())
}

pub(super) fn stage_calamares(repo_root: &Path, staging: &Path) -> Result<()> {
    let install = component_install(repo_root, "calamares").join("usr");
    if !install.join("bin/calamares").is_file() {
        bail!("Calamares package requires a completed calamares stage");
    }
    copy_tree_preserving(&install, &staging.join("usr"))?;
    // Upstream ships a generic desktop entry which runs `pkexec calamares`
    // directly.  That bypasses the MattOS launcher environment and creates a
    // second, unreliable installer entry beside the MattOS one.  Calamares
    // remains source-owned here; only its generic launcher is replaced with
    // the single MattOS-owned entry below.
    remove_path_if_exists(&staging.join("usr/share/applications/calamares.desktop"))?;
    let integration = repo_root.join("src/system/installer/calamares/mattos");
    // Calamares' normal, upstream configuration contract is
    // /etc/calamares.  Keeping the MattOS configuration there means the
    // packaged launcher does not depend on the debug-only/optional XDG
    // search mode (-X), and avoids having two configuration authorities.
    let config_root = staging.join("etc/calamares");
    copy_tree_preserving(&integration.join("branding"), &config_root.join("branding"))?;
    copy_preserving(
        &integration.join("settings.conf"),
        &config_root.join("settings.conf"),
    )?;
    copy_preserving(
        &integration.join("modules/partition.conf"),
        &config_root.join("modules/partition.conf"),
    )?;
    for module in ["mount", "fstab", "users", "locale", "keyboard"] {
        copy_preserving(
            &repo_root
                .join("src/system/installer/calamares/upstream/src/modules")
                .join(module)
                .join(format!("{module}.conf")),
            &config_root.join("modules").join(format!("{module}.conf")),
        )?;
    }
    copy_preserving(
        &integration.join("modules/users.conf"),
        &config_root.join("modules/users.conf"),
    )?;
    copy_preserving(
        &integration.join("modules/mattos-profile.conf"),
        &config_root.join("modules/mattos-profile.conf"),
    )?;
    copy_preserving(
        &integration.join("modules/mattos-finalize.conf"),
        &config_root.join("modules/mattos-finalize.conf"),
    )?;
    // Use the upstream shellprocess plugin as the narrow MattOS adapter
    // boundary.  The adapter is deliberately a separate executable so the
    // graphical frontend cannot silently invent a package list or bypass the
    // Rust installer policy.
    fs::write(
        config_root.join("modules/mattos-executor.conf"),
        "dontChroot: true\ntimeout: 3600\nverbose: true\nscript: /usr/libexec/mattos/calamares-target-executor --phase compose --profile ${gs[packagechooser_profile]} --target ${ROOT}\n",
    )?;
    let executor = staging.join("usr/libexec/mattos/calamares-target-executor");
    fs::create_dir_all(executor.parent().expect("executor parent"))?;
    fs::write(
        &executor,
        r#"#!/bin/sh
set -eu
if [ $# -ne 6 ] || [ "$1" != "--phase" ] || [ "$3" != "--profile" ] || [ "$5" != "--target" ]; then
  echo 'MattOS Calamares adapter: missing validated phase/profile/target from Calamares' >&2
  exit 64
fi
exec /usr/bin/mattos-install calamares --phase "$2" --profile "$4" --target "$6" 2>&1
"#,
    )?;
    set_mode(executor, 0o755)?;
    let launcher = staging.join("usr/bin/mattos-install-gui");
    fs::create_dir_all(launcher.parent().expect("launcher parent"))?;
    // Generate one canonical launcher payload after the policy/environment setup.
    // Keep the generated shell script free of Rust escape/continuation
    // artifacts: the final write is the canonical launcher payload.
    fs::write(
        &launcher,
        r#"#!/bin/sh
set -eu
export XDG_CONFIG_DIRS=/etc/xdg${XDG_CONFIG_DIRS:+:$XDG_CONFIG_DIRS}
export XDG_DATA_DIRS=/usr/share${XDG_DATA_DIRS:+:$XDG_DATA_DIRS}
export QT_PLUGIN_PATH=/usr/lib/x86_64-linux-gnu/plugins:/usr/plugins${QT_PLUGIN_PATH:+:$QT_PLUGIN_PATH}
export WAYLAND_DISPLAY="${WAYLAND_DISPLAY:-wayland-0}"
export QT_QPA_PLATFORM="${QT_QPA_PLATFORM:-wayland}"
# The live profile grants the logged-in installer user passwordless sudo.
# Use it for the GUI itself so the desktop launcher cannot strand Calamares
# behind an invisible/terminal-only Polkit authentication request.  Calamares
# still performs its privileged work through KPMCore's normal helpers.
exec /usr/bin/sudo -n /usr/bin/env \
XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-}" \
WAYLAND_DISPLAY="$WAYLAND_DISPLAY" \
DBUS_SESSION_BUS_ADDRESS="${DBUS_SESSION_BUS_ADDRESS:-}" \
XDG_CONFIG_DIRS="$XDG_CONFIG_DIRS" \
XDG_DATA_DIRS="$XDG_DATA_DIRS" \
QT_PLUGIN_PATH="$QT_PLUGIN_PATH" \
QT_QPA_PLATFORM="$QT_QPA_PLATFORM" \
/usr/bin/calamares "$@"
"#,
    )?;
    set_mode(launcher, 0o755)?;
    let desktop = staging.join("usr/share/applications/calamares.desktop");
    fs::create_dir_all(desktop.parent().expect("desktop parent"))?;
    fs::write(
        desktop,
        "[Desktop Entry]\nType=Application\nVersion=1.0\nName=Install MattOS (Calamares)\nGenericName=MattOS Graphical Installer\nKeywords=calamares;MattOS;system;installer;\nTryExec=/usr/bin/mattos-install-gui\nExec=/usr/bin/mattos-install-gui\nComment=Install MattOS with the graphical Calamares installer\nIcon=calamares\nTerminal=false\nStartupNotify=true\nCategories=Qt;System;Settings;\nX-AppStream-Ignore=true\n",
    )?;
    for required in [
        "usr/bin/calamares",
        "usr/bin/mattos-install-gui",
        "usr/libexec/mattos/calamares-target-executor",
        "etc/calamares/settings.conf",
        "etc/calamares/modules/partition.conf",
        "etc/calamares/modules/mount.conf",
        "etc/calamares/modules/fstab.conf",
        "etc/calamares/modules/users.conf",
        "etc/calamares/modules/locale.conf",
        "etc/calamares/modules/keyboard.conf",
        "etc/calamares/modules/mattos-executor.conf",
        "etc/calamares/modules/mattos-finalize.conf",
        "etc/calamares/modules/mattos-profile.conf",
        "etc/calamares/branding/mattos/branding.desc",
        "usr/share/applications/calamares.desktop",
    ] {
        if !staging.join(required).exists() {
            bail!("Calamares package is missing /{required}");
        }
    }
    Ok(())
}

pub(crate) fn stage_flatpak(repo_root: &Path, staging: &Path) -> Result<()> {
    // Flatpak is shipped as one first-class MattOS package. Until the support
    // libraries below receive their own package boundaries, this package owns
    // their target-built runtime files as an explicit, closed payload. This
    // is deliberately not a host fallback: every copied tree comes from a
    // declared MattOS build stage and is covered by the package cache input.
    // Seed only the configured remote. Optional applications are installed
    // into the target later by the installer and are never package payload.
    stage_flatpak_system_remote(
        &repo_root.join("src/system/packages/config/flatpak/flathub.flatpakrepo"),
        staging,
    )?;
    // The libraries and helpers Flatpak runs on are their own packages
    // (`FLATPAK_CLOSURE_PACKAGES`); this package carries Flatpak itself.
    let install = component_install(repo_root, "flatpak");
    for top_level in ["usr", "etc", "var"] {
        let source = install.join(top_level);
        if source.is_dir() {
            copy_tree_preserving(&source, &staging.join(top_level))?;
        }
    }
    // Libtool archives are build-time link metadata, not part of Flatpak's
    // runtime ABI.  The source stages retain them only where a later target
    // build needs them; do not copy their disposable absolute build paths
    // into the runtime Flatpak package.
    remove_staged_libtool_archives_from_package(&staging)?;
    copy_preserving(
        &repo_root.join("out/build/flatpak/install/usr/libexec/mattos-flatpak-target-install"),
        &staging.join("usr/libexec/mattos-flatpak-target-install"),
    )?;
    copy_preserving(
        &repo_root.join("src/system/packages/flatpak/COPYING"),
        &staging.join("usr/share/doc/flatpak/copyright"),
    )?;
    // MattOS ships Flathub as a distro-owned, signed system remote. Flatpak
    // imports this descriptor for both system and user installations without
    // any first-run shell setup or application-specific override policy.
    copy_preserving(
        &repo_root.join("src/system/packages/config/flatpak/flathub.flatpakrepo"),
        &staging.join("usr/share/flatpak/remotes.d/flathub.flatpakrepo"),
    )?;
    let resources = repo_root.join("src/system/packages/config/flatpak");
    for unit in [
        "mattos-flatpak-system-update.service",
        "mattos-flatpak-system-update.timer",
        "mattos-flatpak-user-update.service",
        "mattos-flatpak-user-update.timer",
    ] {
        let destination = if unit.contains("user") {
            staging.join("usr/lib/systemd/user").join(unit)
        } else {
            staging.join("usr/lib/systemd/system").join(unit)
        };
        copy_preserving(&resources.join(unit), &destination)?;
    }
    for (target, link) in [
        (
            "usr/lib/systemd/system/mattos-flatpak-system-update.timer",
            "etc/systemd/system/timers.target.wants/mattos-flatpak-system-update.timer",
        ),
        (
            "usr/lib/systemd/user/mattos-flatpak-user-update.timer",
            "etc/systemd/user/timers.target.wants/mattos-flatpak-user-update.timer",
        ),
    ] {
        let link = staging.join(link);
        if let Some(parent) = link.parent() {
            fs::create_dir_all(parent)?;
        }
        if link.exists() || link.symlink_metadata().is_ok() {
            fs::remove_file(&link)?;
        }
        std::os::unix::fs::symlink(format!("/{target}"), link)?;
    }
    Ok(())
}

/// The packages of Flatpak's runtime closure, named as in Debian: each is
/// (package, source component, paths from the component's install tree, license).
/// A path ending in `*` names every entry of its directory with that prefix
/// (a library's SONAME link and its versioned file).  Headers, pkg-config
/// files and other development files are not shipped.
const FLATPAK_CLOSURE_PACKAGES: &[(&str, &str, &[&str], &str)] = &[
    ("libostree-1-1", "ostree", &["usr/lib/x86_64-linux-gnu/libostree-1.so.1*", "usr/share/ostree"], "src/system/packages/ostree/COPYING"),
    ("ostree", "ostree", &["usr/bin/ostree", "usr/share/bash-completion/completions/ostree"], "src/system/packages/ostree/COPYING"),
    ("libgpgme45", "gpgme", &["usr/lib/x86_64-linux-gnu/libgpgme.so.45*"], "src/system/security/gpgme/COPYING.LESSER"),
    ("libgdk-pixbuf-2.0-0", "gdk-pixbuf", &["usr/lib/x86_64-linux-gnu/libgdk_pixbuf-2.0.so.0*", "usr/lib/x86_64-linux-gnu/gdk-pixbuf-2.0", "usr/share/locale"], "src/system/libraries/gdk-pixbuf/COPYING"),
    ("libgdk-pixbuf2.0-bin", "gdk-pixbuf", &["usr/bin/gdk-pixbuf-csource", "usr/bin/gdk-pixbuf-pixdata", "usr/bin/gdk-pixbuf-query-loaders", "usr/bin/gdk-pixbuf-thumbnailer", "usr/share/thumbnailers"], "src/system/libraries/gdk-pixbuf/COPYING"),
    ("libappstream5", "appstream", &["usr/lib/x86_64-linux-gnu/libappstream.so.5", "usr/lib/x86_64-linux-gnu/libappstream.so.1*", "usr/share/appstream", "usr/share/locale"], "src/system/libraries/appstream/COPYING"),
    ("libappstreamqt3", "appstream", &["usr/lib/x86_64-linux-gnu/libAppStreamQt.so.*"], "src/system/libraries/appstream/COPYING"),
    ("appstream", "appstream", &["usr/bin/appstreamcli", "usr/share/metainfo", "usr/share/gettext"], "src/system/libraries/appstream/COPYING"),
    ("libjson-glib-1.0-0", "json-glib", &["usr/lib/x86_64-linux-gnu/libjson-glib-1.0.so.0*", "usr/share/locale"], "src/system/libraries/json-glib/COPYING"),
    ("libxmlb2", "libxmlb", &["usr/lib/x86_64-linux-gnu/libxmlb.so.2*"], "src/system/libraries/libxmlb/LICENSE"),
    ("libfyaml0", "libfyaml", &["usr/lib/x86_64-linux-gnu/libfyaml.so.0*"], "src/system/libraries/libfyaml/LICENSE"),
    ("libfuse3-4", "fuse3", &["usr/lib/x86_64-linux-gnu/libfuse3.so.4", "usr/lib/x86_64-linux-gnu/libfuse3.so.3*"], "src/system/libraries/fuse3/LICENSE"),
    ("fuse3", "fuse3", &["usr/bin/fusermount3", "usr/sbin/mount.fuse3", "usr/lib/udev/rules.d/99-fuse3.rules", "usr/share/man/man1/fusermount3.1", "usr/share/man/man8/mount.fuse3.8", "etc/fuse.conf"], "src/system/libraries/fuse3/LICENSE"),
    ("bubblewrap", "bubblewrap", &["usr/bin/bwrap"], "src/system/security/bubblewrap/COPYING"),
    ("xdg-dbus-proxy", "xdg-dbus-proxy", &["usr/bin/xdg-dbus-proxy"], "src/system/packages/xdg-dbus-proxy/COPYING"),
];

/// gdk-pixbuf finds its loader modules (GIF here; PNG is built in) only
/// through `loaders.cache`, which its install step does not write under
/// DESTDIR.  Generate it for the staged modules and ship it: every MattOS
/// loader comes from gdk-pixbuf itself.  A package adding loaders would need
/// Debian's trigger-driven regeneration instead.
pub(crate) fn write_pixbuf_loader_cache(
    staging: &Path,
    query: impl Fn(&[PathBuf]) -> Result<String>,
) -> Result<()> {
    let loaders = staging.join("usr/lib/x86_64-linux-gnu/gdk-pixbuf-2.0/2.10.0/loaders");
    let mut modules = fs::read_dir(&loaders)
        .with_context(|| format!("libgdk-pixbuf-2.0-0 has no loader directory {}", loaders.display()))?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    modules.retain(|path| path.extension().and_then(OsStr::to_str) == Some("so"));
    modules.sort();
    if modules.is_empty() {
        bail!("libgdk-pixbuf-2.0-0 stages no loader modules in {}", loaders.display());
    }
    let staged = staging.to_str().context("staging path is not UTF-8")?;
    // The query names each module by the path it was given; record the
    // installed path instead of the staging one.
    let cache = query(&modules)?.replace(staged, "");
    for module in &modules {
        let installed = format!("\"/{}\"", module.strip_prefix(staging)?.display());
        if !cache.contains(&installed) {
            bail!("gdk-pixbuf-query-loaders did not describe {installed}");
        }
    }
    fs::write(loaders.parent().expect("loader directory has a parent").join("loaders.cache"), cache)?;
    Ok(())
}

/// Run the build's gdk-pixbuf-query-loaders through the MattOS loader.
fn query_pixbuf_loaders(repo_root: &Path, modules: &[PathBuf]) -> Result<String> {
    let glibc = component_install(repo_root, "glibc");
    let mut library_path = vec![glibc.join("usr/lib/x86_64-linux-gnu"), glibc.join("lib64")];
    for component in ["glib", "libffi", "pcre2", "zlib", "libpng", "gdk-pixbuf"] {
        library_path.push(component_install(repo_root, component).join("usr/lib/x86_64-linux-gnu"));
    }
    let output = std::process::Command::new(glibc.join("lib64/ld-linux-x86-64.so.2"))
        .arg("--library-path")
        .arg(std::env::join_paths(library_path)?)
        .arg(component_install(repo_root, "gdk-pixbuf").join("usr/bin/gdk-pixbuf-query-loaders"))
        .args(modules)
        .output()
        .context("failed to run gdk-pixbuf-query-loaders")?;
    if !output.status.success() {
        bail!(
            "gdk-pixbuf-query-loaders failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(String::from_utf8(output.stdout)?)
}

pub(crate) fn stage_flatpak_closure_package(repo_root: &Path, staging: &Path, name: &str) -> Result<()> {
    let (_, component, paths, license) = FLATPAK_CLOSURE_PACKAGES
        .iter()
        .find(|(package, ..)| *package == name)
        .ok_or_else(|| anyhow!("{name} is not a Flatpak closure package"))?;
    let install = component_install(repo_root, component);
    for path in *paths {
        let sources = match path.strip_suffix('*') {
            Some(prefix) => {
                let relative = Path::new(prefix);
                let directory = install.join(relative.parent().expect("pattern has a directory"));
                let stem = relative.file_name().and_then(OsStr::to_str).expect("pattern has a name");
                let mut matches = fs::read_dir(&directory)
                    .with_context(|| format!("{name}: cannot list {}", directory.display()))?
                    .map(|entry| entry.map(|entry| entry.path()))
                    .collect::<std::io::Result<Vec<_>>>()?;
                matches.retain(|path| {
                    path.file_name().and_then(OsStr::to_str).is_some_and(|file| file.starts_with(stem))
                });
                matches.sort();
                matches
            }
            None => vec![install.join(path)],
        };
        if sources.is_empty() {
            bail!("{name}: nothing in the {component} install matches {path}");
        }
        for source in sources {
            let relative = source.strip_prefix(&install).expect("source is in the install tree");
            if fs::symlink_metadata(&source).is_err() {
                bail!("{name}: the {component} install lacks /{}", relative.display());
            }
            if source.is_dir() && !source.is_symlink() {
                copy_tree_preserving(&source, &staging.join(relative))?;
            } else {
                copy_path_preserving(&source, &staging.join(relative))?;
            }
        }
    }
    // Every link must land inside the package: a SONAME link whose target
    // file was not selected would install as a dangling link.
    walk_tree(staging, &mut |path, metadata| {
        if metadata.file_type().is_symlink() {
            let target = fs::read_link(path)?;
            let resolved = if target.is_absolute() {
                staging.join(target.strip_prefix("/").expect("absolute path"))
            } else {
                path.parent().expect("link has a parent").join(&target)
            };
            if fs::metadata(&resolved).is_err() {
                bail!(
                    "{name}: {} links to {}, which the package does not contain",
                    path.strip_prefix(staging).unwrap_or(path).display(),
                    target.display()
                );
            }
        }
        Ok(())
    })?;
    copy_preserving(&repo_root.join(license), &staging.join("usr/share/doc").join(name).join("copyright"))?;
    if name == "libgdk-pixbuf-2.0-0" {
        write_pixbuf_loader_cache(staging, |modules| query_pixbuf_loaders(repo_root, modules))?;
    }
    if name == "fuse3" {
        // The document portal mounts its per-user document filesystem through
        // libfuse's privileged helper. The source build deliberately avoids
        // setting ownership bits (it runs unprivileged), so establish the
        // package's documented root-owned setuid contract at package staging.
        set_mode(staging.join("usr/bin/fusermount3"), 0o4755)?;
        fs::create_dir_all(staging.join("DEBIAN"))?;
        fs::write(staging.join("DEBIAN/conffiles"), "/etc/fuse.conf\n")?;
    }
    Ok(())
}

pub(super) fn remove_staged_libtool_archives_from_package(staging: &Path) -> Result<()> {
    let mut archives = Vec::new();
    walk_tree(staging, &mut |path, metadata| {
        if metadata.is_file() && path.extension().and_then(OsStr::to_str) == Some("la") {
            archives.push(path.to_owned());
        }
        Ok(())
    })?;
    for archive in archives {
        fs::remove_file(archive)?;
    }
    Ok(())
}

/// Seed Flatpak's default system installation from MattOS's signed static
/// policy. Flatpak normally imports descriptors in `remotes.d` lazily when a
/// writable OSTree repository is first opened. A fresh live system has no such
/// repository yet, so a non-root `flatpak remotes` would otherwise show
/// nothing even though the descriptor is present. This is the same normal
/// system-remote configuration Flatpak writes when it imports a descriptor,
/// materialized deterministically while composing the MattOS package.
///
/// The repository contains no objects or refs: only the required OSTree
/// layout, remote configuration, and public verification key derived from the
/// packaged descriptor. Application and runtime content remains user data.
pub(crate) fn stage_flatpak_system_remote(descriptor: &Path, staging: &Path) -> Result<()> {
    let policy = fs::read_to_string(descriptor).with_context(|| {
        format!(
            "failed to read Flatpak remote policy {}",
            descriptor.display()
        )
    })?;
    let value = |key: &str| -> Result<String> {
        policy
            .lines()
            .find_map(|line| line.strip_prefix(&format!("{key}=")))
            .map(str::to_owned)
            .with_context(|| format!("Flatpak remote policy is missing {key}"))
    };
    let url = value("Url")?;
    let title = value("Title")?;
    let homepage = value("Homepage")?;
    let comment = value("Comment")?;
    let description = value("Description")?;
    let icon = value("Icon")?;
    let gpg_key = decode_flatpak_base64(&value("GPGKey")?)?;
    if gpg_key.len() < 10 {
        bail!("Flatpak remote policy contains an invalid short GPG key");
    }

    let repo = staging.join("var/lib/flatpak/repo");
    // libostree requires the remote-ref namespace to exist even before the
    // first pull.  `flatpak remote-add` creates it as part of repository
    // initialization; omit it and a later install fails opening
    // `refs/remotes` after downloading content.
    for directory in [
        "objects",
        "refs",
        "refs/heads",
        "refs/remotes",
        "state",
        "tmp",
        "extensions",
    ] {
        fs::create_dir_all(repo.join(directory))?;
    }
    fs::write(
        repo.join("config"),
        format!(
            "[core]\nrepo_version=1\nmode=bare-user-only\nmin-free-space-size=500MB\nxa.applied-remotes=flathub;\n\n[remote \"flathub\"]\nurl={url}\nxa.title={title}\ngpg-verify=true\ngpg-verify-summary=true\nxa.comment={comment}\nxa.description={description}\nxa.icon={icon}\nxa.homepage={homepage}\n"
        ),
    )?;
    fs::write(repo.join("flathub.trustedkeys.gpg"), gpg_key)?;
    Ok(())
}

/// Decode the standard base64 GPG payload in a `.flatpakrepo` without adding a
/// host tool or an unrelated runtime dependency to package composition.
pub(super) fn decode_flatpak_base64(input: &str) -> Result<Vec<u8>> {
    let mut output = Vec::with_capacity(input.len() * 3 / 4);
    let mut chunk = [0u8; 4];
    let mut chunk_len = 0;
    for byte in input.bytes().filter(|byte| !byte.is_ascii_whitespace()) {
        chunk[chunk_len] = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => 64,
            _ => bail!("invalid base64 byte {byte:?} in Flatpak remote policy"),
        };
        chunk_len += 1;
        if chunk_len != 4 {
            continue;
        }
        if chunk[0] == 64 || chunk[1] == 64 || (chunk[2] == 64 && chunk[3] != 64) {
            bail!("invalid base64 padding in Flatpak remote policy");
        }
        output.push((chunk[0] << 2) | (chunk[1] >> 4));
        if chunk[2] != 64 {
            output.push((chunk[1] << 4) | (chunk[2] >> 2));
            if chunk[3] != 64 {
                output.push((chunk[2] << 6) | chunk[3]);
            }
        }
        chunk_len = 0;
    }
    if chunk_len != 0 {
        bail!("incomplete base64 payload in Flatpak remote policy");
    }
    Ok(output)
}

pub(super) fn stage_xwayland(repo_root: &Path, staging: &Path) -> Result<()> {
    // The Xwayland package owns every auxiliary library built solely for the
    // server. Each tree is a declared MattOS stage, never a host fallback.
    for component in [
        "libepoxy",
        "libfontenc",
        "libxfont",
        "libxcvt",
        "libxshmfence",
        "xkbcomp",
        "xwayland",
    ] {
        copy_component_usr_and_etc(repo_root, staging, component)?;
    }
    Ok(())
}

pub(super) fn stage_fontconfig(repo_root: &Path, staging: &Path) -> Result<()> {
    stage_runtime_paths(
        repo_root,
        staging,
        "fontconfig",
        &[
            "usr/bin/fc-cache",
            "usr/bin/fc-cat",
            "usr/bin/fc-conflist",
            "usr/bin/fc-list",
            "usr/bin/fc-match",
            "usr/bin/fc-pattern",
            "usr/bin/fc-query",
            "usr/bin/fc-scan",
            "usr/bin/fc-validate",
        ],
    )?;
    copy_preserving(
        &repo_root.join("src/system/libraries/fontconfig/COPYING"),
        &staging.join("usr/share/doc/fontconfig/copyright"),
    )?;
    if !staging.join("usr/bin/fc-match").exists() {
        bail!("fontconfig package missing /usr/bin/fc-match");
    }
    Ok(())
}

/// Rebuilds the MIME database whenever a package adds or removes MIME
/// definitions.  Offline composition (the image build and the installer,
/// which set DPKG_ROOT) builds it once after every package is unpacked.
pub(crate) const SHARED_MIME_INFO_POSTINST: &str = "#!/bin/sh\nset -e\n[ -n \"${DPKG_ROOT:-}\" ] && exit 0\ncase \"$1\" in\n    configure|triggered) update-mime-database /usr/share/mime ;;\nesac\n";

/// The generated database is not package payload: purge removes it, leaving
/// only `packages/`, whose definitions belong to their packages.
pub(crate) const SHARED_MIME_INFO_POSTRM: &str = "#!/bin/sh\nset -e\nif [ \"$1\" = purge ]; then\n    mime=\"${DPKG_ROOT:-}/usr/share/mime\"\n    if [ -d \"$mime\" ]; then\n        find \"$mime\" -mindepth 1 -maxdepth 1 ! -name packages -exec rm -rf {} +\n    fi\nfi\n";

pub(crate) fn stage_shared_mime_info(repo_root: &Path, staging: &Path) -> Result<()> {
    let install = component_install(repo_root, "shared-mime-info");
    for relative in [
        "usr/bin/update-mime-database",
        "usr/share/mime/packages/freedesktop.org.xml",
        "usr/share/man/man1/update-mime-database.1",
        "usr/share/gettext/its",
        "usr/share/pkgconfig/shared-mime-info.pc",
        "usr/share/locale",
    ] {
        let source = install.join(relative);
        if source.is_dir() {
            copy_tree_preserving(&source, &staging.join(relative))?;
        } else {
            copy_path_preserving(&source, &staging.join(relative))?;
        }
    }
    copy_preserving(
        &repo_root.join("src/system/libraries/shared-mime-info/COPYING"),
        &staging.join("usr/share/doc/shared-mime-info/copyright"),
    )?;
    let debian = staging.join("DEBIAN");
    fs::create_dir_all(&debian)?;
    fs::write(debian.join("triggers"), "interest-noawait /usr/share/mime/packages\n")?;
    fs::write(debian.join("postinst"), SHARED_MIME_INFO_POSTINST)?;
    set_mode(debian.join("postinst"), 0o755)?;
    fs::write(debian.join("postrm"), SHARED_MIME_INFO_POSTRM)?;
    set_mode(debian.join("postrm"), 0o755)?;
    Ok(())
}

/// The configuration libfontconfig reads (`/etc/fonts`), the available
/// configuration snippets and the DTD, as Debian's fontconfig-config.
pub(super) fn stage_fontconfig_config(repo_root: &Path, staging: &Path) -> Result<()> {
    let install = component_install(repo_root, "fontconfig");
    for relative in [
        "etc/fonts",
        "usr/share/fontconfig",
        "usr/share/xml/fontconfig",
    ] {
        copy_tree_preserving(&install.join(relative), &staging.join(relative))?;
    }
    copy_preserving(
        &repo_root.join("src/system/libraries/fontconfig/COPYING"),
        &staging.join("usr/share/doc/fontconfig-config/copyright"),
    )?;
    for required in ["etc/fonts/fonts.conf", "usr/share/fontconfig/conf.avail"] {
        if !staging.join(required).exists() {
            bail!("fontconfig-config package missing /{required}");
        }
    }
    Ok(())
}

pub(super) fn stage_pop_fonts(repo_root: &Path, staging: &Path) -> Result<()> {
    let install = component_install(repo_root, "pop-fonts");
    copy_tree_preserving(&install.join("usr"), &staging.join("usr"))?;
    for font in POP_FIRA_RUNTIME_FONTS {
        let path = staging.join("usr/share/fonts/opentype/fira").join(font);
        if !path.is_file() {
            bail!("fonts-fira package missing /usr/share/fonts/opentype/fira/{font}");
        }
    }
    if !staging.join("usr/share/doc/fonts-fira/copyright").is_file() {
        bail!("fonts-fira package missing its source-owned license");
    }
    Ok(())
}

pub(crate) fn stage_xdg_desktop_portal(repo_root: &Path, staging: &Path) -> Result<()> {
    // The generic broker and its GStreamer pbutils closure ship together. The
    // portal executes Bubblewrap at /usr/bin/bwrap, but Flatpak owns that
    // target-built executable and is the portal package's declared runtime
    // dependency. Copying it here would create two Debian package owners for
    // the same path. Desktop-specific portal backends are packaged by their
    // own first-class stages.
    for component in ["gstreamer", "gstreamer-base", "xdg-desktop-portal"] {
        copy_component_usr_and_etc(repo_root, staging, component)?;
    }
    Ok(())
}

pub(super) fn stage_amdgpu_ids(install: &Path, staging: &Path) -> Result<()> {
    let relative = "usr/share/libdrm/amdgpu.ids";
    let source = install.join(relative);
    if fs::metadata(&source)?.len() == 0 {
        bail!("libdrm AMD device database is empty: {}", source.display());
    }
    copy_preserving(&source, &staging.join(relative))
}

#[cfg(test)]
mod desktop_data_tests {
    use super::*;
    use std::os::unix::fs::symlink;

    #[test]
    fn fontconfig_staging_preserves_font_discovery_payload() {
        let repo = tempfile::tempdir().unwrap();
        let install = repo.path().join("out/build/fontconfig/install");
        for relative in [
            "usr/bin/fc-cache",
            "usr/bin/fc-cat",
            "usr/bin/fc-conflist",
            "usr/bin/fc-list",
            "usr/bin/fc-match",
            "usr/bin/fc-pattern",
            "usr/bin/fc-query",
            "usr/bin/fc-scan",
            "usr/bin/fc-validate",
            "etc/fonts/fonts.conf",
            "usr/share/fontconfig/conf.avail/45-generic.conf",
            "usr/share/xml/fontconfig/fonts.dtd",
        ] {
            let path = install.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, relative.as_bytes()).unwrap();
        }
        fs::create_dir_all(install.join("etc/fonts/conf.d")).unwrap();
        symlink(
            "../../../usr/share/fontconfig/conf.avail/45-generic.conf",
            install.join("etc/fonts/conf.d/45-generic.conf"),
        )
        .unwrap();
        let license = repo.path().join("src/system/libraries/fontconfig/COPYING");
        fs::create_dir_all(license.parent().unwrap()).unwrap();
        fs::write(&license, b"fontconfig license").unwrap();

        // The utilities and the configuration are separate packages.
        let tools = repo.path().join("tools");
        fs::create_dir_all(&tools).unwrap();
        stage_fontconfig(repo.path(), &tools).unwrap();
        assert!(tools.join("usr/bin/fc-match").is_file());
        assert!(!tools.join("etc").exists());
        assert!(!tools.join("usr/share/fontconfig").exists());

        let staging = repo.path().join("staged");
        fs::create_dir_all(&staging).unwrap();
        stage_fontconfig_config(repo.path(), &staging).unwrap();
        assert!(!staging.join("usr/bin").exists());

        assert_eq!(
            fs::read(staging.join("etc/fonts/fonts.conf")).unwrap(),
            b"etc/fonts/fonts.conf"
        );
        assert!(
            staging
                .join("usr/share/fontconfig/conf.avail/45-generic.conf")
                .is_file()
        );
        assert!(staging.join("usr/share/xml/fontconfig/fonts.dtd").is_file());
        assert!(
            staging
                .join("etc/fonts/conf.d/45-generic.conf")
                .is_symlink()
        );
        assert_eq!(
            fs::read(staging.join("usr/share/doc/fontconfig-config/copyright")).unwrap(),
            b"fontconfig license"
        );
        assert_eq!(
            fs::read(tools.join("usr/share/doc/fontconfig/copyright")).unwrap(),
            b"fontconfig license"
        );
    }

    #[test]
    fn amd_database_is_required_and_staged_byte_for_byte() {
        let source = tempfile::tempdir().unwrap();
        let target = tempfile::tempdir().unwrap();
        assert!(stage_amdgpu_ids(source.path(), target.path()).is_err());
        let relative = "usr/share/libdrm/amdgpu.ids";
        let file = source.path().join(relative);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, "").unwrap();
        assert!(stage_amdgpu_ids(source.path(), target.path()).is_err());
        fs::write(&file, "# AMD device names\n164E, C1, AMD Radeon Graphics\n").unwrap();
        stage_amdgpu_ids(source.path(), target.path()).unwrap();
        assert_eq!(
            fs::read(file).unwrap(),
            fs::read(target.path().join(relative)).unwrap()
        );
    }

    #[test]
    fn breeze_staging_preserves_standard_theme_and_icon_lookup_paths() {
        let fixture = tempfile::tempdir().unwrap();
        let install = fixture.path().join("out/build/breeze-icons/install/usr");
        for relative in [
            "share/icons/breeze/index.theme",
            "share/icons/breeze-dark/index.theme",
            "share/icons/breeze/actions/22/go-next.svg",
        ] {
            let path = install.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, relative.as_bytes()).unwrap();
        }
        let license = fixture.path().join("src/desktop/kde/breeze-icons/COPYING");
        fs::create_dir_all(license.parent().unwrap()).unwrap();
        fs::write(&license, b"breeze license").unwrap();

        let staging = fixture.path().join("staged");
        stage_kde_module(fixture.path(), &staging, "breeze-icons", "breeze-icons").unwrap();

        for relative in [
            "usr/share/icons/breeze/index.theme",
            "usr/share/icons/breeze-dark/index.theme",
            "usr/share/icons/breeze/actions/22/go-next.svg",
        ] {
            assert!(
                staging.join(relative).is_file(),
                "missing staged /{relative}"
            );
        }
        assert_eq!(
            fs::read(staging.join("usr/share/doc/breeze-icons/copyright")).unwrap(),
            b"breeze license"
        );
    }

    #[test]
    fn plasma_workspace_staging_preserves_application_menu_contract() {
        let fixture = tempfile::tempdir().unwrap();
        let install_root = fixture.path().join("out/build/plasma-workspace/install");
        let executable = install_root.join("usr/bin/plasmashell");
        fs::create_dir_all(executable.parent().unwrap()).unwrap();
        fs::write(&executable, b"plasmashell").unwrap();
        let menu = install_root.join("etc/xdg/menus/plasma-applications.menu");
        fs::create_dir_all(menu.parent().unwrap()).unwrap();
        fs::write(&menu, b"<Menu><Name>Applications</Name></Menu>\n").unwrap();
        let license = fixture
            .path()
            .join("src/desktop/kde/plasma-workspace/LICENSES/GPL-2.0-only.txt");
        fs::create_dir_all(license.parent().unwrap()).unwrap();
        fs::write(&license, b"license").unwrap();

        let staging = fixture.path().join("staged");
        stage_kde_module(
            fixture.path(),
            &staging,
            "plasma-workspace",
            "plasma-workspace",
        )
        .unwrap();

        assert_eq!(
            fs::read(staging.join("etc/xdg/menus/plasma-applications.menu")).unwrap(),
            b"<Menu><Name>Applications</Name></Menu>\n"
        );
    }

    #[test]
    fn qt_declarative_staging_preserves_qml_module_and_plugin_paths() {
        let fixture = tempfile::tempdir().unwrap();
        let install = fixture.path().join("out/build/qtdeclarative/install/usr");
        for relative in [
            "lib/x86_64-linux-gnu/qml/QtQuick/qmldir",
            "lib/x86_64-linux-gnu/qml/QtQuick/libqtquick2plugin.so",
        ] {
            let path = install.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, relative.as_bytes()).unwrap();
        }
        let source = fixture.path().join("src/desktop/qt/qtdeclarative");
        fs::create_dir_all(source.join("LICENSES")).unwrap();
        fs::write(source.join("LICENSES/Qt-LICENSE"), b"qt license").unwrap();
        fs::write(source.join("licenseRule.json"), b"{}\n").unwrap();

        let staging = fixture.path().join("staged");
        stage_qt_module(fixture.path(), &staging, "qtdeclarative", "qt6-declarative").unwrap();

        assert!(
            staging
                .join("usr/lib/x86_64-linux-gnu/qml/QtQuick/qmldir")
                .is_file()
        );
        assert!(
            staging
                .join("usr/lib/x86_64-linux-gnu/qml/QtQuick/libqtquick2plugin.so")
                .is_file()
        );
        assert!(
            staging
                .join("usr/share/doc/qt6-declarative/licenses/Qt-LICENSE")
                .is_file()
        );
    }

    #[test]
    fn fira_staging_requires_complete_source_owned_runtime_set() {
        let repo = tempfile::tempdir().unwrap();
        let install = repo.path().join("out/build/pop-fonts/install");
        for font in POP_FIRA_RUNTIME_FONTS {
            let path = install.join("usr/share/fonts/opentype/fira").join(font);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, font.as_bytes()).unwrap();
        }
        let license = install.join("usr/share/doc/fonts-fira/copyright");
        fs::create_dir_all(license.parent().unwrap()).unwrap();
        fs::write(&license, b"OFL").unwrap();

        let staging = repo.path().join("staged");
        fs::create_dir_all(&staging).unwrap();
        stage_pop_fonts(repo.path(), &staging).unwrap();
        assert!(
            staging
                .join("usr/share/fonts/opentype/fira/FiraSans-Regular.otf")
                .is_file()
        );
        assert_eq!(
            fs::read(staging.join("usr/share/doc/fonts-fira/copyright")).unwrap(),
            b"OFL"
        );

        fs::remove_file(install.join("usr/share/fonts/opentype/fira/FiraMono-Regular.otf"))
            .unwrap();
        let incomplete = repo.path().join("incomplete");
        fs::create_dir_all(&incomplete).unwrap();
        assert!(stage_pop_fonts(repo.path(), &incomplete).is_err());
    }
}

pub(super) fn stage_cozy(repo_root: &Path, staging: &Path) -> Result<()> {
    stage_executable(
        &component_install(repo_root, "cozy").join("usr/bin/cozy"),
        &staging.join("usr/bin/cozy"),
        0o755,
    )?;
    for license in ["LICENSE-MIT", "LICENSE-APACHE"] {
        copy_preserving(
            &repo_root.join("src/userland/cozy").join(license),
            &staging.join("usr/share/doc/mattos-cozy").join(license),
        )?;
    }
    Ok(())
}

pub(super) fn stage_pipewire(repo_root: &Path, staging: &Path) -> Result<()> {
    let install = component_install(repo_root, "pipewire");
    for relative in [
        "usr/bin",
        "usr/lib/x86_64-linux-gnu",
        "usr/lib/systemd/user",
        "usr/share/pipewire",
    ] {
        copy_tree_preserving(&install.join(relative), &staging.join(relative))?;
    }
    copy_preserving(
        &repo_root.join("src/system/multimedia/pipewire/COPYING"),
        &staging.join("usr/share/doc/pipewire/copyright"),
    )?;
    let socket_wants = staging.join("usr/lib/systemd/user/sockets.target.wants");
    fs::create_dir_all(&socket_wants)?;
    #[cfg(unix)]
    for unit in ["pipewire.socket", "pipewire-pulse.socket"] {
        std::os::unix::fs::symlink(format!("../{unit}"), socket_wants.join(unit))?;
    }
    for required in [
        "usr/bin/pipewire",
        "usr/bin/pipewire-pulse",
        "usr/lib/x86_64-linux-gnu/libpipewire-0.3.so.0",
        "usr/lib/systemd/user/pipewire.service",
        "usr/lib/systemd/user/pipewire.socket",
        "usr/lib/systemd/user/sockets.target.wants/pipewire.socket",
    ] {
        if fs::symlink_metadata(staging.join(required)).is_err() {
            bail!("pipewire package is missing /{required}");
        }
    }
    Ok(())
}

pub(super) fn stage_mesa_dri_runtime(repo_root: &Path, staging: &Path) -> Result<()> {
    let install = component_install(repo_root, "mesa");
    let library_dir = install.join("usr/lib/x86_64-linux-gnu");
    let gallium = fs::read_dir(&library_dir)?
        .filter_map(Result::ok)
        .map(|entry| entry.file_name())
        .find(|name| {
            let name = name.to_string_lossy();
            name.starts_with("libgallium-") && name.ends_with(".so")
        })
        .ok_or_else(|| anyhow!("Mesa did not install its versioned Gallium DRI runtime"))?;
    let gallium_rel = format!("usr/lib/x86_64-linux-gnu/{}", gallium.to_string_lossy());
    stage_runtime_paths(
        repo_root,
        staging,
        "mesa",
        &[&gallium_rel, "usr/lib/x86_64-linux-gnu/gbm/dri_gbm.so"],
    )?;
    copy_tree_preserving(
        &install.join("usr/share/drirc.d"),
        &staging.join("usr/share/drirc.d"),
    )?;
    copy_preserving(
        &repo_root.join("src/system/graphics/mesa/docs/license.rst"),
        &staging.join("usr/share/doc/libgl1-mesa-dri/copyright"),
    )?;
    Ok(())
}

pub(super) fn stage_mesa_egl_vendor(repo_root: &Path, staging: &Path) -> Result<()> {
    stage_runtime_paths(
        repo_root,
        staging,
        "mesa",
        &[
            "usr/lib/x86_64-linux-gnu/libEGL_mesa.so.0.0.0",
            "usr/lib/x86_64-linux-gnu/libEGL_mesa.so.0",
            "usr/share/glvnd/egl_vendor.d/50_mesa.json",
        ],
    )?;
    copy_preserving(
        &repo_root.join("src/system/graphics/mesa/docs/license.rst"),
        &staging.join("usr/share/doc/libegl-mesa0/copyright"),
    )
}

pub(super) fn stage_nvidia_package(repo_root: &Path, staging: &Path, package: &str) -> Result<()> {
    let install = component_install(repo_root, "nvidia-driver");
    let lib = "usr/lib/x86_64-linux-gnu";
    let copy_libraries = |names: &[&str]| -> Result<()> {
        for name in names {
            copy_path_preserving(&install.join(lib).join(name), &staging.join(lib).join(name))?;
        }
        Ok(())
    };
    match package {
        NVIDIA_OPEN_MODULES_PACKAGE => {
            copy_tree_preserving(
                &install.join(concat!("usr/lib/modules/", mattos_kernel_release!(), "/updates/nvidia")),
                &staging.join(concat!("usr/lib/modules/", mattos_kernel_release!(), "/updates/nvidia")),
            )?;
            copy_preserving(
                &repo_root.join("src/system/graphics/nvidia-driver/nvidia-modprobe.conf"),
                &staging.join("etc/modprobe.d/nvidia.conf"),
            )?;
            copy_preserving(
                &install.join("usr/lib/modprobe.d/nvidia-supported-gpus.conf"),
                &staging.join("usr/lib/modprobe.d/nvidia-supported-gpus.conf"),
            )?;
            copy_preserving(
                &install.join("usr/libexec/mattos-nvidia-select"),
                &staging.join("usr/libexec/mattos-nvidia-select"),
            )?;
            fs::write(
                staging.join("DEBIAN/conffiles"),
                "/etc/modprobe.d/nvidia.conf\n",
            )?;
            fs::write(
                staging.join("DEBIAN/postinst"),
                concat!("#!/bin/sh\nset -e\n# Offline image assembly runs depmod after all module packages are unpacked.\n[ -n \"${DPKG_ROOT:-}\" ] && exit 0\nif command -v depmod >/dev/null 2>&1; then depmod ", mattos_kernel_release!(), "; fi\n"),
            )?;
            set_mode(staging.join("DEBIAN/postinst"), 0o755)?;
            fs::write(
                staging.join("DEBIAN/postrm"),
                concat!("#!/bin/sh\nset -e\n# Do not modify the build host while assembling an offline root.\n[ -n \"${DPKG_ROOT:-}\" ] && exit 0\nif command -v depmod >/dev/null 2>&1; then depmod ", mattos_kernel_release!(), "; fi\n"),
            )?;
            set_mode(staging.join("DEBIAN/postrm"), 0o755)?;
        }
        "nvidia-firmware-595" => copy_tree_preserving(
            &install.join("usr/lib/firmware/nvidia/595.84"),
            &staging.join("usr/lib/firmware/nvidia/595.84"),
        )?,
        "libnvidia-gl-595" => {
            copy_libraries(&[
                "libEGL_nvidia.so.595.84",
                "libEGL_nvidia.so.0",
                "libGLESv1_CM_nvidia.so.595.84",
                "libGLESv1_CM_nvidia.so.1",
                "libGLESv2_nvidia.so.595.84",
                "libGLESv2_nvidia.so.2",
                "libGLX_nvidia.so.595.84",
                "libGLX_nvidia.so.0",
                "libnvidia-allocator.so.595.84",
                "libnvidia-allocator.so.1",
                "libnvidia-egl-gbm.so.1.1.3",
                "libnvidia-egl-gbm.so.1",
                "libnvidia-egl-wayland.so.1.1.20",
                "libnvidia-egl-wayland.so.1",
                "libnvidia-egl-wayland2.so.1.0.1",
                "libnvidia-egl-wayland2.so.1",
                "libnvidia-eglcore.so.595.84",
                "libnvidia-glcore.so.595.84",
                "libnvidia-glsi.so.595.84",
                "libnvidia-glvkspirv.so.595.84",
                "libnvidia-gpucomp.so.595.84",
                "libnvidia-present.so.595.84",
                "libnvidia-tls.so.595.84",
            ])?;
            for relative in [
                "usr/share/glvnd/egl_vendor.d/10_nvidia.json",
                "usr/share/vulkan/icd.d/nvidia_icd.json",
                "usr/share/vulkan/implicit_layer.d/nvidia_layers.json",
                "usr/share/egl/egl_external_platform.d/09_nvidia_wayland2.json",
                "usr/share/egl/egl_external_platform.d/10_nvidia_wayland.json",
                "usr/share/egl/egl_external_platform.d/15_nvidia_gbm.json",
            ] {
                copy_path_preserving(&install.join(relative), &staging.join(relative))?;
            }
            let backend = staging.join("usr/lib/x86_64-linux-gnu/gbm/nvidia-drm_gbm.so");
            fs::create_dir_all(backend.parent().expect("NVIDIA GBM backend parent"))?;
            std::os::unix::fs::symlink("../libnvidia-allocator.so.1", backend)?;
            validate_nvidia_graphics_metadata(staging)?;
        }
        "libnvidia-compute-595" => copy_libraries(&[
            "libcuda.so.595.84",
            "libcuda.so.1",
            "libnvidia-ml.so.595.84",
            "libnvidia-ml.so.1",
            "libnvidia-ptxjitcompiler.so.595.84",
            "libnvidia-ptxjitcompiler.so.1",
        ])?,
        "libnvidia-encode-595" => {
            copy_libraries(&["libnvidia-encode.so.595.84", "libnvidia-encode.so.1"])?
        }
        "libnvidia-decode-595" => copy_libraries(&["libnvcuvid.so.595.84", "libnvcuvid.so.1"])?,
        "nvidia-utils-595" => {
            for name in ["nvidia-smi", "nvidia-modprobe", "nvidia-persistenced"] {
                copy_path_preserving(
                    &install.join("usr/bin").join(name),
                    &staging.join("usr/bin").join(name),
                )?;
            }
        }
        "nvidia-driver-595-open" => {}
        _ => bail!("unknown NVIDIA package {package}"),
    }
    copy_preserving(
        &install.join("usr/share/doc/nvidia-driver-595/LICENSE"),
        &staging
            .join("usr/share/doc")
            .join(package)
            .join("copyright"),
    )?;
    copy_preserving(
        &install.join("usr/share/doc/nvidia-driver-595/manifest.toml"),
        &staging
            .join("usr/share/doc")
            .join(package)
            .join("manifest.toml"),
    )?;
    for name in [
        "README.md",
        "runfile.sha256",
        "supported-gpus.json",
        "supported-gpus.LICENSE",
    ] {
        copy_preserving(
            &install.join("usr/share/doc/nvidia-driver-595").join(name),
            &staging.join("usr/share/doc").join(package).join(name),
        )?;
    }
    Ok(())
}

pub(super) fn validate_nvidia_graphics_metadata(root: &Path) -> Result<()> {
    let icd_path = root.join("usr/share/vulkan/icd.d/nvidia_icd.json");
    let icd: serde_json::Value = serde_json::from_slice(&fs::read(&icd_path)?)?;
    let library = icd
        .pointer("/ICD/library_path")
        .and_then(|value| value.as_str());
    if library != Some("libGLX_nvidia.so.0")
        || !root
            .join("usr/lib/x86_64-linux-gnu/libGLX_nvidia.so.0")
            .is_symlink()
    {
        bail!("NVIDIA Vulkan ICD does not resolve through its canonical system SONAME");
    }
    for relative in [
        "usr/share/glvnd/egl_vendor.d/10_nvidia.json",
        "usr/share/egl/egl_external_platform.d/09_nvidia_wayland2.json",
        "usr/share/egl/egl_external_platform.d/10_nvidia_wayland.json",
        "usr/share/egl/egl_external_platform.d/15_nvidia_gbm.json",
    ] {
        let value: serde_json::Value = serde_json::from_slice(&fs::read(root.join(relative))?)?;
        if value
            .get("file_format_version")
            .and_then(|version| version.as_str())
            .is_none()
        {
            bail!("NVIDIA metadata /{relative} is not a valid vendor manifest");
        }
    }
    Ok(())
}

pub(super) fn stage_mesa_vulkan_runtime(repo_root: &Path, staging: &Path) -> Result<()> {
    stage_runtime_paths(
        repo_root,
        staging,
        "mesa",
        &[
            "usr/lib/x86_64-linux-gnu/libVkLayer_MESA_device_select.so",
            "usr/lib/x86_64-linux-gnu/libvulkan_radeon.so",
            "usr/lib/x86_64-linux-gnu/libvulkan_intel.so",
            "usr/lib/x86_64-linux-gnu/libvulkan_nouveau.so",
            "usr/lib/x86_64-linux-gnu/libvulkan_virtio.so",
            "usr/lib/x86_64-linux-gnu/libvulkan_lvp.so",
            "usr/share/vulkan/icd.d/radeon_icd.x86_64.json",
            "usr/share/vulkan/icd.d/intel_icd.x86_64.json",
            "usr/share/vulkan/icd.d/nouveau_icd.x86_64.json",
            "usr/share/vulkan/icd.d/virtio_icd.x86_64.json",
            "usr/share/vulkan/icd.d/lvp_icd.x86_64.json",
            "usr/share/vulkan/implicit_layer.d/VkLayer_MESA_device_select.json",
        ],
    )?;
    copy_preserving(
        &repo_root.join("src/system/graphics/mesa/docs/license.rst"),
        &staging.join("usr/share/doc/mesa-vulkan-drivers/copyright"),
    )?;
    validate_vulkan_icd_manifests(staging)?;
    Ok(())
}

pub(crate) fn validate_vulkan_icd_manifests(root: &Path) -> Result<()> {
    let manifests = [
        ("radeon_icd.x86_64.json", "libvulkan_radeon.so"),
        ("intel_icd.x86_64.json", "libvulkan_intel.so"),
        ("nouveau_icd.x86_64.json", "libvulkan_nouveau.so"),
        ("virtio_icd.x86_64.json", "libvulkan_virtio.so"),
        ("lvp_icd.x86_64.json", "libvulkan_lvp.so"),
    ];
    for (manifest, library) in manifests {
        let path = root.join("usr/share/vulkan/icd.d").join(manifest);
        let value: serde_json::Value = serde_json::from_slice(&fs::read(&path)?)
            .with_context(|| format!("invalid Vulkan ICD manifest {}", path.display()))?;
        let expected = format!("/usr/lib/x86_64-linux-gnu/{library}");
        if value.pointer("/ICD/library_path").and_then(|v| v.as_str()) != Some(&expected)
            || value
                .pointer("/ICD/api_version")
                .and_then(|v| v.as_str())
                .is_none()
            || value
                .get("file_format_version")
                .and_then(|v| v.as_str())
                .is_none()
        {
            bail!("Vulkan ICD manifest {} is not canonical", path.display())
        }
        if !root.join(expected.trim_start_matches('/')).is_file() {
            bail!("Vulkan ICD {manifest} references missing {expected}")
        }
    }
    Ok(())
}

pub(super) fn stage_vulkan_tools(repo_root: &Path, staging: &Path) -> Result<()> {
    stage_runtime_paths(
        repo_root,
        staging,
        "vulkan-tools",
        &["usr/bin/vulkaninfo", "usr/bin/vkcube"],
    )?;
    copy_preserving(
        &repo_root.join("src/system/graphics/vulkan-tools/LICENSE.txt"),
        &staging.join("usr/share/doc/vulkan-tools/copyright"),
    )?;
    Ok(())
}

/// Install generated XKB rules from the output-owned xkeyboard-config mirror.
/// The Git import contains rules fragments; Meson produces `rules/evdev`.
pub(super) fn stage_xkeyboard_config_data(repo_root: &Path, staging: &Path) -> Result<()> {
    build_xkeyboard_config(repo_root)?;
    let source = repo_root.join("out/build/xkeyboard-config/install/usr/share");
    copy_tree_preserving(
        &source.join("xkeyboard-config-2"),
        &staging.join("usr/share/xkeyboard-config-2"),
    )?;
    // Meson's installed legacy link is absolute (`/usr/share/...`).  Preserve
    // its in-image meaning rather than copying an absolute host-root link
    // into package staging.
    let legacy_root = staging.join("usr/share/X11/xkb");
    if let Some(parent) = legacy_root.parent() {
        fs::create_dir_all(parent)?;
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink("../xkeyboard-config-2", &legacy_root)?;
    #[cfg(not(unix))]
    bail!("xkeyboard-config package staging requires Unix symlinks");
    let rules = staging.join("usr/share/xkeyboard-config-2/rules/evdev");
    if !rules.is_file() {
        bail!(
            "xkeyboard-config staging did not contain generated {}",
            rules.display()
        );
    }
    copy_preserving(
        &repo_root.join("src/system/data/xkeyboard-config/COPYING"),
        &staging.join("usr/share/doc/xkb-data/copyright"),
    )?;
    Ok(())
}

#[cfg(test)]
mod mattos_plasma_theme_tests {
    use super::*;

    const TEMPLATE: &str = "floating=@@MATTOS_PANEL_FLOATING@@;length=@@MATTOS_PANEL_LENGTH_MODE@@;opacity=@@MATTOS_PANEL_OPACITY@@;hiding=@@MATTOS_PANEL_HIDING@@;height=@@MATTOS_PANEL_THICKNESS@@";

    #[test]
    fn system_defaults_select_the_bundled_nordic_color_scheme() {
        let kdeglobals =
            include_str!("../../../../../system/desktop/branding/MattOS/xdg/kdeglobals");
        let look_and_feel_defaults =
            include_str!("../../../../../system/desktop/branding/MattOS/contents/defaults");
        let nordic = include_str!("../../../../../desktop/themes/nordic-kde/colors");

        assert!(kdeglobals.contains("ColorScheme=Nordic"));
        assert!(look_and_feel_defaults.contains("[kdeglobals][General]\nColorScheme=Nordic"));
        assert!(nordic.contains("Name=Nordic"));
    }

    #[test]
    fn panel_policy_is_consumed_and_mapped_to_supported_plasma_values() {
        let policy =
            "[Panel]\nfloating=true\nlengthMode=0\nopacityMode=0\nvisibilityMode=0\nthickness=60\n";
        assert_eq!(
            render_mattos_panel_layout(TEMPLATE, policy).unwrap(),
            "floating=true;length=fill;opacity=adaptive;hiding=none;height=60"
        );
    }

    #[test]
    fn panel_policy_fails_closed_on_unknown_or_out_of_range_values() {
        assert!(render_mattos_panel_layout(TEMPLATE, "[Panel]\nfloating=true\nlengthMode=8\nopacityMode=0\nvisibilityMode=0\nthickness=60\n").is_err());
        assert!(render_mattos_panel_layout(TEMPLATE, "[Panel]\nfloating=true\nlengthMode=0\nopacityMode=0\nvisibilityMode=0\nthickness=999\n").is_err());
        assert!(
            render_mattos_panel_layout(
                TEMPLATE,
                "[Panel]\nfloating=true\nlengthMode=0\nopacityMode=0\nvisibilityMode=0\n"
            )
            .is_err()
        );
    }

    #[test]
    fn wallpaper_mount_is_preserved_without_packaging_wallpaper_files() {
        let layout = include_str!(
            "../../../../../system/desktop/branding/MattOS/contents/layouts/org.kde.plasma.desktop-layout.js"
        );
        assert!(layout.contains("/mnt/storage/OneDrive/Media/Wallpapers/Wide/"));
        assert!(layout.contains("/usr/share/wallpapers/"));
        let rendered = render_mattos_panel_layout(
            layout,
            "[Panel]\nfloating=true\nlengthMode=0\nopacityMode=0\nvisibilityMode=0\nthickness=60\n",
        )
        .unwrap();
        assert!(!rendered.contains("@@MATTOS_PANEL_"));
        assert!(rendered.contains("/mnt/storage/OneDrive/Media/Wallpapers/Wide/"));
    }

    #[test]
    fn plasma_workspace_filter_transfers_only_the_ten_custom_categories() {
        for name in MATTOS_DESKTOP_DIRECTORY_OVERRIDES {
            assert!(is_mattos_desktop_directory_override(
                Path::new("share/desktop-directories").join(name).as_path()
            ));
        }
        assert!(!is_mattos_desktop_directory_override(Path::new(
            "share/desktop-directories/kf5-audio.directory"
        )));
        assert!(!is_mattos_desktop_directory_override(Path::new(
            "share/applications/kf5-games.directory"
        )));
    }

    #[test]
    fn papirus_dark_inherits_the_full_app_icon_theme_without_changing_upstream() {
        let upstream = include_str!(
            "../../../../../desktop/themes/papirus-icon-theme/Papirus-Dark/index.theme"
        );
        let base =
            include_str!("../../../../../desktop/themes/papirus-icon-theme/Papirus/index.theme");
        let staged = add_papirus_base_fallback(upstream).unwrap();
        assert!(staged.contains("Inherits=Papirus,breeze-dark,hicolor"));
        assert_eq!(upstream.matches("Inherits=breeze-dark,hicolor").count(), 1);
        assert!(base.contains("[Icon Theme]"));
        let apps = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../desktop/themes/papirus-icon-theme/Papirus/24x24/apps");
        for app_icon in ["org.kde.dolphin.svg", "kate.svg", "utilities-terminal.svg"] {
            assert!(
                apps.join(app_icon).is_file(),
                "Papirus app fallback lacks {app_icon}"
            );
        }
    }

    #[test]
    fn papirus_theme_patch_rejects_unknown_or_ambiguous_upstream_metadata() {
        assert!(add_papirus_base_fallback("[Icon Theme]\nInherits=breeze-dark,hicolor\n").is_ok());
        assert!(add_papirus_base_fallback("[Icon Theme]\nInherits=hicolor\n").is_err());
        assert!(
            add_papirus_base_fallback(
                "Inherits=breeze-dark,hicolor\nInherits=breeze-dark,hicolor\n"
            )
            .is_err()
        );
    }
}
