fn build_grub(repo_root: &Path) -> Result<()> {
    let out = repo_root.join("out/build/grub");
    let source = out.join("source");
    let gnulib = out.join("gnulib-source");
    let build = out.join("build");
    let install = out.join("install");
    for private in [&source, &gnulib, &build] { remove_path_if_exists(private)?; }
    sync_build_source(&repo_root.join("src/boot/grub/upstream"), &source)?;
    apply_component_patches(repo_root, "grub", &source)?;
    sync_build_source(&repo_root.join("src/build-support/grub-gnulib"), &gnulib)?;
    run_cmd_with_env_overrides(&source, "bash", &[
        "bootstrap", "--no-git", "--no-bootstrap-sync", "--skip-po",
        &format!("--gnulib-srcdir={}", gnulib.display()),
    ], &[("PYTHONDONTWRITEBYTECODE", "1".to_string()),
        ("ACLOCAL_PATH", repo_root.join("src/build-support/autoconf-archive/m4").display().to_string())])?;
    remove_path_if_exists(&install)?;
    let mut env = staged_library_environment(repo_root, &["zlib", "xz", "freetype"])?;
    // The build-only font converters must use owned libraries too. Their
    // absolute loader/rpaths remain in disposable build tools, not packages.
    for (normal, build) in [("CPPFLAGS", "BUILD_CPPFLAGS"), ("CFLAGS", "BUILD_CFLAGS")] {
        if let Some((_, value)) = env.iter().find(|(key, _)| *key == normal) {
            env.push((build, value.clone()));
        }
    }
    let mut build_link = env.iter().find(|(key, _)| *key == "LDFLAGS")
        .map(|(_, value)| value.clone()).unwrap_or_default();
    build_link.push_str(&format!(" -Wl,--dynamic-linker={}",
        repo_root.join("out/build/glibc/install/lib64/ld-linux-x86-64.so.2").display()));
    for component in ["glibc", "gcc-runtime", "zlib", "freetype"] {
        build_link.push_str(&format!(" -Wl,-rpath,{}",
            repo_root.join(format!("out/build/{component}/install/usr/lib/x86_64-linux-gnu")).display()));
    }
    env.push(("BUILD_LDFLAGS", build_link));
    env.push(("enable_build_grub_mkfont", "yes".to_string()));
    // Binutils 2.46 accepts --image-base for ELF, but includes the ELF
    // headers in that address: GRUB's PC entry becomes 0x9074, not 0x9000.
    // Select the supported -Ttext path (LFS #5857), retaining mkimage's
    // strict entry-address validation. This is a target linker constraint,
    // not a stage-cache override.
    env.push(("ax_cv_check_ldflags___Wl___image_base_0x400000", "no".to_string()));
    // Preserve the existing dual-firmware ISO contract. Both module sets are
    // built from this same pinned source; install EFI utilities last.
    for (target, platform) in [("i386", "pc"), ("x86_64", "efi")] {
    let build = build.join(platform);
    fs::create_dir_all(&build)?;
    run_cmd_with_env_overrides(&build, path_str(&source.join("configure"))?, &[
        "--prefix=/usr", "--sbindir=/usr/sbin", "--sysconfdir=/etc",
        "--libdir=/usr/lib", &format!("--target={target}"), &format!("--with-platform={platform}"),
        "--disable-werror", "--disable-nls", "--disable-grub-mount",
        "--disable-device-mapper", "--disable-libzfs", "--disable-grub-mkfont",
        "--disable-grub-themes", "--disable-grub-protect", "--enable-liblzma",
        // Unifont's embedded 128 control/ASCII bitmap generator requires
        // Unifont itself. Keep upstream's built-in ASCII fallback and generate
        // only the external PF2 resource from our owned ordinary font below.
        "--without-unifont",
    ], &env)?;
    run_cmd_with_env_overrides(&build, "make", &["-j", "4"], &env)?;
    run_cmd_with_env_overrides(&build, "make", &["install", &format!("DESTDIR={}", install.display())], &env)?;
    run_cmd_with_env_overrides(&build, "make", &["build-grub-mkfont"], &env)?;
    run_cmd_with_env_overrides(&build, path_str(&build.join("build-grub-mkfont"))?, &[
        "-o", path_str(&install.join("usr/share/grub/unicode.pf2"))?,
        path_str(&repo_root.join("src/desktop/fonts/open-sans/fonts/ttf/OpenSans-Regular.ttf"))?,
    ], &env)?;
    }
    fs::create_dir_all(install.join("usr/share/doc/grub-efi-amd64"))?;
    fs::copy(repo_root.join("src/desktop/fonts/open-sans/OFL.txt"),
        install.join("usr/share/doc/grub-efi-amd64/OpenSans-OFL.txt"))?;
    for required in ["usr/bin/grub-mkimage", "usr/bin/grub-mkrescue", "usr/sbin/grub-install",
        "usr/sbin/grub-mkconfig", "etc/grub.d/10_linux", "etc/grub.d/30_uefi-firmware",
        "usr/lib/grub/x86_64-efi/linux.mod", "usr/lib/grub/x86_64-efi/efifwsetup.mod",
        "usr/lib/grub/i386-pc/boot_hybrid.img", "usr/share/grub/unicode.pf2"] {
        if !install.join(required).is_file() { bail!("GRUB output missing: {required}"); }
    }
    Ok(())
}

#[cfg(test)]
mod grub_tests {
    use super::*;

    #[test]
    fn grub_is_source_owned_in_both_image_consumers() {
        for consumer in [BuildStage::Installer, BuildStage::Iso] {
            assert!(stage_graph::direct_dependencies(consumer).contains(&"grub"));
            assert!(!stage_inputs::tool_names(consumer).iter().any(|tool| tool == "grub-mkimage" || tool == "grub-mkrescue"));
        }
        let sources = stage_inputs::source_inputs(BuildStage::Grub);
        for source in ["src/boot/grub/upstream", "src/build-support/grub-gnulib", "src/build-support/autoconf-archive", "upstream/patches/grub"] {
            assert!(sources.contains(&PathBuf::from(source)));
        }
        assert!(!stage_inputs::source_inputs(BuildStage::NetworkManager).contains(&PathBuf::from("src/tools/mattos-build/src/stages/grub.rs")));
        assert!(stage_graph::direct_dependencies(BuildStage::Grub).contains(&"freetype"));
        assert!(sources.contains(&PathBuf::from("src/desktop/fonts/open-sans/fonts/ttf/OpenSans-Regular.ttf")));
    }

    #[test]
    fn grub_policy_preserves_default_live_boot_and_upstream_kernel_menu() {
        let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let live = fs::read_to_string(repo.join("src/boot/grub/grub.cfg")).unwrap();
        assert!(live.find("menuentry \"Start MattOS Live\"").unwrap() < live.find("menuentry \"UEFI Firmware Settings\"").unwrap());
        assert!(live.contains("fwsetup --is-supported"));
        let update = fs::read_to_string(repo.join("src/boot/grub/config/update-grub")).unwrap();
        assert!(update.contains("exec /usr/sbin/grub-mkconfig -o /boot/grub/grub.cfg"));
    }
}
