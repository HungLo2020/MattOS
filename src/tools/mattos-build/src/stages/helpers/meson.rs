fn build_meson_runtime(
    repo_root: &Path,
    component: &str,
    source_relative: &str,
    dependencies: &[&str],
    options: &[&str],
    required_output: &str,
    extra_env: &[(&str, String)],
) -> Result<()> {
    let source = repo_root.join(source_relative);
    if !source.join("meson.build").is_file() {
        bail!(
            "{component} source not found in {}; run its upstream import first",
            source.display()
        );
    }
    let out_root = repo_root.join("out/build").join(component);
    let source_copy = out_root.join("source");
    let build_dir = out_root.join("build");
    let install_dir = out_root.join("install");
    // GStreamer core and plugins-base are separate stage outputs from one
    // immutable source superproject, so both derive their provenance stamp
    // from the single declared GStreamer import.
    let provenance_component = if component == "gstreamer-base" {
        "gstreamer"
    } else {
        component
    };
    let state = fs::read_to_string(
        repo_root
            .join("upstream/state")
            .join(format!("{provenance_component}.toml")),
    )?;
    let adaptation_stamp = match component {
        "networkmanager" => "output-policy-install-adaptation-v4",
        "polkit" => "output-duktape-link-adaptation-v2",
        "appstream" => "output-source-closure-adaptation-v2",
        "xdg-desktop-portal" => "output-owned-subproject-closure-v2",
        // gst-plugins-base asks GLib's staged gio-2.0.pc for variables that
        // describe GIO's *runtime* locations.  The descriptor deliberately
        // uses output-owned staging paths so native consumers can find the
        // target headers and libraries, but those paths must not be compiled
        // into an installed GStreamer plugin.
        "gstreamer-base" => "output-target-gio-runtime-paths-v1",
        _ => "",
    };
    // Meson stores compiler/build-tool state in build.dat.  A cache miss can
    // be caused by a changed dependency output while the component's own
    // recipe stamp remains unchanged; reusing that old Meson directory can
    // then fail (or, worse, consume stale dependency metadata).  Bind the
    // output-owned build directory to the actual producer output digests so
    // dependency changes force a fresh configure before compilation.
    let dependency_outputs = dependencies
        .iter()
        .map(|dependency| {
            let manifest = stage_cache::read_stage_manifest(repo_root, dependency)
                .with_context(|| format!("failed to read {dependency} dependency manifest"))?;
            Ok::<_, anyhow::Error>(format!("{dependency}={}", manifest.output_content_digest))
        })
        .collect::<Result<Vec<_>>>()?;
    let stamp = format!(
        "{state}\n{}\ndependencies={}\ndependency-outputs={}\n{adaptation_stamp}\n",
        options.join("\n"),
        dependencies.join(","),
        dependency_outputs.join(",")
    );
    let stamp_path = out_root.join("build-stamp.txt");
    if fs::read_to_string(&stamp_path).ok().as_deref() != Some(stamp.as_str()) {
        remove_path_if_exists(&source_copy)?;
        remove_path_if_exists(&build_dir)?;
    }
    fs::create_dir_all(&out_root)?;
    sync_build_source(&source, &source_copy)?;
    if component == "xdg-desktop-portal" {
        // The pinned portal release declares exact gvdb and libglnx Meson
        // wraps. Materialize those independently pinned MattOS imports in the
        // output-owned mirror so Meson never downloads a wrap or consults a
        // host copy. The authoritative portal and helper source trees remain
        // untouched.
        copy_tree_contents(
            &repo_root.join("src/system/packages/xdg-desktop-portal-gvdb"),
            &source_copy.join("subprojects/gvdb"),
        )?;
        copy_tree_contents(
            &repo_root.join("src/system/packages/xdg-desktop-portal-libglnx"),
            &source_copy.join("subprojects/libglnx"),
        )?;
        // Meson's find_program() resolves bwrap through the staged build
        // environment, which is correct for compiling the validators but
        // makes bwrap.full_path() an output-staging path.  The validator
        // embeds that value as its runtime helper.  Publish the target path
        // instead, in this disposable mirror only, so installed portal
        // helpers execute the packaged Bubblewrap and cannot contain a host
        // checkout path.
        let source_meson = source_copy.join("src/meson.build");
        let body = fs::read_to_string(&source_meson)?;
        let old = "'-DHELPER=\"@0@\"'.format(bwrap.full_path())";
        let replacement = "'-DHELPER=\"/usr/bin/bwrap\"'";
        let occurrences = body.matches(old).count();
        if occurrences != 2 {
            bail!(
                "xdg-desktop-portal Bubblewrap validator layout changed unexpectedly: expected two staged helper paths, found {occurrences}"
            );
        }
        fs::write(&source_meson, body.replace(old, replacement))?;
    }
    if component == "appstream" {
        // The host does not provide itstool.  AppStream's untranslated
        // release-note metadata is still a valid source-owned artifact, so
        // replace only the output mirror's optional localization join with a
        // deterministic install of that upstream XML.  The authoritative
        // imported source remains untouched.
        let data_meson = source_copy.join("data/meson.build");
        let body = fs::read_to_string(&data_meson)?;
        let start = body
            .find("metainfo_i18n = i18n.itstool_join(")
            .context("AppStream data layout changed: missing itstool join")?;
        let end = body[start..]
            .find("\n\n")
            .map(|offset| start + offset)
            .context("AppStream data layout changed: unterminated itstool join")?;
        let replacement = "metainfo_i18n = files('org.freedesktop.appstream.cli.metainfo.xml')\ninstall_data(metainfo_i18n, install_dir: metainfo_dir)";
        let adapted = format!("{}{}{}", &body[..start], replacement, &body[end..]);
        fs::write(data_meson, adapted)?;
    }
    if component == "flatpak" {
        // The system-helper policy is valid untranslated XML.  Meson's
        // i18n.merge_file() invokes msgfmt with ITS rules, but the target
        // build intentionally does not carry the host itstool rules.  Keep
        // the authoritative policy source intact and install it directly in
        // this output mirror; authorization semantics remain unchanged.
        let helper_meson = source_copy.join("system-helper/meson.build");
        let body = fs::read_to_string(&helper_meson)?;
        let old = "i18n.merge_file(\n  input : 'org.freedesktop.Flatpak.policy.in',\n  output : 'org.freedesktop.Flatpak.policy',\n  po_dir : '../po',\n  install : true,\n  install_dir : get_option('datadir') / 'polkit-1' / 'actions',\n)";
        let replacement = "install_data(\n  'org.freedesktop.Flatpak.policy.in',\n  rename : 'org.freedesktop.Flatpak.policy',\n  install_dir : get_option('datadir') / 'polkit-1' / 'actions',\n)";
        if !body.contains(old) {
            bail!("Flatpak system-helper policy layout changed unexpectedly");
        }
        fs::write(helper_meson, body.replace(old, replacement))?;
    }
    if component == "networkmanager" {
        let data_meson = source_copy.join("data/meson.build");
        let body = fs::read_to_string(&data_meson)?.replace(
            r#"  i18n.merge_file(
    input: 'org.freedesktop.NetworkManager.policy.in',
    output: '@BASENAME@',
    po_dir: po_dir,
    install: true,
    install_dir: polkit_policydir,
  )"#,
            r#"  install_data(
    'org.freedesktop.NetworkManager.policy.in',
    rename: 'org.freedesktop.NetworkManager.policy',
    install_dir: polkit_policydir,
  )"#,
        );
        fs::write(data_meson, body)?;
        let root_meson = source_copy.join("meson.build");
        let body = fs::read_to_string(&root_meson)?.replace(
            "readline_dep = declare_dependency(link_args: '-lreadline')",
            "readline_dep = declare_dependency(link_args: ['-lreadline', '-lncursesw', '-ltinfow'])",
        );
        fs::write(root_meson, body)?;
    }
    if component == "gstreamer-base" {
        // Keep the output-owned pkg-config view for build-time discovery, but
        // do not turn its physical staging prefix into installed runtime
        // configuration.  This is the target layout selected by this stage's
        // --prefix and --libdir options.  Apply it only in the disposable
        // source mirror: the authoritative GStreamer import remains exactly
        // pinned upstream source.
        let meson = source_copy.join("meson.build");
        let body = fs::read_to_string(&meson)?;
        let old = "if gio_dep.type_name() == 'pkgconfig'\n    core_conf.set_quoted('GIO_MODULE_DIR',\n        gio_dep.get_variable('giomoduledir'))\n    core_conf.set_quoted('GIO_LIBDIR',\n        gio_dep.get_variable('libdir'))\n    core_conf.set_quoted('GIO_PREFIX',\n        gio_dep.get_variable('prefix'))\nelse\n    core_conf.set_quoted('GIO_MODULE_DIR', join_paths(get_option('prefix'),\n      get_option('libdir'), 'gio/modules'))\n    core_conf.set_quoted('GIO_LIBDIR', join_paths(get_option('prefix'),\n      get_option('libdir')))\n    core_conf.set_quoted('GIO_PREFIX', join_paths(get_option('prefix')))\nendif";
        let replacement = "core_conf.set_quoted('GIO_MODULE_DIR', '/usr/lib/x86_64-linux-gnu/gio/modules')\ncore_conf.set_quoted('GIO_LIBDIR', '/usr/lib/x86_64-linux-gnu')\ncore_conf.set_quoted('GIO_PREFIX', '/usr')";
        if !body.contains(old) {
            bail!("GStreamer base GIO runtime configuration block changed unexpectedly");
        }
        fs::write(&meson, body.replace(old, replacement))?;
    }
    if component == "polkit" {
        let meson = source_copy.join("meson.build");
        let body = fs::read_to_string(&meson)?;
        let old = "  js_dep = dependency('duktape', version: duktape_req_version, required: false)\n  if not js_dep.found()\n    message('Falling back to looking for library and header...')\n    js_dep = cc.find_library('duktape', has_headers: ['duktape.h'], required: true)\n  endif";
        let replacement = format!(
            "  js_dep = declare_dependency(compile_args: ['-I{}'], link_args: ['-lduktape'])",
            repo_root
                .join("out/build/duktape/install/usr/include")
                .display()
        );
        if !body.contains(old) {
            bail!("polkit Duktape dependency block changed unexpectedly");
        }
        let body = body.replace(old, &replacement);
        fs::write(meson, body)?;
    }
    let mut env = staged_library_environment(repo_root, dependencies)?;
    if component == "flatpak" {
        if let Some((_, flags)) = env.iter_mut().find(|(key, _)| *key == "LDFLAGS") {
            flags.push_str(&format!(
                " -Wl,--no-as-needed {} {} -Wl,--as-needed",
                repo_root
                    .join("out/build/libxmlb/install/usr/lib/x86_64-linux-gnu/libxmlb.so.2")
                    .display(),
                repo_root
                    .join("out/build/libfyaml/install/usr/lib/x86_64-linux-gnu/libfyaml.so.0")
                    .display()
            ));
        }
    }
    env.extend(extra_env.iter().map(|(key, value)| (*key, value.clone())));
    if build_dir.join("build.ninja").is_file() {
        // Meson's serialized build state is version-sensitive.  Stage cache
        // validity deliberately does not make normal host-tool provenance a
        // rebuild input, so a legitimate dependency-output miss can enter
        // this helper with a build directory created by an older Meson.
        // Reconfigure before compiling or installing: this updates only the
        // disposable build directory and prevents `meson install` from
        // failing after successful compilation because it cannot read the
        // older build.dat.
        let mut args = vec![
            "setup",
            "--reconfigure",
            path_str(&build_dir)?,
            path_str(&source_copy)?,
        ];
        args.extend(options.iter().copied());
        run_cmd_with_env_overrides(repo_root, "meson", &args, &env)?;
    } else {
        let mut args = vec!["setup", path_str(&build_dir)?, path_str(&source_copy)?];
        args.extend(options.iter().copied());
        run_cmd_with_env_overrides(repo_root, "meson", &args, &env)?;
    }
    run_cmd_with_env_overrides(
        repo_root,
        "meson",
        &["compile", "-C", path_str(&build_dir)?],
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
            "--destdir",
            path_str(&install_dir)?,
        ],
        &env,
    )?;
    rewrite_staged_pkgconfig_files(&install_dir)?;
    let required = install_dir.join(required_output);
    if !required.is_file() {
        bail!("{component} install did not produce {}", required.display());
    }
    fs::write(&stamp_path, stamp)?;
    Ok(())
}
