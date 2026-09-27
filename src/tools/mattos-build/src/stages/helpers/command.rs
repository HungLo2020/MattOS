// Generic command execution and target build environment shared by every
// stage recipe: process environment, jobserver arguments, the MattOS
// sysroot/toolchain PATH, Rust toolchain selection and ccache.  Kept out of
// the image recipes' file so editing it does not rebuild the image stages;
// like the scheduler, it is build infrastructure rather than a stage input.

fn run_cmd(cwd: &Path, program: &str, args: &[&str]) -> Result<()> {
    let status = run_cmd_status(cwd, program, args)?;
    if status.success() {
        Ok(())
    } else {
        bail!(
            "command failed with status {status}: {} {}",
            program,
            args.join(" ")
        )
    }
}

fn run_cmd_status(cwd: &Path, program: &str, args: &[&str]) -> Result<std::process::ExitStatus> {
    let mut command = Command::new(program);
    let scheduler_args = scheduler_command_args(args);
    command.args(&scheduler_args).current_dir(cwd);
    apply_reproducible_process_environment(&mut command);
    apply_mattos_tmp_environment(&mut command, cwd)?;
    apply_scheduler_parallelism(&mut command);
    apply_mattos_sysroot_environment(&mut command, cwd, program, &[])?;
    let display = effective_command_display(program, &scheduler_args);
    performance::run_logged_command(&mut command, &display)
}

fn run_cmd_with_env(
    cwd: &Path,
    program: &str,
    args: &[&str],
    tool_env: Option<&LocalToolEnv>,
) -> Result<()> {
    let mut cmd = Command::new(program);
    let scheduler_args = scheduler_command_args(args);
    cmd.args(&scheduler_args).current_dir(cwd);
    apply_reproducible_process_environment(&mut cmd);
    apply_mattos_tmp_environment(&mut cmd, cwd)?;
    apply_scheduler_parallelism(&mut cmd);

    // Some build systems (notably Kbuild) recursively invoke make. When a
    // caller explicitly supplies KBUILD_CPPFLAGS, propagate that exact
    // command-line assignment through MAKEFLAGS so recursive sub-makes retain
    // it as a command-line variable. This keeps source-path normalization
    // consistent across ordinary C compilation and linker-script
    // preprocessing without globally enabling make's environment override
    // mode (which would also override Kbuild's internal srctree variables).
    if let Some(cppflags) = args
        .iter()
        .find_map(|arg| arg.strip_prefix("KBUILD_CPPFLAGS="))
    {
        let escaped = cppflags.replace(' ', "\\ ");
        let inherited = std::env::var("MAKEFLAGS").unwrap_or_default();
        let makeflags = if inherited.is_empty() {
            format!("KBUILD_CPPFLAGS={escaped}")
        } else {
            format!("{inherited} KBUILD_CPPFLAGS={escaped}")
        };
        cmd.env("MAKEFLAGS", makeflags);
    }

    if let Some(env) = tool_env {
        let current_path = std::env::var("PATH").unwrap_or_default();
        let composed_path = format!("{}:{}", env.tool_bin_dir.display(), current_path);
        let current_ld = std::env::var("LD_LIBRARY_PATH").unwrap_or_default();
        let composed_ld = if current_ld.is_empty() {
            env.tool_lib_dir.display().to_string()
        } else {
            format!("{}:{current_ld}", env.tool_lib_dir.display())
        };
        let include = env.tool_include_dir.display().to_string();
        let lib = env.tool_lib_dir.display().to_string();

        cmd.env("PATH", composed_path)
            .env("LD_LIBRARY_PATH", composed_ld)
            .env(
                "BISON_PKGDATADIR",
                env.bison_pkg_data_dir.display().to_string(),
            )
            .env("M4", env.m4_bin.display().to_string())
            .env("CFLAGS", format!("-I{include}"))
            .env("HOSTCFLAGS", format!("-I{include}"))
            .env("LDFLAGS", format!("-L{lib}"))
            .env("HOSTLDFLAGS", format!("-L{lib}"));
    }
    apply_mattos_sysroot_environment(&mut cmd, cwd, program, &[])?;

    let display = effective_command_display(program, &scheduler_args);
    let status = performance::run_logged_command(&mut cmd, &display)?;
    if status.success() {
        Ok(())
    } else {
        bail!(
            "command failed with status {status}: {} {}",
            program,
            args.join(" ")
        )
    }
}

fn run_cmd_with_env_overrides(
    cwd: &Path,
    program: &str,
    args: &[&str],
    env_overrides: &[(&str, String)],
) -> Result<()> {
    let mut cmd = Command::new(program);
    let scheduler_args = scheduler_command_args(args);
    cmd.args(&scheduler_args).current_dir(cwd);
    apply_reproducible_process_environment(&mut cmd);
    for (key, value) in env_overrides {
        cmd.env(key, value);
    }
    apply_mattos_tmp_environment(&mut cmd, cwd)?;
    apply_scheduler_parallelism(&mut cmd);
    apply_mattos_sysroot_environment(&mut cmd, cwd, program, env_overrides)?;

    let display = effective_command_display(program, &scheduler_args);
    let status = performance::run_logged_command(&mut cmd, &display)?;
    if status.success() {
        Ok(())
    } else {
        bail!(
            "command failed with status {status}: {} {}",
            program,
            args.join(" ")
        )
    }
}

fn apply_reproducible_process_environment(command: &mut Command) {
    command
        .env("LC_ALL", "C")
        .env("LANG", "C")
        .env("TZ", "UTC")
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .env("SOURCE_DATE_EPOCH", MATTOS_SOURCE_DATE_EPOCH);
}

fn mattos_build_tmp(repo_root: &Path) -> PathBuf {
    repo_root.join(MATTOS_BUILD_TMP_RELATIVE)
}

fn mattos_tmp_min_free_bytes() -> u64 {
    // Unit tests exercise routing, writability, and concurrency inside
    // tempfile-backed filesystems. Their result must not depend on how full
    // the host's /tmp happens to be. Production builds retain the 4 GiB guard.
    if cfg!(test) {
        0
    } else {
        MIN_MATTOS_TMP_FREE_BYTES
    }
}

fn ensure_mattos_build_tmp(repo_root: &Path) -> Result<PathBuf> {
    let directory = mattos_build_tmp(repo_root);
    fs::create_dir_all(&directory).with_context(|| {
        format!(
            "failed to create MattOS build temp directory {}",
            directory.display()
        )
    })?;
    let free_bytes = free_bytes_at(&directory)?;
    let required_free_bytes = mattos_tmp_min_free_bytes();
    if free_bytes < required_free_bytes {
        bail!(
            "MattOS build temp directory {} has only {} free bytes; at least {} are required",
            directory.display(),
            free_bytes,
            required_free_bytes
        );
    }

    // `build all` prepares commands from multiple scheduler threads inside one
    // mattos-build process. A PID-only probe name lets those threads delete one
    // another's probe. Give every invocation a process-local unique sequence so
    // strict cleanup remains meaningful without serializing command setup.
    let sequence = MATTOS_TMP_PROBE_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let probe = directory.join(format!(".write-probe-{}-{sequence}", std::process::id()));
    fs::write(&probe, b"mattos-build temp directory probe\n").with_context(|| {
        format!(
            "MattOS build temp directory is not writable: {}",
            directory.display()
        )
    })?;
    fs::remove_file(&probe).with_context(|| {
        format!(
            "failed to remove MattOS build temp probe {}",
            probe.display()
        )
    })?;
    Ok(directory)
}

fn free_bytes_at(path: &Path) -> Result<u64> {
    let path = std::ffi::CString::new(path.to_string_lossy().as_bytes())
        .context("invalid MattOS temp path")?;
    let mut stats = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    let result = unsafe { libc::statvfs(path.as_ptr(), stats.as_mut_ptr()) };
    if result != 0 {
        return Err(anyhow!(
            "failed to inspect free space for MattOS temp directory"
        ));
    }
    let stats = unsafe { stats.assume_init() };
    Ok((stats.f_bavail as u64).saturating_mul(stats.f_frsize as u64))
}

fn apply_mattos_tmp_environment(command: &mut Command, cwd: &Path) -> Result<()> {
    let Some(repo_root) = cwd.ancestors().find(|candidate| {
        candidate
            .join("src/tools/mattos-build/Cargo.toml")
            .is_file()
    }) else {
        return Ok(());
    };
    let directory = ensure_mattos_build_tmp(repo_root)?;
    // The repository-owned directory deliberately takes precedence over a
    // caller's TMPDIR: build correctness must not depend on a full host /tmp.
    command.env("TMPDIR", directory);
    Ok(())
}

fn effective_command_display(program: &str, args: &[String]) -> String {
    let argv = std::iter::once(program.to_string())
        .chain(args.iter().cloned())
        .collect::<Vec<_>>();
    format!(
        "{}\n[mattos-command] child_jobs={} argv={argv:?}",
        argv.join(" "),
        scheduler::child_job_limit()
    )
}

/// With a shared jobserver, Make/Ninja/Cargo take parallelism from the token
/// pool; an explicit `-jN` would make a top-level client ignore the pool and
/// fix its parallelism for the whole run.  Explicit serial requests (`-j1`)
/// are recipe semantics and are kept.
fn strip_job_count_arguments(args: &[&str]) -> Vec<String> {
    let is_count = |value: &str| !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit());
    let is_serial = |value: &str| value.parse::<usize>().is_ok_and(|count| count <= 1);
    let mut result = Vec::with_capacity(args.len());
    let mut index = 0;
    while index < args.len() {
        let argument = args[index];
        let next = args.get(index + 1).copied();
        if matches!(argument, "-j" | "--jobs" | "--parallel") && next.is_some_and(is_count) {
            if is_serial(next.unwrap()) {
                result.push(argument.to_string());
                result.push(next.unwrap().to_string());
            }
            index += 2;
            continue;
        }
        let joined = argument
            .strip_prefix("--jobs=")
            .or_else(|| argument.strip_prefix("--parallel="))
            .or_else(|| argument.strip_prefix("-j").filter(|value| !value.is_empty()));
        if let Some(value) = joined.filter(|value| is_count(value)) {
            if is_serial(value) {
                result.push(argument.to_string());
            }
        } else {
            result.push(argument.to_string());
        }
        index += 1;
    }
    result
}

fn scheduler_command_args(args: &[&str]) -> Vec<String> {
    let experimental_limit = EXPERIMENTAL_CHILD_JOBS.with(Cell::get);
    if experimental_limit.is_none() && scheduler::elastic_child_jobs() {
        return strip_job_count_arguments(args);
    }
    // A very small cgroup memory ceiling can yield no parallel CPU grant.
    // External build tools require a positive jobs value; retain serial
    // progress while the cgroup remains the hard memory safety boundary.
    let limit = scheduler::child_job_limit().max(1);
    let mut previous_sets_jobs = false;
    args.iter()
        .map(|argument| {
            let normalized =
                if previous_sets_jobs && argument.bytes().all(|byte| byte.is_ascii_digit()) {
                    experimental_limit
                        .unwrap_or_else(|| argument.parse::<usize>().unwrap().min(limit))
                        .to_string()
                } else if argument.starts_with("-j")
                    && argument.len() > 2
                    && argument[2..].bytes().all(|byte| byte.is_ascii_digit())
                {
                    format!(
                        "-j{}",
                        experimental_limit
                            .unwrap_or_else(|| argument[2..].parse::<usize>().unwrap().min(limit))
                    )
                } else if let Some(value) = argument
                    .strip_prefix("--jobs=")
                    .or_else(|| argument.strip_prefix("--parallel="))
                    .filter(|value| value.bytes().all(|byte| byte.is_ascii_digit()))
                {
                    let option = argument.split_once('=').unwrap().0;
                    format!(
                        "{option}={}",
                        experimental_limit
                            .unwrap_or_else(|| value.parse::<usize>().unwrap().min(limit))
                    )
                } else {
                    (*argument).to_string()
                };
            previous_sets_jobs = matches!(*argument, "-j" | "--jobs" | "--parallel");
            normalized
        })
        .collect()
}

fn apply_scheduler_parallelism(command: &mut Command) {
    if EXPERIMENTAL_CHILD_JOBS.with(Cell::get).is_none() && scheduler::elastic_child_jobs() {
        if let Some(makeflags) = jobserver::makeflags() {
            let ceiling = scheduler::child_job_limit().max(1).to_string();
            // Make and Ninja join the shared jobserver through MAKEFLAGS;
            // Cargo reads CARGO_MAKEFLAGS.  Fixed -j overrides are removed.
            command
                .env("MAKEFLAGS", &makeflags)
                .env("CARGO_MAKEFLAGS", &makeflags)
                .env("CARGO_BUILD_JOBS", &ceiling)
                .env("MESON_NUM_PROCESSES", &ceiling)
                .env_remove("CMAKE_BUILD_PARALLEL_LEVEL")
                .env_remove("NINJAFLAGS");
            return;
        }
    }
    // External build tools uniformly reject a zero job count.  A tight
    // memory admission budget may intentionally grant no parallel token, but
    // it must still permit one serial child inside the cgroup ceiling.
    let tokens = scheduler::child_job_limit().max(1).to_string();
    command
        .env("MAKEFLAGS", format!("-j{tokens}"))
        .env("CARGO_BUILD_JOBS", &tokens)
        .env("CMAKE_BUILD_PARALLEL_LEVEL", &tokens)
        .env("MESON_NUM_PROCESSES", &tokens)
        .env("NINJAFLAGS", format!("-j{tokens}"));
}

fn apply_mattos_sysroot_environment(
    command: &mut Command,
    cwd: &Path,
    program: &str,
    overrides: &[(&str, String)],
) -> Result<()> {
    let Some(repo_root) = cwd.ancestors().find(|candidate| {
        candidate
            .join("src/tools/mattos-build/Cargo.toml")
            .is_file()
    }) else {
        return Ok(());
    };
    // A working directory spelled with `..` (for example a test's
    // CARGO_MANIFEST_DIR/../../..) must resolve to the same checkout path as
    // the build itself: the compiler wrappers regenerated below embed it, and
    // a different spelling rewrites them and changes every target stage's
    // toolchain cache key.
    let normalized_root = lexically_normalized(repo_root);
    let repo_root = if normalized_root.join("src/tools/mattos-build/Cargo.toml").is_file() {
        normalized_root.as_path()
    } else {
        repo_root
    };
    let sysroot = repo_root.join("out/sysroot");
    if !sysroot.join("usr/include/stdio.h").is_file()
        || cwd.starts_with(repo_root.join("src/kernel/linux"))
        || cwd.starts_with(repo_root.join("out/build/linux"))
        || cwd.starts_with(repo_root.join("out/build/glibc"))
        || cwd.starts_with(repo_root.join("src/system/libc/glibc"))
    {
        return Ok(());
    }
    let sysroot_flag = format!("--sysroot={}", sysroot.display());
    let value_for = |key: &str| {
        overrides
            .iter()
            .find(|(candidate, _)| *candidate == key)
            .map(|(_, value)| value.clone())
            .or_else(|| std::env::var(key).ok())
            .unwrap_or_default()
    };
    for key in ["CPPFLAGS", "CFLAGS", "CXXFLAGS", "LDFLAGS"] {
        let current = value_for(key);
        let mut value = if current.split_whitespace().any(|flag| flag == sysroot_flag) {
            current
        } else if current.is_empty() {
            sysroot_flag.clone()
        } else {
            format!("{current} {sysroot_flag}")
        };
        if matches!(key, "CFLAGS" | "CXXFLAGS") {
            let prefix_map = format!("-ffile-prefix-map={}=/usr/src/mattos", repo_root.display());
            if !value.split_whitespace().any(|flag| flag == prefix_map) {
                value.push_str(&format!(
                    " {prefix_map} -fdebug-prefix-map={}=/usr/src/mattos -fmacro-prefix-map={}=/usr/src/mattos",
                    repo_root.display(),
                    repo_root.display()
                ));
            }
        }
        command.env(key, value);
    }
    if program == "cargo" {
        let current = value_for("RUSTFLAGS");
        // Cargo fingerprints RUSTFLAGS verbatim and rustc incorporates codegen
        // options into crate identity.  Keep the linker argument independent of
        // the absolute checkout location while still resolving to this tree's
        // output-owned sysroot from Cargo's working directory.
        let relative = cwd
            .strip_prefix(repo_root)
            .context("Cargo working directory is outside the MattOS repository")?;
        let mut relative_sysroot = PathBuf::new();
        for component in relative.components() {
            if matches!(component, std::path::Component::Normal(_)) {
                relative_sysroot.push("..");
            }
        }
        relative_sysroot.push("out/sysroot");
        let rust_sysroot = format!(
            "-C link-arg=--sysroot={}",
            relative_sysroot.to_string_lossy()
        );
        let remap = format!(
            "--remap-path-prefix={}=/usr/src/mattos",
            repo_root.display()
        );
        let value = if current.contains(&rust_sysroot) {
            current
        } else if current.is_empty() {
            rust_sysroot
        } else {
            format!("{current} {rust_sysroot}")
        };
        let value = if value.contains(&remap) {
            value
        } else {
            format!("{value} {remap}")
        };
        command.env("RUSTFLAGS", value);
    }
    // Target builds compile with the MattOS-built GCC and Binutils: their
    // wrappers shadow the host `cc`, `gcc`, `g++`, `ld`, `ar`, ... on PATH.
    if let Some(toolchain) = mattos_target_toolchain(repo_root)? {
        // Like `Command::env`, the last override of a variable wins.
        let inherited = overrides
            .iter()
            .rev()
            .find(|(candidate, _)| *candidate == "PATH")
            .map(|(_, value)| OsString::from(value))
            .or_else(|| std::env::var_os("PATH"))
            .unwrap_or_default();
        let mut paths = vec![toolchain.bin.clone()];
        // Target Rust code (Cargo crates and Meson's Rust support) is compiled
        // by the MattOS-built rustc, never a host rustc/rustup found on PATH.
        let rust_bin = mattos_rust_toolchain(repo_root, cwd);
        if let Some(rust_bin) = &rust_bin {
            paths.push(rust_bin.clone());
            command
                .env("RUSTC", rust_bin.join("rustc"))
                .env("RUSTDOC", rust_bin.join("rustdoc"));
        } else if matches!(program, "cargo" | "rustc") && !builds_mattos_rust(repo_root, cwd) {
            bail!(
                "target Rust code requires the MattOS Rust toolchain at {}; build the rust stage first",
                repo_root.join(MATTOS_RUST_TOOLS).display()
            );
        }
        // ccache masquerades as the compiler names first on PATH and runs the
        // MattOS wrapper that follows it.
        if let Some(ccache_bin) = ccache_masquerade(repo_root)? {
            paths.insert(0, ccache_bin);
            apply_ccache_environment(command, repo_root)?;
            add_cmake_ccache_launcher(command, program, repo_root)?;
        }
        let owned = paths.clone();
        paths.extend(std::env::split_paths(&inherited).filter(|path| !owned.contains(path)));
        command.env("PATH", std::env::join_paths(paths)?);
    }
    command.env("MATTOS_SYSROOT", &sysroot);
    Ok(())
}

/// `path` with `.` and `..` components resolved textually.
fn lexically_normalized(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}

/// Install location of the MattOS-built Rust toolchain (the `rust` stage).
const MATTOS_RUST_TOOLS: &str = "out/build/rust/install/usr/bin";

/// The rust stage bootstraps the MattOS rustc from its pinned stage0 compiler.
fn builds_mattos_rust(repo_root: &Path, cwd: &Path) -> bool {
    cwd.starts_with(repo_root.join("out/build/rust"))
}

/// MattOS Rust toolchain used for target Rust code, when it has been built.
fn mattos_rust_toolchain(repo_root: &Path, cwd: &Path) -> Option<PathBuf> {
    if builds_mattos_rust(repo_root, cwd) {
        return None;
    }
    let bin = repo_root.join(MATTOS_RUST_TOOLS);
    ["rustc", "cargo", "rustdoc"]
        .iter()
        .all(|tool| bin.join(tool).is_file())
        .then_some(bin)
}

/// Host ccache used to cache MattOS target C/C++ compiles, unless
/// `MATTOS_CCACHE=0`.  It is a build accelerator only: cache keys include the
/// MattOS toolchain identity, and hits return byte-identical objects.
fn host_ccache() -> Option<PathBuf> {
    static CCACHE: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();
    CCACHE
        .get_or_init(|| {
            if std::env::var("MATTOS_CCACHE").is_ok_and(|value| value == "0") {
                return None;
            }
            std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
                .map(|directory| directory.join("ccache"))
                .find(|candidate| candidate.is_file())
        })
        .clone()
}

const CCACHE_COMPILER_NAMES: &[&str] = &[
    "gcc",
    "g++",
    "cc",
    "c++",
    "x86_64-pc-linux-gnu-gcc",
    "x86_64-pc-linux-gnu-g++",
    "x86_64-pc-linux-gnu-cc",
    "x86_64-pc-linux-gnu-c++",
];

/// Directory of compiler-named links to ccache, placed before the MattOS
/// compiler wrappers on PATH.
fn ccache_masquerade(repo_root: &Path) -> Result<Option<PathBuf>> {
    match host_ccache() {
        Some(ccache) => ccache_masquerade_for(repo_root, &ccache),
        None => Ok(None),
    }
}

fn ccache_masquerade_for(repo_root: &Path, ccache: &Path) -> Result<Option<PathBuf>> {
    if target_toolchain_identity(repo_root)?.is_none() {
        return Ok(None);
    }
    let directory = repo_root.join("out/toolchain/ccache-bin");
    fs::create_dir_all(&directory)?;
    for name in CCACHE_COMPILER_NAMES {
        let link = directory.join(name);
        if fs::read_link(&link).ok().as_deref() != Some(ccache) {
            remove_path_if_exists(&link)?;
            std::os::unix::fs::symlink(ccache, &link)?;
        }
    }
    Ok(Some(directory))
}

fn apply_ccache_environment(command: &mut Command, repo_root: &Path) -> Result<()> {
    let Some(identity) = target_toolchain_identity(repo_root)? else {
        return Ok(());
    };
    command
        .env("CCACHE_DIR", repo_root.join("out/cache/ccache"))
        .env("CCACHE_MAXSIZE", "40G")
        // The wrapper scripts never change when the compiler or hardening
        // specs do; key the cache to the actual toolchain configuration.
        .env("CCACHE_COMPILERCHECK", format!("string:{identity}"));
    // CMake initializes CMAKE_<LANG>_COMPILER_LAUNCHER from these, which also
    // covers build trees whose cache already records an absolute compiler
    // path (bypassing the PATH masquerade).
    if let Some(ccache) = host_ccache() {
        command
            .env("CMAKE_C_COMPILER_LAUNCHER", &ccache)
            .env("CMAKE_CXX_COMPILER_LAUNCHER", &ccache);
    }
    Ok(())
}

/// CMake records an absolute compiler path in an existing build tree and reads
/// the launcher environment variables only when a tree is first created, so
/// pass the launcher explicitly on every CMake configure invocation.
fn add_cmake_ccache_launcher(command: &mut Command, program: &str, repo_root: &Path) -> Result<()> {
    if Path::new(program).file_name().and_then(OsStr::to_str) != Some("cmake") {
        return Ok(());
    }
    let is_configure = !command.get_args().any(|argument| {
        matches!(
            argument.to_str(),
            Some("--build" | "--install" | "-E" | "-P" | "--version" | "--help")
        ) || argument
            .to_str()
            .is_some_and(|value| value.starts_with("-DCMAKE_C_COMPILER_LAUNCHER="))
    });
    if is_configure {
        command.args(ccache_cmake_launcher_args(repo_root)?);
    }
    Ok(())
}

/// `CMAKE_<LANG>_COMPILER_LAUNCHER` arguments for CMake builds that name the
/// compiler explicitly (bypassing the PATH masquerade).
fn ccache_cmake_launcher_args(repo_root: &Path) -> Result<Vec<String>> {
    let Some(ccache) = host_ccache() else {
        return Ok(Vec::new());
    };
    if target_toolchain_identity(repo_root)?.is_none() {
        return Ok(Vec::new());
    }
    Ok(vec![
        format!("-DCMAKE_C_COMPILER_LAUNCHER={}", ccache.display()),
        format!("-DCMAKE_CXX_COMPILER_LAUNCHER={}", ccache.display()),
    ])
}

fn run_cmd_output(cwd: &Path, program: &str, args: &[&str]) -> Result<Output> {
    let mut command = Command::new(program);
    command.args(args).current_dir(cwd);
    apply_reproducible_process_environment(&mut command);
    apply_mattos_tmp_environment(&mut command, cwd)?;
    command
        .output()
        .with_context(|| format!("failed to spawn command: {program}"))
}

fn run_cmd_capture(cwd: &Path, program: &str, args: &[&str]) -> Result<String> {
    let output = run_cmd_output(cwd, program, args)?;
    if !output.status.success() {
        bail!(
            "command failed with status {}: {} {}",
            output.status,
            program,
            args.join(" ")
        );
    }
    let text = String::from_utf8(output.stdout).context("stdout was not valid UTF-8")?;
    Ok(text)
}
