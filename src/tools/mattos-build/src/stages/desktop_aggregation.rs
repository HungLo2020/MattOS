fn build_cosmic_desktop(repo_root: &Path) -> Result<()> {
    let out_root = repo_root.join("out/build/cosmic-desktop");
    let install = out_root.join("install");
    remove_path_if_exists(&install)?;
    fs::create_dir_all(&install)?;
    for component in [
        "cosmic-session",
        "cosmic-greeter",
        "cosmic-panel",
        "cosmic-applets",
        "cosmic-applibrary",
        "cosmic-launcher",
        "cosmic-settings",
        "cosmic-settings-daemon",
        "cosmic-notifications",
        "cosmic-osd",
        "cosmic-bg",
        "cosmic-workspaces",
        "cosmic-files",
        "cosmic-term",
        "cosmic-tweaks",
        "cosmic-randr",
        "cosmic-screenshot",
        "pop-launcher",
        "cosmic-calculator",
        "cosmic-storage",
        "cosmic-monitor",
        "cosmic-store",
        "cosmic-portal",
        "cosmic-assets",
        "greetd",
    ] {
        let component_install = repo_root.join("out/build").join(component).join("install");
        if !component_install.is_dir() {
            bail!(
                "COSMIC aggregate input missing: {}",
                component_install.display()
            );
        }
        copy_tree_contents(&component_install, &install)?;
    }
    for required in [
        "usr/bin/cosmic-session",
        "usr/bin/cosmic-panel",
        "usr/bin/cosmic-launcher",
        "usr/bin/cosmic-settings-daemon",
        "usr/bin/cosmic-notifications",
        "usr/bin/cosmic-osd",
        "usr/bin/cosmic-bg",
        "usr/bin/cosmic-workspaces",
        "usr/bin/cosmic-files",
        "usr/bin/cosmic-term",
        "usr/bin/cosmic-ext-tweaks",
        "usr/bin/cosmic-ext-calculator",
        "usr/bin/cosmic-ext-storage",
        "usr/bin/cosmic-monitor",
        "usr/bin/cosmic-store",
        "usr/bin/greetd",
        "usr/share/wayland-sessions/cosmic.desktop",
        "usr/share/icons/Cosmic/index.theme",
        "usr/share/fonts/truetype/open-sans/OpenSans-Regular.ttf",
        "usr/share/fonts/truetype/noto/NotoSansMono[wdth,wght].ttf",
    ] {
        if !install.join(required).is_file() {
            bail!("COSMIC desktop aggregate did not install /{required}");
        }
    }
    Ok(())
}
