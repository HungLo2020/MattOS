// Runtime-facing multimedia and foundational library recipes.
// Included into the crate root to preserve existing helper visibility.
fn build_dav1d(repo_root: &Path) -> Result<()> {
    build_meson_runtime(
        repo_root,
        "dav1d",
        "src/system/multimedia/dav1d",
        &[],
        &[
            "--prefix=/usr",
            "--libdir=lib/x86_64-linux-gnu",
            "--buildtype=release",
            "-Denable_asm=false",
            "-Denable_tools=false",
            "-Denable_examples=false",
            "-Denable_tests=false",
            "-Denable_docs=false",
        ],
        "usr/lib/x86_64-linux-gnu/libdav1d.so.7",
        &[],
    )
}

fn build_glib(repo_root: &Path) -> Result<()> {
    build_meson_runtime(
        repo_root,
        "glib",
        "src/system/libraries/glib",
        &["libffi", "pcre2", "zlib"],
        &[
            "--prefix=/usr",
            "--libdir=lib/x86_64-linux-gnu",
            "--buildtype=release",
            "-Dtests=false",
            "-Dinstalled_tests=false",
            "-Dnls=disabled",
            "-Dselinux=disabled",
            "-Dlibmount=disabled",
            "-Dlibelf=disabled",
            "-Dintrospection=disabled",
            "-Dman-pages=disabled",
            "-Ddtrace=disabled",
            "-Dsystemtap=disabled",
            "-Dsysprof=disabled",
            "-Dglib_debug=disabled",
        ],
        "usr/lib/x86_64-linux-gnu/libglib-2.0.so.0",
        &[],
    )?;
    let glib_usr = repo_root.join("out/build/glib/install/usr");
    let glib_pc = glib_usr.join("lib/x86_64-linux-gnu/pkgconfig");
    rewrite_pkgconfig_prefixes(&glib_pc, &glib_usr)?;
    // GLib's public .pc files expose these private requirements even for a
    // dynamic consumer. Keep their development metadata in the same
    // output-owned SDK directory so pkg-config cannot fall back to the host.
    for (component, names) in [
        ("pcre2", &["libpcre2-8.pc"][..]),
        ("libffi", &["libffi.pc"][..]),
    ] {
        let dependency_usr = repo_root
            .join("out/build")
            .join(component)
            .join("install/usr");
        let dependency_pc = dependency_usr.join("lib/x86_64-linux-gnu/pkgconfig");
        for name in names {
            fs::copy(dependency_pc.join(name), glib_pc.join(name))?;
        }
        rewrite_selected_pkgconfig_prefixes(&glib_pc, names, &dependency_usr)?;
    }
    for required in [
        "usr/lib/x86_64-linux-gnu/libgobject-2.0.so.0",
        "usr/lib/x86_64-linux-gnu/libgio-2.0.so.0",
        "usr/bin/glib-compile-schemas",
    ] {
        if !repo_root
            .join("out/build/glib/install")
            .join(required)
            .is_file()
        {
            bail!("GLib build did not install /{required}");
        }
    }
    Ok(())
}


fn build_pipewire(repo_root: &Path) -> Result<()> {
    build_meson_runtime(
        repo_root,
        "pipewire",
        "src/system/multimedia/pipewire",
        &["systemd", "dbus"],
        &[
            "--prefix=/usr",
            "--libdir=lib/x86_64-linux-gnu",
            "--buildtype=release",
            "-Ddocs=disabled",
            "-Dman=disabled",
            "-Dexamples=disabled",
            "-Dtests=disabled",
            "-Dinstalled_tests=disabled",
            "-Dgstreamer=disabled",
            "-Dsystemd=enabled",
            "-Dlogind=enabled",
            "-Dsystemd-system-service=disabled",
            "-Dsystemd-user-service=enabled",
            "-Dselinux=disabled",
            "-Dpipewire-alsa=disabled",
            "-Dpipewire-jack=disabled",
            "-Dpipewire-v4l2=disabled",
            "-Dalsa=disabled",
            "-Dbluez5=disabled",
            "-Dffmpeg=disabled",
            "-Djack=disabled",
            "-Dv4l2=disabled",
            "-Dlibcamera=disabled",
            "-Dvulkan=disabled",
            "-Dsdl2=disabled",
            "-Dsndfile=disabled",
            "-Dlibmysofa=disabled",
            "-Dlibpulse=disabled",
            "-Davahi=disabled",
            "-Dlibusb=disabled",
            "-Dsession-managers=[]",
            "-Dx11=disabled",
            "-Dx11-xfixes=disabled",
            "-Dlibcanberra=disabled",
            "-Dlegacy-rtkit=false",
            "-Dflatpak=disabled",
            "-Dreadline=disabled",
            "-Dgsettings=disabled",
            "-Dgsettings-pulse-schema=disabled",
        ],
        "usr/lib/x86_64-linux-gnu/libpipewire-0.3.so.0",
        &[],
    )?;
    let pipewire_usr = repo_root.join("out/build/pipewire/install/usr");
    rewrite_pkgconfig_prefixes(
        &pipewire_usr.join("lib/x86_64-linux-gnu/pkgconfig"),
        &pipewire_usr,
    )?;
    for required in [
        "usr/bin/pipewire",
        "usr/bin/pipewire-pulse",
        "usr/lib/systemd/user/pipewire.service",
        "usr/lib/systemd/user/pipewire.socket",
    ] {
        if !repo_root
            .join("out/build/pipewire/install")
            .join(required)
            .exists()
        {
            bail!("PipeWire build did not install /{required}");
        }
    }
    Ok(())
}

