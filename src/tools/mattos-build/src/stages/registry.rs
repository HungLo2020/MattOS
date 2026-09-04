fn stage_resource_profile(stage: BuildStage) -> scheduler::StageResourceProfile {
    if stage == BuildStage::Libcap {
        return scheduler::StageResourceProfile::serial();
    }
    if matches!(
        stage,
        BuildStage::Llvm
            | BuildStage::Mesa
            | BuildStage::CosmicComp
            | BuildStage::CosmicSession
            | BuildStage::CosmicGreeter
            | BuildStage::CosmicPanel
            | BuildStage::CosmicApplets
            | BuildStage::CosmicAppLibrary
            | BuildStage::CosmicLauncher
            | BuildStage::CosmicSettings
            | BuildStage::CosmicSettingsDaemon
            | BuildStage::CosmicNotifications
            | BuildStage::CosmicOsd
            | BuildStage::CosmicBg
            | BuildStage::CosmicWorkspaces
            | BuildStage::CosmicFiles
            | BuildStage::CosmicTerm
            | BuildStage::CosmicTweaks
            | BuildStage::CosmicUtilities
            | BuildStage::CosmicRandr
            | BuildStage::CosmicScreenshot
            | BuildStage::PopLauncher
            | BuildStage::CosmicCalculator
            | BuildStage::CosmicStorage
            | BuildStage::CosmicMonitor
            | BuildStage::CosmicStore
            | BuildStage::Flatpak
            | BuildStage::CosmicPortal
            | BuildStage::CosmicEdit
            | BuildStage::CosmicInitialSetup
            | BuildStage::Greetd
    ) {
        return scheduler::StageResourceProfile::high_memory_parallel();
    }
    match stage {
        BuildStage::Kernel
        | BuildStage::Glibc
        | BuildStage::GccRuntime
        | BuildStage::Binutils
        | BuildStage::GccToolchain
        | BuildStage::Brush
        | BuildStage::Coreutils
        | BuildStage::Grep
        | BuildStage::Sed
        | BuildStage::Findutils
        | BuildStage::Diffutils
        | BuildStage::Git
        | BuildStage::Libffi
        | BuildStage::NvidiaDriver
        | BuildStage::Python
        | BuildStage::Rust
        | BuildStage::SudoRs
        | BuildStage::Init
        // Zstd-backed squashfs compression scales cleanly to four workers but
        // needs the same bounded per-worker memory admission as compilers.
        | BuildStage::LiveRoot => scheduler::StageResourceProfile::memory_heavy(),
        _ => scheduler::StageResourceProfile::standard(),
    }
}

#[cfg(test)]
fn scheduler_child_job_policy(stage: BuildStage) -> scheduler::ChildJobPolicy {
    stage_resource_profile(stage).child_jobs
}

fn is_cacheable_stage(stage: BuildStage) -> bool {
    !matches!(
        stage,
        BuildStage::Rootfs
            | BuildStage::LiveRoot
            | BuildStage::Initramfs
            | BuildStage::Iso
            | BuildStage::All
    )
}

fn build_stage_id(stage: BuildStage) -> &'static str {
    stage_graph::stage_id(stage)
}

fn build_stage_spec(stage: BuildStage) -> performance::StageSpec {
    let id = build_stage_id(stage);
    let sources = stage_inputs::source_inputs(stage);
    let outputs: Vec<PathBuf> = match stage {
        BuildStage::Kernel => vec![
            "out/build/linux/build/arch/x86/boot/bzImage".into(),
            "out/build/linux/modules/usr/lib/modules".into(),
            "out/build/linux/kernel-release".into(),
        ],
        BuildStage::Glibc => vec![
            "out/build/glibc/install".into(),
            "out/build/glibc/linux-headers".into(),
            "out/build/glibc/linux-headers-inventory.txt".into(),
            "out/sysroot/usr/include/stdio.h".into(),
            "out/sysroot/usr/lib/x86_64-linux-gnu/libc.so.6".into(),
            "out/sysroot/lib64/ld-linux-x86-64.so.2".into(),
        ],
        BuildStage::GccRuntime => vec![
            "out/build/gcc-runtime/install".into(),
            "out/build/gcc-runtime/runtime".into(),
            "out/build/gcc-runtime/runtime-abi.tsv".into(),
            "out/sysroot/usr/lib/x86_64-linux-gnu/libgcc_s.so.1".into(),
            "out/sysroot/usr/lib/x86_64-linux-gnu/libstdc++.so.6.0.34".into(),
        ],
        BuildStage::Binutils => vec![
            "out/build/binutils/cross-install".into(),
            "out/build/binutils/install".into(),
            "out/build/binutils/configure-invocation.txt".into(),
        ],
        BuildStage::GccToolchain => vec![
            "out/build/gcc-toolchain/install".into(),
            "out/build/gcc-toolchain/configure-invocation.txt".into(),
        ],
        BuildStage::Make => vec!["out/build/make/install".into()],
        BuildStage::Brush => vec!["out/build/brush/cargo-target/release/brush".into()],
        BuildStage::Coreutils => {
            vec!["out/build/coreutils/cargo-target/release/coreutils".into()]
        }
        BuildStage::Grep => vec!["out/build/grep/cargo-target/release/grep".into()],
        BuildStage::Sed => vec!["out/build/sed/cargo-target/release/sed".into()],
        BuildStage::Findutils => vec!["out/build/findutils/cargo-target/release/find".into()],
        BuildStage::Diffutils => {
            vec!["out/build/diffutils/cargo-target/release/diffutils".into()]
        }
        BuildStage::Gzip => vec!["out/build/gzip/install".into()],
        BuildStage::Patch => vec!["out/build/patch/install".into()],
        BuildStage::File => vec!["out/build/file/install".into()],
        BuildStage::Less => vec!["out/build/less/install".into()],
        BuildStage::Git => vec!["out/build/git/install".into()],
        BuildStage::Openssh => vec!["out/build/openssh/install".into()],
        BuildStage::Libffi => vec!["out/build/libffi/install".into()],
        BuildStage::Wayland => vec!["out/build/wayland/install".into()],
        BuildStage::Xkbcommon => vec!["out/build/xkbcommon/install".into()],
        BuildStage::Libseat => vec!["out/build/seatd/install".into()],
        BuildStage::LibdisplayInfo => vec!["out/build/libdisplay-info/install".into()],
        BuildStage::Libevdev => vec!["out/build/libevdev/install".into()],
        BuildStage::Libinput => vec!["out/build/libinput/install".into()],
        BuildStage::Pixman => vec!["out/build/pixman/install".into()],
        BuildStage::Libdrm => vec!["out/build/libdrm/install".into()],
        BuildStage::VulkanHeaders => vec!["out/build/vulkan-headers/install".into()],
        BuildStage::VulkanLoader => vec!["out/build/vulkan-loader/install".into()],
        BuildStage::VulkanTools => vec!["out/build/vulkan-tools/install".into()],
        BuildStage::X11Compat => vec!["out/build/x11-compat/install".into()],
        BuildStage::Libepoxy => vec!["out/build/libepoxy/install/usr/lib/x86_64-linux-gnu/libepoxy.so.0".into()],
        BuildStage::Freetype => vec!["out/build/freetype/install/usr/lib/x86_64-linux-gnu/libfreetype.so.6".into()],
        BuildStage::Libfontenc => vec!["out/build/libfontenc/install/usr/lib/x86_64-linux-gnu/libfontenc.so.1".into()],
        BuildStage::Libxfont => vec!["out/build/libxfont/install/usr/lib/x86_64-linux-gnu/libXfont2.so.2".into()],
        BuildStage::Libxcvt => vec!["out/build/libxcvt/install/usr/lib/x86_64-linux-gnu/libxcvt.so.0".into()],
        BuildStage::Libxshmfence => vec!["out/build/libxshmfence/install/usr/lib/x86_64-linux-gnu/libxshmfence.so.1".into()],
        BuildStage::Libxkbfile => vec!["out/build/libxkbfile/install/usr/lib/x86_64-linux-gnu/libxkbfile.so.1".into()],
        BuildStage::Xkbcomp => vec!["out/build/xkbcomp/install/usr/bin/xkbcomp".into()],
        BuildStage::Libglvnd => vec!["out/build/libglvnd/install".into()],
        BuildStage::Mesa => vec!["out/build/mesa/install".into()],
        BuildStage::Xwayland => vec!["out/build/xwayland/install/usr/bin/Xwayland".into()],
        BuildStage::NvidiaDriver => vec![
            "out/build/nvidia-driver/install".into(),
            "out/build/nvidia-driver/source/LICENSE".into(),
            "out/build/nvidia-driver/runfile.sha256".into(),
        ],
        BuildStage::CosmicComp => vec!["out/build/cosmic-comp/install/usr/bin/cosmic-comp".into()],
        BuildStage::CosmicSession => {
            vec!["out/build/cosmic-session/install/usr/bin/cosmic-session".into()]
        }
        BuildStage::CosmicGreeter => {
            vec!["out/build/cosmic-greeter/install/usr/bin/cosmic-greeter".into()]
        }
        BuildStage::CosmicPanel => {
            vec!["out/build/cosmic-panel/install/usr/bin/cosmic-panel".into()]
        }
        BuildStage::CosmicApplets => {
            vec!["out/build/cosmic-applets/install/usr/bin/cosmic-applets".into()]
        }
        BuildStage::CosmicAppLibrary => {
            vec!["out/build/cosmic-applibrary/install/usr/bin/cosmic-app-library".into()]
        }
        BuildStage::CosmicLauncher => {
            vec!["out/build/cosmic-launcher/install/usr/bin/cosmic-launcher".into()]
        }
        BuildStage::CosmicSettings => {
            vec!["out/build/cosmic-settings/install/usr/bin/cosmic-settings".into()]
        }
        BuildStage::CosmicSettingsDaemon => {
            vec!["out/build/cosmic-settings-daemon/install/usr/bin/cosmic-settings-daemon".into()]
        }
        BuildStage::CosmicNotifications => {
            vec!["out/build/cosmic-notifications/install/usr/bin/cosmic-notifications".into()]
        }
        BuildStage::CosmicOsd => {
            vec!["out/build/cosmic-osd/install/usr/bin/cosmic-osd".into()]
        }
        BuildStage::CosmicBg => {
            vec!["out/build/cosmic-bg/install/usr/bin/cosmic-bg".into()]
        }
        BuildStage::CosmicWorkspaces => {
            vec!["out/build/cosmic-workspaces/install/usr/bin/cosmic-workspaces".into()]
        }
        BuildStage::CosmicFiles => {
            vec!["out/build/cosmic-files/install/usr/bin/cosmic-files".into()]
        }
        BuildStage::CosmicEdit => vec![
            "out/build/cosmic-edit/install/usr/bin/cosmic-edit".into(),
            "out/build/cosmic-edit/install/usr/share/applications/com.system76.CosmicEdit.desktop".into(),
        ],
        BuildStage::CosmicInitialSetup => vec![
            "out/build/cosmic-initial-setup/install/usr/bin/cosmic-initial-setup".into(),
            "out/build/cosmic-initial-setup/install/usr/share/applications/com.system76.CosmicInitialSetup.desktop".into(),
            "out/build/cosmic-initial-setup/install/usr/share/cosmic-layouts/top-panel-and-bottom-dock/layout.kdl".into(),
            "out/build/cosmic-initial-setup/install/usr/share/cosmic-themes/nebula-dark.ron".into(),
        ],
        // Publish the complete target Duktape install, not only its SONAME
        // symlink.  The generated shared object and headers are the actual
        // ABI consumed by Polkit; omitting them from the inventory let a
        // corrupted library retain the old stage output digest and prevented
        // dependency-output propagation into Polkit.
        BuildStage::Duktape => vec!["out/build/duktape/install".into()],
        BuildStage::CosmicTerm => {
            vec!["out/build/cosmic-term/install/usr/bin/cosmic-term".into()]
        }
        BuildStage::CosmicTweaks => {
            vec!["out/build/cosmic-tweaks/install/usr/bin/cosmic-ext-tweaks".into()]
        }
        BuildStage::CosmicUtilities => vec!["out/build/cosmic-utilities/install".into()],
        BuildStage::CosmicRandr => vec!["out/build/cosmic-randr/install/usr/bin/cosmic-randr".into()],
        BuildStage::CosmicScreenshot => vec!["out/build/cosmic-screenshot/install/usr/bin/cosmic-screenshot".into()],
        BuildStage::PopLauncher => vec!["out/build/pop-launcher/install/usr/bin/pop-launcher".into()],
        BuildStage::CosmicCalculator => vec!["out/build/cosmic-calculator/install/usr/bin/cosmic-ext-calculator".into()],
        BuildStage::CosmicStorage => vec!["out/build/cosmic-storage/install/usr/bin/cosmic-ext-storage".into()],
        BuildStage::CosmicMonitor => vec!["out/build/cosmic-monitor/install/usr/bin/cosmic-monitor".into()],
        BuildStage::CosmicStore => vec!["out/build/cosmic-store/install/usr/bin/cosmic-store".into()],
        BuildStage::Flatpak => vec![
            "out/build/flatpak/install/usr/bin/flatpak".into(),
            "out/build/flatpak/install/usr/lib/x86_64-linux-gnu/libflatpak.so.0".into(),
            "out/build/flatpak/install/usr/libexec/mattos-flatpak-target-install".into(),
        ],
        BuildStage::Bubblewrap => vec!["out/build/bubblewrap/install/usr/bin/bwrap".into()],
        BuildStage::XdgDbusProxy => {
            vec!["out/build/xdg-dbus-proxy/install/usr/bin/xdg-dbus-proxy".into()]
        }
        // The portal package consumes the complete GStreamer installs,
        // including plugins such as libgstgio.so.  Publishing only one
        // library here allowed a changed plugin to leave the stage output
        // digest unchanged and a stale portal package cache to be reused.
        BuildStage::Gstreamer => vec!["out/build/gstreamer/install".into()],
        BuildStage::GstreamerBase => vec!["out/build/gstreamer-base/install".into()],
        // The portal package publishes the broker, document services,
        // validators, D-Bus activation files, and GStreamer plugins.  Its
        // cache contract must therefore cover the complete install tree, not
        // only the broker binary, or a changed helper can leave a stale .deb.
        BuildStage::XdgDesktopPortal => {
            vec!["out/build/xdg-desktop-portal/install".into()]
        }
        BuildStage::Libarchive => vec!["out/build/libarchive/install/usr/lib/x86_64-linux-gnu/libarchive.so.13".into()],
        BuildStage::Libxml2 => vec!["out/build/libxml2/install/usr/lib/x86_64-linux-gnu/libxml2.so.16".into()],
        BuildStage::Libpng => vec!["out/build/libpng/install/usr/lib/x86_64-linux-gnu/libpng16.so.16".into()],
        BuildStage::Fuse3 => vec!["out/build/fuse3/install/usr/lib/x86_64-linux-gnu/libfuse3.so.4".into()],
        BuildStage::Libfyaml => vec!["out/build/libfyaml/install/usr/lib/x86_64-linux-gnu/libfyaml.so.0".into()],
        BuildStage::Libxmlb => vec!["out/build/libxmlb/install/usr/lib/x86_64-linux-gnu/libxmlb.so.2".into()],
        BuildStage::JsonGlib => vec!["out/build/json-glib/install/usr/lib/x86_64-linux-gnu/libjson-glib-1.0.so.0".into()],
        BuildStage::Appstream => vec!["out/build/appstream/install/usr/lib/x86_64-linux-gnu/libappstream.so.5".into()],
        BuildStage::GdkPixbuf => vec!["out/build/gdk-pixbuf/install/usr/lib/x86_64-linux-gnu/libgdk_pixbuf-2.0.so.0".into()],
        BuildStage::Gpgme => vec!["out/build/gpgme/install/usr/lib/x86_64-linux-gnu/libgpgme.so.45".into()],
        BuildStage::Ostree => vec!["out/build/ostree/install/usr/lib/x86_64-linux-gnu/libostree-1.so.1".into()],
        BuildStage::CosmicPortal => {
            vec!["out/build/cosmic-portal/install/usr/libexec/xdg-desktop-portal-cosmic".into()]
        }
        BuildStage::CosmicAssets => {
            vec![
                "out/build/cosmic-assets/install/usr/share/icons/Cosmic/index.theme".into(),
                "out/build/cosmic-assets/install/usr/share/cosmic/com.system76.CosmicPanel/v1/entries".into(),
            ]
        }
        BuildStage::Greetd => vec!["out/build/greetd/install/usr/bin/greetd".into()],
        BuildStage::CosmicDesktop => vec![
            "out/build/cosmic-desktop/install/usr/bin/cosmic-session".into(),
            "out/build/cosmic-desktop/install/usr/bin/cosmic-panel".into(),
            "out/build/cosmic-desktop/install/usr/bin/cosmic-term".into(),
            "out/build/cosmic-desktop/install/usr/bin/greetd".into(),
        ],
        BuildStage::Cozy => vec!["out/build/cozy/install/usr/bin/cozy".into()],
        BuildStage::Python => vec!["out/build/cpython/install".into()],
        BuildStage::Llvm => vec!["out/build/llvm/install".into()],
        BuildStage::Rust => vec!["out/build/rust/install".into()],
        BuildStage::SudoRs => vec!["out/build/sudo-rs/cargo-target/release/sudo".into()],
        BuildStage::Init => vec!["target/release/mattos-init".into()],
        BuildStage::Installer => vec![
            "out/build/installer/cargo-target/release/mattos-install".into(),
            "out/build/installer/cosmic-target/release/mattos-install-cosmic".into(),
            "out/build/btrfs-progs/install/usr/bin/btrfs".into(),
            "out/build/btrfs-progs/install/usr/include/btrfsutil.h".into(),
            "out/build/btrfs-progs/install/usr/lib/x86_64-linux-gnu/libbtrfsutil.so".into(),
            "out/build/btrfs-progs/install/usr/lib/x86_64-linux-gnu/pkgconfig/libbtrfsutil.pc".into(),
            "out/build/dosfstools/install/usr/sbin/mkfs.fat".into(),
            "out/build/e2fsprogs/install/usr/sbin/mkfs.ext4".into(),
            "out/build/installed-initramfs.cpio.xz".into(),
            "out/build/installer/BOOTX64.EFI".into(),
        ],
        BuildStage::LiveRoot => vec![
            LIVE_ROOT_IMAGE_PATH.into(),
            "out/reports/live-root-inventory.tsv".into(),
        ],
        BuildStage::Initramfs => vec![INITRAMFS_ARCHIVE_PATH.into()],
        BuildStage::Iso => vec![
            "out/build/iso".into(),
            "out/images/mattos-x86_64.iso".into(),
            "out/reports/live-image-inventory.tsv".into(),
            "out/reports/artifacts.tsv".into(),
        ],
        BuildStage::Rootfs => vec!["out/build/rootfs".into()],
        _ => vec![format!("out/build/{}/install", stage_output_directory(stage)).into()],
    };
    performance::StageSpec {
        id: id.to_string(),
        source_inputs: sources,
        configuration_inputs: stage_inputs::configuration_inputs(stage),
        tools: stage_inputs::tool_names(stage),
        dependencies: build_stage_dependencies(stage)
            .iter()
            .map(|value| value.to_string())
            .collect(),
        outputs,
        recipe: format!(
            "mattos-build-stage:{id}:recipe={}:schema={}",
            stage_inputs::recipe_revision(stage),
            performance::STAGE_MANIFEST_SCHEMA_VERSION
        ),
    }
}

fn linux_x86_uapi_inputs() -> Vec<&'static str> {
    stage_inputs::linux_x86_uapi_inputs()
}

fn stage_output_directory(stage: BuildStage) -> &'static str {
    match stage {
        BuildStage::GccToolchain => "gcc-toolchain",
        BuildStage::Procps => "procps-ng",
        BuildStage::Iputils => "iputils",
        BuildStage::Pam => "linux-pam",
        _ => build_stage_id(stage),
    }
}

fn build_stage_dependencies(stage: BuildStage) -> &'static [&'static str] {
    stage_graph::direct_dependencies(stage)
}

fn linux_headers_stage_spec() -> performance::StageSpec {
    performance::StageSpec {
        id: "linux-headers".to_string(),
        source_inputs: linux_x86_uapi_inputs()
            .into_iter()
            .map(PathBuf::from)
            .collect(),
        configuration_inputs: Vec::new(),
        tools: vec!["make".to_string(), "gcc".to_string()],
        dependencies: vec!["glibc".to_string()],
        outputs: vec![
            "out/build/glibc/linux-headers".into(),
            "out/build/glibc/linux-headers-inventory.txt".into(),
        ],
        recipe: "make ARCH=x86 headers_install".to_string(),
    }
}

fn formal_sysroot_stage_spec() -> performance::StageSpec {
    performance::StageSpec {
        id: "formal-sysroot".to_string(),
        source_inputs: Vec::new(),
        configuration_inputs: Vec::new(),
        tools: vec!["gcc".to_string(), "ld".to_string()],
        dependencies: vec![
            "linux-headers".to_string(),
            "glibc".to_string(),
            "gcc-runtime".to_string(),
        ],
        outputs: vec![
            "out/sysroot/usr/include/stdio.h".into(),
            "out/sysroot/usr/include/linux/version.h".into(),
            "out/sysroot/usr/lib/x86_64-linux-gnu/libc.so.6".into(),
            "out/sysroot/lib64/ld-linux-x86-64.so.2".into(),
            "out/sysroot/usr/lib/x86_64-linux-gnu/libgcc_s.so.1".into(),
            "out/sysroot/usr/lib/x86_64-linux-gnu/libstdc++.so.6.0.34".into(),
        ],
        recipe: "formal MattOS sysroot inventory".to_string(),
    }
}

fn validate_cached_build_stage(repo_root: &Path, stage: BuildStage) -> Result<()> {
    match stage {
        BuildStage::Kernel => {
            if !repo_root
                .join("out/build/linux/build/arch/x86/boot/bzImage")
                .is_file()
            {
                bail!("cached Linux image is missing")
            }
        }
        BuildStage::Glibc => {
            for path in [
                "out/sysroot/usr/include/stdio.h",
                "out/sysroot/usr/lib/x86_64-linux-gnu/libc.so.6",
                "out/sysroot/lib64/ld-linux-x86-64.so.2",
            ] {
                if !repo_root.join(path).exists() {
                    bail!("cached glibc/sysroot output is missing: {path}")
                }
            }
        }
        BuildStage::GccRuntime => {
            if !repo_root
                .join("out/sysroot/usr/lib/x86_64-linux-gnu/libgcc_s.so.1")
                .is_file()
            {
                bail!("cached GCC runtime is missing")
            }
        }
        BuildStage::Rust => validate_cached_rust_install(repo_root)?,
        BuildStage::Binutils => {
            for tool in ["as", "ld", "readelf", "strip"] {
                if !repo_root
                    .join("out/build/binutils/install/usr/bin")
                    .join(tool)
                    .is_file()
                {
                    bail!("cached native Binutils tool is missing: {tool}")
                }
            }
        }
        BuildStage::GccToolchain => {
            for tool in ["gcc", "g++"] {
                if !repo_root
                    .join("out/build/gcc-toolchain/install/usr/bin")
                    .join(tool)
                    .is_file()
                {
                    bail!("cached native compiler is missing: {tool}")
                }
            }
        }
        BuildStage::Make => {
            if !repo_root
                .join("out/build/make/install/usr/bin/make")
                .is_file()
            {
                bail!("cached native GNU Make is missing")
            }
        }
        _ => {}
    }
    Ok(())
}

fn validate_cached_rust_install(repo_root: &Path) -> Result<()> {
    let install = repo_root.join("out/build/rust/install/usr");
    let rustc = install.join("bin/rustc");
    let cargo = install.join("bin/cargo");
    if !rustc.is_file() || !cargo.is_file() {
        bail!("cached Rust installation is missing rustc or Cargo")
    }
    let rustc_path = path_str(&rustc)?;
    let sysroot = run_cmd_capture(&install, rustc_path, &["--print", "sysroot"])?;
    let reported_sysroot = PathBuf::from(sysroot.trim());
    let expected_sysroot = install.clone();
    let canonical_reported = reported_sysroot.canonicalize().with_context(|| {
        format!(
            "published rustc reported missing sysroot {}",
            reported_sysroot.display()
        )
    })?;
    let canonical_expected = expected_sysroot.canonicalize()?;
    if canonical_reported != canonical_expected {
        bail!(
            "published rustc/sysroot mismatch: rustc reports {}, expected {}",
            canonical_reported.display(),
            canonical_expected.display()
        )
    }
    let target_libdir = run_cmd_capture(&install, rustc_path, &["--print", "target-libdir"])?;
    let target_libdir = PathBuf::from(target_libdir.trim());
    if !target_libdir.is_dir() || !target_libdir.starts_with(&install) {
        bail!(
            "published rustc target library directory is outside its install: {}",
            target_libdir.display()
        )
    }
    if fs::read_dir(&target_libdir)?
        .filter_map(Result::ok)
        .all(|entry| {
            !entry
                .path()
                .extension()
                .is_some_and(|extension| extension == "rlib" || extension == "rmeta")
        })
    {
        bail!("published Rust target library directory has no compiler sysroot artifacts")
    }
    Ok(())
}

fn build_plan(stage: BuildStage) -> Vec<BuildStage> {
    stage_graph::build_plan(stage)
}

fn cacheable_stage_specs(repo_root: &Path) -> Result<Vec<performance::StageSpec>> {
    let mut specs = build_plan(BuildStage::All)
        .into_iter()
        .filter(|stage| {
            is_cacheable_stage(*stage)
                || matches!(
                    stage,
                    BuildStage::Rootfs
                        | BuildStage::LiveRoot
                        | BuildStage::Initramfs
                        | BuildStage::Iso
                )
        })
        .map(build_stage_spec)
        .collect::<Vec<_>>();
    specs.push(linux_headers_stage_spec());
    specs.push(formal_sysroot_stage_spec());
    if let Ok(repository) = packaging::repository_stage_spec(repo_root) {
        specs.push(repository);
    }
    specs.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(specs)
}

fn build_stage(repo_root: &Path, stage: BuildStage) -> Result<()> {
    performance::trace_log_context("build_stage-entry");
    match stage {
        BuildStage::Kernel => build_kernel(repo_root),
        BuildStage::Glibc => build_glibc(repo_root),
        BuildStage::GccRuntime => build_gcc_runtime(repo_root),
        BuildStage::Binutils => build_binutils(repo_root),
        BuildStage::GccToolchain => {
            performance::trace_log_context("build_stage-before-gcc-toolchain-dispatch");
            build_gcc_toolchain(repo_root)
        }
        BuildStage::Make => build_make(repo_root),
        BuildStage::Brush => build_brush(repo_root),
        BuildStage::Coreutils => build_coreutils(repo_root),
        BuildStage::Grep => build_grep(repo_root),
        BuildStage::Sed => build_sed(repo_root),
        BuildStage::Findutils => build_findutils(repo_root),
        BuildStage::Diffutils => build_diffutils(repo_root),
        BuildStage::Gzip => build_gzip(repo_root),
        BuildStage::Patch => build_patch(repo_root),
        BuildStage::File => build_file(repo_root),
        BuildStage::Less => build_less(repo_root),
        BuildStage::Git => build_git(repo_root),
        BuildStage::Openssh => build_openssh(repo_root),
        BuildStage::Libffi => build_libffi(repo_root),
        BuildStage::Wayland => build_wayland(repo_root),
        BuildStage::Xkbcommon => build_xkbcommon(repo_root),
        BuildStage::Libseat => build_libseat(repo_root),
        BuildStage::LibdisplayInfo => build_libdisplay_info(repo_root),
        BuildStage::Libevdev => build_libevdev(repo_root),
        BuildStage::Libinput => build_libinput(repo_root),
        BuildStage::Pixman => build_pixman(repo_root),
        BuildStage::Libdrm => build_libdrm(repo_root),
        BuildStage::VulkanHeaders => build_vulkan_headers(repo_root),
        BuildStage::VulkanLoader => build_vulkan_loader(repo_root),
        BuildStage::VulkanTools => build_vulkan_tools(repo_root),
        BuildStage::X11Compat => build_x11_compat(repo_root),
        BuildStage::Libepoxy => build_libepoxy(repo_root),
        BuildStage::Freetype => build_freetype(repo_root),
        BuildStage::Libfontenc => build_libfontenc(repo_root),
        BuildStage::Libxfont => build_libxfont(repo_root),
        BuildStage::Libxcvt => build_libxcvt(repo_root),
        BuildStage::Libxshmfence => build_libxshmfence(repo_root),
        BuildStage::Libxkbfile => build_libxkbfile(repo_root),
        BuildStage::Xkbcomp => build_xkbcomp(repo_root),
        BuildStage::Libglvnd => build_libglvnd(repo_root),
        BuildStage::Mesa => build_mesa(repo_root),
        BuildStage::Xwayland => build_xwayland(repo_root),
        BuildStage::NvidiaDriver => build_nvidia_driver(repo_root),
        BuildStage::Flatpak => build_flatpak(repo_root),
        BuildStage::Bubblewrap => build_bubblewrap(repo_root),
        BuildStage::XdgDbusProxy => build_xdg_dbus_proxy(repo_root),
        BuildStage::Gstreamer => build_gstreamer(repo_root),
        BuildStage::GstreamerBase => build_gstreamer_base(repo_root),
        BuildStage::XdgDesktopPortal => build_xdg_desktop_portal(repo_root),
        BuildStage::Libarchive => build_libarchive(repo_root),
        BuildStage::Libxml2 => build_libxml2(repo_root),
        BuildStage::Libpng => build_libpng(repo_root),
        BuildStage::Fuse3 => build_fuse3(repo_root),
        BuildStage::Libfyaml => build_libfyaml(repo_root),
        BuildStage::Libxmlb => build_libxmlb(repo_root),
        BuildStage::JsonGlib => build_json_glib(repo_root),
        BuildStage::Appstream => build_appstream(repo_root),
        BuildStage::GdkPixbuf => build_gdk_pixbuf(repo_root),
        BuildStage::Gpgme => build_gpgme(repo_root),
        BuildStage::Ostree => build_ostree(repo_root),
        BuildStage::CosmicComp => build_cosmic_comp(repo_root),
        BuildStage::CosmicSession
        | BuildStage::CosmicGreeter
        | BuildStage::CosmicPanel
        | BuildStage::CosmicApplets
        | BuildStage::CosmicAppLibrary
        | BuildStage::CosmicLauncher
        | BuildStage::CosmicSettings
        | BuildStage::CosmicSettingsDaemon
        | BuildStage::CosmicNotifications
        | BuildStage::CosmicOsd
        | BuildStage::CosmicBg
        | BuildStage::CosmicWorkspaces
        | BuildStage::CosmicFiles
        | BuildStage::CosmicTerm
        | BuildStage::CosmicTweaks
        | BuildStage::CosmicUtilities
        | BuildStage::CosmicRandr
        | BuildStage::CosmicScreenshot
        | BuildStage::PopLauncher
        | BuildStage::CosmicCalculator
        | BuildStage::CosmicStorage
        | BuildStage::CosmicMonitor
        | BuildStage::CosmicStore
        | BuildStage::CosmicPortal
        | BuildStage::CosmicAssets
        | BuildStage::Greetd => build_cosmic_desktop_component(repo_root, stage),
        BuildStage::CosmicEdit => build_cosmic_edit(repo_root),
        BuildStage::CosmicInitialSetup => build_cosmic_initial_setup(repo_root),
        BuildStage::CosmicDesktop => build_cosmic_desktop(repo_root),
        BuildStage::Cozy => build_cozy(repo_root),
        BuildStage::Python => build_cpython(repo_root),
        BuildStage::Llvm => build_llvm(repo_root),
        BuildStage::Rust => build_rust(repo_root),
        BuildStage::Kmod => build_kmod(repo_root),
        BuildStage::Procps => build_procps(repo_root),
        BuildStage::Ncurses => build_ncurses(repo_root),
        BuildStage::Iproute2 => build_iproute2(repo_root),
        BuildStage::Iputils => build_iputils(repo_root),
        BuildStage::Curl => build_curl(repo_root),
        BuildStage::Expat => build_expat(repo_root),
        BuildStage::Libcap => build_libcap(repo_root),
        BuildStage::Attr => build_attr(repo_root),
        BuildStage::Tar => build_tar(repo_root),
        BuildStage::Acl => build_acl(repo_root),
        BuildStage::Zlib => build_zlib(repo_root),
        BuildStage::Bzip2 => build_bzip2(repo_root),
        BuildStage::Lz4 => build_lz4(repo_root),
        BuildStage::Xz => build_xz(repo_root),
        BuildStage::Xxhash => build_xxhash(repo_root),
        BuildStage::Zstd => build_zstd(repo_root),
        BuildStage::Dav1d => build_dav1d(repo_root),
        BuildStage::Glib => build_glib(repo_root),
        BuildStage::Pipewire => build_pipewire(repo_root),
        BuildStage::Openssl => build_openssl(repo_root),
        BuildStage::Elfutils => build_elfutils(repo_root),
        BuildStage::Pcre2 => build_pcre2(repo_root),
        BuildStage::Selinux => build_selinux(repo_root),
        BuildStage::Libxcrypt => build_libxcrypt(repo_root),
        BuildStage::Libmd => build_libmd(repo_root),
        BuildStage::Libbsd => build_libbsd(repo_root),
        BuildStage::Libndp => build_libndp(repo_root),
        BuildStage::Readline => build_readline(repo_root),
        BuildStage::Pam => build_linux_pam(repo_root),
        BuildStage::Shadow => build_shadow(repo_root),
        BuildStage::SudoRs => build_sudo_rs(repo_root),
        BuildStage::UtilLinux => build_util_linux(repo_root),
        BuildStage::Systemd => build_systemd(repo_root),
        BuildStage::Dbus => build_dbus(repo_root),
        BuildStage::DbusBroker => build_dbus_broker(repo_root),
        BuildStage::Dpkg => packaging::build_dpkg(repo_root),
        BuildStage::LibgpgError => {
            build_gpg_autotools_library(repo_root, "libgpg-error", &[], "libgpg-error.so.0")
        }
        BuildStage::Libgcrypt => build_gpg_autotools_library(
            repo_root,
            "libgcrypt",
            &["libgpg-error"],
            "libgcrypt.so.20",
        ),
        BuildStage::Libassuan => {
            build_gpg_autotools_library(repo_root, "libassuan", &["libgpg-error"], "libassuan.so.9")
        }
        BuildStage::Libksba => {
            build_gpg_autotools_library(repo_root, "libksba", &["libgpg-error"], "libksba.so.8")
        }
        BuildStage::Npth => build_gpg_autotools_library(repo_root, "npth", &[], "libnpth.so.0"),
        BuildStage::Gpgv => build_gpgv(repo_root),
        BuildStage::Polkit => build_polkit(repo_root),
        BuildStage::Duktape => build_duktape(repo_root),
        BuildStage::NetworkManager => build_networkmanager(repo_root),
        BuildStage::Apt => packaging::build_apt(repo_root),
        BuildStage::Init => build_init(repo_root),
        BuildStage::Installer => build_installer(repo_root),
        BuildStage::Rootfs => build_rootfs(repo_root),
        BuildStage::LiveRoot => build_live_root(repo_root),
        BuildStage::Initramfs => build_initramfs(repo_root),
        BuildStage::Iso => build_iso(repo_root),
        BuildStage::All => {
            bail!("internal error: BuildStage::All should be expanded by build_plan")
        }
    }
}

#[derive(Debug, Deserialize)]
struct KernelConfigPolicy {
    minimum_module_symbols: usize,
    builtin: Vec<String>,
    module: Vec<String>,
    unsupported: Vec<String>,
    unsupported_prefixes: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum KernelConfigState {
    Builtin,
    Module,
    Unsupported,
}
