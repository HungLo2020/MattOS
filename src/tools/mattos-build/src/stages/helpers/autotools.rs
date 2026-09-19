fn build_autotools_import(
    repo_root: &Path,
    component: &str,
    source_relative: &str,
    dependencies: &[&str],
    options: &[&str],
    required_outputs: &[&str],
) -> Result<()> {
    let source = repo_root.join(source_relative);
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
    let adaptation_stamp = match component {
        "networkmanager" => "output-policy-install-adaptation-v4",
        "readline" => "output-pkgconfig-adaptation-v1",
        "ostree" => "output-submodule-and-docs-staging-adaptation-v5",
        // The libcanberra mirror carries generated Autotools files whose
        // build-aux entries are symlinks into the import host's automake
        // installation.  Regenerate them in the output-owned source mirror
        // so the build never depends on those stale host paths.
        "libcanberra" => "output-autoreconf-generated-build-aux-v1",
        "icu" => "output-icu-license-staging-v1",
        "libblockdev" => "output-autoconf-archive-debug-macro-v1",
        "bluez" => "host-configure-tools-and-readline-terminal-link-v2",
        "udisks2" => "output-disable-unbuilt-gtk-doc-resources-and-normalize-build-dir-v4",
        "gmp" => "skip-regenerated-info-docs-and-normalize-public-cflags-v2",
        _ => "",
    };
    let stamp = format!(
        "{state}\n{}\ndependencies={}\n{adaptation_stamp}\n",
        options.join("\n"),
        dependencies.join(",")
    );
    if fs::read_to_string(&stamp_path).ok().as_deref() != Some(stamp.as_str()) {
        remove_path_if_exists(&source_copy)?;
        remove_path_if_exists(&build_dir)?;
    }
    fs::create_dir_all(&out_root)?;
    sync_build_source(&source, &source_copy)?;
    if component == "libblockdev" {
        // AX_CHECK_ENABLE_DEBUG is supplied by autoconf-archive and affects
        // only compile-time debug defines. Keep that host build-tool macro
        // out of the target closure by spelling out the release result in
        // the disposable mirror before autoreconf.
        let configure_ac = source_copy.join("configure.ac");
        let body = fs::read_to_string(&configure_ac)?;
        let old = "AX_CHECK_ENABLE_DEBUG([no], [DEBUG], [NDEBUG])";
        if !body.contains(old) {
            bail!("libblockdev debug macro layout changed unexpectedly");
        }
        fs::write(
            configure_ac,
            body.replace(old, "AC_DEFINE([NDEBUG], [1], [Disable debug assertions])")
                .replace(
                    "AX_RECURSIVE_EVAL([$with_drivedb], [drivedb_path])",
                    "drivedb_path=\"$with_drivedb\"",
                ),
        )?;
    }
    if component == "udisks2" {
        // Documentation is outside the runtime closure. The release tree
        // expects gtk-doc's host Automake fragment even when disabled, so
        // make only the disposable documentation makefile inert.
        let configure_ac = source_copy.join("configure.ac");
        let body = fs::read_to_string(&configure_ac)?;
        let old = "GTK_DOC_CHECK([1.3],[--flavour no-tmpl])";
        if !body.contains(old) {
            bail!("UDisks2 gtk-doc configure layout changed unexpectedly");
        }
        fs::write(
            configure_ac,
            body.replace(old, "AM_CONDITIONAL([ENABLE_GTK_DOC], [false])"),
        )?;
        fs::write(source_copy.join("doc/Makefile.am"), "SUBDIRS =\n")?;
        let makefile = source_copy.join("src/Makefile.am");
        let body = fs::read_to_string(&makefile)?;
        let adjusted = body.replace(
            "--sourcedir=$(top_srcdir) udisks-daemon-resources.xml",
            "--sourcedir=$(top_srcdir) $(srcdir)/udisks-daemon-resources.xml",
        );
        if adjusted == body {
            bail!("UDisks2 out-of-tree resource adaptation no longer matches upstream");
        }
        fs::write(makefile, adjusted)?;
        // UDisks compiles its build directory into the daemon for its
        // explicit --uninstalled developer mode.  Keep that diagnostic mode
        // deterministic without exposing the build machine in the shipped
        // executable; installed operation never reads this path.
        for relative in ["src/Makefile.am"] {
            let path = source_copy.join(relative);
            let body = fs::read_to_string(&path)?;
            let old = "-DBUILD_DIR=\\\"$(abs_top_builddir)/\\\"";
            let new = "-DBUILD_DIR=\\\"/usr/src/mattos/udisks2-build/\\\"";
            let adjusted = body.replace(old, new);
            if adjusted == body && !body.contains(new) {
                bail!("UDisks2 BUILD_DIR normalization no longer matches {relative}");
            }
            fs::write(path, adjusted)?;
        }
    }
    if component == "icu" {
        // ICU's generated Makefile installs ../LICENSE relative to its
        // source directory.  The source stage intentionally builds only
        // icu4c/source, so materialize the immutable imported license at the
        // disposable build root rather than widening the staged source.
        let license = source
            .parent()
            .and_then(Path::parent)
            .map(|root| root.join("LICENSE"))
            .ok_or_else(|| anyhow!("unable to locate ICU imported license"))?;
        fs::copy(license, out_root.join("LICENSE"))?;
    }
    if component == "libcanberra" {
        // This old release expects gtk-doc's aclocal fragment even when
        // documentation is not enabled.  Keep the adaptation in the
        // disposable output mirror; the authoritative import is untouched.
        let gtk_doc_m4 = source_copy.join("m4/gtk-doc.m4");
        if gtk_doc_m4.is_symlink() || !gtk_doc_m4.is_file() {
            remove_path_if_exists(&gtk_doc_m4)?;
            fs::write(gtk_doc_m4, "dnl gtk-doc disabled for this target\n")?;
        }
        let gtk_doc_make = source_copy.join("gtk-doc.make");
        if gtk_doc_make.is_symlink() || !gtk_doc_make.is_file() {
            remove_path_if_exists(&gtk_doc_make)?;
            fs::write(gtk_doc_make, "# gtk-doc disabled for this target\n")?;
        }
        let gtk_doc_subdir_make = source_copy.join("gtkdoc/gtk-doc.make");
        if gtk_doc_subdir_make.is_symlink() || !gtk_doc_subdir_make.is_file() {
            remove_path_if_exists(&gtk_doc_subdir_make)?;
            fs::write(gtk_doc_subdir_make, "# gtk-doc disabled for this target\n")?;
        }
        // Automake 1.18 rejects the old gtk-doc template's `+=` without an
        // earlier assignment.  Documentation is outside this runtime
        // closure, so make the disposable documentation subdir inert too.
        let top_makefile = source_copy.join("Makefile.am");
        let top_contents = fs::read_to_string(&top_makefile)?;
        let top_adjusted = top_contents.replace("SUBDIRS = src gtkdoc doc", "SUBDIRS = src");
        if top_adjusted != top_contents {
            fs::write(top_makefile, top_adjusted)?;
        }
        let gtk_makefile = source_copy.join("gtkdoc/Makefile.am");
        let gtk_contents = fs::read_to_string(&gtk_makefile)?;
        let gtk_adjusted = gtk_contents.replace("EXTRA_DIST +=", "EXTRA_DIST =");
        if gtk_adjusted != gtk_contents {
            fs::write(gtk_makefile, gtk_adjusted)?;
        }
        let src_makefile = source_copy.join("src/Makefile.am");
        let src_contents = fs::read_to_string(&src_makefile)?;
        let src_adjusted = src_contents.replace("noinst_PROGRAMS = \\\n\ttest-canberra", "noinst_PROGRAMS =");
        if src_adjusted != src_contents {
            fs::write(src_makefile, src_adjusted)?;
        }
        // The target deliberately uses libcanberra's built-in null backend:
        // KWin needs the ABI for system-bell notifications, while loading
        // arbitrary backend DSOs would add libltdl and an unneeded audio
        // stack to the graphics closure.  The upstream configure check is
        // unconditional, so remove only that check in the disposable mirror.
        let configure_ac = source_copy.join("configure.ac");
        let configure_contents = fs::read_to_string(&configure_ac)?;
        let ltdl_check = "AC_CHECK_HEADER([ltdl.h],\n    [AC_CHECK_LIB([ltdl], [lt_dladvise_init], [LIBLTDL=-lltdl], [LIBLTDL=])],\n    [LIBLTDL=])\n\nAS_IF([test \"x$LIBLTDL\" = \"x\"],\n    [AC_MSG_ERROR([Unable to find libltdl.])])\nAC_SUBST([LIBLTDL])";
        let without_ltdl_check = configure_contents.replace(
            ltdl_check,
            "LIBLTDL=\nAC_SUBST([LIBLTDL])",
        );
        let without_vorbis_check = without_ltdl_check.replace(
            "PKG_CHECK_MODULES(VORBIS, [ vorbisfile ])",
            "VORBIS_CFLAGS=\nVORBIS_LIBS=",
        );
        let without_gtk_doc = without_vorbis_check.replace("GTK_DOC_CHECK(1.9)", "dnl gtk-doc disabled");
        if without_gtk_doc != configure_contents {
            fs::write(configure_ac, without_gtk_doc)?;
        }
        // libcanberra's sound-file reader requires Vorbis even for the null
        // backend.  Keep this optional decoder out of the minimal KWin ABI
        // closure with an output-only unsupported implementation; WAV remains
        // available and the public canberra ABI is unchanged.
        fs::write(
            source_copy.join("src/read-vorbis.c"),
            "#include <stdio.h>\n#include <stdint.h>\n#include <sys/types.h>\n#include \"canberra.h\"\n#include \"read-vorbis.h\"\nstruct ca_vorbis { int unused; };\nint ca_vorbis_open(ca_vorbis **v, FILE *f) { (void)f; *v = NULL; return CA_ERROR_NOTSUPPORTED; }\nvoid ca_vorbis_close(ca_vorbis *v) { (void)v; }\nunsigned ca_vorbis_get_nchannels(ca_vorbis *v) { (void)v; return 0; }\nunsigned ca_vorbis_get_rate(ca_vorbis *v) { (void)v; return 0; }\nconst ca_channel_position_t *ca_vorbis_get_channel_map(ca_vorbis *v) { (void)v; return NULL; }\nint ca_vorbis_read_s16ne(ca_vorbis *v, int16_t *d, size_t *n) { (void)v; (void)d; (void)n; return CA_ERROR_NOTSUPPORTED; }\noff_t ca_vorbis_get_size(ca_vorbis *v) { (void)v; return 0; }\n",
        )?;
    }
    if component == "ostree" {
        // The release repository keeps this generated include out of the
        // source tree.  Materialize it in the output mirror before
        // autoreconf; authoritative imported source remains unchanged.
        for (directory, template_name, variable) in [
            ("libglnx", "Makefile-libglnx.am", "$$(libglnx_srcpath)"),
            ("bsdiff", "Makefile-bsdiff.am", "$$(libbsdiff_srcpath)"),
        ] {
            let generated = source_copy
                .join(directory)
                .join(format!("{template_name}.inc"));
            if !generated.is_file() {
                let template = fs::read_to_string(source_copy.join(directory).join(template_name))?;
                fs::write(generated, template.replace(variable, directory))?;
            }
        }
        // gtk-doc is disabled for the target package, but automake still
        // parses the conditional apidoc makefile and requires this generated
        // include to exist during autoreconf.
        let gtk_doc_make = source_copy.join("gtk-doc.make");
        if !gtk_doc_make.is_file() {
            fs::write(gtk_doc_make, "# gtk-doc disabled in this MattOS build\n")?;
        }
        let makefile = source_copy.join("Makefile.am");
        let make_contents = fs::read_to_string(&makefile)?;
        let make_without_apidoc = make_contents.replace(
            "if ENABLE_GTK_DOC\nSUBDIRS += apidoc\nendif\n",
            "# gtk-doc disabled in this MattOS build\n",
        );
        if make_without_apidoc != make_contents {
            fs::write(makefile, make_without_apidoc)?;
        }
        let configure = source_copy.join("configure.ac");
        let configure_contents = fs::read_to_string(&configure)?;
        let configure_without_apidoc = configure_contents.replace("apidoc/Makefile\n", "");
        if configure_without_apidoc != configure_contents {
            fs::write(configure, configure_without_apidoc)?;
        }
        let syscall_header = source_copy.join("libglnx/glnx-missing-syscall.h");
        let syscall_contents = fs::read_to_string(&syscall_header)?;
        let syscall_fixed = syscall_contents.replace(
            "#if !HAVE_DECL_NAME_TO_HANDLE_AT && defined(__NR_name_to_handle_at)",
            "#if defined(HAVE_DECL_NAME_TO_HANDLE_AT) && !HAVE_DECL_NAME_TO_HANDLE_AT && defined(__NR_name_to_handle_at)",
        );
        if syscall_fixed != syscall_contents {
            fs::write(syscall_header, syscall_fixed)?;
        }
        let dump = source_copy.join("src/ostree/ot-dump.c");
        let dump_contents = fs::read_to_string(&dump)?;
        let dump_fixed = dump_contents
            .replace("#include <bsd/err.h>", "#include <err.h>")
            .replace(
                "errx (1, \"Failed to read commit: %s\",",
                "g_error (\"Failed to read commit: %s\",",
            );
        if dump_fixed != dump_contents {
            fs::write(dump, dump_fixed)?;
        }
        let err_compat = source_copy.join("mattos-err-compat.h");
        fs::write(
            &err_compat,
            "#ifndef MATTOS_OSTREE_ERR_COMPAT_H\n#define MATTOS_OSTREE_ERR_COMPAT_H\n#include <stdarg.h>\nvoid err(int, const char *, ...);\nvoid errx(int, const char *, ...);\n#endif\n",
        )?;
    }
    if component == "libcanberra" || !source_copy.join("configure").is_file() {
        run_cmd(&source_copy, "autoreconf", &["-fiv"])?;
    }
    let mut env = staged_library_environment(repo_root, dependencies)?;
    if component == "gmp" {
        // The immutable VCS snapshot intentionally has no generated
        // version.texi. Documentation is outside the runtime/development
        // package closure, so never regenerate it with a host makeinfo.
        env.push(("MAKEINFO", "true".to_string()));
    }
    if component == "bluez" {
        // Configure invokes host gawk while generating config.status. Never
        // make host build tools load target readline/ncurses libraries;
        // target linking remains governed by LDFLAGS/LIBRARY_PATH.
        if let Some((_, value)) = env.iter_mut().find(|(key, _)| *key == "LD_LIBRARY_PATH") {
            value.clear();
        }
        env.push(("LIBS", "-lncursesw -ltinfow".to_string()));
    }
    if component == "udisks2" {
        // UDisks consumes libmount directly. util-linux's target pkg-config
        // file exposes optional static SELinux metadata that is not part of
        // this dynamic runtime closure, so provide the exact target ABI here.
        let util_usr = repo_root.join("out/build/util-linux/install/usr");
        env.push((
            "LIBMOUNT_CFLAGS",
            format!("-I{}", util_usr.join("include").display()),
        ));
        env.push((
            "LIBMOUNT_LIBS",
            "-lmount".to_string(),
        ));
        env.push((
            "GETTEXTDATADIRS",
            repo_root
                .join("out/build/polkit/install/usr/share/gettext")
                .display()
                .to_string(),
        ));
    }
    if component == "ostree" {
        // libbsd's compatibility headers include the target libc headers by
        // their normal names.  Its nested `include/bsd` directory must not
        // be placed on the general include search path: doing so makes
        // <sys/cdefs.h> resolve to bsd/sys/cdefs.h and recurse into itself
        // under the MattOS sysroot.  Keep libbsd's public root available and
        // link it explicitly below, but remove only this accidental nested
        // include directory from the generated environment.
        let libbsd_nested = repo_root
            .join("out/build/libbsd/install/usr/include/bsd")
            .display()
            .to_string();
        for (key, value) in &mut env {
            if *key == "CPPFLAGS" {
                *value = value
                    .split_whitespace()
                    .filter(|flag| *flag != format!("-I{libbsd_nested}"))
                    .collect::<Vec<_>>()
                    .join(" ");
                value.push_str(&format!(
                    " -include {}",
                    source_copy.join("mattos-err-compat.h").display()
                ));
            }
        }
        // e2p is part of the target-owned e2fsprogs development install,
        // which is produced as an installer sub-output rather than a
        // standalone BuildStage.
        let e2fs_usr = repo_root.join("out/build/e2fsprogs/install/usr");
        let e2fs_include = e2fs_usr.join("include");
        let e2fs_lib = e2fs_usr.join("lib/x86_64-linux-gnu");
        let e2fs_pc = e2fs_lib.join("pkgconfig");
        for (key, value) in &mut env {
            if *key == "CPPFLAGS" {
                value.push_str(&format!(" -I{}", e2fs_include.display()));
            } else if *key == "LDFLAGS" {
                value.push_str(&format!(
                    " -L{} -Wl,-rpath-link,{}",
                    e2fs_lib.display(),
                    e2fs_lib.display()
                ));
            } else if *key == "LIBRARY_PATH" || *key == "LD_LIBRARY_PATH" {
                *value = format!("{}:{}", e2fs_lib.display(), value);
            } else if *key == "PKG_CONFIG_PATH" || *key == "PKG_CONFIG_LIBDIR" {
                *value = format!("{}:{}", e2fs_pc.display(), value);
            }
        }
    }
    if component == "libblockdev" {
        // Upstream's custom header probe invokes ${CC} directly and omits
        // CPPFLAGS, unlike normal Autoconf compile checks. That hides
        // target-owned headers and can only succeed from a host installation.
        let macros = source_copy.join("acinclude.m4");
        let contents = fs::read_to_string(&macros)?;
        let adjusted = contents
            .replace(
                "${CC} -c [$2] $temp_file",
                "${CC} [$]{CPPFLAGS} -c [$2] $temp_file",
            )
            .replace(
                "${CC} ${CPPFLAGS} -c [$2] $temp_file",
                "${CC} [$]{CPPFLAGS} -c [$2] $temp_file",
            );
        if adjusted == contents
            && !contents.contains("${CC} [$]{CPPFLAGS} -c [$2] $temp_file")
        {
            bail!("libblockdev header-probe isolation adaptation no longer matches upstream");
        }
        if adjusted != contents {
            fs::write(macros, adjusted)?;
        }
        let configure_ac = source_copy.join("configure.ac");
        let contents = fs::read_to_string(&configure_ac)?;
        let adjusted = contents.replace(
            "LIBBLOCKDEV_CHECK_HEADER([keyutils.h], [], [keyutils.h not available])",
            "LIBBLOCKDEV_CHECK_HEADER([keyutils.h], [${CPPFLAGS}], [keyutils.h not available])",
        );
        if adjusted == contents
            && !contents.contains("LIBBLOCKDEV_CHECK_HEADER([keyutils.h], [${CPPFLAGS}], [keyutils.h not available])")
        {
            bail!("libblockdev keyutils probe adaptation no longer matches upstream");
        }
        if adjusted != contents {
            fs::write(configure_ac, adjusted)?;
        }
        // The source import does not carry generated configure output. The
        // first autoreconf above necessarily ran before this isolated probe
        // correction, so regenerate once with the corrected inputs.
        run_cmd(&source_copy, "autoreconf", &["-fiv"])?;
        // libmount's target .pc correctly records libselinux/libsepol as
        // private static-link dependencies. libblockdev dynamically links
        // libmount, so do not expand that unrelated static closure here.
        let util_usr = repo_root.join("out/build/util-linux/install/usr");
        env.push((
            "MOUNT_CFLAGS",
            format!("-I{}", util_usr.join("include/libmount").display()),
        ));
        env.push((
            "MOUNT_LIBS",
            "-lmount".to_string(),
        ));
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
    if component == "lvm2" {
        // Cryptsetup only requires libdevmapper.  Upstream explicitly
        // supports this target without libaio; building the unrelated LVM
        // command suite would add libaio and daemon policy to this closure.
        run_cmd_with_env_overrides(&build_dir, "make", &["-j", "4", "device-mapper"], &env)?;
    } else {
        run_cmd_with_env_overrides(&build_dir, "make", &["-j", "4"], &env)?;
    }
    remove_path_if_exists(&install_dir)?;
    let destdir = format!("DESTDIR={}", install_dir.display());
    let install_target = if component == "lvm2" { "install_device-mapper" } else { "install" };
    run_cmd_with_env_overrides(&build_dir, "make", &[install_target, &destdir], &env)?;
    for relative in required_outputs {
        if !install_dir.join(relative).is_file() {
            bail!("{component} install did not produce {relative}");
        }
    }
    fs::write(stamp_path, stamp)?;
    Ok(())
}

fn build_file(repo_root: &Path) -> Result<()> {
    build_autotools_import(
        repo_root,
        "file",
        "src/userland/file",
        &["zlib"],
        &[
            "--prefix=/usr",
            "--libdir=/usr/lib/x86_64-linux-gnu",
            "--disable-static",
            // libseccomp is not in this stage's declared MattOS closure.
            // Do not let configure discover a host library and then compile
            // a target binary against headers MattOS does not provide.
            "--disable-libseccomp",
        ],
        &[
            "usr/bin/file",
            "usr/lib/x86_64-linux-gnu/libmagic.so.1",
            "usr/share/misc/magic.mgc",
        ],
    )
}

fn build_less(repo_root: &Path) -> Result<()> {
    build_release_autotools_program(
        repo_root,
        "less",
        "less-704.tar.gz",
        LESS_RELEASE_ARCHIVE_URL,
        LESS_RELEASE_ARCHIVE_SHA256,
        &["ncurses", "pcre2"],
        &["--prefix=/usr", "--sysconfdir=/etc", "--with-regex=pcre2"],
        &["usr/bin/less", "usr/bin/lesskey", "usr/libexec/lessecho"],
    )
}

fn build_git(repo_root: &Path) -> Result<()> {
    let source = repo_root.join("src/userland/git");
    let out_root = repo_root.join("out/build/git");
    let source_copy = out_root.join("source");
    let install_dir = out_root.join("install");
    fs::create_dir_all(&out_root)?;
    sync_build_source(&source, &source_copy)?;
    let env = staged_library_environment(
        repo_root,
        &["curl", "expat", "openssl", "zlib", "zstd", "pcre2"],
    )?;
    let curl_config = repo_root.join("out/build/curl/install/usr/bin/curl-config");
    if !curl_config.is_file() {
        bail!(
            "Git requires MattOS curl-config at {}",
            curl_config.display()
        );
    }
    let common = vec![
        "prefix=/usr".to_string(),
        "NO_GETTEXT=YesPlease".to_string(),
        "NO_TCLTK=YesPlease".to_string(),
        "NO_PERL=YesPlease".to_string(),
        "NO_PYTHON=YesPlease".to_string(),
        "NO_RUST=YesPlease".to_string(),
        "USE_LIBPCRE2=YesPlease".to_string(),
        format!("CURL_CONFIG={}", curl_config.display()),
        "CURL_LDFLAGS=-lcurl".to_string(),
    ];
    let mut build_args = vec!["-j", "4"];
    build_args.extend(common.iter().map(String::as_str));
    run_cmd_with_env_overrides(&source_copy, "make", &build_args, &env)?;
    remove_path_if_exists(&install_dir)?;
    let destdir = format!("DESTDIR={}", install_dir.display());
    let mut install_args = vec!["install", destdir.as_str()];
    install_args.extend(common.iter().map(String::as_str));
    run_cmd_with_env_overrides(&source_copy, "make", &install_args, &env)?;
    for rel in ["usr/bin/git", "usr/libexec/git-core/git-remote-https"] {
        if !install_dir.join(rel).is_file() {
            bail!("Git install did not produce {rel}");
        }
    }
    Ok(())
}

fn build_openssh(repo_root: &Path) -> Result<()> {
    build_autotools_import(
        repo_root,
        "openssh",
        "src/system/network/openssh-portable",
        &["openssl", "zlib", "zstd", "linux-pam", "libxcrypt"],
        &[
            "--prefix=/usr",
            "--sysconfdir=/etc/ssh",
            "--sbindir=/usr/sbin",
            "--libexecdir=/usr/lib/openssh",
            "--with-pam",
            "--with-privsep-path=/run/sshd",
            "--with-privsep-user=sshd",
            "--with-default-path=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
        ],
        &["usr/bin/ssh", "usr/sbin/sshd", "usr/bin/ssh-keygen"],
    )
}

fn build_libffi(repo_root: &Path) -> Result<()> {
    build_autotools_import(
        repo_root,
        "libffi",
        "src/system/libraries/libffi/libffi",
        &[],
        &[
            "--prefix=/usr",
            "--libdir=/usr/lib/x86_64-linux-gnu",
            "--disable-static",
            "--disable-docs",
            "--disable-multi-os-directory",
        ],
        &[
            "usr/lib/x86_64-linux-gnu/libffi.so.8",
            "usr/include/ffi.h",
            "usr/include/ffitarget.h",
        ],
    )
}

/// Build the Wayland client runtime used by native desktop clients.
/// Winit loads libwayland-client with dlopen, so it is not visible to the ELF
/// NEEDED audit and must be represented as an explicit source-built runtime
/// dependency rather than falling back to a host library.
fn build_wayland(repo_root: &Path) -> Result<()> {
    build_meson_runtime(
        repo_root,
        "wayland",
        "src/system/libraries/wayland",
        &["libffi"],
        &[
            "--prefix=/usr",
            "--libdir=lib/x86_64-linux-gnu",
            "-Dlibraries=true",
            // The source tree uses its own scanner to generate the protocol
            // glue for the libraries.  Build it in the output mirror; it is
            // deliberately not shipped by the runtime package.
            "-Dscanner=true",
            "-Dtests=false",
            "-Ddocumentation=false",
            "-Ddtd_validation=false",
        ],
        "usr/lib/x86_64-linux-gnu/libwayland-client.so.0",
        &[],
    )
}

/// Build the xkbcommon runtime ABI used by both Wayland and KWin's Xwayland
/// bridge.  The X11 companion library is a runtime requirement once KWin is
/// built with its supported `--xwayland` session mode, so it is kept in the
/// same source-owned component/package rather than being satisfied by a host
/// pkg-config file or library.
fn build_xkbcommon(repo_root: &Path) -> Result<()> {
    let source = repo_root.join("src/system/libraries/xkbcommon");
    if !source.join("meson.build").is_file() {
        bail!(
            "xkbcommon source not found in {}; run upstream import xkbcommon first",
            source.display()
        );
    }
    let out_root = repo_root.join("out/build/xkbcommon");
    let source_copy = out_root.join("source");
    let build_dir = out_root.join("build");
    let install_dir = out_root.join("install");
    let state = fs::read_to_string(repo_root.join("upstream/state/xkbcommon.toml"))?;
    let options = [
        "--prefix=/usr",
        "--libdir=lib/x86_64-linux-gnu",
        "-Denable-tools=false",
        "-Denable-x11=true",
        "-Denable-wayland=false",
        "-Denable-xkbregistry=true",
        "-Denable-docs=false",
        "-Denable-bash-completion=false",
        "-Dxkb-config-root=/usr/share/X11/xkb",
    ];
    let stamp = format!("{state}\n{}\n", options.join("\n"));
    let stamp_path = out_root.join("build-stamp.txt");
    if fs::read_to_string(&stamp_path).ok().as_deref() != Some(stamp.as_str()) {
        remove_path_if_exists(&source_copy)?;
        remove_path_if_exists(&build_dir)?;
    }
    fs::create_dir_all(&out_root)?;
    sync_build_source(&source, &source_copy)?;
    let env = staged_library_environment(repo_root, &["x11-compat", "libxml2"])?;
    if !build_dir.join("build.ninja").is_file() {
        let mut args = vec!["setup", path_str(&build_dir)?, path_str(&source_copy)?];
        args.extend(options);
        run_cmd_with_env_overrides(repo_root, "meson", &args, &env)?;
    } else {
        // Meson serializes its internal build model.  A build directory made
        // by an older Meson can still have build.ninja while meson compile
        // rejects build.dat; reconfigure the derived directory before use.
        let mut args = vec![
            "setup",
            "--reconfigure",
            path_str(&build_dir)?,
            path_str(&source_copy)?,
        ];
        args.extend(options);
        run_cmd_with_env_overrides(repo_root, "meson", &args, &env)?;
    }
    run_cmd_with_env_overrides(
        repo_root,
        "ninja",
        &[
            "-C",
            path_str(&build_dir)?,
            "libxkbcommon.so.0.9.2",
            "libxkbcommon-x11.so.0.9.2",
            "libxkbregistry.so.0.9.2",
        ],
        &env,
    )?;
    remove_path_if_exists(&install_dir)?;
    run_cmd_with_env_overrides(
        repo_root,
        "meson",
        &[
            "install",
            "-C",
            path_str(&build_dir)?,
            "--no-rebuild",
            "--destdir",
            path_str(&install_dir)?,
            "--tags",
            "runtime,devel",
        ],
        &env,
    )?;
    let soname = install_dir.join("usr/lib/x86_64-linux-gnu/libxkbcommon.so.0");
    if !soname.is_file() {
        bail!("xkbcommon install did not produce {}", soname.display());
    }
    fs::write(&stamp_path, stamp)?;
    println!(
        "xkbcommon origin: {}; features=x11/registry enabled, wayland/tools/docs disabled",
        install_dir.display()
    );
    Ok(())
}

/// Build generated XKB rules in an output-owned mirror.  The pinned upstream
/// Git tree contains source fragments; `rules/evdev` is a Meson output and
/// must never be generated inside the authoritative import.
fn build_xkeyboard_config(repo_root: &Path) -> Result<()> {
    let source = repo_root.join("src/system/data/xkeyboard-config");
    if !source.join("meson.build").is_file() {
        bail!(
            "xkeyboard-config source not found in {}; run upstream import xkeyboard-config first",
            source.display()
        );
    }
    let out_root = repo_root.join("out/build/xkeyboard-config");
    let source_copy = out_root.join("source");
    let build_dir = out_root.join("build");
    let install_dir = out_root.join("install");
    let state = fs::read_to_string(repo_root.join("upstream/state/xkeyboard-config.toml"))?;
    let options = ["--prefix=/usr", "--datadir=share", "-Dnls=false"];
    // Meson serializes its own version-sensitive state in build.dat.  Include
    // the active Meson identity in this output-owned stamp so a host Meson
    // upgrade cannot leave us reusing an incompatible build directory.
    let meson_version = run_cmd_capture(repo_root, "meson", &["--version"])?;
    let stamp = format!(
        "{state}\n{}\nmeson-version={meson_version}\n",
        options.join("\n")
    );
    let stamp_path = out_root.join("build-stamp.txt");
    if fs::read_to_string(&stamp_path).ok().as_deref() != Some(stamp.as_str()) {
        remove_path_if_exists(&source_copy)?;
        remove_path_if_exists(&build_dir)?;
    }
    fs::create_dir_all(&out_root)?;
    sync_build_source(&source, &source_copy)?;
    if !build_dir.join("build.ninja").is_file() {
        let mut args = vec!["setup", path_str(&build_dir)?, path_str(&source_copy)?];
        args.extend(options);
        run_cmd(repo_root, "meson", &args)?;
    }
    run_cmd(
        repo_root,
        "meson",
        &["compile", "-C", path_str(&build_dir)?],
    )?;
    remove_path_if_exists(&install_dir)?;
    run_cmd(
        repo_root,
        "meson",
        &[
            "install",
            "-C",
            path_str(&build_dir)?,
            "--destdir",
            path_str(&install_dir)?,
        ],
    )?;
    let rules = install_dir.join("usr/share/xkeyboard-config-2/rules/evdev");
    let legacy_root = install_dir.join("usr/share/X11/xkb");
    if !rules.is_file() || !legacy_root.is_symlink() {
        bail!("xkeyboard-config install did not produce generated rules or the legacy XKB symlink");
    }
    fs::write(&stamp_path, stamp)?;
    println!(
        "xkeyboard-config origin: {}; generated XKB rules in output-owned mirror",
        install_dir.display()
    );
    Ok(())
}
