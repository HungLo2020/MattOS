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
    if !source_copy.join("configure").is_file() {
        run_cmd(&source_copy, "autoreconf", &["-fiv"])?;
    }
    let mut env = staged_library_environment(repo_root, dependencies)?;
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
    fs::create_dir_all(&build_dir)?;
    if !build_dir.join("Makefile").is_file() {
        run_cmd_with_env_overrides(
            &build_dir,
            path_str(&source_copy.join("configure"))?,
            options,
            &env,
        )?;
    }
    run_cmd_with_env_overrides(&build_dir, "make", &["-j", "4"], &env)?;
    remove_path_if_exists(&install_dir)?;
    run_cmd_with_env_overrides(
        &build_dir,
        "make",
        &["install", &format!("DESTDIR={}", install_dir.display())],
        &env,
    )?;
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

/// Build the Wayland client runtime needed by the native COSMIC installer.
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

/// Build only libxkbcommon itself.  The native COSMIC installer dynamically
/// needs `libxkbcommon.so.0`; X11 helpers, Wayland helper tools, registry,
/// documentation, and shell completion are deliberately not source-closure
/// requirements for this runtime library.
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
        "-Denable-x11=false",
        "-Denable-wayland=false",
        "-Denable-xkbregistry=false",
        "-Denable-docs=false",
        "-Denable-bash-completion=false",
    ];
    let stamp = format!("{state}\n{}\n", options.join("\n"));
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
        run_cmd(repo_root, "meson", &args)?;
    }
    run_cmd(
        repo_root,
        "ninja",
        &["-C", path_str(&build_dir)?, "libxkbcommon.so.0.9.2"],
    )?;
    remove_path_if_exists(&install_dir)?;
    run_cmd(
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
    )?;
    let soname = install_dir.join("usr/lib/x86_64-linux-gnu/libxkbcommon.so.0");
    if !soname.is_file() {
        bail!("xkbcommon install did not produce {}", soname.display());
    }
    fs::write(&stamp_path, stamp)?;
    println!(
        "xkbcommon origin: {}; features=x11,wayland,tools,registry,docs disabled",
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
