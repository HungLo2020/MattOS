fn build_libfyaml(repo_root: &Path) -> Result<()> {
    build_vulkan_cmake(
        repo_root,
        "libfyaml",
        "src/system/libraries/libfyaml",
        &[],
        &["-DFYAML_BUILD_TESTS=OFF".to_string()],
        &["usr/lib/x86_64-linux-gnu/libfyaml.so.0"],
        None,
    )
}

fn build_libxmlb(repo_root: &Path) -> Result<()> {
    build_meson_runtime(
        repo_root,
        "libxmlb",
        "src/system/libraries/libxmlb",
        &["glib", "libffi", "xz", "zlib"],
        &[
            "--prefix=/usr",
            "--libdir=lib/x86_64-linux-gnu",
            "-Dtests=false",
            "-Dgtkdoc=false",
            "-Dintrospection=false",
            "-Dcli=false",
        ],
        "usr/lib/x86_64-linux-gnu/libxmlb.so.2",
        &[],
    )
}

fn build_json_glib(repo_root: &Path) -> Result<()> {
    build_meson_runtime(
        repo_root,
        "json-glib",
        "src/system/libraries/json-glib",
        &["glib", "libffi", "pcre2", "zlib"],
        &[
            "--prefix=/usr",
            "--libdir=lib/x86_64-linux-gnu",
            "-Dtests=false",
            "-Dintrospection=disabled",
            "--wrap-mode=nofallback",
        ],
        "usr/lib/x86_64-linux-gnu/libjson-glib-1.0.so.0",
        &[],
    )
}

fn build_appstream(repo_root: &Path) -> Result<()> {
    build_meson_runtime(
        repo_root,
        "appstream",
        "src/system/libraries/appstream",
        &[
            "glib", "libffi", "pcre2", "libxml2", "zlib", "curl", "openssl", "libfyaml", "libxmlb", "xz",
            "zstd", "systemd", "wayland",
        ],
        &[
            "--prefix=/usr",
            "--libdir=lib/x86_64-linux-gnu",
            "-Dapidocs=false",
            "-Dstemming=false",
            "-Dbash-completion=false",
            "-Dinstall-docs=false",
            "-Dman=false",
            "-Dvapi=false",
            "-Dgir=false",
            "--wrap-mode=nofallback",
        ],
        "usr/lib/x86_64-linux-gnu/libappstream.so.5",
        &[],
    )
}

fn build_gdk_pixbuf(repo_root: &Path) -> Result<()> {
    build_meson_runtime(
        repo_root,
        "gdk-pixbuf",
        "src/system/libraries/gdk-pixbuf",
        // gdk-pixbuf links helper binaries against libglib; GLib's target
        // ABI requires PCRE2 at that link step, not merely at final runtime.
        &["glib", "libffi", "zlib", "libpng", "pcre2"],
        &[
            "--prefix=/usr",
            "--libdir=lib/x86_64-linux-gnu",
            "-Dtests=false",
            "-Dinstalled_tests=false",
            "-Dintrospection=disabled",
            "-Dman=false",
            "-Dgio_sniffing=false",
            "-Djpeg=disabled",
            "-Dtiff=disabled",
            "-Dothers=disabled",
            "--wrap-mode=nofallback",
        ],
        "usr/lib/x86_64-linux-gnu/libgdk_pixbuf-2.0.so.0",
        &[],
    )
}

fn build_gpgme(repo_root: &Path) -> Result<()> {
    build_autotools_import(
        repo_root,
        "gpgme",
        "src/system/security/gpgme",
        &["libassuan", "libgcrypt", "libgpg-error", "libksba", "zlib"],
        &[
            "--prefix=/usr",
            "--libdir=/usr/lib/x86_64-linux-gnu",
            "--disable-gpgsm",
            "--disable-gpgconf",
            "--disable-gpg-test",
        ],
        &["usr/lib/x86_64-linux-gnu/libgpgme.so.45"],
    )?;
    // Libtool consumers otherwise resolve this build-tree .la file and embed
    // its absolute staging directory as a RUNPATH.  The target .so and
    // pkg-config metadata are the published interface; the .la archive is a
    // build-private libtool convenience file and is not part of it.
    remove_path_if_exists(
        &repo_root.join("out/build/gpgme/install/usr/lib/x86_64-linux-gnu/libgpgme.la"),
    )?;
    Ok(())
}

fn build_flatpak(repo_root: &Path) -> Result<()> {
    // Flatpak is a native target package-manager runtime.  Keep its build
    // isolated from the COSMIC aggregate so its pkg-config and ELF closure
    // can be audited independently.
    build_meson_runtime(
        repo_root,
        "flatpak",
        "src/system/packages/flatpak",
        &[
            "glib",
            "pcre2",
            "libffi",
            "pcre2",
            "zlib",
            "xz",
            "curl",
            "openssl",
            "libcap",
            "libarchive",
            "bzip2",
            "lz4",
            "libxml2",
            "fuse3",
            "ostree",
            "systemd",
            "dbus",
            "gpgv",
            "zstd",
            "wayland",
            "xkbcommon",
            "libpng",
            "libbsd",
            "libmd",
            "libassuan",
            "libgcrypt",
            "libgpg-error",
            "libksba",
            "json-glib",
            "appstream",
            "gdk-pixbuf",
            "gpgme",
            "polkit",
            "bubblewrap",
            "xdg-dbus-proxy",
        ],
        &[
            "--prefix=/usr",
            "--libdir=lib/x86_64-linux-gnu",
            "-Dtests=false",
            "-Dinstalled_tests=false",
            "-Dman=disabled",
            "-Ddocbook_docs=disabled",
            "-Dgtkdoc=disabled",
            "-Dgir=disabled",
            "-Ddconf=disabled",
            "-Dmalcontent=disabled",
            "-Dselinux_module=disabled",
            "-Dxauth=disabled",
            "-Dwayland_security_context=disabled",
            "-Dsystem_helper=enabled",
            "-Dsystemd=enabled",
            // MattOS grants ordinary administrative users membership in
            // `sudo`, not Debian/Fedora's `wheel`.  Flatpak's generated
            // system-helper polkit rule must follow that distro policy so
            // COSMIC Store can authorize system installs without making the
            // installation tree writable or running Store as root.
            "-Dprivileged_group=sudo",
            "-Dseccomp=disabled",
            // Never let Meson record the staged build-tree path returned by
            // find_program("fusermount3") in the shipped binary.  Flatpak
            // executes fusermount from the target package closure at this
            // stable runtime location.
            "-Dsystem_fusermount=/usr/bin/fusermount3",
            "-Dsystem_bubblewrap=/usr/bin/bwrap",
            "-Dsystem_dbus_proxy=/usr/bin/xdg-dbus-proxy",
            "--wrap-mode=nofallback",
        ],
        "usr/bin/flatpak",
        &[],
    )?;
    build_flatpak_target_install_helper(repo_root)?;
    Ok(())
}

/// Build the MattOS-owned installer helper against the target-built
/// libflatpak shipped in the Flatpak package. It opens an explicit target
/// installation while running in the booted live system, never in a chroot.
fn build_flatpak_target_install_helper(repo_root: &Path) -> Result<()> {
    let source = repo_root.join("src/system/installer/flatpak-target-install.c");
    if !source.is_file() {
        bail!("missing MattOS Flatpak target-install helper {}", source.display());
    }
    let install = repo_root.join("out/build/flatpak/install");
    let output = install.join("usr/libexec/mattos-flatpak-target-install");
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    let compiler = repo_root.join("out/build/gcc-toolchain/install/usr/bin/gcc");
    let sysroot = repo_root.join("out/sysroot");
    let libc_search = format!("-B{}/usr/lib/x86_64-linux-gnu/", sysroot.display());
    let gcc_search = format!(
        "-B{}/usr/lib/x86_64-linux-gnu/gcc/x86_64-pc-linux-gnu/15.3.0/",
        sysroot.display()
    );
    let flatpak_usr = install.join("usr");
    let flatpak_lib = flatpak_usr.join("lib/x86_64-linux-gnu");
    let glib_usr = repo_root.join("out/build/glib/install/usr");
    let glib_lib = glib_usr.join("lib/x86_64-linux-gnu");
    let ostree_usr = repo_root.join("out/build/ostree/install/usr");
    let gcc_runtime_lib = repo_root.join("out/build/gcc-runtime/install/usr/lib/lib64");
    let helper_components = [
        "flatpak",
        "glib",
        "libffi",
        "pcre2",
        "zlib",
        "xz",
        "curl",
        "openssl",
        "libcap",
        "libarchive",
        "bzip2",
        "lz4",
        "libxml2",
        "fuse3",
        "ostree",
        "systemd",
        "dbus",
        "gpgv",
        "zstd",
        "wayland",
        "xkbcommon",
        "libpng",
        "libbsd",
        "libmd",
        "libassuan",
        "libgcrypt",
        "libgpg-error",
        "libksba",
        "json-glib",
        "appstream",
        "gdk-pixbuf",
        "gpgme",
        "polkit",
        "bubblewrap",
        "xdg-dbus-proxy",
    ];
    let mut args = vec![
        format!("--sysroot={}", sysroot.display()),
        libc_search,
        gcc_search,
        "-std=c11".to_string(),
        "-O2".to_string(),
        "-fno-ident".to_string(),
        format!("-ffile-prefix-map={}=/usr/src/mattos", repo_root.display()),
        format!("-fdebug-prefix-map={}=/usr/src/mattos", repo_root.display()),
        format!("-fmacro-prefix-map={}=/usr/src/mattos", repo_root.display()),
        format!("-I{}", flatpak_usr.join("include").display()),
        format!("-I{}", flatpak_usr.join("include/flatpak").display()),
        format!("-I{}", glib_usr.join("include/glib-2.0").display()),
        format!("-I{}", glib_lib.join("glib-2.0/include").display()),
        format!("-I{}", ostree_usr.join("include/ostree-1").display()),
        format!("-L{}", flatpak_lib.display()),
        format!("-L{}", glib_lib.display()),
        format!("-L{}", gcc_runtime_lib.display()),
        format!("-Wl,-rpath-link,{}", flatpak_lib.display()),
        format!("-Wl,-rpath-link,{}", glib_lib.display()),
        format!("-Wl,-rpath-link,{}", gcc_runtime_lib.display()),
        format!("-Wl,-rpath-link,{}", ostree_usr.join("lib/x86_64-linux-gnu").display()),
    ];
    for component in helper_components {
        let library = repo_root
            .join("out/build")
            .join(component)
            .join("install/usr/lib/x86_64-linux-gnu");
        args.push(format!("-L{}", library.display()));
        args.push(format!("-Wl,-rpath-link,{}", library.display()));
    }
    args.extend([
        path_str(&source)?.to_owned(),
        "-Wl,--no-as-needed".to_string(),
        "-lflatpak".to_string(),
        "-lgio-2.0".to_string(),
        "-lgobject-2.0".to_string(),
        "-lglib-2.0".to_string(),
        "-Wl,--as-needed".to_string(),
        "-o".to_string(),
        path_str(&output)?.to_owned(),
    ]);
    let args_ref = args.iter().map(String::as_str).collect::<Vec<_>>();
    let environment = staged_library_environment(repo_root, &helper_components)?;
    run_cmd_with_env_overrides(repo_root, path_str(&compiler)?, &args_ref, &environment)?;
    set_mode(output.clone(), 0o755)?;
    if !output.is_file() {
        bail!("Flatpak target-install helper was not produced at {}", output.display());
    }
    Ok(())
}

fn build_libarchive(repo_root: &Path) -> Result<()> {
    build_vulkan_cmake(
        repo_root,
        "libarchive",
        "src/system/libraries/libarchive",
        &["zlib", "zstd", "bzip2", "xz", "lz4", "libcap"],
        &[
            "-DENABLE_TEST=OFF".to_string(),
            "-DENABLE_TAR=OFF".to_string(),
            "-DENABLE_CPIO=OFF".to_string(),
            "-DENABLE_CAT=OFF".to_string(),
            "-DENABLE_OPENSSL=OFF".to_string(),
            "-DENABLE_ACL=OFF".to_string(),
            "-DENABLE_XATTR=OFF".to_string(),
            "-DENABLE_ICONV=OFF".to_string(),
            "-DENABLE_EXPAT=OFF".to_string(),
        ],
        &["usr/lib/x86_64-linux-gnu/libarchive.so.13"],
        None,
    )
}

fn build_libxml2(repo_root: &Path) -> Result<()> {
    build_vulkan_cmake(
        repo_root,
        "libxml2",
        "src/system/libraries/libxml2",
        &["zlib", "expat"],
        &[
            "-DLIBXML2_WITH_TESTS=OFF".to_string(),
            "-DLIBXML2_WITH_PYTHON=OFF".to_string(),
            "-DLIBXML2_WITH_LZMA=OFF".to_string(),
            "-DLIBXML2_WITH_ZSTD=OFF".to_string(),
            "-DLIBXML2_WITH_ICU=OFF".to_string(),
        ],
        &["usr/lib/x86_64-linux-gnu/libxml2.so.16"],
        None,
    )
}

fn build_libpng(repo_root: &Path) -> Result<()> {
    build_autotools_import(
        repo_root,
        "libpng",
        "src/system/libraries/libpng",
        &["zlib"],
        &[
            "--prefix=/usr",
            "--libdir=/usr/lib/x86_64-linux-gnu",
            "--disable-tests",
        ],
        &["usr/lib/x86_64-linux-gnu/libpng16.so.16"],
    )
}

fn build_fuse3(repo_root: &Path) -> Result<()> {
    build_meson_runtime(
        repo_root,
        "fuse3",
        "src/system/libraries/fuse3",
        &[],
        &[
            "--prefix=/usr",
            "--libdir=lib/x86_64-linux-gnu",
            "-Dtests=false",
            "-Dexamples=false",
            "-Duseroot=false",
            "-Denable-io-uring=false",
            "-Dudevrulesdir=/usr/lib/udev/rules.d",
            "-Dinitscriptdir=",
        ],
        "usr/lib/x86_64-linux-gnu/libfuse3.so.4",
        &[],
    )
}

fn build_ostree(repo_root: &Path) -> Result<()> {
    build_autotools_import(
        repo_root,
        "ostree",
        "src/system/packages/ostree",
        &[
            "glib",
            "pcre2",
            "libffi",
            "zlib",
            "bzip2",
            "lz4",
            "xz",
            "zstd",
            "curl",
            "openssl",
            "libarchive",
            "libxml2",
            "fuse3",
            "gpgme",
            "libassuan",
            "libgpg-error",
            "gpgv",
            "libmd",
            "libbsd",
            "installer",
        ],
        &[
            "--host=x86_64-linux-gnu",
            "--prefix=/usr",
            "--libdir=/usr/lib/x86_64-linux-gnu",
            "--disable-tests",
            "--disable-man",
            "--disable-gtk-doc",
            "--disable-introspection",
            "--with-gpgme",
            // Flatpak pulls OSTree commits from HTTPS remotes such as
            // Flathub.  The target-built curl stage is the selected fetcher
            // backend; disabling both Soup backends remains intentional.
            "--with-curl",
            "--disable-selinux",
            "--disable-composefs",
            "--disable-systemd",
            "--disable-rofiles-fuse",
            "--with-soup3=no",
            "--with-soup=no",
            "LIBS=-lbsd",
        ],
        &["usr/lib/x86_64-linux-gnu/libostree-1.so.1"],
    )
}

fn build_duktape(repo_root: &Path) -> Result<()> {
    let out_root = repo_root.join("out/build/duktape");
    let source = out_root.join("source");
    let install = out_root.join("install/usr");
    sync_build_source(&repo_root.join("src/system/security/duktape"), &source)?;
    remove_path_if_exists(&install)?;
    let configure = source.join("tools/configure.py");
    let configure_body = fs::read_to_string(&configure)?
        .replace("open(apiheader_filename, 'rb')", "open(apiheader_filename, 'r')")
        .replace("open(src, 'rb')", "open(src, 'r', encoding='utf-8')")
        .replace("open(dst, 'wb')", "open(dst, 'w', encoding='utf-8')")
        .replace("open(value, 'rb')", "open(value, 'r', encoding='utf-8')")
        .replace("open(license_file, 'rb')", "open(license_file, 'r', encoding='utf-8')")
        .replace("open(authors_file, 'rb')", "open(authors_file, 'r', encoding='utf-8')")
        .replace("open(tmpfn, 'wb')", "open(tmpfn, 'w', encoding='utf-8')")
        .replace("open(tmpfn, 'rb')", "open(tmpfn, 'r', encoding='utf-8')")
        .replace("open(os.path.join(tempdir, suffix + '.txt'), 'wb')", "open(os.path.join(tempdir, suffix + '.txt'), 'w', encoding='utf-8')")
        .replace("open(os.path.join(tempdir, 'caseconv.txt'), 'wb')", "open(os.path.join(tempdir, 'caseconv.txt'), 'w', encoding='utf-8')")
        .replace("open(os.path.join(tempdir, 'caseconv_re_canon_lookup.txt'), 'wb')", "open(os.path.join(tempdir, 'caseconv_re_canon_lookup.txt'), 'w', encoding='utf-8')")
        .replace("open(os.path.join(tempdir, 'caseconv_re_canon_bitmap.txt'), 'wb')", "open(os.path.join(tempdir, 'caseconv_re_canon_bitmap.txt'), 'w', encoding='utf-8')")
        .replace("open(os.path.join(tempdir, 'duk_used_stridx_bidx_defs.json.tmp'), 'wb')", "open(os.path.join(tempdir, 'duk_used_stridx_bidx_defs.json.tmp'), 'w', encoding='utf-8')")
        .replace("'rb')", "'r', encoding='utf-8')")
        .replace("'wb')", "'w', encoding='utf-8')")
        .replace("line = line.decode('utf-8')", "line = line")
        .replace("f.write(i)", "f.write(i.decode('utf-8') if isinstance(i, bytes) else i)")
        .replace("ret = proc.communicate(input=input)", "ret = proc.communicate(input=input)\n        ret = (ret[0].decode('utf-8'), ret[1].decode('utf-8'))")
        .replace("f.write(res.decode('utf-8'))", "f.write(res)")
        .replace("f.write(i)", "f.write(i)")
        .replace("f_out.write(f_in.read())", "f_out.write(f_in.read())")
        .replace("f_out.write(c.encode('ascii'))", "f_out.write(c)")
        .replace("f.write(json.dumps(doc, indent=4))", "f.write(json.dumps(doc, indent=4))")
        .replace("duk_version / 10000", "duk_version // 10000")
        .replace("duk_version % 10000 / 100", "duk_version % 10000 // 100");
    fs::write(&configure, configure_body)?;
    let scanner = source.join("tools/scan_used_stridx_bidx.py");
    let scanner_body =
        fs::read_to_string(&scanner)?.replace("open(fn, 'rb')", "open(fn, 'r', encoding='utf-8')");
    fs::write(scanner, scanner_body)?;
    let genconfig = source.join("tools/genconfig.py");
    let mut genconfig_body = fs::read_to_string(&genconfig)?
        .replace(
            "import logging",
            "unicode = str\nlong = int\nxrange = range\nimport logging",
        )
        .replace("'rb')", "'r', encoding='utf-8')")
        .replace("'wb')", "'w', encoding='utf-8')")
        .replace("yaml.load(", "yaml.safe_load(")
        .replace(
            "import logging",
            "from functools import cmp_to_key\nimport logging",
        )
        .replace(
            "strs.sort(cmp=sortCmp)",
            "strs.sort(key=cmp_to_key(sortCmp))",
        );
    for (old, new) in [
        ("self.provides.has_key(m)", "m in self.provides"),
        ("assumed_provides.has_key(k)", "k in assumed_provides"),
        ("sn2.provides.has_key(k)", "k in sn2.provides"),
        ("not graph.has_key(sn)", "sn not in graph"),
        ("handled.has_key(sn)", "sn in handled"),
        ("not handled.has_key(sn)", "sn not in handled"),
        ("handled.has_key(dep)", "dep in handled"),
        (
            "not emitted_provides.has_key(k)",
            "k not in emitted_provides",
        ),
        ("handled.has_key(dname)", "dname in handled"),
        ("not handled.has_key(dname)", "dname not in handled"),
        ("use_defs.has_key(k)", "k in use_defs"),
        ("defval.has_key('verbatim')", "'verbatim' in defval"),
        ("defval.has_key('string')", "'string' in defval"),
        (
            "not forced_opts.has_key(doc['define'])",
            "doc['define'] not in forced_opts",
        ),
        (
            "forced_opts.has_key('DUK_USE_CPP_EXCEPTIONS')",
            "'DUK_USE_CPP_EXCEPTIONS' in forced_opts",
        ),
        (
            "not forced_opts.has_key(defname)",
            "defname not in forced_opts",
        ),
        ("not doc.has_key('default')", "'default' not in doc"),
        ("tmp.provides.has_key(defname)", "defname in tmp.provides"),
        ("need.has_key(k)", "k in need"),
        (
            "not defs_used.has_key(meta['define'])",
            "meta['define'] not in defs_used",
        ),
        ("not meta.has_key('removed')", "'removed' not in meta"),
        ("keys = use_defs.keys()", "keys = list(use_defs.keys())"),
        ("keys = opt_defs.keys()", "keys = list(opt_defs.keys())"),
        (
            "use_tags_list = use_tags.keys()",
            "use_tags_list = list(use_tags.keys())",
        ),
    ] {
        genconfig_body = genconfig_body.replace(old, new);
    }
    genconfig_body = rewrite_python2_has_key(genconfig_body);
    fs::write(genconfig, genconfig_body)?;
    let genbuiltins = source.join("tools/genbuiltins.py");
    let mut genbuiltins_body = fs::read_to_string(&genbuiltins)?
        .replace("#!/usr/bin/env python2", "#!/usr/bin/env python3")
        .replace("import logging", "import base64\nunicode = str\nunichr = chr\nlong = int\nxrange = range\ncmp = lambda a, b: (a > b) - (a < b)\nimport logging")
        .replace("except Exception, e:", "except Exception as e:")
        .replace("'rb')", "'r', encoding='utf-8')")
        .replace("'wb')", "'w', encoding='utf-8')")
        .replace("yaml.load(", "yaml.safe_load(")
        .replace("strs.sort(cmp=sortCmp)", "strs.sort(key=cmp_to_key(sortCmp))")
        .replace("val['bytes'].decode('hex')", "bytes.fromhex(val['bytes'])")
        .replace("val.decode('hex')", "bytes.fromhex(val)")
        .replace("data = ''.join([ val[indexlist[idx]] for idx in xrange(8) ])", "data = bytes([val[indexlist[idx]] for idx in xrange(8)])")
        .replace("val.encode('hex')", "val.hex()")
        .replace("data.encode('hex')", "data.hex()")
        .replace("struct.pack('>d', float(v)).encode('hex')", "struct.pack('>d', float(v)).hex()")
        .replace("ord(c2)", "(c2 if isinstance(c2, int) else ord(c2))")
        .replace("ord(c)", "(c if isinstance(c, int) else ord(c))")
        .replace("ord(val[i])", "(val[i] if isinstance(val[i], int) else ord(val[i]))")
        .replace("ord(v[0])", "(v[0] if isinstance(v[0], int) else ord(v[0]))")
        .replace("for idx, c in enumerate(s):", "for idx, c in enumerate(s):\n        c = chr(c) if isinstance(c, int) else c")
        .replace("c2 = s[idx+1]", "c2 = s[idx+1]\n            c2 = chr(c2) if isinstance(c2, int) else c2")
        .replace("unicode_to_bytes(s['str']).encode('base64').strip()", "base64.b64encode(unicode_to_bytes(s['str']).encode('utf-8')).decode('ascii').strip()")
        .replace("import logging", "from functools import cmp_to_key\nimport logging");
    for (old, new) in [
        (
            "user_meta.has_key('add_objects')",
            "'add_objects' in user_meta",
        ),
        (
            "user_meta.has_key('replace_objects')",
            "'replace_objects' in user_meta",
        ),
        (
            "user_meta.has_key('modify_objects')",
            "'modify_objects' in user_meta",
        ),
        ("if o.has_key('nargs')", "if 'nargs' in o"),
        ("assert(o.has_key('nargs'))", "assert('nargs' in o)"),
        ("not pval.has_key('length')", "'length' not in pval"),
        ("not pval.has_key('nargs')", "'nargs' not in pval"),
        ("not val.has_key('getter')", "'getter' not in val"),
        ("not val.has_key('setter')", "'setter' not in val"),
        ("prop.has_key(k)", "k in prop"),
        ("val['value'].has_key('getter')", "'getter' in val['value']"),
        ("val['value'].has_key('setter')", "'setter' in val['value']"),
        ("if o.has_key('native')", "if 'native' in o"),
        ("and not o.has_key('bidx')", "and 'bidx' not in o"),
        ("prop.has_key('value')", "'value' in prop"),
        ("targ.has_key('magic')", "'magic' in targ"),
        ("not reachable.has_key(o['id'])", "o['id'] not in reachable"),
        ("special_defs.has_key(v)", "v in special_defs"),
        ("s.has_key('define')", "'define' in s"),
        (
            "defs_needed.has_key(s['define'])",
            "s['define'] in defs_needed",
        ),
        ("not defs_found.has_key(k)", "k not in defs_found"),
        ("prev.has_key(k)", "k in prev"),
        ("kw_index.has_key(s['str'])", "s['str'] in kw_index"),
        (
            "meta.has_key('objects_ram_toplevel')",
            "'objects_ram_toplevel' in meta",
        ),
        ("elem.has_key('type')", "'type' in elem"),
        ("bi.has_key('nargs')", "'nargs' in bi"),
        ("bi.has_key('callable')", "'callable' in bi"),
        (
            "bi.has_key('internal_prototype')",
            "'internal_prototype' in bi",
        ),
        ("not emitted.has_key(fname)", "fname not in emitted"),
        ("v.has_key('getter_id')", "'getter_id' in v"),
        ("v.has_key('length')", "'length' in v"),
        ("v.has_key('magic')", "'magic' in v"),
        (
            "not chain_lens.has_key(chainlen)",
            "chainlen not in chain_lens",
        ),
        ("reserved_words.has_key(v)", "v in reserved_words"),
        (
            "strict_reserved_words.has_key(v)",
            "v in strict_reserved_words",
        ),
        ("romstr_next.has_key(v)", "v in romstr_next"),
        (
            "if obj.has_key('internal_prototype')",
            "if 'internal_prototype' in obj",
        ),
        ("elif obj.has_key('nargs')", "elif 'nargs' in obj"),
        ("not emitted.has_key(fname)", "fname not in emitted"),
        ("assert(v.has_key('native'))", "assert('native' in v)"),
        ("target.has_key('native')", "'native' in target"),
        ("not reachable.has_key(o['id'])", "o['id'] not in reachable"),
        ("string_to_stridx.has_key(val)", "val in string_to_stridx"),
        ("val.has_key('getter_id')", "'getter_id' in val"),
        ("val.has_key('setter_id')", "'setter_id' in val"),
        ("funobj.has_key('nargs')", "'nargs' in funobj"),
        ("not defs_found.has_key(k)", "k not in defs_found"),
        (
            "metadata_lookup_object(meta, prop['value']['id']).has_key('native')",
            "'native' in metadata_lookup_object(meta, prop['value']['id'])",
        ),
        (
            "not metadata_lookup_object(meta, prop['value']['id']).has_key('bidx')",
            "'bidx' not in metadata_lookup_object(meta, prop['value']['id'])",
        ),
    ] {
        genbuiltins_body = genbuiltins_body.replace(old, new);
    }
    fs::write(genbuiltins, genbuiltins_body)?;
    let dukutil = source.join("tools/dukutil.py");
    let dukutil_body = fs::read_to_string(&dukutil)?
        .replace("xrange", "range")
        .replace("unicode", "str")
        // Duktape's generators use Python 2 ``str`` as a byte string.  The
        // Python 3 compatibility alias above makes that value a Unicode
        // string, so emitArray() must restore the original one-byte mapping
        // instead of UTF-8 expanding values above 0xff.  Those expansions
        // silently truncate generated tables in C and make every Duktape
        // evaluation fatal at runtime.
        .replace("data = data.encode('utf-8')", "data = data.encode('latin-1')")
        .replace("return nbits / 8", "return nbits // 8")
        .replace("(skip * (res % 256)) / 256", "(skip * (res % 256)) // 256")
        .replace(
            "ord(x[i])",
            "(x[i] if isinstance(x[i], int) else ord(x[i]))",
        );
    fs::write(dukutil, dukutil_body)?;
    let unicode_prepare = source.join("tools/prepare_unicode_data.py");
    let unicode_prepare_body = fs::read_to_string(&unicode_prepare)?
        .replace("#!/usr/bin/env python2", "#!/usr/bin/env python3")
        .replace("import os", "from functools import cmp_to_key\nlong = int\nxrange = range\ncmp = lambda a, b: (a > b) - (a < b)\nimport os")
        .replace("open(opts.unicode_data, 'rb')", "open(opts.unicode_data, 'r', encoding='utf-8')")
        .replace("open(opts.output, 'wb')", "open(opts.output, 'w', encoding='utf-8')");
    fs::write(unicode_prepare, unicode_prepare_body)?;
    let extract_chars = source.join("tools/extract_chars.py");
    let mut extract_chars_body = fs::read_to_string(&extract_chars)?
        .replace("#!/usr/bin/env python2", "#!/usr/bin/env python3")
        .replace("import os", "from functools import cmp_to_key\nlong = int\nxrange = range\ncmp = lambda a, b: (a > b) - (a < b)\nimport os")
        .replace("open(unidata, 'rb')", "open(unidata, 'r', encoding='utf-8')")
        .replace("open(opts.out_source, 'wb')", "open(opts.out_source, 'w', encoding='utf-8')")
        .replace("open(opts.out_header, 'wb')", "open(opts.out_header, 'w', encoding='utf-8')");
    for (old, new) in [
        (
            "exclude_cat_exact.has_key(category)",
            "category in exclude_cat_exact",
        ),
        (
            "include_cat_exact.has_key(category)",
            "category in include_cat_exact",
        ),
        ("m.has_key(long(cp))", "long(cp) in m"),
        (
            "print 'CATSEXC: %s' % repr(catsexc)",
            "print('CATSEXC: %s' % repr(catsexc))",
        ),
        (
            "print 'CATSINC: %s' % repr(catsinc)",
            "print('CATSINC: %s' % repr(catsinc))",
        ),
        (
            "print 'match table length: %d bytes' % len(matchtable3)",
            "print('match table length: %d bytes' % len(matchtable3))",
        ),
        ("print 'encoding freq:'", "print('encoding freq:')"),
        (
            "print '  %6d: %d' % (i, freq[i])",
            "print('  %6d: %d' % (i, freq[i]))",
        ),
    ] {
        extract_chars_body = extract_chars_body.replace(old, new);
    }
    extract_chars_body =
        extract_chars_body.replace("res.sort(cmp=mycmp)", "res.sort(key=cmp_to_key(mycmp))");
    fs::write(extract_chars, extract_chars_body)?;
    let extract_caseconv = source.join("tools/extract_caseconv.py");
    let mut extract_caseconv_body = fs::read_to_string(&extract_caseconv)?
        .replace("#!/usr/bin/env python2", "#!/usr/bin/env python3")
        .replace("import os", "from functools import cmp_to_key\nlong = int\nxrange = range\nunichr = chr\ncmp = lambda a, b: (a > b) - (a < b)\nimport os")
        .replace("open(filename, 'rb')", "open(filename, 'r', encoding='utf-8')")
        .replace("open(opts.out_source, 'wb')", "open(opts.out_source, 'w', encoding='utf-8')")
        .replace("open(opts.out_header, 'wb')", "open(opts.out_header, 'w', encoding='utf-8')")
        .replace("res.sort(cmp=mycmp)", "res.sort(key=cmp_to_key(mycmp))");
    for (old, new) in [
        ("convmap.has_key(i)", "i in convmap"),
        ("not convmap.has_key(conv_i)", "conv_i not in convmap"),
        ("not convmap.has_key(new_i)", "new_i not in convmap"),
        ("convmap.has_key(cp)", "cp in convmap"),
    ] {
        extract_caseconv_body = extract_caseconv_body.replace(old, new);
    }
    for (old, new) in [
        (
            "print '- singles: ' + repr(t)",
            "print('- singles: ' + repr(t))",
        ),
        (
            "print '- multis: ' + repr(t)",
            "print('- multis: ' + repr(t))",
        ),
        (
            "print '- range mappings: %d' % len(ranges)",
            "print('- range mappings: %d' % len(ranges))",
        ),
        (
            "print '- single character mappings: %d' % len(singles)",
            "print('- single character mappings: %d' % len(singles))",
        ),
        (
            "print '- complex mappings (1:n): %d' % len(multis)",
            "print('- complex mappings (1:n): %d' % len(multis))",
        ),
        (
            "print '- remaining (should be zero): %d' % len(convmap.keys())",
            "print('- remaining (should be zero): %d' % len(convmap.keys()))",
        ),
        (
            "print '- %d %d' % (t[0] - prev[0], t[1] - prev[1])",
            "print('- %d %d' % (t[0] - prev[0], t[1] - prev[1]))",
        ),
        (
            "print '- start: %d %d' % (t[0], t[1])",
            "print('- start: %d %d' % (t[0], t[1]))",
        ),
    ] {
        extract_caseconv_body = extract_caseconv_body.replace(old, new);
    }
    extract_caseconv_body =
        extract_caseconv_body.replace("k = convmap.keys()", "k = list(convmap.keys())");
    extract_caseconv_body = extract_caseconv_body
        .replace(
            "(conv_i - start_i) / skip + 1",
            "(conv_i - start_i) // skip + 1",
        )
        .replace("65536 / block_size", "65536 // block_size");
    fs::write(extract_caseconv, extract_caseconv_body)?;
    let combine_src = source.join("tools/combine_src.py");
    let mut combine_src_body = fs::read_to_string(&combine_src)?
        .replace("#!/usr/bin/env python2", "#!/usr/bin/env python3")
        .replace("import logging", "unicode = str\nimport logging")
        .replace(
            "open(filename, 'rb')",
            "open(filename, 'r', encoding='utf-8')",
        )
        .replace(
            "open(prologue_filename, 'rb')",
            "open(prologue_filename, 'r', encoding='utf-8')",
        )
        .replace(
            "open(opts.output_source, 'wb')",
            "open(opts.output_source, 'w', encoding='utf-8')",
        )
        .replace(
            "open(opts.output_metadata, 'wb')",
            "open(opts.output_metadata, 'w', encoding='utf-8')",
        )
        .replace(
            "apply(os.path.join, [ path ] + inccomp)",
            "os.path.join(path, *inccomp)",
        );
    for (old, new) in [
        ("defined.has_key(m.group(1))", "m.group(1) in defined"),
        ("included.has_key(incpath)", "incpath in included"),
    ] {
        combine_src_body = combine_src_body.replace(old, new);
    }
    fs::write(combine_src, combine_src_body)?;
    let prep = source.join("prep/nondebug");
    fs::create_dir_all(source.join("prep"))?;
    run_cmd(
        &source,
        "python3",
        &[
            "tools/configure.py",
            "--output-directory",
            "prep/nondebug",
            "--source-directory",
            "src-input",
            "--config-metadata",
            "config",
            "--option-file",
            "util/makeduk_base.yaml",
            "--line-directives",
        ],
    )?;
    fs::create_dir_all(install.join("lib/x86_64-linux-gnu/pkgconfig"))?;
    let lib = install.join("lib/x86_64-linux-gnu/libduktape.so.207.2.7.0");
    run_cmd_with_env_overrides(
        &source,
        "cc",
        &[
            "-shared",
            "-fPIC",
            "-O2",
            "-Iprep/nondebug",
            "-Wl,-soname,libduktape.so.207",
            "-o",
            path_str(&lib)?,
            "prep/nondebug/duktape.c",
            "-lm",
        ],
        &[],
    )?;
    std::os::unix::fs::symlink(
        "libduktape.so.207.2.7.0",
        install.join("lib/x86_64-linux-gnu/libduktape.so.207"),
    )?;
    std::os::unix::fs::symlink(
        "libduktape.so.207",
        install.join("lib/x86_64-linux-gnu/libduktape.so"),
    )?;
    fs::create_dir_all(install.join("include"))?;
    for name in ["duktape.h", "duk_config.h"] {
        fs::copy(prep.join(name), install.join("include").join(name))?;
    }
    fs::write(
        install.join("lib/x86_64-linux-gnu/pkgconfig/duktape.pc"),
        "prefix=/usr\nlibdir=${prefix}/lib/x86_64-linux-gnu\nincludedir=${prefix}/include\nName: duktape\nDescription: Duktape JavaScript engine\nVersion: 2.7.0\nLibs: -L${libdir} -lduktape\nCflags: -I${includedir}\n",
    )?;
    Ok(())
}

fn rewrite_python2_has_key(mut body: String) -> String {
    while let Some(marker) = body.find(".has_key(") {
        let lhs_end = marker;
        let mut lhs_start = lhs_end;
        while lhs_start > 0 {
            let byte = body.as_bytes()[lhs_start - 1];
            if byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'.' {
                lhs_start -= 1;
            } else {
                break;
            }
        }
        let arg_start = marker + ".has_key(".len();
        let Some(arg_end_rel) = body[arg_start..].find(')') else {
            break;
        };
        let arg_end = arg_start + arg_end_rel;
        let lhs = body[lhs_start..lhs_end].to_string();
        let arg = body[arg_start..arg_end].trim().to_string();
        let before = &body[..lhs_start];
        let negated = before.trim_end().ends_with("not");
        let replacement = if negated {
            format!("{} not in {}", arg, lhs)
        } else {
            format!("{} in {}", arg, lhs)
        };
        let replace_start = if negated {
            before.trim_end().len() - 3
        } else {
            lhs_start
        };
        body.replace_range(replace_start..=arg_end, &replacement);
    }
    body
}
