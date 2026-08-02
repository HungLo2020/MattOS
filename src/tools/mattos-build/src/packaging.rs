use super::*;
use clap::Subcommand;
use filetime::{FileTime, set_file_times, set_symlink_file_times};
use sha2::{Digest, Sha256};
use std::io::Read;

const ARCH: &str = "amd64";
const REVISION: &str = "1mattos1";
const SOURCE_DATE_EPOCH: i64 = 1_767_225_600; // 2026-01-01T00:00:00Z
const PACKAGE_NAMES: &[&str] = &[
    "mattos-filesystem",
    "mattos-base-files",
    "mattos-brush",
    "mattos-coreutils",
    "mattos-curl",
];

#[derive(Subcommand, Debug)]
pub(crate) enum PackageCommands {
    Build {
        #[arg(long, conflicts_with = "package")]
        all: bool,
        package: Option<String>,
    },
    Repo,
    Inspect {
        package: String,
    },
    Status,
}

#[derive(Clone, Debug)]
struct PackageSpec {
    name: &'static str,
    description: &'static str,
    source_component: &'static str,
    depends: &'static [&'static str],
    provides: &'static [&'static str],
    conflicts: &'static [&'static str],
    replaces: &'static [&'static str],
}

#[derive(Debug, Serialize, Deserialize)]
struct PackageInventory {
    package: Vec<PackageInventoryEntry>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct PackageInventoryEntry {
    name: String,
    version: String,
    architecture: String,
    artifact_path: String,
    source_component: String,
    dependencies: Vec<String>,
    runtime_libraries: Vec<String>,
    file_count: u64,
    sha256: String,
}

#[derive(Serialize)]
struct Provenance<'a> {
    package: &'a str,
    version: &'a str,
    architecture: &'a str,
    mattos_source_path: &'a str,
    upstream_repository: &'a str,
    upstream_commit: &'a str,
    build_configuration: &'a str,
    runtime_libraries: &'a [String],
}

fn package_specs() -> Vec<PackageSpec> {
    vec![
        PackageSpec {
            name: "mattos-filesystem",
            description: "MattOS base filesystem hierarchy",
            source_component: "MattOS",
            depends: &[],
            provides: &["mattos-filesystem-hierarchy"],
            conflicts: &[],
            replaces: &[],
        },
        PackageSpec {
            name: "mattos-base-files",
            description: "MattOS identity and baseline configuration",
            source_component: "MattOS",
            depends: &["mattos-filesystem"],
            provides: &["mattos-release"],
            conflicts: &["base-files"],
            replaces: &["base-files"],
        },
        PackageSpec {
            name: "mattos-brush",
            description: "Brush shell built for MattOS",
            source_component: "brush",
            depends: &["mattos-filesystem"],
            provides: &["mattos-shell"],
            conflicts: &[],
            replaces: &[],
        },
        PackageSpec {
            name: "mattos-coreutils",
            description: "uutils core utilities built for MattOS",
            source_component: "coreutils",
            depends: &["mattos-filesystem"],
            provides: &["coreutils"],
            conflicts: &["coreutils"],
            replaces: &["coreutils"],
        },
        PackageSpec {
            name: "mattos-curl",
            description: "curl command-line transfer client built for MattOS",
            source_component: "curl",
            depends: &["mattos-filesystem"],
            provides: &["curl"],
            conflicts: &["curl"],
            replaces: &["curl"],
        },
    ]
}

pub(crate) fn run_package_command(repo_root: &Path, command: PackageCommands) -> Result<()> {
    match command {
        PackageCommands::Build { all, package } => {
            if all {
                build_all_packages(repo_root)?;
            } else if let Some(name) = package {
                build_packages(repo_root, &[name])?;
            } else {
                bail!("package build requires a package name or --all")
            }
            Ok(())
        }
        PackageCommands::Repo => generate_repository(repo_root),
        PackageCommands::Inspect { package } => inspect_package(repo_root, &package),
        PackageCommands::Status => print_inventory(repo_root),
    }
}

pub(crate) fn build_all_packages(repo_root: &Path) -> Result<()> {
    build_packages(
        repo_root,
        &PACKAGE_NAMES
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>(),
    )
}

fn build_packages(repo_root: &Path, names: &[String]) -> Result<()> {
    let specs = package_specs();
    let mut selected = Vec::new();
    for name in names {
        validate_package_name(name)?;
        let spec = specs
            .iter()
            .find(|spec| spec.name == name)
            .ok_or_else(|| anyhow!("unknown MattOS package {name}"))?;
        selected.push(spec.clone());
    }

    let staging_root = repo_root.join("out/packages/staging");
    let artifact_root = repo_root.join("out/packages/amd64");
    fs::create_dir_all(&staging_root)?;
    fs::create_dir_all(&artifact_root)?;
    for spec in &selected {
        stage_package(repo_root, spec)?;
    }
    // Check the complete prototype set whenever it is fully staged, otherwise the
    // selected subset. Shared directories are intentionally permitted.
    let collision_specs: Vec<PackageSpec> = if PACKAGE_NAMES
        .iter()
        .all(|name| staging_root.join(name).is_dir())
    {
        specs.clone()
    } else {
        selected.clone()
    };
    detect_staging_collisions(&staging_root, &collision_specs)?;

    let mut inventory = read_inventory(repo_root).unwrap_or(PackageInventory {
        package: Vec::new(),
    });
    for spec in selected {
        let version = package_version(repo_root, &spec)?;
        let staging = staging_root.join(spec.name);
        normalize_tree_timestamps(&staging)?;
        let artifact = artifact_root.join(format!("{}_{}_{}.deb", spec.name, version, ARCH));
        let staging_arg = path_str(&staging)?;
        let artifact_arg = path_str(&artifact)?;
        let status = Command::new("dpkg-deb")
            .args([
                "--root-owner-group",
                "-Zzstd",
                "-z19",
                "--build",
                staging_arg,
                artifact_arg,
            ])
            .env("SOURCE_DATE_EPOCH", SOURCE_DATE_EPOCH.to_string())
            .status()
            .context("failed to run dpkg-deb")?;
        if !status.success() {
            bail!("dpkg-deb failed for {} with {status}", spec.name)
        }
        verify_deb(&artifact, spec.name, &version)?;
        let runtime_libraries = runtime_libraries_for_spec(repo_root, &spec)?;
        let entry = PackageInventoryEntry {
            name: spec.name.to_string(),
            version,
            architecture: ARCH.to_string(),
            artifact_path: relative_display(repo_root, &artifact)?,
            source_component: spec.source_component.to_string(),
            dependencies: spec.depends.iter().map(|s| s.to_string()).collect(),
            runtime_libraries,
            file_count: count_package_entries(&staging)?,
            sha256: sha256_file(&artifact)?,
        };
        inventory.package.retain(|old| old.name != entry.name);
        inventory.package.push(entry);
    }
    inventory.package.sort_by(|a, b| a.name.cmp(&b.name));
    write_inventory(repo_root, &inventory)?;
    print_inventory(repo_root)
}

fn stage_package(repo_root: &Path, spec: &PackageSpec) -> Result<()> {
    let staging = repo_root.join("out/packages/staging").join(spec.name);
    remove_path_if_exists(&staging)?;
    fs::create_dir_all(staging.join("DEBIAN"))?;
    match spec.name {
        "mattos-filesystem" => stage_filesystem(&staging)?,
        "mattos-base-files" => stage_base_files(repo_root, &staging)?,
        "mattos-brush" => {
            let source = repo_root.join("src/userland/brush/target/release/brush");
            stage_executable(&source, &staging.join("usr/bin/brush"), 0o755)?;
        }
        "mattos-coreutils" => stage_coreutils(repo_root, &staging)?,
        "mattos-curl" => {
            let source = repo_root.join("out/build/curl/install/usr/bin/curl");
            stage_executable(&source, &staging.join("usr/bin/curl"), 0o755)?;
            let source_libdir = repo_root.join("out/build/curl/install/usr/lib/x86_64-linux-gnu");
            let destination_libdir = staging.join("usr/lib/x86_64-linux-gnu");
            fs::create_dir_all(&destination_libdir)?;
            stage_executable(
                &source_libdir.join("libcurl.so.4.8.0"),
                &destination_libdir.join("libcurl.so.4.8.0"),
                0o644,
            )?;
            std::os::unix::fs::symlink(
                "libcurl.so.4.8.0",
                destination_libdir.join("libcurl.so.4"),
            )?;
        }
        _ => bail!("no staging implementation for {}", spec.name),
    }

    let version = package_version(repo_root, spec)?;
    validate_debian_version(&version)?;
    let runtime_libraries = runtime_libraries_for_spec(repo_root, spec)?;
    write_provenance(repo_root, &staging, spec, &version, &runtime_libraries)?;
    let installed_size = installed_size_kib(&staging)?;
    let control = render_control(spec, &version, installed_size, &runtime_libraries)?;
    fs::write(staging.join("DEBIAN/control"), control)?;
    normalize_package_modes(&staging)?;
    Ok(())
}

fn stage_filesystem(staging: &Path) -> Result<()> {
    for rel in [
        "usr/bin",
        "usr/sbin",
        "usr/lib",
        "usr/lib64",
        "usr/share",
        "usr/share/doc",
        "etc",
        "var",
        "var/lib",
        "home",
        "root",
        "run",
        "tmp",
    ] {
        fs::create_dir_all(staging.join(rel))?;
    }
    set_mode(staging.join("root"), 0o700)?;
    set_mode(staging.join("tmp"), 0o1777)?;
    #[cfg(unix)]
    for (link, target) in [
        ("bin", "usr/bin"),
        ("sbin", "usr/sbin"),
        ("lib", "usr/lib"),
        ("lib64", "usr/lib64"),
    ] {
        std::os::unix::fs::symlink(target, staging.join(link))?;
    }
    Ok(())
}

fn stage_base_files(repo_root: &Path, staging: &Path) -> Result<()> {
    let skeleton = repo_root.join("src/rootfs/skeleton/etc");
    for name in ["os-release", "hostname", "profile", "shells"] {
        copy_preserving(&skeleton.join(name), &staging.join("etc").join(name))?;
    }
    let config = repo_root.join("src/system/packages/config/base-files");
    copy_preserving(&config.join("issue"), &staging.join("etc/issue"))?;
    copy_preserving(
        &config.join("mattos.sources"),
        &staging.join("etc/apt/sources.list.d/mattos.sources"),
    )?;
    let conffiles = [
        "/etc/hostname",
        "/etc/profile",
        "/etc/shells",
        "/etc/issue",
        "/etc/apt/sources.list.d/mattos.sources",
    ];
    fs::write(
        staging.join("DEBIAN/conffiles"),
        format!("{}\n", conffiles.join("\n")),
    )?;
    Ok(())
}

fn stage_coreutils(repo_root: &Path, staging: &Path) -> Result<()> {
    let source = resolve_coreutils_multicall(repo_root)?;
    stage_executable(&source, &staging.join("usr/bin/coreutils"), 0o755)?;
    let applets = package_coreutils_applets(&source)?;
    #[cfg(unix)]
    for applet in applets {
        let path = staging.join("usr/bin").join(&applet);
        if path_entry_exists(&path) {
            bail!("duplicate coreutils command alias {applet}")
        }
        std::os::unix::fs::symlink("coreutils", path)?;
    }
    Ok(())
}

pub(crate) fn package_coreutils_applets(binary: &Path) -> Result<Vec<String>> {
    let applets = list_coreutils_applets(binary)?;
    let component_commands: BTreeSet<&str> = COMPONENT_INSTALL_MANIFESTS
        .iter()
        .flat_map(|manifest| manifest.binaries.iter().map(|binary| binary.command_name))
        .filter(|command| *command != "curl")
        .collect();
    Ok(applets
        .into_iter()
        .filter(|applet| !component_commands.contains(applet.as_str()))
        .collect())
}

fn stage_executable(source: &Path, destination: &Path, mode: u32) -> Result<()> {
    if !source.is_file() {
        bail!("required package input missing at {}", source.display())
    }
    copy_preserving(source, destination)?;
    set_mode(destination.to_path_buf(), mode)
}

fn copy_preserving(source: &Path, destination: &Path) -> Result<()> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(source, destination).with_context(|| {
        format!(
            "failed to copy {} to {}",
            source.display(),
            destination.display()
        )
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(source)?.permissions().mode();
        fs::set_permissions(destination, fs::Permissions::from_mode(mode))?;
    }
    Ok(())
}

fn package_version(repo_root: &Path, spec: &PackageSpec) -> Result<String> {
    let upstream = match spec.name {
        "mattos-filesystem" | "mattos-base-files" => "0.1".to_string(),
        "mattos-brush" => {
            cargo_package_version(&repo_root.join("src/userland/brush/brush/Cargo.toml"))?
        }
        "mattos-coreutils" => {
            cargo_workspace_version(&repo_root.join("src/userland/coreutils/Cargo.toml"))?
        }
        "mattos-curl" => curl_version(&repo_root.join("src/userland/curl/include/curl/curlver.h"))?,
        _ => bail!("unknown package {}", spec.name),
    };
    Ok(format!("{upstream}-{REVISION}"))
}

fn cargo_package_version(path: &Path) -> Result<String> {
    let value: toml::Value = toml::from_str(&fs::read_to_string(path)?)?;
    value
        .get("package")
        .and_then(|v| v.get("version"))
        .and_then(toml::Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| anyhow!("version missing from {}", path.display()))
}

fn cargo_workspace_version(path: &Path) -> Result<String> {
    let value: toml::Value = toml::from_str(&fs::read_to_string(path)?)?;
    value
        .get("workspace")
        .and_then(|v| v.get("package"))
        .and_then(|v| v.get("version"))
        .and_then(toml::Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| anyhow!("workspace.package.version missing from {}", path.display()))
}

fn curl_version(path: &Path) -> Result<String> {
    let body = fs::read_to_string(path)?;
    for line in body.lines() {
        if let Some(value) = line
            .trim()
            .strip_prefix("#define LIBCURL_VERSION \"")
            .and_then(|s| s.strip_suffix('"'))
        {
            return Ok(value.trim_end_matches("-DEV").to_string());
        }
    }
    bail!("LIBCURL_VERSION missing from {}", path.display())
}

fn validate_package_name(name: &str) -> Result<()> {
    let bytes = name.as_bytes();
    if bytes.len() < 2
        || !bytes[0].is_ascii_lowercase()
        || !bytes.iter().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'+' | b'-' | b'.')
        })
    {
        bail!("invalid Debian package name {name:?}")
    }
    Ok(())
}

fn validate_debian_version(version: &str) -> Result<()> {
    let upstream = version
        .rsplit_once('-')
        .map(|(left, _)| left)
        .unwrap_or(version);
    if version.is_empty()
        || !upstream.as_bytes().first().is_some_and(u8::is_ascii_digit)
        || !version
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'+' | b'~' | b'-' | b':'))
    {
        bail!("invalid Debian version {version:?}")
    }
    Ok(())
}

fn render_control(
    spec: &PackageSpec,
    version: &str,
    installed_size: u64,
    runtime_libraries: &[String],
) -> Result<String> {
    validate_package_name(spec.name)?;
    validate_debian_version(version)?;
    let mut fields = vec![
        format!("Package: {}", spec.name),
        format!("Version: {version}"),
        format!("Architecture: {ARCH}"),
        "Maintainer: MattOS Project <packages@mattos.invalid>".to_string(),
        format!("Installed-Size: {installed_size}"),
        format!(
            "Depends: {}",
            if spec.depends.is_empty() {
                "".to_string()
            } else {
                spec.depends.join(", ")
            }
        ),
    ];
    if !spec.provides.is_empty() {
        fields.push(format!("Provides: {}", spec.provides.join(", ")));
    }
    if !spec.conflicts.is_empty() {
        fields.push(format!("Conflicts: {}", spec.conflicts.join(", ")));
    }
    if !spec.replaces.is_empty() {
        fields.push(format!("Replaces: {}", spec.replaces.join(", ")));
    }
    if !runtime_libraries.is_empty() {
        fields.push(format!(
            "X-MattOS-Legacy-Runtime-Libraries: {}",
            runtime_libraries.join(", ")
        ));
    }
    fields.push(format!("Description: {}", spec.description));
    Ok(format!("{}\n", fields.join("\n")))
}

fn write_provenance(
    repo_root: &Path,
    staging: &Path,
    spec: &PackageSpec,
    version: &str,
    runtime_libraries: &[String],
) -> Result<()> {
    let (source_path, repository, commit, configuration) = match spec.source_component {
        "brush" => component_provenance(
            repo_root,
            "brush",
            "src/userland/brush",
            "cargo build --release",
        )?,
        "coreutils" => component_provenance(
            repo_root,
            "coreutils",
            "src/userland/coreutils",
            "cargo build --release",
        )?,
        "curl" => component_provenance(
            repo_root,
            "curl",
            "src/userland/curl",
            &curl_configure_options().join(" "),
        )?,
        _ => (
            "src/rootfs/skeleton".to_string(),
            "MattOS monorepo".to_string(),
            "working-tree".to_string(),
            "mattos package staging".to_string(),
        ),
    };
    let info = Provenance {
        package: spec.name,
        version,
        architecture: ARCH,
        mattos_source_path: &source_path,
        upstream_repository: &repository,
        upstream_commit: &commit,
        build_configuration: &configuration,
        runtime_libraries,
    };
    let destination = staging
        .join("usr/share/doc")
        .join(spec.name)
        .join("mattos-build-info.toml");
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(destination, toml::to_string_pretty(&info)?)?;
    Ok(())
}

fn component_provenance(
    repo_root: &Path,
    component: &str,
    path: &str,
    config: &str,
) -> Result<(String, String, String, String)> {
    let state = read_sync_state(repo_root, component)?
        .ok_or_else(|| anyhow!("upstream state missing for {component}"))?;
    Ok((
        path.to_string(),
        state.repo,
        state.imported_commit,
        config.to_string(),
    ))
}

fn runtime_libraries_for_spec(repo_root: &Path, spec: &PackageSpec) -> Result<Vec<String>> {
    match spec.name {
        "mattos-brush" => ldd_sonames(
            &repo_root.join("src/userland/brush/target/release/brush"),
            None,
        ),
        "mattos-coreutils" => ldd_sonames(&resolve_coreutils_multicall(repo_root)?, None),
        "mattos-curl" => {
            let install = repo_root.join("out/build/curl/install");
            ldd_sonames(
                &install.join("usr/bin/curl"),
                Some(&install.join("usr/lib/x86_64-linux-gnu")),
            )
        }
        _ => Ok(Vec::new()),
    }
}

fn ldd_sonames(binary: &Path, library_path: Option<&Path>) -> Result<Vec<String>> {
    let mut command = Command::new("ldd");
    command.arg(binary);
    if let Some(library_path) = library_path {
        command.env("LD_LIBRARY_PATH", library_path);
    }
    let output = command.output().with_context(|| {
        format!(
            "failed to inspect runtime libraries for {}",
            binary.display()
        )
    })?;
    if !output.status.success() {
        bail!("ldd failed for {}", binary.display());
    }
    let mut libraries = BTreeSet::new();
    for line in String::from_utf8(output.stdout)?.lines() {
        let token = line.trim().split_whitespace().next().unwrap_or_default();
        if token.contains(".so") {
            libraries.insert(token.to_string());
        }
    }
    Ok(libraries.into_iter().collect())
}

fn installed_size_kib(root: &Path) -> Result<u64> {
    let mut bytes = 0u64;
    walk_tree(root, &mut |path, meta| {
        if meta.is_file() && !path.starts_with(root.join("DEBIAN")) {
            bytes += meta.len();
        }
        Ok(())
    })?;
    Ok(bytes.div_ceil(1024))
}

fn count_package_entries(root: &Path) -> Result<u64> {
    let mut count = 0;
    walk_tree(root, &mut |path, _| {
        if !path.starts_with(root.join("DEBIAN")) {
            count += 1;
        }
        Ok(())
    })?;
    Ok(count)
}

fn walk_tree(
    root: &Path,
    callback: &mut dyn FnMut(&Path, &fs::Metadata) -> Result<()>,
) -> Result<()> {
    if !root.is_dir() {
        bail!("tree missing at {}", root.display());
    }
    let mut entries: Vec<_> = fs::read_dir(root)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        let meta = fs::symlink_metadata(&path)?;
        callback(&path, &meta)?;
        if meta.is_dir() {
            walk_tree(&path, callback)?;
        }
    }
    Ok(())
}

fn detect_staging_collisions(staging_root: &Path, specs: &[PackageSpec]) -> Result<()> {
    let mut owners: BTreeMap<PathBuf, (&str, bool)> = BTreeMap::new();
    for spec in specs {
        let root = staging_root.join(spec.name);
        walk_tree(&root, &mut |path, meta| {
            if path.starts_with(root.join("DEBIAN")) {
                return Ok(());
            }
            let rel = path.strip_prefix(&root)?.to_path_buf();
            let is_dir = meta.is_dir();
            if let Some((owner, owner_is_dir)) = owners.get(&rel) {
                if !is_dir || !owner_is_dir {
                    bail!(
                        "package ownership collision at /{}: {} and {}",
                        rel.display(),
                        owner,
                        spec.name
                    )
                }
            } else {
                owners.insert(rel, (spec.name, is_dir));
            }
            Ok(())
        })?;
    }
    Ok(())
}

fn normalize_tree_timestamps(root: &Path) -> Result<()> {
    let time = FileTime::from_unix_time(SOURCE_DATE_EPOCH, 0);
    walk_tree(root, &mut |path, meta| {
        if meta.file_type().is_symlink() {
            set_symlink_file_times(path, time, time)?;
        } else {
            set_file_times(path, time, time)?;
        }
        Ok(())
    })?;
    set_file_times(root, time, time)?;
    Ok(())
}

fn normalize_package_modes(root: &Path) -> Result<()> {
    walk_tree(root, &mut |path, meta| {
        if meta.file_type().is_symlink() {
            return Ok(());
        }
        let rel = path.strip_prefix(root)?;
        let mode = if meta.is_dir() {
            if rel == Path::new("root") {
                0o700
            } else if rel == Path::new("tmp") {
                0o1777
            } else {
                0o755
            }
        } else {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if meta.permissions().mode() & 0o111 != 0 {
                    0o755
                } else {
                    0o644
                }
            }
            #[cfg(not(unix))]
            {
                0o644
            }
        };
        set_mode(path.to_path_buf(), mode)
    })?;
    set_mode(root.to_path_buf(), 0o755)
}

fn sha256_file(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn verify_deb(path: &Path, expected_name: &str, expected_version: &str) -> Result<()> {
    for (field, expected) in [
        ("Package", expected_name),
        ("Version", expected_version),
        ("Architecture", ARCH),
    ] {
        let info = Command::new("dpkg-deb")
            .args(["--field", path_str(path)?, field])
            .output()
            .context("failed to inspect package metadata")?;
        if !info.status.success() {
            bail!("dpkg-deb --field failed for {}", path.display());
        }
        if String::from_utf8(info.stdout)?.trim() != expected {
            bail!(
                "package {} has invalid {field}; expected {expected}",
                path.display()
            );
        }
    }
    let contents = Command::new("dpkg-deb")
        .args(["--contents", path_str(path)?])
        .output()?;
    if !contents.status.success() {
        bail!("dpkg-deb --contents failed for {}", path.display());
    }
    let listing = String::from_utf8(contents.stdout)?;
    if listing.lines().any(|line| {
        line.split_whitespace()
            .last()
            .is_some_and(|entry| entry.contains("../"))
    }) {
        bail!("unsafe parent path leaked into {}", path.display());
    }
    Ok(())
}

fn write_inventory(repo_root: &Path, inventory: &PackageInventory) -> Result<()> {
    let path = repo_root.join("out/packages/inventory.toml");
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, toml::to_string_pretty(inventory)?)?;
    Ok(())
}

fn read_inventory(repo_root: &Path) -> Result<PackageInventory> {
    let path = repo_root.join("out/packages/inventory.toml");
    toml::from_str(&fs::read_to_string(&path)?)
        .with_context(|| format!("failed to read {}", path.display()))
}

fn print_inventory(repo_root: &Path) -> Result<()> {
    let inventory = read_inventory(repo_root)?;
    println!(
        "{:<22} {:<19} {:<6} {:<10} {}",
        "PACKAGE", "VERSION", "ARCH", "FILES", "SHA256 / ARTIFACT"
    );
    for package in inventory.package {
        println!(
            "{:<22} {:<19} {:<6} {:<10} {}  {}",
            package.name,
            package.version,
            package.architecture,
            package.file_count,
            package.sha256,
            package.artifact_path
        );
        println!(
            "  source={} depends={} runtime-libraries={}",
            package.source_component,
            if package.dependencies.is_empty() {
                "<none>".to_string()
            } else {
                package.dependencies.join(",")
            },
            if package.runtime_libraries.is_empty() {
                "<none>".to_string()
            } else {
                package.runtime_libraries.join(",")
            }
        );
    }
    Ok(())
}

fn inspect_package(repo_root: &Path, name: &str) -> Result<()> {
    validate_package_name(name)?;
    let inventory = read_inventory(repo_root)?;
    let entry = inventory
        .package
        .iter()
        .find(|entry| entry.name == name)
        .ok_or_else(|| anyhow!("package {name} is not in the built inventory"))?;
    println!(
        "package: {}\nversion: {}\narchitecture: {}\nartifact: {}\nsource: {}\ndepends: {}\nruntime libraries: {}\nfiles: {}\nsha256: {}",
        entry.name,
        entry.version,
        entry.architecture,
        entry.artifact_path,
        entry.source_component,
        if entry.dependencies.is_empty() {
            "<none>".to_string()
        } else {
            entry.dependencies.join(", ")
        },
        if entry.runtime_libraries.is_empty() {
            "<none>".to_string()
        } else {
            entry.runtime_libraries.join(", ")
        },
        entry.file_count,
        entry.sha256
    );
    let artifact = repo_root.join(&entry.artifact_path);
    run_cmd(repo_root, "dpkg-deb", &["--info", path_str(&artifact)?])?;
    run_cmd(repo_root, "dpkg-deb", &["--contents", path_str(&artifact)?])
}

pub(crate) fn generate_repository(repo_root: &Path) -> Result<()> {
    let inventory = read_inventory(repo_root)?;
    for name in PACKAGE_NAMES {
        if !inventory.package.iter().any(|entry| entry.name == *name) {
            bail!("package {name} has not been built");
        }
    }
    let repository = repo_root.join("out/repository");
    remove_path_if_exists(&repository)?;
    let pool = repository.join("pool/main");
    let index_dir = repository.join("dists/mattos/main/binary-amd64");
    fs::create_dir_all(&pool)?;
    fs::create_dir_all(&index_dir)?;
    for entry in &inventory.package {
        let source = repo_root.join(&entry.artifact_path);
        let file_name = source
            .file_name()
            .ok_or_else(|| anyhow!("invalid artifact path"))?;
        fs::copy(&source, pool.join(file_name))?;
    }
    let scan = Command::new("dpkg-scanpackages")
        .args(["pool/main", "/dev/null"])
        .current_dir(&repository)
        .output()
        .context("failed to run dpkg-scanpackages")?;
    if !scan.status.success() {
        bail!(
            "dpkg-scanpackages failed: {}",
            String::from_utf8_lossy(&scan.stderr)
        );
    }
    let packages = index_dir.join("Packages");
    fs::write(&packages, scan.stdout)?;
    let gzip = Command::new("gzip")
        .args(["-n", "-9", "-c", path_str(&packages)?])
        .output()?;
    if !gzip.status.success() {
        bail!("gzip failed for Packages index");
    }
    fs::write(index_dir.join("Packages.gz"), gzip.stdout)?;

    let release = Command::new("apt-ftparchive")
        .args([
            "-o",
            "APT::FTPArchive::Release::Origin=MattOS",
            "-o",
            "APT::FTPArchive::Release::Label=MattOS",
            "-o",
            "APT::FTPArchive::Release::Suite=mattos",
            "-o",
            "APT::FTPArchive::Release::Codename=mattos",
            "-o",
            "APT::FTPArchive::Release::Architectures=amd64",
            "-o",
            "APT::FTPArchive::Release::Components=main",
            "-o",
            "APT::FTPArchive::Release::Description=Local MattOS bootstrap repository",
            "release",
            "dists/mattos",
        ])
        .env("SOURCE_DATE_EPOCH", SOURCE_DATE_EPOCH.to_string())
        .current_dir(&repository)
        .output()
        .context("failed to run apt-ftparchive")?;
    if !release.status.success() {
        bail!(
            "apt-ftparchive failed: {}",
            String::from_utf8_lossy(&release.stderr)
        );
    }
    let release_body = String::from_utf8(release.stdout)?;
    let release_body = release_body
        .lines()
        .map(|line| {
            if line.starts_with("Date: ") {
                "Date: Thu, 01 Jan 2026 00:00:00 +0000"
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    fs::write(repository.join("dists/mattos/Release"), release_body)?;
    validate_repository(&repository)?;
    println!(
        "generated local MattOS repository at {}",
        repository.display()
    );
    Ok(())
}

fn validate_repository(repository: &Path) -> Result<()> {
    let packages = fs::read_to_string(repository.join("dists/mattos/main/binary-amd64/Packages"))?;
    for name in PACKAGE_NAMES {
        if !packages.contains(&format!("Package: {name}\n")) {
            bail!("Packages index missing {name}");
        }
    }
    if packages.contains("deb.debian.org") || packages.contains("archive.ubuntu.com") {
        bail!("foreign repository URL found in Packages");
    }
    let release = fs::read_to_string(repository.join("dists/mattos/Release"))?;
    for field in [
        "Origin: MattOS",
        "Suite: mattos",
        "Codename: mattos",
        "Architectures: amd64",
        "Components: main",
        "SHA256:",
    ] {
        if !release.contains(field) {
            bail!("Release missing {field}");
        }
    }
    Ok(())
}

pub(crate) fn install_prototype_packages(repo_root: &Path, rootfs: &Path) -> Result<()> {
    build_all_packages(repo_root)?;
    generate_repository(repo_root)?;
    let inventory = read_inventory(repo_root)?;
    let admindir = rootfs.join("var/lib/dpkg");
    for rel in ["info", "updates", "triggers", "parts"] {
        fs::create_dir_all(admindir.join(rel))?;
    }
    fs::create_dir_all(rootfs.join("var/log"))?;
    fs::write(admindir.join("status"), "")?;
    fs::write(admindir.join("available"), "")?;
    let mut command = Command::new("dpkg");
    command
        .arg(format!("--root={}", rootfs.display()))
        .arg(format!("--admindir={}", admindir.display()))
        .arg(format!(
            "--log={}",
            rootfs.join("var/log/dpkg.log").display()
        ))
        .args(["--force-not-root", "--force-bad-path", "--install"]);
    for name in PACKAGE_NAMES {
        let entry = inventory
            .package
            .iter()
            .find(|entry| entry.name == *name)
            .unwrap();
        command.arg(repo_root.join(&entry.artifact_path));
    }
    let status = command.status().context("failed to run dpkg for rootfs")?;
    if !status.success() {
        bail!("dpkg package installation into rootfs failed with {status}");
    }
    validate_dpkg_database(rootfs)?;
    Ok(())
}

pub(crate) fn validate_dpkg_database(rootfs: &Path) -> Result<()> {
    let admindir = rootfs.join("var/lib/dpkg");
    for name in PACKAGE_NAMES {
        let output = Command::new("dpkg-query")
            .arg(format!("--admindir={}", admindir.display()))
            .args(["-W", "-f=${db:Status-Status}", name])
            .output()?;
        if !output.status.success() || String::from_utf8_lossy(&output.stdout) != "installed" {
            bail!("dpkg database does not report {name} installed");
        }
    }
    for (path, owner) in [
        ("/usr/bin/brush", "mattos-brush"),
        ("/usr/bin/curl", "mattos-curl"),
        ("/usr/bin/ls", "mattos-coreutils"),
    ] {
        let output = Command::new("dpkg-query")
            .arg(format!("--admindir={}", admindir.display()))
            .args(["-S", path])
            .output()?;
        if !output.status.success() || !String::from_utf8_lossy(&output.stdout).starts_with(owner) {
            bail!("dpkg ownership query failed for {path}");
        }
    }
    Ok(())
}

pub(crate) fn package_owned_paths(rootfs: &Path) -> Result<BTreeSet<PathBuf>> {
    let admindir = rootfs.join("var/lib/dpkg/info");
    let mut owned = BTreeSet::new();
    for name in PACKAGE_NAMES {
        let list = fs::read_to_string(admindir.join(format!("{name}.list")))?;
        for line in list.lines() {
            let rel = line.trim_start_matches('/');
            if !rel.is_empty() {
                owned.insert(PathBuf::from(rel));
            }
        }
    }
    Ok(owned)
}

pub(crate) fn reject_legacy_collision(
    owned: &BTreeSet<PathBuf>,
    destination_rel: &Path,
) -> Result<()> {
    let normalized = destination_rel.strip_prefix("/").unwrap_or(destination_rel);
    if owned.contains(normalized) {
        bail!(
            "legacy rootfs install would overwrite package-owned /{}",
            normalized.display()
        );
    }
    Ok(())
}

pub(crate) fn snapshot_package_files(
    rootfs: &Path,
    owned: &BTreeSet<PathBuf>,
) -> Result<BTreeMap<PathBuf, String>> {
    let mut snapshot = BTreeMap::new();
    for rel in owned {
        let path = rootfs.join(rel);
        let meta = fs::symlink_metadata(&path)
            .with_context(|| format!("package-owned path disappeared: /{}", rel.display()))?;
        let identity = if meta.file_type().is_symlink() {
            format!("symlink:{}", fs::read_link(&path)?.display())
        } else if meta.is_file() {
            format!("file:{}", sha256_file(&path)?)
        } else if meta.is_dir() {
            "directory".to_string()
        } else {
            format!("special:{:?}", meta.file_type())
        };
        snapshot.insert(rel.clone(), identity);
    }
    Ok(snapshot)
}

pub(crate) fn validate_package_snapshot(
    rootfs: &Path,
    expected: &BTreeMap<PathBuf, String>,
) -> Result<()> {
    let owned: BTreeSet<PathBuf> = expected.keys().cloned().collect();
    let actual = snapshot_package_files(rootfs, &owned)?;
    if actual != *expected {
        let changed = expected
            .iter()
            .find(|(path, identity)| actual.get(*path) != Some(*identity))
            .map(|(path, _)| path.display().to_string())
            .unwrap_or_else(|| "<unknown>".into());
        bail!("legacy rootfs assembly changed package-owned /{changed}")
    }
    Ok(())
}

pub(crate) fn stage_built_dpkg_runtime(
    repo_root: &Path,
    rootfs: &Path,
    owned: &BTreeSet<PathBuf>,
) -> Result<()> {
    let install = repo_root.join("out/build/dpkg/install");
    for rel in ["usr/bin/dpkg", "usr/bin/dpkg-query", "usr/bin/dpkg-deb"] {
        reject_legacy_collision(owned, Path::new(rel))?;
        let source = install.join(rel);
        let destination = rootfs.join(rel);
        copy_built_binary_and_runtime(&source, &destination, rootfs)?;
    }
    for rel in ["usr/share/dpkg", "usr/libexec/dpkg"] {
        let source = install.join(rel);
        if source.exists() {
            copy_tree_excluding_dotgit(&source, &rootfs.join(rel))?;
        }
    }
    Ok(())
}

pub(crate) fn embed_repository(repo_root: &Path, rootfs: &Path) -> Result<()> {
    let source = repo_root.join("out/repository");
    if !source.join("dists/mattos/Release").is_file() {
        bail!("local repository has not been generated");
    }
    copy_tree_excluding_dotgit(&source, &rootfs.join("usr/share/mattos/repository"))
}

pub(crate) fn build_dpkg(repo_root: &Path) -> Result<()> {
    let source = repo_root.join("src/system/packages/dpkg");
    if !source.join("configure.ac").is_file() {
        bail!("dpkg source missing; run upstream import dpkg");
    }
    let out = repo_root.join("out/build/dpkg");
    let source_copy = out.join("source");
    let build = out.join("build");
    let install = out.join("install");
    remove_path_if_exists(&source_copy)?;
    remove_path_if_exists(&build)?;
    remove_path_if_exists(&install)?;
    fs::create_dir_all(&out)?;
    sync_build_source(&source, &source_copy)?;
    let state = read_sync_state(repo_root, "dpkg")?
        .ok_or_else(|| anyhow!("upstream state missing for dpkg"))?;
    let changelog = fs::read_to_string(source_copy.join("debian/changelog"))?;
    let upstream_version = changelog
        .lines()
        .next()
        .and_then(|line| line.split_once('('))
        .and_then(|(_, rest)| rest.split_once(')'))
        .map(|(version, _)| version)
        .ok_or_else(|| anyhow!("unable to derive dpkg version from debian/changelog"))?;
    let short_commit = state
        .imported_commit
        .get(..8)
        .unwrap_or(&state.imported_commit);
    fs::write(
        source_copy.join(".dist-version"),
        format!("{upstream_version}+git.{short_commit}\n"),
    )?;
    fs::write(
        source_copy.join(".dist-vcs-id"),
        format!("{}\n", state.imported_commit),
    )?;
    run_cmd(&source_copy, "./autogen", &[])?;
    fs::create_dir_all(&build)?;
    let configure = source_copy.join("configure");
    run_cmd(
        &build,
        path_str(&configure)?,
        &[
            "--prefix=/usr",
            "--sysconfdir=/etc",
            "--localstatedir=/var",
            "--libexecdir=/usr/libexec",
            "--disable-dselect",
            "--disable-start-stop-daemon",
            "--disable-update-alternatives",
            "--disable-nls",
        ],
    )?;
    run_cmd(&build, "make", &["-j", "4"])?;
    fs::create_dir_all(&install)?;
    run_cmd(
        &build,
        "make",
        &["install", &format!("DESTDIR={}", install.display())],
    )?;
    for rel in ["usr/bin/dpkg", "usr/bin/dpkg-query", "usr/bin/dpkg-deb"] {
        if !install.join(rel).is_file() {
            bail!("dpkg build did not produce {rel}");
        }
    }
    println!("built imported dpkg into {}", install.display());
    Ok(())
}

pub(crate) fn build_apt(repo_root: &Path) -> Result<()> {
    let source = repo_root.join("src/system/packages/apt");
    if !source.join("CMakeLists.txt").is_file() {
        bail!("APT source missing; run upstream import apt");
    }
    let out = repo_root.join("out/build/apt");
    let source_copy = out.join("source");
    let build = out.join("build");
    let install = out.join("install");
    remove_path_if_exists(&source_copy)?;
    remove_path_if_exists(&build)?;
    remove_path_if_exists(&install)?;
    fs::create_dir_all(&out)?;
    sync_build_source(&source, &source_copy)?;
    run_cmd(
        repo_root,
        "cmake",
        &[
            "-S",
            path_str(&source_copy)?,
            "-B",
            path_str(&build)?,
            "-G",
            "Ninja",
            "-DCMAKE_BUILD_TYPE=Release",
            "-DCMAKE_INSTALL_PREFIX=/usr",
            "-DCMAKE_INSTALL_SYSCONFDIR=/etc",
            "-DCURRENT_VENDOR=mattos",
            "-DCOMMON_ARCH=amd64",
            "-DDPKG_DATADIR=/usr/share/dpkg",
            "-DWITH_DOC=OFF",
            "-DWITH_TESTS=OFF",
            "-DUSE_NLS=OFF",
        ],
    )?;
    run_cmd(
        repo_root,
        "cmake",
        &["--build", path_str(&build)?, "--parallel", "4"],
    )?;
    fs::create_dir_all(&install)?;
    run_cmd_with_env_overrides(
        repo_root,
        "cmake",
        &["--install", path_str(&build)?],
        &[("DESTDIR", install.display().to_string())],
    )?;
    for rel in ["usr/bin/apt", "usr/bin/apt-cache", "usr/bin/apt-get"] {
        if !install.join(rel).is_file() {
            bail!("APT build did not produce {rel}");
        }
    }
    println!("built imported APT into {}", install.display());
    Ok(())
}

fn relative_display(root: &Path, path: &Path) -> Result<String> {
    Ok(path.strip_prefix(root)?.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{PermissionsExt, symlink};

    fn run_ok(cwd: &Path, program: &str, args: &[&str]) {
        let status = Command::new(program)
            .args(args)
            .current_dir(cwd)
            .status()
            .unwrap();
        assert!(
            status.success(),
            "command failed: {program} {}",
            args.join(" ")
        );
    }

    #[test]
    fn validates_package_names_versions_and_architecture() {
        assert!(validate_package_name("mattos-coreutils").is_ok());
        assert!(validate_package_name("MattOS").is_err());
        assert!(validate_package_name("mattos_coreutils").is_err());
        assert!(validate_debian_version("0.9.0-1mattos1").is_ok());
        assert!(validate_debian_version("today!").is_err());
        assert_eq!(ARCH, "amd64");
    }

    #[test]
    fn control_contains_required_metadata() {
        let spec = package_specs()
            .into_iter()
            .find(|s| s.name == "mattos-curl")
            .unwrap();
        let control = render_control(&spec, "8.22.0-1mattos1", 42, &["libc.so.6".into()]).unwrap();
        for field in [
            "Package:",
            "Version:",
            "Architecture: amd64",
            "Maintainer:",
            "Description:",
            "Depends:",
            "Provides:",
            "Conflicts:",
            "Replaces:",
            "Installed-Size:",
            "X-MattOS-Legacy-Runtime-Libraries:",
        ] {
            assert!(control.contains(field), "missing {field}");
        }
    }

    #[test]
    fn collision_policy_allows_shared_directories_but_rejects_files_and_symlinks() {
        let temp = tempfile::tempdir().unwrap();
        let specs = &package_specs()[..2];
        for spec in specs {
            fs::create_dir_all(temp.path().join(spec.name).join("usr/bin")).unwrap();
        }
        assert!(detect_staging_collisions(temp.path(), specs).is_ok());
        fs::write(temp.path().join(specs[0].name).join("usr/bin/tool"), "a").unwrap();
        symlink(
            "target",
            temp.path().join(specs[1].name).join("usr/bin/tool"),
        )
        .unwrap();
        assert!(detect_staging_collisions(temp.path(), specs).is_err());
    }

    #[test]
    fn stage_preserves_mode_and_symlink_and_checksum_is_stable() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        fs::write(&source, "payload").unwrap();
        fs::set_permissions(&source, fs::Permissions::from_mode(0o751)).unwrap();
        let destination = temp.path().join("stage/usr/bin/tool");
        copy_preserving(&source, &destination).unwrap();
        assert_eq!(
            fs::metadata(&destination).unwrap().permissions().mode() & 0o777,
            0o751
        );
        symlink("tool", temp.path().join("stage/usr/bin/alias")).unwrap();
        assert_eq!(
            fs::read_link(temp.path().join("stage/usr/bin/alias")).unwrap(),
            Path::new("tool")
        );
        assert_eq!(
            sha256_file(&destination).unwrap(),
            sha256_file(&destination).unwrap()
        );
    }

    #[test]
    fn staging_and_output_paths_are_bounded() {
        assert!(validate_package_name("../../escape").is_err());
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("out/packages/staging/mattos-filesystem");
        assert!(root.starts_with(temp.path().join("out/packages/staging")));
    }

    #[test]
    fn legacy_collision_is_rejected() {
        let owned = BTreeSet::from([PathBuf::from("usr/bin/brush")]);
        assert!(reject_legacy_collision(&owned, Path::new("usr/bin/brush")).is_err());
        assert!(reject_legacy_collision(&owned, Path::new("usr/bin/systemctl")).is_ok());
    }

    #[test]
    fn permanent_packages_exclude_live_profile_and_foreign_sources() {
        let files = [
            "etc/os-release",
            "etc/profile",
            "etc/apt/sources.list.d/mattos.sources",
        ];
        assert!(files.iter().all(|path| !path.contains("live-profile")));
        let sources = include_str!("../../../system/packages/config/base-files/mattos.sources");
        assert!(sources.contains("file:/usr/share/mattos/repository"));
        assert!(
            !sources.contains("debian")
                && !sources.contains("ubuntu")
                && !sources.contains("http:")
        );
    }

    #[test]
    fn repository_layout_and_release_metadata_are_validated() {
        let temp = tempfile::tempdir().unwrap();
        let index = temp.path().join("dists/mattos/main/binary-amd64");
        fs::create_dir_all(&index).unwrap();
        let packages = PACKAGE_NAMES
            .iter()
            .map(|name| format!("Package: {name}\nVersion: 1\n\n"))
            .collect::<String>();
        fs::write(index.join("Packages"), packages).unwrap();
        fs::write(temp.path().join("dists/mattos/Release"), "Origin: MattOS\nSuite: mattos\nCodename: mattos\nArchitectures: amd64\nComponents: main\nSHA256:\n").unwrap();
        assert!(validate_repository(temp.path()).is_ok());
        fs::write(
            index.join("Packages"),
            "Package: foreign\nHomepage: https://deb.debian.org\n",
        )
        .unwrap();
        assert!(validate_repository(temp.path()).is_err());
    }

    #[test]
    fn dpkg_semantics_create_database_and_ownership_queries() {
        let temp = tempfile::tempdir().unwrap();
        let stage = temp.path().join("stage");
        fs::create_dir_all(stage.join("DEBIAN")).unwrap();
        fs::create_dir_all(stage.join("usr/bin")).unwrap();
        fs::write(stage.join("DEBIAN/control"), "Package: mattos-test\nVersion: 1.0-1mattos1\nArchitecture: amd64\nMaintainer: MattOS Test <test@mattos.invalid>\nInstalled-Size: 1\nDepends:\nDescription: test package\n").unwrap();
        fs::write(stage.join("usr/bin/mattos-test"), "test\n").unwrap();
        let deb = temp.path().join("mattos-test.deb");
        run_ok(
            temp.path(),
            "dpkg-deb",
            &[
                "--root-owner-group",
                "--build",
                path_str(&stage).unwrap(),
                path_str(&deb).unwrap(),
            ],
        );
        let root = temp.path().join("root");
        let admindir = root.join("var/lib/dpkg");
        fs::create_dir_all(admindir.join("info")).unwrap();
        fs::create_dir_all(admindir.join("updates")).unwrap();
        fs::create_dir_all(root.join("var/log")).unwrap();
        fs::write(admindir.join("status"), "").unwrap();
        run_ok(
            temp.path(),
            "dpkg",
            &[
                &format!("--root={}", root.display()),
                &format!("--admindir={}", admindir.display()),
                &format!("--log={}", root.join("var/log/dpkg.log").display()),
                "--force-not-root",
                "--install",
                path_str(&deb).unwrap(),
            ],
        );
        let owned = Command::new("dpkg-query")
            .arg(format!("--admindir={}", admindir.display()))
            .args(["-S", "/usr/bin/mattos-test"])
            .output()
            .unwrap();
        assert!(owned.status.success());
        assert!(String::from_utf8_lossy(&owned.stdout).starts_with("mattos-test:"));
        assert!(admindir.join("status").metadata().unwrap().len() > 0);
    }
}
