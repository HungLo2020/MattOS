fn build_libseat(repo_root: &Path) -> Result<()> {
    build_meson_runtime(
        repo_root,
        "seatd",
        "src/system/libraries/seatd",
        &["systemd"],
        &[
            "--prefix=/usr",
            "--libdir=lib/x86_64-linux-gnu",
            "-Dserver=disabled",
            "-Dlibseat-seatd=disabled",
            "-Dlibseat-logind=systemd",
            "-Dlibseat-builtin=enabled",
            "-Dexamples=disabled",
            "-Dman-pages=disabled",
        ],
        "usr/lib/x86_64-linux-gnu/libseat.so.1",
        &[],
    )
}

fn rewrite_staged_pkgconfig_files(install_dir: &Path) -> Result<()> {
    fn visit(path: &Path, prefix: &Path) -> Result<()> {
        if !path.is_dir() {
            return Ok(());
        }
        let mut entries = fs::read_dir(path)?.collect::<std::io::Result<Vec<_>>>()?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path)?;
            if metadata.is_dir() {
                visit(&path, prefix)?;
            } else if metadata.is_file() && path.extension().and_then(OsStr::to_str) == Some("pc") {
                let contents = fs::read_to_string(&path)?;
                let rewritten = rewrite_pkgconfig_for_staged_consumer(&contents, prefix);
                fs::write(&path, rewritten)?;
            }
        }
        Ok(())
    }
    visit(install_dir, &install_dir.join("usr"))
}

fn rewrite_pkgconfig_for_staged_consumer(contents: &str, prefix: &Path) -> String {
    contents
        .lines()
        .map(|line| {
            if let Some(value) = line.strip_prefix("prefix=/usr") {
                format!("prefix={}{}", prefix.display(), value)
            } else if let Some(value) = line.strip_prefix("libdir=/usr") {
                format!("libdir=${{prefix}}{}", value)
            } else if let Some(value) = line.strip_prefix("includedir=/usr") {
                format!("includedir=${{prefix}}{}", value)
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

fn remove_staged_libtool_archives(path: &Path) -> Result<()> {
    if !path.is_dir() {
        return Ok(());
    }
    let mut entries = fs::read_dir(path)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.is_dir() {
            remove_staged_libtool_archives(&path)?;
        } else if metadata.is_file() && path.extension().and_then(OsStr::to_str) == Some("la") {
            fs::remove_file(path)?;
        }
    }
    Ok(())
}

fn build_xorg_autotools_component(
    repo_root: &Path,
    component: &str,
    dependencies: &[&str],
    options: &[&str],
    required_outputs: &[&str],
) -> Result<()> {
    let source = repo_root.join("src/system/graphics").join(component);
    let out_root = repo_root.join("out/build").join(component);
    let source_copy = out_root.join("source");
    let build_dir = out_root.join("build");
    let install_dir = out_root.join("install");
    let stamp_path = out_root.join("build-stamp.txt");
    let state = fs::read_to_string(
        repo_root
            .join("upstream/state")
            .join(format!("{component}.toml")),
    )?;
    let stamp = format!(
        "{state}\n{}\ndependencies={}\nxorg-compat-recipe=2\n",
        options.join("\n"),
        dependencies.join(",")
    );
    if fs::read_to_string(&stamp_path).ok().as_deref() != Some(stamp.as_str()) {
        remove_path_if_exists(&source_copy)?;
        remove_path_if_exists(&build_dir)?;
        remove_path_if_exists(&install_dir)?;
    }
    fs::create_dir_all(&out_root)?;
    sync_build_source(&source, &source_copy)?;
    let mut env = staged_library_environment(repo_root, dependencies)?;
    let aclocal = repo_root.join("out/build/xorg-util-macros/install/usr/share/aclocal");
    if aclocal.is_dir() {
        env.push(("ACLOCAL_PATH", aclocal.display().to_string()));
    }
    if !source_copy.join("configure").is_file() {
        run_cmd_with_env_overrides(&source_copy, "autoreconf", &["-fiv"], &env)?;
    }
    fs::create_dir_all(&build_dir)?;
    if !build_dir.join("Makefile").is_file() {
        run_cmd_with_env_overrides(
            &build_dir,
            path_str(&source_copy.join("configure"))?,
            options,
            &env,
        )?;
    }
    let jobs = scheduler::child_job_limit().max(1).to_string();
    run_cmd_with_env_overrides(&build_dir, "make", &["-j", &jobs], &env)?;
    remove_path_if_exists(&install_dir)?;
    run_cmd_with_env_overrides(
        &build_dir,
        "make",
        &["install", &format!("DESTDIR={}", install_dir.display())],
        &env,
    )?;
    rewrite_staged_pkgconfig_files(&install_dir)?;
    // Libtool archives encode build-time absolute paths and make later Xorg
    // components chase target /usr paths on the host. Shared-library and
    // pkg-config metadata are the canonical output of this runtime closure.
    remove_staged_libtool_archives(&install_dir)?;
    for relative in required_outputs {
        if !install_dir.join(relative).is_file() {
            bail!("{component} install did not produce {relative}");
        }
    }
    fs::write(stamp_path, stamp)?;
    Ok(())
}

fn build_x11_compat(repo_root: &Path) -> Result<()> {
    let common = [
        "--prefix=/usr",
        "--libdir=/usr/lib/x86_64-linux-gnu",
        "--disable-static",
    ];
    build_xorg_autotools_component(repo_root, "xorg-util-macros", &[], &["--prefix=/usr"], &[])?;
    build_meson_runtime(
        repo_root,
        "xorgproto",
        "src/system/graphics/xorgproto",
        &[],
        &[
            "--prefix=/usr",
            "--libdir=lib/x86_64-linux-gnu",
            "-Dlegacy=true",
        ],
        "usr/include/X11/X.h",
        &[],
    )?;
    rewrite_staged_pkgconfig_files(&repo_root.join("out/build/xorgproto/install"))?;
    build_xorg_autotools_component(
        repo_root,
        "xtrans",
        &["xorg-util-macros", "xorgproto"],
        &common,
        &[],
    )?;
    build_xorg_autotools_component(
        repo_root,
        "libxau",
        &["xorg-util-macros", "xorgproto"],
        &common,
        &["usr/lib/x86_64-linux-gnu/libXau.so.6"],
    )?;
    build_xorg_autotools_component(
        repo_root,
        "libxdmcp",
        &["xorg-util-macros", "xorgproto"],
        &common,
        &["usr/lib/x86_64-linux-gnu/libXdmcp.so.6"],
    )?;
    build_xorg_autotools_component(
        repo_root,
        "xcb-proto",
        &["xorg-util-macros", "cpython"],
        &["--prefix=/usr"],
        &["usr/share/xcb/xproto.xml"],
    )?;
    build_xorg_autotools_component(
        repo_root,
        "libxcb",
        &[
            "xorg-util-macros",
            "xorgproto",
            "libxau",
            "libxdmcp",
            "xcb-proto",
            "cpython",
            "expat",
        ],
        &common,
        &["usr/lib/x86_64-linux-gnu/libxcb.so.1"],
    )?;
    build_xorg_autotools_component(
        repo_root,
        "libx11",
        &[
            "xorg-util-macros",
            "xorgproto",
            "xtrans",
            "libxau",
            "libxdmcp",
            "libxcb",
        ],
        &common,
        &["usr/lib/x86_64-linux-gnu/libX11.so.6"],
    )?;
    build_xorg_autotools_component(
        repo_root,
        "libxext",
        &[
            "xorg-util-macros",
            "xorgproto",
            "libxau",
            "libxdmcp",
            "libxcb",
            "libx11",
        ],
        &common,
        &["usr/lib/x86_64-linux-gnu/libXext.so.6"],
    )?;

    let aggregate = repo_root.join("out/build/x11-compat/install");
    remove_path_if_exists(&aggregate)?;
    // Later target-native consumers such as Xwayland need the full public
    // X.Org development contract (headers, .pc metadata, xtrans, and the
    // utility macros), not merely the five runtime libraries. Publish the
    // aggregate from the already-built component outputs so downstream
    // stages depend on one coherent, cache-tracked X11 compatibility stage.
    for component in [
        "xorg-util-macros",
        "xorgproto",
        "xtrans",
        "libxau",
        "libxdmcp",
        "xcb-proto",
        "libxcb",
        "libx11",
        "libxext",
    ] {
        copy_tree_contents(
            &repo_root.join("out/build").join(component).join("install"),
            &aggregate,
        )?;
    }
    for relative in [
        "usr/lib/x86_64-linux-gnu/libX11.so.6",
        "usr/lib/x86_64-linux-gnu/libXext.so.6",
        "usr/lib/x86_64-linux-gnu/libxcb.so.1",
    ] {
        if !aggregate.join(relative).exists() {
            bail!("X11 compatibility runtime did not produce {relative}");
        }
    }
    Ok(())
}

fn build_libepoxy(repo_root: &Path) -> Result<()> {
    build_meson_runtime(
        repo_root,
        "libepoxy",
        "src/system/graphics/libepoxy",
        &["x11-compat", "libglvnd"],
        &[
            "--prefix=/usr",
            "--libdir=lib/x86_64-linux-gnu",
            "-Ddocs=false",
            "-Dtests=false",
            "-Dglx=yes",
            "-Degl=yes",
        ],
        "usr/lib/x86_64-linux-gnu/libepoxy.so.0",
        &[],
    )
}

fn build_freetype(repo_root: &Path) -> Result<()> {
    build_meson_runtime(
        repo_root,
        "freetype",
        "src/system/libraries/freetype",
        &["zlib"],
        &[
            "--prefix=/usr",
            "--libdir=lib/x86_64-linux-gnu",
            "-Dbzip2=disabled",
            "-Dpng=disabled",
            "-Dharfbuzz=disabled",
            "-Dbrotli=disabled",
            "-Dtests=disabled",
        ],
        "usr/lib/x86_64-linux-gnu/libfreetype.so.6",
        &[],
    )
}

fn build_libfontenc(repo_root: &Path) -> Result<()> {
    build_xorg_autotools_component(
        repo_root,
        "libfontenc",
        &["xorg-util-macros", "xorgproto"],
        &[
            "--prefix=/usr",
            "--libdir=/usr/lib/x86_64-linux-gnu",
            "--disable-static",
        ],
        &["usr/lib/x86_64-linux-gnu/libfontenc.so.1"],
    )
}

fn build_libxfont(repo_root: &Path) -> Result<()> {
    build_xorg_autotools_component(
        repo_root,
        "libxfont",
        &[
            "xorg-util-macros",
            "xorgproto",
            "xtrans",
            "freetype",
            "libfontenc",
            "zlib",
        ],
        &[
            "--prefix=/usr",
            "--libdir=/usr/lib/x86_64-linux-gnu",
            "--disable-static",
        ],
        &["usr/lib/x86_64-linux-gnu/libXfont2.so.2"],
    )
}

fn build_libxcvt(repo_root: &Path) -> Result<()> {
    build_meson_runtime(
        repo_root,
        "libxcvt",
        "src/system/graphics/libxcvt",
        &["x11-compat"],
        &["--prefix=/usr", "--libdir=lib/x86_64-linux-gnu"],
        "usr/lib/x86_64-linux-gnu/libxcvt.so.0",
        &[],
    )
}

fn build_libxshmfence(repo_root: &Path) -> Result<()> {
    build_xorg_autotools_component(
        repo_root,
        "libxshmfence",
        &["x11-compat"],
        &[
            "--prefix=/usr",
            "--libdir=/usr/lib/x86_64-linux-gnu",
            "--disable-static",
        ],
        &["usr/lib/x86_64-linux-gnu/libxshmfence.so.1"],
    )
}

fn build_libxkbfile(repo_root: &Path) -> Result<()> {
    build_meson_runtime(
        repo_root,
        "libxkbfile",
        "src/system/graphics/libxkbfile",
        &["x11-compat"],
        &["--prefix=/usr", "--libdir=lib/x86_64-linux-gnu"],
        "usr/lib/x86_64-linux-gnu/libxkbfile.so.1",
        &[],
    )
}

fn build_xkbcomp(repo_root: &Path) -> Result<()> {
    // Xwayland invokes this target-owned helper at runtime to compile its
    // initial keyboard map.  The keyboard layouts themselves remain in the
    // separately pinned xkeyboard-config/xkb-data package.
    build_meson_runtime(
        repo_root,
        "xkbcomp",
        "src/system/graphics/xkbcomp",
        &["x11-compat", "libxkbfile"],
        &[
            "--prefix=/usr",
            "--libdir=lib/x86_64-linux-gnu",
            "-Dxkb-config-root=/usr/share/X11/xkb",
        ],
        "usr/bin/xkbcomp",
        &[],
    )
}

fn build_xwayland(repo_root: &Path) -> Result<()> {
    build_meson_runtime(
        repo_root,
        "xwayland",
        "src/system/graphics/xwayland",
        &[
            "x11-compat",
            "pixman",
            "wayland",
            "libffi",
            "xkbcommon",
            "libxkbfile",
            "libxfont",
            "libfontenc",
            "freetype",
            "zlib",
            "libxcvt",
            "libxshmfence",
            "libepoxy",
            "libdrm",
            "libglvnd",
            "mesa",
        ],
        &[
            "--prefix=/usr",
            "--libdir=lib/x86_64-linux-gnu",
            "-Dxvfb=false",
            "-Dglamor=true",
            "-Dglx=true",
            "-Dlibdecor=false",
            "-Dxwayland_ei=false",
            "-Dsystemd_notify=false",
            // Xwayland does not need legacy Secure RPC; disabling it avoids
            // introducing a target libtirpc dependency solely for Xorg-era
            // authentication support.
            "-Dsecure-rpc=false",
            "-Ddocs=false",
            "-Ddevel-docs=false",
        ],
        "usr/bin/Xwayland",
        &[],
    )
}

fn build_bubblewrap(repo_root: &Path) -> Result<()> {
    build_meson_runtime(
        repo_root,
        "bubblewrap",
        "src/system/security/bubblewrap",
        &["libcap"],
        &[
            "--prefix=/usr",
            "--libdir=lib/x86_64-linux-gnu",
            "-Dtests=false",
            "-Dman=disabled",
            "-Dbash_completion=disabled",
            "-Dzsh_completion=disabled",
            "-Dselinux=disabled",
        ],
        "usr/bin/bwrap",
        &[],
    )
}

fn build_xdg_dbus_proxy(repo_root: &Path) -> Result<()> {
    build_meson_runtime(
        repo_root,
        "xdg-dbus-proxy",
        "src/system/packages/xdg-dbus-proxy",
        // xdg-dbus-proxy links through GLib, whose published target ABI
        // requires PCRE2 at the link step.
        &["glib", "libffi", "zlib", "pcre2"],
        &[
            "--prefix=/usr",
            "--libdir=lib/x86_64-linux-gnu",
            "-Dtests=false",
            "-Dman=disabled",
            "--wrap-mode=nofallback",
        ],
        "usr/bin/xdg-dbus-proxy",
        &[],
    )
}

fn build_gstreamer(repo_root: &Path) -> Result<()> {
    build_meson_runtime(
        repo_root,
        "gstreamer",
        "src/system/multimedia/gstreamer/subprojects/gstreamer",
        &["glib", "libffi", "zlib", "pcre2"],
        &[
            "--prefix=/usr",
            "--libdir=lib/x86_64-linux-gnu",
            "-Dtests=disabled",
            "-Dexamples=disabled",
            "-Dbenchmarks=disabled",
            "-Dtools=enabled",
            "-Dintrospection=disabled",
            "-Ddoc=disabled",
            "-Dnls=disabled",
            "-Dlibunwind=disabled",
            "-Dlibdw=disabled",
            "-Dcheck=disabled",
            "-Dbash-completion=disabled",
        ],
        "usr/lib/x86_64-linux-gnu/libgstreamer-1.0.so.0",
        &[],
    )
}

fn build_gstreamer_base(repo_root: &Path) -> Result<()> {
    build_meson_runtime(
        repo_root,
        "gstreamer-base",
        "src/system/multimedia/gstreamer/subprojects/gst-plugins-base",
        // gst-plugins-base links helper binaries through GLib, whose
        // published target ABI requires PCRE2 at the link step.
        &["glib", "libffi", "zlib", "pcre2", "gstreamer"],
        &[
            "--prefix=/usr",
            "--libdir=lib/x86_64-linux-gnu",
            "-Dtests=disabled",
            "-Dexamples=disabled",
            "-Dtools=disabled",
            "-Dintrospection=disabled",
            "-Ddoc=disabled",
            "-Dnls=disabled",
            "-Dorc=disabled",
            "-Ddrm=disabled",
            "-Dx11=disabled",
            "-Dgl=disabled",
            "-Dalsa=disabled",
            "-Dogg=disabled",
            "-Dopus=disabled",
            "-Dpango=disabled",
            "-Dtheora=disabled",
            "-Dvorbis=disabled",
        ],
        "usr/lib/x86_64-linux-gnu/libgstpbutils-1.0.so.0",
        &[],
    )
}

fn build_xdg_desktop_portal(repo_root: &Path) -> Result<()> {
    build_meson_runtime(
        repo_root,
        "xdg-desktop-portal",
        "src/system/packages/xdg-desktop-portal",
        &[
            "glib",
            "libffi",
            "zlib",
            "json-glib",
            "fuse3",
            "gdk-pixbuf",
            "libpng",
            "gstreamer",
            "gstreamer-base",
            "pipewire",
            "systemd",
            "dbus",
            "flatpak",
            "polkit",
            "ostree",
            "xz",
            "curl",
            "openssl",
            "gpgme",
            "libgpg-error",
            "libassuan",
            "libxml2",
            "zstd",
            "libarchive",
            "bubblewrap",
        ],
        &[
            "--prefix=/usr",
            "--libdir=lib/x86_64-linux-gnu",
            "--libexecdir=libexec",
            "-Dtests=disabled",
            "-Dinstalled-tests=false",
            "-Ddocumentation=disabled",
            "-Dman-pages=disabled",
            "-Dgeoclue=disabled",
            "-Dgudev=disabled",
            "-Dsystemd=enabled",
            "-Dflatpak-interfaces=enabled",
            "--wrap-mode=nofallback",
        ],
        "usr/libexec/xdg-desktop-portal",
        &[],
    )
}

fn build_libglvnd(repo_root: &Path) -> Result<()> {
    build_meson_runtime(
        repo_root,
        "libglvnd",
        "src/system/graphics/libglvnd",
        &["x11-compat"],
        &[
            "--prefix=/usr",
            "--libdir=lib/x86_64-linux-gnu",
            // Xwayland's GLX server needs the target-owned libGL dispatcher.
            // Keep this enabled rather than relying on a host libGL.pc.
            "-Dx11=enabled",
            "-Dglx=enabled",
            "-Degl=true",
            "-Dgles1=true",
            "-Dgles2=true",
            "-Dhgl=false",
        ],
        "usr/lib/x86_64-linux-gnu/libEGL.so.1",
        &[],
    )?;
    rewrite_staged_pkgconfig_files(&repo_root.join("out/build/libglvnd/install"))
}

fn nvidia_library_soname(path: &Path) -> Result<String> {
    let output = run_cmd_capture(
        path.parent().context("NVIDIA library has no parent")?,
        "readelf",
        &["-d", path_str(path)?],
    )?;
    output
        .lines()
        .find(|line| line.contains("(SONAME)"))
        .and_then(|line| line.split_once('['))
        .and_then(|(_, rest)| rest.split_once(']'))
        .map(|(soname, _)| soname.to_owned())
        .with_context(|| format!("NVIDIA library {} has no ELF SONAME", path.display()))
}

fn stage_nvidia_library(source: &Path, destination: &Path) -> Result<()> {
    let filename = source
        .file_name()
        .context("NVIDIA library has no filename")?;
    fs::create_dir_all(destination)?;
    let target = destination.join(filename);
    fs::copy(source, &target)?;
    let soname = nvidia_library_soname(source)?;
    let soname_path = destination.join(&soname);
    if soname_path != target {
        remove_path_if_exists(&soname_path)?;
        std::os::unix::fs::symlink(filename, soname_path)?;
    }
    Ok(())
}

fn render_nvidia_driver_selection(open_device_ids: &BTreeSet<u16>) -> (String, String) {
    let config = "# Generated from NVIDIA 595.84 supported-gpus.json.\n\
# Route both competing drivers through the release-matched hardware gate.\n\
install nvidia /usr/libexec/mattos-nvidia-select nvidia $CMDLINE_OPTS\n\
install nvidia_drm /usr/libexec/mattos-nvidia-select nvidia_drm $CMDLINE_OPTS\n\
install nvidia_modeset /usr/libexec/mattos-nvidia-select nvidia_modeset $CMDLINE_OPTS\n\
install nvidia_uvm /usr/libexec/mattos-nvidia-select nvidia_uvm $CMDLINE_OPTS\n\
install nvidia_peermem /usr/libexec/mattos-nvidia-select nvidia_peermem $CMDLINE_OPTS\n\
install nouveau /usr/libexec/mattos-nvidia-select nouveau $CMDLINE_OPTS\n"
        .to_string();
    let patterns = open_device_ids
        .iter()
        .map(|device| format!("0x{device:04x}"))
        .collect::<Vec<_>>()
        .join("|");
    let selector = format!(
        "#!/bin/sh\n\
set -eu\n\
module=$1\n\
shift\n\
supported=0\n\
devices=${{MATTOS_NVIDIA_SYSFS_ROOT:-/sys/bus/pci/devices}}\n\
for path in \"$devices\"/*; do\n\
    [ -d \"$path\" ] || continue\n\
    [ \"$(cat \"$path/vendor\" 2>/dev/null || true)\" = 0x10de ] || continue\n\
    device=$(tr 'A-F' 'a-f' < \"$path/device\" 2>/dev/null || true)\n\
    case \"$device\" in\n\
        {patterns}) supported=1; break ;;\n\
    esac\n\
done\n\
case \"$module\" in\n\
    nouveau) [ \"$supported\" -eq 0 ] || exit 1 ;;\n\
    nvidia*) [ \"$supported\" -eq 1 ] || exit 1 ;;\n\
    *) exit 2 ;;\n\
esac\n\
exec \"${{MATTOS_MODPROBE:-/usr/sbin/modprobe}}\" --ignore-install \"$module\" \"$@\"\n"
    );
    (config, selector)
}

fn build_nvidia_driver(repo_root: &Path) -> Result<()> {
    let manifest_path = repo_root.join("src/system/graphics/nvidia-driver/manifest.toml");
    let manifest_body = fs::read_to_string(&manifest_path)?;
    let manifest: NvidiaDriverManifest = toml::from_str(&manifest_body)?;
    if manifest.schema_version != 1
        || manifest.version != "595.84"
        || manifest.release_branch != "production"
        || manifest.architecture != "x86_64"
        || manifest.kernel_source_commit != "722ae84526a09ed672fbe75448e2909834ba4cce"
        || manifest.binary_policy != "verbatim-extraction-no-strip-no-patch"
        || !manifest.include_in_iso
    {
        bail!("NVIDIA driver manifest does not match MattOS's pinned production policy");
    }
    let out_root = repo_root.join("out/build/nvidia-driver");
    fs::create_dir_all(&out_root)?;
    let runfile = ensure_verified_release_archive(
        &out_root,
        &manifest.runfile,
        &manifest.url,
        &manifest.sha256,
    )?;
    let extracted = out_root.join("source");
    let extraction_stamp = out_root.join("extraction.stamp");
    if fs::read_to_string(&extraction_stamp).ok().as_deref() != Some(manifest.sha256.as_str())
        || !extracted.join("LICENSE").is_file()
    {
        remove_path_if_exists(&extracted)?;
        run_cmd(
            &out_root,
            "sh",
            &[
                path_str(&runfile)?,
                "--extract-only",
                "--target",
                path_str(&extracted)?,
            ],
        )?;
        fs::write(&extraction_stamp, &manifest.sha256)?;
    }
    let license_hash = performance::sha256_file(&extracted.join("LICENSE"))?;
    if license_hash != manifest.license_sha256 {
        bail!(
            "NVIDIA license checksum mismatch: expected {}, got {license_hash}",
            manifest.license_sha256
        );
    }

    let release = fs::read_to_string(repo_root.join("out/build/linux/kernel-release"))?
        .trim()
        .to_owned();
    let kernel_source = repo_root.join("out/build/linux/source");
    let kernel_output = repo_root.join("out/build/linux/build");
    if !kernel_output
        .join("include/config/kernel.release")
        .is_file()
    {
        bail!("NVIDIA modules require the prepared MattOS kernel output");
    }
    let open_source = repo_root.join("out/build/nvidia-driver/kernel-source");
    let open_stamp_path = out_root.join("kernel-source.stamp");
    let open_state =
        fs::read_to_string(repo_root.join("upstream/state/nvidia-open-gpu-kernel-modules.toml"))?;
    let open_stamp = format!("{open_state}\nkernel-release={release}\nrecipe=2\n");
    if fs::read_to_string(&open_stamp_path).ok().as_deref() != Some(open_stamp.as_str()) {
        remove_path_if_exists(&open_source)?;
        sync_build_source(
            &repo_root.join("src/system/graphics/nvidia-open-gpu-kernel-modules"),
            &open_source,
        )?;
        apply_component_patches(repo_root, "nvidia-open-gpu-kernel-modules", &open_source)?;
        fs::write(&open_stamp_path, &open_stamp)?;
    }
    let jobs = scheduler::child_job_limit().max(1).to_string();
    let sys_source = format!("SYSSRC={}", kernel_source.display());
    let sys_output = format!("SYSOUT={}", kernel_output.display());
    run_cmd(
        &open_source,
        "make",
        &[
            "modules",
            "-j",
            &jobs,
            &sys_source,
            &sys_output,
            // Linux 7.2's delayed final-link objtool pass cannot rewrite the
            // immutable precompiled NVIDIA core. Per-object objtool checking
            // remains enabled for every source-built open-module object.
            "delay-objtool=",
        ],
    )?;
    let raw_install = out_root.join("modules-install");
    remove_path_if_exists(&raw_install)?;
    let install_mod_path = format!("INSTALL_MOD_PATH={}", raw_install.display());
    run_cmd(
        &open_source,
        "make",
        &[
            "modules_install",
            &sys_source,
            &sys_output,
            &install_mod_path,
            "INSTALL_MOD_DIR=updates/nvidia",
            "DEPMOD=true",
            "delay-objtool=",
        ],
    )?;

    let install = out_root.join("install");
    remove_path_if_exists(&install)?;
    let raw_module_root = raw_install.join("lib/modules").join(&release);
    let module_root = install.join("usr/lib/modules").join(&release);
    copy_tree_contents(&raw_module_root, &module_root)?;
    for link in ["build", "source"] {
        remove_path_if_exists(&module_root.join(link))?;
    }
    let mut module_files = Vec::new();
    collect_regular_files(&module_root, &mut module_files)?;
    let mut module_count = 0usize;
    for module in module_files.into_iter().filter(|path| {
        path.extension().and_then(OsStr::to_str) == Some("ko")
            || path
                .file_name()
                .and_then(OsStr::to_str)
                .is_some_and(|name| name.ends_with(".ko.zst"))
    }) {
        let vermagic = run_cmd_capture(
            repo_root,
            "modinfo",
            &["-F", "vermagic", path_str(&module)?],
        )?;
        if !vermagic.starts_with(&release) {
            bail!(
                "{} has mismatched vermagic {}",
                module.display(),
                vermagic.trim()
            );
        }
        if module.extension().and_then(OsStr::to_str) == Some("ko") {
            let compressed = PathBuf::from(format!("{}.zst", module.display()));
            run_cmd(
                repo_root,
                "zstd",
                &[
                    "-q",
                    "-19",
                    "-T1",
                    "-f",
                    path_str(&module)?,
                    "-o",
                    path_str(&compressed)?,
                ],
            )?;
            remove_path_if_exists(&module)?;
        }
        module_count += 1;
    }
    if module_count != 5 {
        bail!("NVIDIA open module install produced {module_count} modules, expected 5");
    }
    run_cmd(
        repo_root,
        "depmod",
        &[
            "-b",
            path_str(&install)?,
            "-m",
            "/usr/lib/modules",
            &release,
        ],
    )?;

    let libdir = install.join("usr/lib/x86_64-linux-gnu");
    for filename in [
        "libEGL_nvidia.so.595.84",
        "libGLESv1_CM_nvidia.so.595.84",
        "libGLESv2_nvidia.so.595.84",
        "libGLX_nvidia.so.595.84",
        "libcuda.so.595.84",
        "libnvcuvid.so.595.84",
        "libnvidia-allocator.so.595.84",
        "libnvidia-egl-gbm.so.1.1.3",
        "libnvidia-egl-wayland.so.1.1.20",
        "libnvidia-egl-wayland2.so.1.0.1",
        "libnvidia-eglcore.so.595.84",
        "libnvidia-encode.so.595.84",
        "libnvidia-glcore.so.595.84",
        "libnvidia-glsi.so.595.84",
        "libnvidia-glvkspirv.so.595.84",
        "libnvidia-gpucomp.so.595.84",
        "libnvidia-ml.so.595.84",
        "libnvidia-present.so.595.84",
        "libnvidia-ptxjitcompiler.so.595.84",
        "libnvidia-tls.so.595.84",
    ] {
        stage_nvidia_library(&extracted.join(filename), &libdir)?;
    }
    for filename in ["nvidia-smi", "nvidia-modprobe", "nvidia-persistenced"] {
        let destination = install.join("usr/bin").join(filename);
        fs::create_dir_all(destination.parent().expect("NVIDIA binary parent"))?;
        fs::copy(extracted.join(filename), &destination)?;
        set_mode(
            destination,
            if filename == "nvidia-modprobe" {
                0o4755
            } else {
                0o755
            },
        )?;
    }
    for (source_name, destination_relative) in [
        (
            "10_nvidia.json",
            "usr/share/glvnd/egl_vendor.d/10_nvidia.json",
        ),
        ("nvidia_icd.json", "usr/share/vulkan/icd.d/nvidia_icd.json"),
        (
            "nvidia_layers.json",
            "usr/share/vulkan/implicit_layer.d/nvidia_layers.json",
        ),
        (
            "09_nvidia_wayland2.json",
            "usr/share/egl/egl_external_platform.d/09_nvidia_wayland2.json",
        ),
        (
            "10_nvidia_wayland.json",
            "usr/share/egl/egl_external_platform.d/10_nvidia_wayland.json",
        ),
        (
            "15_nvidia_gbm.json",
            "usr/share/egl/egl_external_platform.d/15_nvidia_gbm.json",
        ),
    ] {
        let destination = install.join(destination_relative);
        fs::create_dir_all(destination.parent().expect("NVIDIA metadata parent"))?;
        fs::copy(extracted.join(source_name), destination)?;
    }
    let firmware_dir = install.join("usr/lib/firmware/nvidia/595.84");
    fs::create_dir_all(&firmware_dir)?;
    for firmware in ["gsp_tu10x.bin", "gsp_ga10x.bin"] {
        fs::copy(
            extracted.join("firmware").join(firmware),
            firmware_dir.join(firmware),
        )?;
    }
    let supported_gpu_source = extracted.join("supported-gpus/supported-gpus.json");
    let supported_gpu_data: serde_json::Value =
        serde_json::from_slice(&fs::read(&supported_gpu_source)?)?;
    let mut open_device_ids = BTreeSet::new();
    for chip in supported_gpu_data["chips"]
        .as_array()
        .context("NVIDIA supported GPU manifest has no chips array")?
    {
        let is_open = chip.get("legacybranch").is_none()
            && chip["features"]
                .as_array()
                .is_some_and(|features| features.iter().any(|feature| feature == "kernelopen"));
        if !is_open {
            continue;
        }
        let raw = chip["devid"]
            .as_str()
            .context("NVIDIA supported GPU entry has no devid")?;
        let device = u16::from_str_radix(raw.trim_start_matches("0x"), 16)
            .with_context(|| format!("invalid NVIDIA device ID {raw}"))?;
        open_device_ids.insert(device);
    }
    if open_device_ids.len() < 100
        || !open_device_ids.contains(&0x1e04)
        || open_device_ids.contains(&0x1b80)
    {
        bail!("NVIDIA kernelopen GPU selection is missing Turing or includes Pascal");
    }
    let (selection_config, selector) = render_nvidia_driver_selection(&open_device_ids);
    let modprobe_dir = install.join("usr/lib/modprobe.d");
    fs::create_dir_all(&modprobe_dir)?;
    fs::write(
        modprobe_dir.join("nvidia-supported-gpus.conf"),
        selection_config,
    )?;
    let selector_path = install.join("usr/libexec/mattos-nvidia-select");
    fs::create_dir_all(selector_path.parent().expect("NVIDIA selector parent"))?;
    fs::write(&selector_path, selector)?;
    set_mode(selector_path, 0o755)?;
    let doc = install.join("usr/share/doc/nvidia-driver-595");
    fs::create_dir_all(&doc)?;
    fs::copy(extracted.join("LICENSE"), doc.join("LICENSE"))?;
    fs::copy(&manifest_path, doc.join("manifest.toml"))?;
    fs::copy(
        repo_root.join("src/system/graphics/nvidia-driver/README.md"),
        doc.join("README.md"),
    )?;
    fs::copy(&supported_gpu_source, doc.join("supported-gpus.json"))?;
    fs::copy(
        extracted.join("supported-gpus/LICENSE"),
        doc.join("supported-gpus.LICENSE"),
    )?;
    fs::write(
        out_root.join("runfile.sha256"),
        format!("{}  {}\n", manifest.sha256, manifest.runfile),
    )?;
    fs::write(
        doc.join("runfile.sha256"),
        fs::read(out_root.join("runfile.sha256"))?,
    )?;
    Ok(())
}

fn build_libdisplay_info(repo_root: &Path) -> Result<()> {
    // libdisplay-info otherwise reads /usr/share/hwdata/pnp.ids at configure
    // time. Supply a tiny output-owned pkg-config descriptor pointing at the
    // imported, pinned hwdata data instead of ever consulting the host.
    let hwdata_root = repo_root.join("out/build/libdisplay-info/hwdata");
    fs::create_dir_all(hwdata_root.join("pkgconfig"))?;
    fs::copy(
        repo_root.join("src/system/data/hwdata/pnp.ids"),
        hwdata_root.join("pnp.ids"),
    )?;
    fs::write(
        hwdata_root.join("pkgconfig/hwdata.pc"),
        format!(
            "prefix={}\npkgdatadir=${{prefix}}\nName: hwdata\nDescription: pinned MattOS hardware data\nVersion: 0.410\n",
            hwdata_root.display()
        ),
    )?;
    build_meson_runtime(
        repo_root,
        "libdisplay-info",
        "src/system/libraries/libdisplay-info",
        &[],
        &["--prefix=/usr", "--libdir=lib/x86_64-linux-gnu"],
        "usr/lib/x86_64-linux-gnu/libdisplay-info.so.3",
        &[
            (
                "PKG_CONFIG_PATH",
                hwdata_root.join("pkgconfig").display().to_string(),
            ),
            (
                "PKG_CONFIG_LIBDIR",
                hwdata_root.join("pkgconfig").display().to_string(),
            ),
        ],
    )
}

fn build_libevdev(repo_root: &Path) -> Result<()> {
    build_meson_runtime(
        repo_root,
        "libevdev",
        "src/system/libraries/libevdev",
        &[],
        &[
            "--prefix=/usr",
            "--libdir=lib/x86_64-linux-gnu",
            "-Dtests=disabled",
            "-Dtools=disabled",
            "-Ddocumentation=disabled",
        ],
        "usr/lib/x86_64-linux-gnu/libevdev.so.2",
        &[],
    )
}

fn build_libinput(repo_root: &Path) -> Result<()> {
    build_meson_runtime(
        repo_root,
        "libinput",
        "src/system/libraries/libinput",
        &["libevdev", "systemd"],
        &[
            "--prefix=/usr",
            "--libdir=lib/x86_64-linux-gnu",
            "-Dtests=false",
            "-Ddocumentation=false",
            "-Ddebug-gui=false",
            "-Dlibwacom=false",
            "-Dmtdev=false",
            "-Dlua-plugins=disabled",
        ],
        "usr/lib/x86_64-linux-gnu/libinput.so.10",
        &[],
    )
}

fn build_pixman(repo_root: &Path) -> Result<()> {
    build_meson_runtime(
        repo_root,
        "pixman",
        "src/system/libraries/pixman",
        &[],
        &[
            "--prefix=/usr",
            "--libdir=lib/x86_64-linux-gnu",
            "-Dtests=disabled",
            "-Ddemos=disabled",
        ],
        "usr/lib/x86_64-linux-gnu/libpixman-1.so.0",
        &[],
    )
}

fn build_libdrm(repo_root: &Path) -> Result<()> {
    build_meson_runtime(
        repo_root,
        "libdrm",
        "src/system/libraries/libdrm",
        &[],
        &[
            "--prefix=/usr",
            "--libdir=lib/x86_64-linux-gnu",
            "-Dtests=false",
            "-Dcairo-tests=disabled",
            "-Dman-pages=disabled",
            // Iris and ANV use the DRM uAPI directly; libdrm_intel is the
            // pre-GEM compatibility helper and would pull in libpciaccess.
            "-Dintel=disabled",
            "-Dradeon=disabled",
            "-Damdgpu=enabled",
            "-Dnouveau=enabled",
            "-Dvmwgfx=enabled",
            "-Dfreedreno=disabled",
            "-Dvc4=disabled",
            "-Detnaviv=disabled",
            "-Dudev=false",
        ],
        "usr/lib/x86_64-linux-gnu/libdrm.so.2",
        &[],
    )
}

fn ensure_pinned_transitive_checkout(root: &Path, repo: &str, commit: &str) -> Result<()> {
    if !root.join(".git").is_dir() {
        remove_path_if_exists(root)?;
        fs::create_dir_all(root.parent().expect("transitive checkout parent"))?;
        run_cmd(
            root.parent().expect("transitive checkout parent"),
            "git",
            &[
                "clone",
                "--filter=blob:none",
                "--no-checkout",
                repo,
                path_str(root)?,
            ],
        )?;
        run_cmd(root, "git", &["checkout", "--detach", commit])?;
    }
    let checked_out = run_cmd_capture(root, "git", &["rev-parse", "HEAD"])?;
    if checked_out.trim() != commit {
        bail!(
            "transitive build input {} is at {}, expected {commit}",
            root.display(),
            checked_out.trim()
        )
    }
    Ok(())
}

fn prepare_mesa_spirv_dependencies(repo_root: &Path) -> Result<PathBuf> {
    const TOOLS_COMMIT: &str = "0539c81f69a3daeb706fd3477dca61435b475156";
    const TOOLS_HEADERS_COMMIT: &str = "ad9184e76a66b1001c29db9b0a3e87f646c64de0";
    const TRANSLATOR_COMMIT: &str = "c88a2e4a1ec77f7adc8916940afd9754c3a30fab";
    const TRANSLATOR_HEADERS_COMMIT: &str = "948a3b0997e2dffea5484b3df7bd5590c5b844cc";

    let root = repo_root.join("out/build/mesa/spirv-deps");
    let tools = root.join("tools");
    let tools_headers = root.join("headers");
    let translator = root.join("translator");
    let translator_headers = root.join("translator-headers");
    ensure_pinned_transitive_checkout(
        &tools,
        "https://github.com/KhronosGroup/SPIRV-Tools.git",
        TOOLS_COMMIT,
    )?;
    ensure_pinned_transitive_checkout(
        &tools_headers,
        "https://github.com/KhronosGroup/SPIRV-Headers.git",
        TOOLS_HEADERS_COMMIT,
    )?;
    ensure_pinned_transitive_checkout(
        &translator,
        "https://github.com/KhronosGroup/SPIRV-LLVM-Translator.git",
        TRANSLATOR_COMMIT,
    )?;
    ensure_pinned_transitive_checkout(
        &translator_headers,
        "https://github.com/KhronosGroup/SPIRV-Headers.git",
        TRANSLATOR_HEADERS_COMMIT,
    )?;

    let install = root.join("install");
    let libdir = install.join("usr/lib/x86_64-linux-gnu");
    let pkgconfig = libdir.join("pkgconfig");
    let tools_build = root.join("tools-build");
    if !pkgconfig.join("SPIRV-Tools.pc").is_file() {
        run_cmd(
            repo_root,
            "cmake",
            &[
                "-S",
                path_str(&tools)?,
                "-B",
                path_str(&tools_build)?,
                "-G",
                "Ninja",
                "-DCMAKE_BUILD_TYPE=Release",
                "-DCMAKE_INSTALL_PREFIX=/usr",
                "-DCMAKE_INSTALL_LIBDIR=lib/x86_64-linux-gnu",
                &format!("-DSPIRV-Headers_SOURCE_DIR={}", tools_headers.display()),
                "-DSPIRV_SKIP_TESTS=ON",
                "-DSPIRV_SKIP_EXECUTABLES=ON",
                "-DSPIRV_WERROR=OFF",
            ],
        )?;
        run_cmd(
            repo_root,
            "cmake",
            &["--build", path_str(&tools_build)?, "--parallel"],
        )?;
        run_cmd_with_env_overrides(
            repo_root,
            "cmake",
            &["--install", path_str(&tools_build)?],
            &[("DESTDIR", install.display().to_string())],
        )?;
    }

    let translator_build = root.join("translator-build");
    if !pkgconfig.join("LLVMSPIRVLib.pc").is_file() {
        let pkg_path = pkgconfig.display().to_string();
        run_cmd_with_env_overrides(
            repo_root,
            "cmake",
            &[
                "-S",
                path_str(&translator)?,
                "-B",
                path_str(&translator_build)?,
                "-G",
                "Ninja",
                "-DCMAKE_BUILD_TYPE=Release",
                "-DCMAKE_INSTALL_PREFIX=/usr",
                "-DCMAKE_INSTALL_LIBDIR=lib/x86_64-linux-gnu",
                &format!(
                    "-DLLVM_DIR={}",
                    repo_root
                        .join("out/build/llvm/install/usr/lib/x86_64-linux-gnu/cmake/llvm")
                        .display()
                ),
                &format!(
                    "-DLLVM_EXTERNAL_SPIRV_HEADERS_SOURCE_DIR={}",
                    translator_headers.display()
                ),
                "-DLLVM_SPIRV_BUILD_EXTERNAL=YES",
                "-DLLVM_SPIRV_INCLUDE_TESTS=OFF",
                "-DLLVM_SPIRV_ENABLE_LIBSPIRV_DIS=OFF",
                "-DBUILD_SHARED_LIBS=OFF",
            ],
            &[("PKG_CONFIG_PATH", pkg_path.clone())],
        )?;
        run_cmd_with_env_overrides(
            repo_root,
            "cmake",
            &["--build", path_str(&translator_build)?, "--parallel"],
            &[("PKG_CONFIG_PATH", pkg_path.clone())],
        )?;
        run_cmd_with_env_overrides(
            repo_root,
            "cmake",
            &["--install", path_str(&translator_build)?],
            &[
                ("DESTDIR", install.display().to_string()),
                ("PKG_CONFIG_PATH", pkg_path),
            ],
        )?;
    }
    // These packages are staged beneath DESTDIR but advertise /usr in their
    // generated .pc files. Point build-only consumers at the output-owned
    // prefix so pkg-config can never resolve matching host headers/libraries.
    for name in ["SPIRV-Tools.pc", "SPIRV-Tools-shared.pc", "LLVMSPIRVLib.pc"] {
        let descriptor = pkgconfig.join(name);
        if descriptor.is_file() {
            let contents = fs::read_to_string(&descriptor)?;
            let output_prefix = format!("prefix={}", install.join("usr").display());
            let normalized = contents.replacen("prefix=/usr", &output_prefix, 1);
            fs::write(&descriptor, normalized)?;
        }
    }
    Ok(pkgconfig)
}

fn rewrite_pkgconfig_prefix(source: &Path, destination: &Path, prefix: &Path) -> Result<()> {
    let contents = fs::read_to_string(source)
        .with_context(|| format!("failed to read {}", source.display()))?;
    let rewritten = contents.replacen("prefix=/usr", &format!("prefix={}", prefix.display()), 1);
    fs::write(destination, rewritten)
        .with_context(|| format!("failed to write {}", destination.display()))
}

/// Vulkan-Tools needs both Wayland's scanner XML and wayland-protocols at
/// configure/build time. Their installed pkg-config files deliberately use
/// the final `/usr` prefix, so make output-owned build descriptors that point
/// at the staged MattOS trees rather than accidentally consulting the host.
fn vulkan_wayland_pkgconfig(repo_root: &Path) -> Result<PathBuf> {
    let output = repo_root.join("out/build/vulkan-tools/build-pkgconfig");
    remove_path_if_exists(&output)?;
    fs::create_dir_all(&output)?;
    let wayland_usr = repo_root.join("out/build/wayland/install/usr");
    let wayland_pc = wayland_usr.join("lib/x86_64-linux-gnu/pkgconfig");
    for name in ["wayland-client.pc", "wayland-scanner.pc"] {
        rewrite_pkgconfig_prefix(&wayland_pc.join(name), &output.join(name), &wayland_usr)?;
    }
    let protocols_usr = repo_root.join("out/build/mesa/install/usr");
    rewrite_pkgconfig_prefix(
        &protocols_usr.join("share/pkgconfig/wayland-protocols.pc"),
        &output.join("wayland-protocols.pc"),
        &protocols_usr,
    )?;
    Ok(output)
}

fn build_vulkan_cmake(
    repo_root: &Path,
    component: &str,
    source_relative: &str,
    dependencies: &[&str],
    options: &[String],
    required_outputs: &[&str],
    pkgconfig_override: Option<&Path>,
) -> Result<()> {
    let source = repo_root.join(source_relative);
    let out_root = repo_root.join("out/build").join(component);
    let source_copy = out_root.join("source");
    let build_dir = out_root.join("build");
    let install_dir = out_root.join("install");
    let stamp_path = out_root.join("recipe.stamp");
    // CMake's Find modules do not necessarily consult CPPFLAGS or the
    // pkg-config overlay.  Give them the exact target-owned prefixes and
    // include that selection in disposable configuration identity; otherwise
    // an old CMakeCache.txt can retain a host header/library discovery after
    // the stage cache has correctly selected a rebuild.
    let cmake_prefixes = dependencies
        .iter()
        .map(|component| {
            repo_root
                .join("out/build")
                .join(component)
                .join("install/usr")
        })
        .filter(|prefix| prefix.is_dir())
        .collect::<Vec<_>>();
    let cmake_prefix_path = std::env::join_paths(&cmake_prefixes)?
        .to_string_lossy()
        .replace(':', ";");
    let state = fs::read_to_string(
        repo_root
            .join("upstream/state")
            .join(format!("{component}.toml")),
    )?;
    let stamp = format!(
        "{state}\ncmake-prefix-path={cmake_prefix_path}\n{}\n",
        options.join("\n")
    );
    if fs::read_to_string(&stamp_path).ok().as_deref() != Some(stamp.as_str()) {
        remove_path_if_exists(&source_copy)?;
        remove_path_if_exists(&build_dir)?;
        remove_path_if_exists(&install_dir)?;
    }
    fs::create_dir_all(&out_root)?;
    if !source_copy.join("CMakeLists.txt").is_file() {
        sync_build_source(&source, &source_copy)?;
    }
    fs::create_dir_all(&build_dir)?;
    let mut env = staged_library_environment(repo_root, dependencies)?;
    if let Some(override_dir) = pkgconfig_override {
        let existing = env
            .iter()
            .find(|(key, _)| *key == "PKG_CONFIG_LIBDIR")
            .map(|(_, value)| value.as_str())
            .unwrap_or_default();
        let value = if existing.is_empty() {
            override_dir.display().to_string()
        } else {
            format!("{}:{existing}", override_dir.display())
        };
        for (key, current) in &mut env {
            if *key == "PKG_CONFIG_PATH" || *key == "PKG_CONFIG_LIBDIR" {
                *current = value.clone();
            }
        }
    }
    if !build_dir.join("build.ninja").is_file() {
        let mut args = vec![
            "-S".to_string(),
            source_copy.display().to_string(),
            "-B".to_string(),
            build_dir.display().to_string(),
            "-G".to_string(),
            "Ninja".to_string(),
            "-DCMAKE_BUILD_TYPE=Release".to_string(),
            "-DCMAKE_INSTALL_PREFIX=/usr".to_string(),
            "-DCMAKE_INSTALL_LIBDIR=lib/x86_64-linux-gnu".to_string(),
            "-DCMAKE_FIND_PACKAGE_NO_PACKAGE_REGISTRY=ON".to_string(),
            "-DCMAKE_FIND_USE_PACKAGE_REGISTRY=OFF".to_string(),
            "-DCMAKE_FIND_USE_SYSTEM_PACKAGE_REGISTRY=OFF".to_string(),
            format!("-DCMAKE_PREFIX_PATH={cmake_prefix_path}"),
        ];
        args.extend(options.iter().cloned());
        let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
        run_cmd_with_env_overrides(repo_root, "cmake", &refs, &env)?;
    }
    let jobs = scheduler::child_job_limit().max(1).to_string();
    run_cmd_with_env_overrides(
        repo_root,
        "cmake",
        &["--build", path_str(&build_dir)?, "--parallel", &jobs],
        &env,
    )?;
    remove_path_if_exists(&install_dir)?;
    run_cmd_with_env_overrides(
        repo_root,
        "cmake",
        &["--install", path_str(&build_dir)?, "--prefix", "/usr"],
        &[
            env.as_slice(),
            &[("DESTDIR", install_dir.display().to_string())],
        ]
        .concat(),
    )?;
    for relative in required_outputs {
        if !install_dir.join(relative).is_file() {
            bail!("{component} install did not produce {relative}")
        }
    }
    fs::write(stamp_path, stamp)?;
    Ok(())
}

fn build_vulkan_headers(repo_root: &Path) -> Result<()> {
    build_vulkan_cmake(
        repo_root,
        "vulkan-headers",
        "src/system/graphics/vulkan-headers",
        &[],
        &[
            "-DVULKAN_HEADERS_ENABLE_TESTS=OFF".to_string(),
            "-DVULKAN_HEADERS_ENABLE_MODULE=OFF".to_string(),
        ],
        &[
            "usr/include/vulkan/vulkan.h",
            "usr/share/vulkan/registry/vk.xml",
        ],
        None,
    )
}

fn build_vulkan_loader(repo_root: &Path) -> Result<()> {
    let headers = repo_root.join("out/build/vulkan-headers/install/usr/share/cmake/VulkanHeaders");
    build_vulkan_cmake(
        repo_root,
        "vulkan-loader",
        "src/system/graphics/vulkan-loader",
        &["vulkan-headers", "wayland", "cpython"],
        &[
            format!("-DVulkanHeaders_DIR={}", headers.display()),
            "-DBUILD_TESTS=OFF".to_string(),
            "-DBUILD_WERROR=OFF".to_string(),
            "-DLOADER_CODEGEN=ON".to_string(),
            "-DBUILD_WSI_XCB_SUPPORT=OFF".to_string(),
            "-DBUILD_WSI_XLIB_SUPPORT=OFF".to_string(),
            "-DBUILD_WSI_XLIB_XRANDR_SUPPORT=OFF".to_string(),
            "-DBUILD_WSI_WAYLAND_SUPPORT=ON".to_string(),
        ],
        &["usr/lib/x86_64-linux-gnu/libvulkan.so.1"],
        None,
    )
}

fn build_vulkan_tools(repo_root: &Path) -> Result<()> {
    let pkgconfig = vulkan_wayland_pkgconfig(repo_root)?;
    let headers = repo_root.join("out/build/vulkan-headers/install/usr/share/cmake/VulkanHeaders");
    build_vulkan_cmake(
        repo_root,
        "vulkan-tools",
        "src/system/graphics/vulkan-tools",
        &[
            "vulkan-headers",
            "vulkan-loader",
            "wayland",
            "libffi",
            "mesa",
            "cpython",
        ],
        &[
            format!("-DVulkanHeaders_DIR={}", headers.display()),
            "-DBUILD_CUBE=ON".to_string(),
            "-DBUILD_VULKANINFO=ON".to_string(),
            "-DBUILD_ICD=OFF".to_string(),
            "-DBUILD_TESTS=OFF".to_string(),
            "-DBUILD_WERROR=OFF".to_string(),
            "-DTOOLS_CODEGEN=OFF".to_string(),
            "-DBUILD_WSI_XCB_SUPPORT=OFF".to_string(),
            "-DBUILD_WSI_XLIB_SUPPORT=OFF".to_string(),
            "-DBUILD_WSI_WAYLAND_SUPPORT=ON".to_string(),
            "-DBUILD_WSI_DISPLAY_SUPPORT=ON".to_string(),
        ],
        &["usr/bin/vulkaninfo", "usr/bin/vkcube"],
        Some(&pkgconfig),
    )
}

fn build_mesa(repo_root: &Path) -> Result<()> {
    // Mesa's generator uses Mako. It is a build-only Python module, not a
    // shipped runtime dependency; keep the pinned wheel installation entirely
    // under the stage output so the host Python environment is never mutated.
    let python_deps = repo_root.join("out/build/mesa/python-deps");
    if !python_deps.join("mako").is_dir() {
        fs::create_dir_all(&python_deps)?;
        run_cmd(
            repo_root,
            "python3",
            &[
                "-m",
                "pip",
                "install",
                "--disable-pip-version-check",
                "--no-deps",
                "--target",
                path_str(&python_deps)?,
                "Mako==1.3.10",
            ],
        )?;
    }
    // Mesa uses glslangValidator at build time to compile the internal BVH
    // shaders shared by RADV, ANV and lavapipe. Keep that transitive build
    // tool pinned and output-owned; none of it is copied into the runtime.
    const GLSLANG_COMMIT: &str = "8a85691a0740d390761a1008b4696f57facd02c4";
    let glslang_root = repo_root.join("out/build/mesa/glslang");
    let glslang_source = glslang_root.join("source");
    let glslang_build = glslang_root.join("build");
    let glslang_validator = glslang_build.join("StandAlone/glslangValidator");
    if !glslang_validator.is_file() {
        remove_path_if_exists(&glslang_root)?;
        fs::create_dir_all(&glslang_root)?;
        ensure_pinned_transitive_checkout(
            &glslang_source,
            "https://github.com/KhronosGroup/glslang.git",
            GLSLANG_COMMIT,
        )?;
        run_cmd(
            repo_root,
            "cmake",
            &[
                "-S",
                path_str(&glslang_source)?,
                "-B",
                path_str(&glslang_build)?,
                "-DCMAKE_BUILD_TYPE=Release",
                "-DENABLE_OPT=OFF",
                "-DENABLE_HLSL=OFF",
                "-DENABLE_GLSLANG_BINARIES=ON",
            ],
        )?;
        run_cmd(
            repo_root,
            "cmake",
            &[
                "--build",
                path_str(&glslang_build)?,
                "--target",
                "glslang-standalone",
                "--parallel",
            ],
        )?;
    }
    let checked_out = run_cmd_capture(&glslang_source, "git", &["rev-parse", "HEAD"])?;
    if checked_out.trim() != GLSLANG_COMMIT {
        bail!(
            "Mesa glslang build tool is at {}, expected {GLSLANG_COMMIT}",
            checked_out.trim()
        )
    }
    let spirv_pkgconfig = prepare_mesa_spirv_dependencies(repo_root)?;
    let rust_tools = repo_root.join("out/build/rust/install/usr/bin");
    let cbindgen_root = repo_root.join("out/build/mesa/cbindgen");
    let cbindgen = cbindgen_root.join("bin/cbindgen");
    if !cbindgen.is_file() {
        let cargo = rust_tools.join("cargo");
        let rustc = rust_tools.join("rustc");
        run_cmd_with_env_overrides(
            repo_root,
            path_str(&cargo)?,
            &[
                "install",
                "cbindgen",
                "--version",
                "0.29.4",
                "--locked",
                "--root",
                path_str(&cbindgen_root)?,
            ],
            &[
                (
                    "CARGO_HOME",
                    repo_root
                        .join("out/build/mesa/cargo-home")
                        .display()
                        .to_string(),
                ),
                ("RUSTC", rustc.display().to_string()),
            ],
        )?;
    }
    // Debian's bindgen 0.71.1 predates the Clang 22 AST behavior used by the
    // source-built MattOS LLVM and emits opaque one-byte Mesa structs with
    // contradictory layout assertions. Pin a current, known-good generator
    // beside cbindgen so Mesa's Rust/NVK bindings stay output-owned too.
    let bindgen_root = repo_root.join("out/build/mesa/bindgen");
    let bindgen = bindgen_root.join("bin/bindgen");
    if !bindgen.is_file() {
        let cargo = rust_tools.join("cargo");
        let rustc = rust_tools.join("rustc");
        run_cmd_with_env_overrides(
            repo_root,
            path_str(&cargo)?,
            &[
                "install",
                "bindgen-cli",
                "--version",
                "0.72.1",
                "--locked",
                "--root",
                path_str(&bindgen_root)?,
            ],
            &[
                (
                    "CARGO_HOME",
                    repo_root
                        .join("out/build/mesa/cargo-home")
                        .display()
                        .to_string(),
                ),
                ("RUSTC", rustc.display().to_string()),
            ],
        )?;
    }
    let glslang_path = glslang_validator
        .parent()
        .expect("glslang validator parent")
        .display()
        .to_string();
    let llvm_tools = repo_root.join("out/build/llvm/install/usr/bin");
    let wayland_tools = repo_root.join("out/build/wayland/install/usr/bin");
    build_meson_runtime(
        repo_root,
        "mesa",
        "src/system/graphics/mesa",
        &[
            "libdrm",
            "libdisplay-info",
            "libffi",
            "llvm",
            "zlib",
            "zstd",
            "systemd",
            "wayland",
            "libglvnd",
        ],
        &[
            "--prefix=/usr",
            "--libdir=lib/x86_64-linux-gnu",
            "-Dplatforms=wayland",
            "-Degl-native-platform=wayland",
            "-Dglx=disabled",
            "-Dglvnd=enabled",
            "-Dopengl=true",
            "-Dgles1=enabled",
            "-Dgles2=enabled",
            // Keep software and QEMU renderers while covering the production
            // DRM drivers enabled by MattOS' generic modular kernel. SVGA is
            // the corresponding VMware guest renderer.
            "-Degl=enabled",
            "-Dgbm=enabled",
            "-Dgallium-drivers=radeonsi,iris,nouveau,virgl,llvmpipe,svga",
            // RADV, ANV and NVK are the hardware Vulkan implementations;
            // lavapipe and Venus provide software and virtio-gpu fallbacks.
            "-Dvulkan-drivers=amd,intel,nouveau,swrast,virtio",
            "-Dvulkan-layers=device-select",
            "-Dllvm=enabled",
            "-Dshared-llvm=enabled",
            "-Dcpp_rtti=false",
            "-Dbuild-tests=false",
            "-Denable-glcpp-tests=false",
            "-Dtools=[]",
            "-Dhtml-docs=disabled",
            "-Dzstd=enabled",
        ],
        "usr/lib/x86_64-linux-gnu/libgbm.so.1",
        &[
            ("PYTHONPATH", python_deps.display().to_string()),
            ("PKG_CONFIG_PATH", spirv_pkgconfig.display().to_string()),
            (
                "PATH",
                format!(
                    "{}:{}:{glslang_path}:{}:{}:/usr/bin:/bin",
                    bindgen_root.join("bin").display(),
                    cbindgen_root.join("bin").display(),
                    llvm_tools.display(),
                    wayland_tools.display()
                ),
            ),
        ],
    )
}

include!("desktop_aggregation.rs");
