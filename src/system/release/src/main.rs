use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    os::{fd::AsRawFd, unix::fs::PermissionsExt},
    path::{Path, PathBuf},
    process::Command,
};

const MANIFEST: &str = "/usr/share/mattos/release.json";
const STATE: &str = "/var/lib/mattos/releases/state.json";
const JOURNAL: &str = "/var/lib/mattos/releases/upgrade.json";

#[derive(Parser)]
#[command(about = "Inspect and upgrade MattOS releases through signed APT packages")]
struct Cli {
    #[command(subcommand)]
    command: Action,
}
#[derive(Subcommand)]
enum Action {
    Status,
    /// Download release metadata and simulate the complete package transaction.
    Plan {
        #[arg(long)]
        release: Option<String>,
    },
    /// Upgrade installed MattOS packages and run pending system migrations.
    Apply {
        #[arg(long)]
        release: Option<String>,
        #[arg(long)]
        yes: bool,
    },
    /// Resume migrations for the installed release (also used at first boot).
    Migrate,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct Manifest {
    schema_version: u32,
    version: String,
    packages: BTreeMap<String, String>,
    migrations: Vec<Migration>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
struct Migration {
    id: String,
    path: String,
    sha256: String,
}
#[derive(Default, Debug, Deserialize, Serialize)]
struct State {
    #[serde(default)]
    original_version: Option<String>,
    #[serde(default)]
    completed_version: Option<String>,
    #[serde(default)]
    migrations: BTreeMap<String, String>,
}
#[derive(Debug, Deserialize, Serialize)]
struct Journal {
    from: String,
    to: String,
    phase: String,
    automatic_packages: Vec<String>,
}

fn valid_release(value: &str) -> bool {
    let p: Vec<_> = value.split('.').collect();
    p.len() == 3
        && p[0].len() == 2
        && p[1].len() == 2
        && p.iter()
            .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
        && p[1].parse::<u8>().is_ok_and(|n| (1..=12).contains(&n))
        && !p[2].starts_with('0')
        && p[2].parse::<u32>().is_ok_and(|n| n > 0)
}
fn validate(m: &Manifest) -> Result<()> {
    if m.schema_version != 1 || !valid_release(&m.version) {
        bail!("Invalid MattOS release manifest");
    }
    for required in [
        "mattos-release",
        "mattos-upgrade",
        "mattos-base",
        "mattos-base-files",
    ] {
        if !m.packages.contains_key(required) {
            bail!("Release lacks {required}");
        }
    }
    for (name, version) in &m.packages {
        if !safe_package_name(name)
            || version.is_empty()
            || !version
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b".+:~_-".contains(&b))
        {
            bail!("Invalid package name or version in release manifest");
        }
    }
    if !m.packages["mattos-release"].starts_with(&format!("{}-", m.version)) {
        bail!("Release package version disagrees with manifest");
    }
    let mut ids = std::collections::BTreeSet::new();
    for migration in &m.migrations {
        if migration.id.is_empty()
            || !ids.insert(&migration.id)
            || migration.sha256.len() != 64
            || !migration.sha256.bytes().all(|b| b.is_ascii_hexdigit())
            || !migration.path.starts_with("usr/lib/mattos/migrations/")
            || Path::new(&migration.path)
                .components()
                .any(|c| !matches!(c, std::path::Component::Normal(_)))
        {
            bail!("Invalid or duplicate release migration");
        }
    }
    Ok(())
}
fn safe_package_name(name: &str) -> bool {
    name.bytes()
        .next()
        .is_some_and(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"+-.".contains(&b))
}
fn capture(program: &str, args: &[&str]) -> Result<String> {
    let output = Command::new(program)
        .env("LC_ALL", "C")
        .args(args)
        .output()
        .with_context(|| format!("Run {program}"))?;
    if !output.status.success() {
        bail!(
            "{program} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(String::from_utf8(output.stdout)?)
}
fn run(program: &str, args: &[String]) -> Result<()> {
    if !Command::new(program)
        .env("LC_ALL", "C")
        .args(args)
        .status()?
        .success()
    {
        bail!("{program} failed; resolve the reported error and rerun this command");
    }
    Ok(())
}
fn installed_packages() -> Result<BTreeMap<String, String>> {
    parse_installed(&capture(
        "dpkg-query",
        &["-W", "-f=${Package}\t${Version}\t${db:Status-Status}\\n"],
    )?)
}
fn parse_installed(body: &str) -> Result<BTreeMap<String, String>> {
    let mut packages = BTreeMap::new();
    for line in body.lines() {
        let parts: Vec<_> = line.split('\t').collect();
        if parts.len() != 3 {
            bail!("Unexpected dpkg-query response");
        }
        if parts[2] == "installed" {
            packages.insert(parts[0].into(), parts[1].into());
        }
    }
    Ok(packages)
}
fn installed_version() -> String {
    fs::read_to_string("/etc/os-release")
        .ok()
        .and_then(|body| {
            body.lines().find_map(|line| {
                line.strip_prefix("VERSION_ID=")
                    .map(|v| v.trim_matches('"').to_string())
            })
        })
        .unwrap_or_else(|| "unversioned".into())
}
fn mattos_identity(os_release: Option<&str>, interrupted: bool) -> bool {
    match os_release {
        Some(body) => body.lines().any(|line| {
            line.trim()
                .strip_prefix("ID=")
                .is_some_and(|value| value.trim_matches('"') == "mattos")
        }),
        None => interrupted,
    }
}
fn require_mattos() -> Result<()> {
    let identity = match fs::read_to_string("/etc/os-release") {
        Ok(body) => Some(body),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.into()),
    };
    // dpkg can be interrupted between transferring the old identity file and
    // configuring mattos-release. A journal written before that transaction
    // permits recovery without treating another distribution as MattOS.
    let interrupted = if identity.is_none() && Path::new(JOURNAL).is_file() {
        let journal: Journal = serde_json::from_slice(&fs::read(JOURNAL)?)?;
        matches!(journal.phase.as_str(), "packages" | "migrations") && valid_release(&journal.to)
    } else {
        false
    };
    if !mattos_identity(identity.as_deref(), interrupted) {
        bail!("The release coordinator requires a MattOS installation (ID=mattos)");
    }
    Ok(())
}
fn atomic_json(path: &Path, data: &impl Serialize) -> Result<()> {
    let parent = path.parent().context("State path has no parent")?;
    fs::create_dir_all(parent)?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    temp.write_all(&serde_json::to_vec_pretty(data)?)?;
    temp.write_all(b"\n")?;
    temp.as_file()
        .set_permissions(fs::Permissions::from_mode(0o644))?;
    temp.as_file().sync_all()?;
    temp.persist(path)?;
    fs::File::open(parent)?.sync_all()?;
    Ok(())
}
fn load_state(path: &Path) -> Result<State> {
    match fs::read(path) {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(State::default()),
        Err(e) => Err(e.into()),
    }
}
fn migrate(root: &Path, manifest: &Manifest, state_path: &Path) -> Result<()> {
    validate(manifest)?;
    let mut state = load_state(state_path)?;
    // Validate all scripts, including completed ones, before changing the system.
    for migration in &manifest.migrations {
        let path = root.join(&migration.path);
        if !fs::symlink_metadata(&path)?.is_file() {
            bail!("Migration must be a regular file: {}", migration.id);
        }
        let actual = format!("{:x}", Sha256::digest(fs::read(&path)?));
        if actual != migration.sha256 {
            bail!("Migration checksum mismatch: {}", migration.id);
        }
        if let Some(previous) = state.migrations.get(&migration.id) {
            if previous != &actual {
                bail!("Completed migration changed: {}", migration.id);
            }
        }
    }
    for migration in &manifest.migrations {
        if state.migrations.contains_key(&migration.id) {
            continue;
        }
        println!("Applying system migration {}", migration.id);
        run(
            "/bin/sh",
            &[root.join(&migration.path).display().to_string()],
        )?;
        state
            .migrations
            .insert(migration.id.clone(), migration.sha256.clone());
        atomic_json(state_path, &state)?;
    }
    state.completed_version = Some(manifest.version.clone());
    atomic_json(state_path, &state)
}
fn package_targets(manifest: &Manifest, installed: &BTreeMap<String, String>) -> Vec<String> {
    manifest
        .packages
        .iter()
        .filter(|(name, _)| installed.contains_key(*name) || *name == "mattos-release")
        .map(|(name, version)| format!("{name}={version}"))
        .collect()
}
fn verify_cohort(manifest: &Manifest, installed: &BTreeMap<String, String>) -> Result<()> {
    for (name, version) in installed {
        if let Some(expected) = manifest.packages.get(name) {
            if version != expected {
                bail!(
                    "{name}={version} does not match release {} ({expected}); complete mattos-upgrade apply before migrations",
                    manifest.version
                );
            }
        }
    }
    if installed.get("mattos-release") != manifest.packages.get("mattos-release") {
        bail!("The selected release package is not configured");
    }
    Ok(())
}
fn download_manifest(release: Option<&str>) -> Result<(tempfile::TempDir, Manifest)> {
    if release.is_some_and(|v| !valid_release(v)) {
        bail!("Expected YY.MM.revision, for example 26.10.1");
    }
    let version = if let Some(release) = release {
        capture("apt-cache", &["madison", "mattos-release"])?.lines().filter_map(|line| line.split('|').nth(1)).map(str::trim)
            .find(|v| v.starts_with(&format!("{release}-"))).map(str::to_string)
    } else {
        capture("apt-cache", &["policy", "mattos-release"])?.lines().find_map(|line| line.trim().strip_prefix("Candidate: ").filter(|v| *v != "(none)").map(str::to_string))
    }.context("No matching release is available in APT. Publish its packages and release metadata, then refresh APT indexes.")?;
    let dir = tempfile::tempdir()?;
    if !Command::new("apt-get")
        .args(["download", &format!("mattos-release={version}")])
        .current_dir(dir.path())
        .status()?
        .success()
    {
        bail!("Cannot download authenticated release metadata");
    }
    let debs: Vec<PathBuf> = fs::read_dir(dir.path())?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "deb"))
        .collect();
    if debs.len() != 1 {
        bail!("Expected one release package");
    }
    let extracted = dir.path().join("metadata");
    run(
        "dpkg-deb",
        &[
            "--extract".into(),
            debs[0].display().to_string(),
            extracted.display().to_string(),
        ],
    )?;
    let manifest: Manifest =
        serde_json::from_slice(&fs::read(extracted.join(MANIFEST.trim_start_matches('/')))?)?;
    validate(&manifest)?;
    if manifest.packages["mattos-release"] != version {
        bail!("Downloaded release metadata has the wrong package version");
    }
    Ok((dir, manifest))
}
fn require_root() -> Result<()> {
    if unsafe { libc::geteuid() } != 0 {
        bail!("Run with sudo to change the system");
    }
    Ok(())
}
fn lock() -> Result<fs::File> {
    fs::create_dir_all("/var/lib/mattos/releases")?;
    let file = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open("/var/lib/mattos/releases/upgrade.lock")?;
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        bail!("Another MattOS release upgrade is running");
    }
    Ok(file)
}
fn upgrade(release: Option<&str>, apply: bool) -> Result<()> {
    let _lock = if apply {
        require_root()?;
        Some(lock()?)
    } else {
        None
    };
    if apply {
        run("apt-get", &["update".into()])?;
    }
    let (_metadata, manifest) = download_manifest(release)?;
    let installed = installed_packages()?;
    for (name, target) in &manifest.packages {
        if let Some(current) = installed.get(name) {
            if Command::new("dpkg")
                .args(["--compare-versions", current, "gt", target])
                .status()?
                .success()
            {
                bail!(
                    "{name} {current} is newer than release {} ({target}); automatic downgrades are refused",
                    manifest.version
                );
            }
        }
    }
    let current = installed_version();
    println!("MattOS {current} -> {}", manifest.version);
    let mut args: Vec<String> = vec![
        "--no-remove".into(),
        "--no-install-recommends".into(),
        "-o".into(),
        "Dpkg::Options::=--force-confdef".into(),
        "-o".into(),
        "Dpkg::Options::=--force-confold".into(),
        "install".into(),
    ];
    args.extend(package_targets(&manifest, &installed));
    let mut simulation = vec!["--simulate".into()];
    simulation.extend(args.clone());
    run("apt-get", &simulation)?;
    if !apply {
        return Ok(());
    }
    let mut journal = if Path::new(JOURNAL).exists() {
        let previous: Journal = serde_json::from_slice(&fs::read(JOURNAL)?)?;
        if previous.phase != "complete" && previous.to != manifest.version {
            bail!("Finish the interrupted upgrade to {} first", previous.to);
        }
        if previous.phase != "complete" {
            previous
        } else {
            Journal {
                from: current.clone(),
                to: manifest.version.clone(),
                phase: "packages".into(),
                automatic_packages: capture("apt-mark", &["showauto"])?
                    .lines()
                    .map(str::to_string)
                    .collect(),
            }
        }
    } else {
        Journal {
            from: current.clone(),
            to: manifest.version.clone(),
            phase: "packages".into(),
            automatic_packages: capture("apt-mark", &["showauto"])?
                .lines()
                .map(str::to_string)
                .collect(),
        }
    };
    let mut state = load_state(Path::new(STATE))?;
    if state.original_version.is_none() {
        state.original_version = Some(journal.from.clone());
        atomic_json(Path::new(STATE), &state)?;
    }
    atomic_json(Path::new(JOURNAL), &journal)?;
    args.insert(0, "-y".into());
    run("apt-get", &args)?;
    if !journal.automatic_packages.is_empty() {
        let mut mark = vec!["auto".into()];
        mark.extend(journal.automatic_packages.clone());
        run("apt-mark", &mark)?;
    }
    if !capture("dpkg", &["--audit"])?.trim().is_empty() {
        bail!(
            "dpkg reports incomplete package configuration; resolve it and resume this release upgrade"
        );
    }
    let actual = installed_packages()?;
    for target in package_targets(&manifest, &actual) {
        let (name, version) = target.split_once('=').unwrap();
        if actual.get(name).map(String::as_str) != Some(version) {
            bail!("Release transaction did not configure {name}={version}");
        }
    }
    journal.phase = "migrations".into();
    atomic_json(Path::new(JOURNAL), &journal)?;
    let installed_manifest: Manifest = serde_json::from_slice(&fs::read(MANIFEST)?)?;
    if installed_manifest.version != manifest.version {
        bail!("Installed release metadata does not match selected release");
    }
    migrate(Path::new("/"), &installed_manifest, Path::new(STATE))?;
    journal.phase = "complete".into();
    atomic_json(Path::new(JOURNAL), &journal)?;
    println!(
        "MattOS {} upgrade completed. Reboot to use the updated system and kernel.",
        manifest.version
    );
    Ok(())
}
fn main() -> Result<()> {
    let cli = Cli::parse();
    require_mattos()?;
    match cli.command {
        Action::Status => {
            println!("MattOS {}", installed_version());
            if Path::new(STATE).exists() {
                let state = load_state(Path::new(STATE))?;
                if let Some(original) = state.original_version {
                    println!("Original installation: {original}");
                }
                if let Some(completed) = state.completed_version {
                    println!("Last completed release: {completed}");
                }
                println!("Completed system migrations: {}", state.migrations.len());
            }
            if Path::new(JOURNAL).exists() {
                let journal: Journal = serde_json::from_slice(&fs::read(JOURNAL)?)?;
                println!(
                    "Upgrade {} -> {}: {}",
                    journal.from, journal.to, journal.phase
                );
                if journal.phase != "complete" {
                    println!(
                        "Resolve the reported error, then resume apply --release {} --yes",
                        journal.to
                    );
                }
            }
            Ok(())
        }
        Action::Plan { release } => upgrade(release.as_deref(), false),
        Action::Apply { release, yes } => {
            if !yes {
                bail!("Review mattos-upgrade plan first; apply requires --yes");
            }
            upgrade(release.as_deref(), true)
        }
        Action::Migrate => {
            require_root()?;
            let _lock = lock()?;
            if Path::new(JOURNAL).exists() {
                let journal: Journal = serde_json::from_slice(&fs::read(JOURNAL)?)?;
                if journal.phase == "packages" {
                    bail!(
                        "Interrupted package transaction; resume mattos-upgrade apply --yes before migrations"
                    );
                }
            }
            let manifest: Manifest = serde_json::from_slice(&fs::read(MANIFEST)?)?;
            let state = load_state(Path::new(STATE))?;
            if state.completed_version.as_deref() != Some(manifest.version.as_str()) {
                if !capture("dpkg", &["--audit"])?.trim().is_empty() {
                    bail!("Finish incomplete dpkg configuration before release migrations");
                }
                verify_cohort(&manifest, &installed_packages()?)?;
            }
            migrate(Path::new("/"), &manifest, Path::new(STATE))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn manifest() -> Manifest {
        Manifest {
            schema_version: 1,
            version: "26.10.1".into(),
            packages: [
                ("mattos-release", "26.10.1-1mattos1"),
                ("mattos-upgrade", "0.1.0-1mattos1"),
                ("mattos-base", "0.1-1mattos6"),
                ("mattos-base-files", "0.1-1mattos5"),
            ]
            .into_iter()
            .map(|(a, b)| (a.into(), b.into()))
            .collect(),
            migrations: vec![],
        }
    }
    #[test]
    fn incomplete_or_mixed_package_cohorts_cannot_run_release_migrations() {
        let m = manifest();
        let mut installed = m.packages.clone();
        installed.insert("foreign-app".into(), "42".into());
        verify_cohort(&m, &installed).unwrap();
        installed.insert("mattos-base".into(), "older-version".into());
        assert!(verify_cohort(&m, &installed).is_err());
        installed = m.packages.clone();
        installed.remove("mattos-release");
        assert!(verify_cohort(&m, &installed).is_err());
    }
    #[test]
    fn release_format() {
        for good in ["26.10.1", "26.10.2", "27.01.1"] {
            assert!(valid_release(good));
        }
        for bad in [
            "2026.10.1",
            "26.13.1",
            "26.1.1",
            "26.10.0",
            "26.10.01",
            "26.10.-1",
        ] {
            assert!(!valid_release(bad));
        }
    }
    #[test]
    fn only_mattos_or_an_interrupted_identity_transfer_can_be_adopted() {
        assert!(mattos_identity(Some("ID=mattos\n"), false));
        assert!(mattos_identity(
            Some("ID=\"mattos\"\nVERSION_ID=26.10.1\n"),
            false
        ));
        assert!(!mattos_identity(Some("ID=ubuntu\nID_LIKE=mattos\n"), false));
        assert!(!mattos_identity(Some("ID=ubuntu\n"), true));
        assert!(!mattos_identity(None, false));
        assert!(mattos_identity(None, true));
    }
    #[test]
    fn bootstrap_targets_preserve_optional_and_foreign_packages() {
        let m = manifest();
        let installed = [
            ("mattos-base".into(), "old".into()),
            ("my-app".into(), "1".into()),
        ]
        .into_iter()
        .collect();
        let t = package_targets(&m, &installed);
        assert_eq!(
            t,
            vec![
                "mattos-base=0.1-1mattos6",
                "mattos-release=26.10.1-1mattos1"
            ]
        );
    }
    #[test]
    fn invalid_manifests_fail_closed() {
        let mut m = manifest();
        m.packages.insert("--bad".into(), "1".into());
        assert!(validate(&m).is_err());
        let mut m = manifest();
        m.migrations.push(Migration {
            id: "test".into(),
            path: "usr/lib/mattos/migrations/../../bad".into(),
            sha256: "0".repeat(64),
        });
        assert!(validate(&m).is_err());
    }
    #[test]
    fn package_query_excludes_removed_or_unconfigured_packages() {
        let p = parse_installed("one\t1\tinstalled\ntwo\t2\tunpacked\nthree\t3\tconfig-files\n")
            .unwrap();
        assert_eq!(p.len(), 1);
        assert!(p.contains_key("one"));
    }
    #[test]
    fn migrations_resume_and_reject_changed_scripts() {
        let root = tempfile::tempdir().unwrap();
        let script = root.path().join("usr/lib/mattos/migrations/test.sh");
        fs::create_dir_all(script.parent().unwrap()).unwrap();
        let marker = root.path().join("calls");
        let body = format!("echo applied >> '{}'\n", marker.display());
        fs::write(&script, &body).unwrap();
        let mut m = manifest();
        m.migrations.push(Migration {
            id: "test".into(),
            path: "usr/lib/mattos/migrations/test.sh".into(),
            sha256: format!("{:x}", Sha256::digest(body.as_bytes())),
        });
        let state = root.path().join("state.json");
        migrate(root.path(), &m, &state).unwrap();
        migrate(root.path(), &m, &state).unwrap();
        assert_eq!(fs::read_to_string(marker).unwrap(), "applied\n");
        fs::write(script, "exit 0\n").unwrap();
        assert!(migrate(root.path(), &m, &state).is_err());
    }
    #[test]
    fn failed_migration_is_not_marked_complete() {
        let root = tempfile::tempdir().unwrap();
        let script = root.path().join("usr/lib/mattos/migrations/fail.sh");
        fs::create_dir_all(script.parent().unwrap()).unwrap();
        fs::write(&script, "exit 7\n").unwrap();
        let mut m = manifest();
        m.migrations.push(Migration {
            id: "fail".into(),
            path: "usr/lib/mattos/migrations/fail.sh".into(),
            sha256: format!("{:x}", Sha256::digest(b"exit 7\n")),
        });
        let state = root.path().join("state.json");
        assert!(migrate(root.path(), &m, &state).is_err());
        assert!(load_state(&state).unwrap().completed_version.is_none());
    }

    #[test]
    fn interrupted_migrations_resume_and_carry_forward_to_the_next_release() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("usr/lib/mattos/migrations");
        fs::create_dir_all(&directory).unwrap();
        let calls = root.path().join("calls");
        let ready = root.path().join("ready");
        let mut m = manifest();
        for (id, body) in [
            ("first", format!("echo first >> '{}'\n", calls.display())),
            (
                "second",
                format!(
                    "test -f '{}' || exit 1\necho second >> '{}'\n",
                    ready.display(),
                    calls.display()
                ),
            ),
        ] {
            fs::write(directory.join(id), &body).unwrap();
            m.migrations.push(Migration {
                id: id.into(),
                path: format!("usr/lib/mattos/migrations/{id}"),
                sha256: format!("{:x}", Sha256::digest(body.as_bytes())),
            });
        }
        let state = root.path().join("state.json");
        assert!(migrate(root.path(), &m, &state).is_err());
        let interrupted = load_state(&state).unwrap();
        assert!(interrupted.migrations.contains_key("first"));
        assert!(!interrupted.migrations.contains_key("second"));
        assert!(interrupted.completed_version.is_none());
        fs::write(ready, "").unwrap();
        migrate(root.path(), &m, &state).unwrap();
        m.version = "26.11.1".into();
        m.packages
            .insert("mattos-release".into(), "26.11.1-1mattos1".into());
        let body = format!("echo third >> '{}'\n", calls.display());
        fs::write(directory.join("third"), &body).unwrap();
        m.migrations.push(Migration {
            id: "third".into(),
            path: "usr/lib/mattos/migrations/third".into(),
            sha256: format!("{:x}", Sha256::digest(body.as_bytes())),
        });
        migrate(root.path(), &m, &state).unwrap();
        migrate(root.path(), &m, &state).unwrap();
        assert_eq!(fs::read_to_string(calls).unwrap(), "first\nsecond\nthird\n");
        assert_eq!(
            load_state(&state).unwrap().completed_version.as_deref(),
            Some("26.11.1")
        );
    }
}
