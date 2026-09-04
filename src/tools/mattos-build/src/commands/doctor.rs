fn doctor() -> Result<()> {
    println!("MattOS doctor");

    if cfg!(windows) {
        bail!("MattOS build is Linux-native for this milestone; run doctor from Linux filesystem")
    }

    let mut missing_required = Vec::new();
    let mut broken_required = Vec::new();
    let mut missing_optional = Vec::new();
    let mut broken_optional = Vec::new();

    println!("\n[Required tools]");
    let local_tools = local_tool_env(&std::env::current_dir().context("cwd")?);
    let local_path_hint = local_tools
        .as_ref()
        .map(|e| e.tool_bin_dir.display().to_string());
    for tool in [
        "git",
        "cargo",
        "rustc",
        "make",
        "gcc",
        "g++",
        "as",
        "autoreconf",
        "autopoint",
        "gnulib-tool",
        "meson",
        "ninja",
        "gperf",
        "gawk",
        "ld",
        "objcopy",
        "objdump",
        "perl",
        "python3",
        "bc",
        "cpio",
        "gzip",
        "mformat",
        "mcopy",
        "grub-mkrescue",
        "xorriso",
        "pkg-config",
        "bash",
        "bison",
        "flex",
        "file",
        "readelf",
        "ldd",
        "rsync",
        "bindgen",
        "cmake",
        "dpkg",
        "dpkg-deb",
        "dpkg-query",
        "dpkg-scanpackages",
        "fakeroot",
        "apt-ftparchive",
        "zstd",
        "xz",
        "tar",
        "triehash",
    ] {
        if !check_host_tool_with_hint(tool, true, local_path_hint.as_deref())? {
            missing_required.push(tool);
        }
    }

    for (tool, args) in [
        ("mformat", vec!["-V"]),
        ("mcopy", vec!["-V"]),
        ("meson", vec!["--version"]),
        ("ninja", vec!["--version"]),
        ("grub-mkrescue", vec!["--version"]),
        ("xorriso", vec!["-version"]),
        ("bindgen", vec!["--version"]),
    ] {
        if missing_required.contains(&tool) {
            continue;
        }
        if let Some(message) = check_tool_runtime(tool, &args)? {
            println!("[broken]  {tool} ({message})");
            broken_required.push(tool);
        }
    }

    if let Some(message) = check_tool_runtime("python3", &["-c", "import jinja2"])? {
        println!("[broken]  python3-jinja2 ({message})");
        broken_required.push("python3-jinja2");
    }

    if !missing_required.contains(&"pkg-config") {
        for (args, package) in [
            (&["--exists", "mount"][..], "libmount-dev"),
            (&["--exists", "openssl"][..], "libssl-dev"),
            (&["--atleast-version=2.2", "expat"][..], "libexpat1-dev"),
            (&["--exists", "zlib"][..], "zlib1g-dev"),
            (&["--exists", "liblzma"][..], "liblzma-dev"),
            (&["--exists", "libzstd"][..], "libzstd-dev"),
            (&["--exists", "liblz4"][..], "liblz4-dev"),
            (&["--exists", "libxxhash"][..], "libxxhash-dev"),
        ] {
            if let Some(message) = check_tool_runtime("pkg-config", args)? {
                println!("[broken]  {package} ({message})");
                broken_required.push(package);
            }
        }
    }

    println!("\n[Optional tools]");
    for tool in ["qemu-system-x86_64", "clang"] {
        if !check_host_tool_with_hint(tool, false, local_path_hint.as_deref())? {
            missing_optional.push(tool);
        }
    }

    for (tool, args) in [("qemu-system-x86_64", vec!["--version"])] {
        if missing_optional.contains(&tool) {
            continue;
        }
        if let Some(message) = check_tool_runtime(tool, &args)? {
            println!("[broken]  {tool} ({message})");
            broken_optional.push(tool);
        }
    }

    let mut required_issues: Vec<&str> = Vec::new();
    required_issues.extend(missing_required.iter().copied());
    required_issues.extend(broken_required.iter().copied());
    required_issues.sort_unstable();
    required_issues.dedup();

    let mut optional_issues: Vec<&str> = Vec::new();
    optional_issues.extend(missing_optional.iter().copied());
    optional_issues.extend(broken_optional.iter().copied());
    optional_issues.sort_unstable();
    optional_issues.dedup();

    if !required_issues.is_empty() || !optional_issues.is_empty() {
        println!("\n[Suggested packages]");
        if let Some(cmd) = suggested_package_command(&required_issues, &optional_issues)? {
            println!("{cmd}");
        } else {
            println!("No package manager hint available; install missing tools manually.");
        }
    }

    if !missing_required.is_empty() {
        println!("\n[Required missing tools] {}", missing_required.join(", "));
    }
    if !broken_required.is_empty() {
        println!("[Required broken tools] {}", broken_required.join(", "));
    }

    if !missing_required.is_empty() || !broken_required.is_empty() {
        bail!("doctor detected missing or broken required prerequisites")
    }

    if !missing_optional.is_empty() || !broken_optional.is_empty() {
        println!("doctor completed with optional warnings");
    } else {
        println!("doctor completed successfully");
    }
    Ok(())
}
