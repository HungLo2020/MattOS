use anyhow::{Context, Result, anyhow, bail};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::thread;
use std::time::Instant;

pub(crate) const STAGE_MANIFEST_SCHEMA_VERSION: u32 = 3;
const TIMING_SCHEMA_VERSION: u32 = 1;
const NORMALIZED_SOURCE_DATE_EPOCH: &str = "1767225600";

#[derive(Clone, Debug)]
pub(crate) struct StageSpec {
    pub(crate) id: String,
    pub(crate) source_inputs: Vec<PathBuf>,
    pub(crate) configuration_inputs: Vec<PathBuf>,
    pub(crate) tools: Vec<String>,
    pub(crate) dependencies: Vec<String>,
    pub(crate) outputs: Vec<PathBuf>,
    pub(crate) recipe: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub(crate) struct InventoryEntry {
    pub(crate) path: String,
    pub(crate) kind: String,
    pub(crate) mode: u32,
    pub(crate) uid: u32,
    pub(crate) gid: u32,
    pub(crate) size: u64,
    pub(crate) content: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub(crate) struct StageInputs {
    pub(crate) source_digest: String,
    pub(crate) configuration_digest: String,
    pub(crate) tool_digest: String,
    pub(crate) environment_digest: String,
    pub(crate) dependency_digests: BTreeMap<String, String>,
    pub(crate) full_digest: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub(crate) struct ToolIdentity {
    pub(crate) resolved_path: String,
    pub(crate) executable_sha256: String,
    pub(crate) version: String,
    pub(crate) target: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub(crate) struct DependencyIdentity {
    pub(crate) input_digest: String,
    pub(crate) output_digest: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, Eq, PartialEq)]
pub(crate) struct StageInputDetails {
    pub(crate) schema_version: u32,
    pub(crate) recipe: String,
    pub(crate) source: BTreeMap<String, String>,
    pub(crate) configuration: BTreeMap<String, String>,
    pub(crate) environment: BTreeMap<String, String>,
    pub(crate) tools: BTreeMap<String, ToolIdentity>,
    pub(crate) dependencies: BTreeMap<String, DependencyIdentity>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct StageManifest {
    pub(crate) schema_version: u32,
    pub(crate) stage: String,
    pub(crate) inputs: StageInputs,
    #[serde(default)]
    pub(crate) input_details: StageInputDetails,
    pub(crate) expected_outputs: Vec<InventoryEntry>,
    pub(crate) output_content_digest: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct TimingRecord {
    pub(crate) stage: String,
    pub(crate) started_at_utc: String,
    pub(crate) ended_at_utc: String,
    pub(crate) wall_seconds: f64,
    pub(crate) result: String,
    pub(crate) cache_status: String,
    pub(crate) reason: String,
    pub(crate) input_digest: String,
    pub(crate) output_digest: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct TimingReport {
    schema_version: u32,
    command: String,
    started_at_utc: String,
    ended_at_utc: Option<String>,
    result: String,
    stages: Vec<TimingRecord>,
}

thread_local! {
    static TIMINGS: RefCell<Option<(PathBuf, TimingReport)>> = const { RefCell::new(None) };
    static ACTIVE_BUILD_LOG: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
}

pub(crate) fn start_timing_run(repo_root: &Path, command: &str) -> Result<()> {
    fs::create_dir_all(repo_root.join("out/reports"))?;
    TIMINGS.with(|slot| {
        *slot.borrow_mut() = Some((
            repo_root.to_path_buf(),
            TimingReport {
                schema_version: TIMING_SCHEMA_VERSION,
                command: command.to_string(),
                started_at_utc: Utc::now().to_rfc3339(),
                ended_at_utc: None,
                result: "running".to_string(),
                stages: Vec::new(),
            },
        ));
    });
    persist_timing_report()
}

pub(crate) fn finish_timing_run(result: &Result<()>) -> Result<()> {
    TIMINGS.with(|slot| {
        if let Some((_, report)) = slot.borrow_mut().as_mut() {
            report.ended_at_utc = Some(Utc::now().to_rfc3339());
            report.result = if result.is_ok() { "success" } else { "failed" }.to_string();
        }
    });
    persist_timing_report()?;
    print_timing_summary()?;
    TIMINGS.with(|slot| *slot.borrow_mut() = None);
    Ok(())
}

pub(crate) fn record_timing(record: TimingRecord) -> Result<()> {
    TIMINGS.with(|slot| {
        if let Some((_, report)) = slot.borrow_mut().as_mut() {
            report.stages.push(record);
        }
    });
    persist_timing_report()
}

pub(crate) fn timed<T, F>(
    stage: &str,
    cache_status: &str,
    reason: &str,
    input_digest: &str,
    action: F,
) -> Result<T>
where
    F: FnOnce() -> Result<T>,
{
    let started_at: DateTime<Utc> = Utc::now();
    let timer = Instant::now();
    let result = action();
    let ended_at = Utc::now();
    record_timing(TimingRecord {
        stage: stage.to_string(),
        started_at_utc: started_at.to_rfc3339(),
        ended_at_utc: ended_at.to_rfc3339(),
        wall_seconds: timer.elapsed().as_secs_f64(),
        result: if result.is_ok() { "success" } else { "failed" }.to_string(),
        cache_status: cache_status.to_string(),
        reason: reason.to_string(),
        input_digest: input_digest.to_string(),
        output_digest: None,
    })?;
    result
}

fn with_stage_log<T, F>(repo_root: &Path, stage: &str, action: F) -> Result<T>
where
    F: FnOnce() -> Result<T>,
{
    let log = repo_root
        .join("out/logs")
        .join(format!("{}.log", sanitize_identifier(stage)));
    if let Some(parent) = log.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&log, format!("MattOS build log: {stage}\n"))?;
    ACTIVE_BUILD_LOG.with(|slot| *slot.borrow_mut() = Some(log.clone()));
    println!("[build] {stage}: running (full log: {})", log.display());
    let result = action();
    ACTIVE_BUILD_LOG.with(|slot| *slot.borrow_mut() = None);
    if let Err(error) = &result {
        eprintln!("[build] {stage}: failed; full log: {}", log.display());
        if let Ok(tail) = log_tail(&log, 40) {
            eprintln!("--- last build output ---\n{tail}--- end build output ---");
        }
        eprintln!("{error:#}");
    }
    result
}

pub(crate) fn run_logged_command(command: &mut Command, display: &str) -> Result<ExitStatus> {
    let verbose = std::env::var_os("MATTOS_VERBOSE_BUILD_OUTPUT").is_some();
    run_logged_command_mode(command, display, verbose)
}

fn run_logged_command_mode(
    command: &mut Command,
    display: &str,
    verbose: bool,
) -> Result<ExitStatus> {
    // Build presentation may inherit from any launcher, but output-producing
    // subprocesses always execute under the same reproducible locale/time
    // policy. Apply this here, immediately before every logged spawn, so a
    // caller cannot accidentally diverge from the environment represented in
    // the stage cache key.
    command
        .env("LC_ALL", "C")
        .env("LANG", "C")
        .env("TZ", "UTC")
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .env("SOURCE_DATE_EPOCH", NORMALIZED_SOURCE_DATE_EPOCH);
    let active = ACTIVE_BUILD_LOG.with(|slot| slot.borrow().clone());
    let Some(log_path) = active else {
        return command
            .status()
            .with_context(|| format!("failed to spawn {display}"));
    };
    let mut log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?;
    writeln!(log, "\n$ {display}")?;
    if verbose {
        command.stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut child = command
            .spawn()
            .with_context(|| format!("failed to spawn {display}"))?;
        let mut child_stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("failed to capture stdout for {display}"))?;
        let mut child_stderr = child
            .stderr
            .take()
            .ok_or_else(|| anyhow!("failed to capture stderr for {display}"))?;
        let mut stdout_log = log.try_clone()?;
        let mut stderr_log = log.try_clone()?;
        let stdout_thread = thread::spawn(move || -> std::io::Result<()> {
            let mut console = std::io::stdout().lock();
            let mut buffer = [0u8; 16 * 1024];
            loop {
                let count = child_stdout.read(&mut buffer)?;
                if count == 0 {
                    break;
                }
                console.write_all(&buffer[..count])?;
                console.flush()?;
                stdout_log.write_all(&buffer[..count])?;
            }
            Ok(())
        });
        let stderr_thread = thread::spawn(move || -> std::io::Result<()> {
            let mut console = std::io::stderr().lock();
            let mut buffer = [0u8; 16 * 1024];
            loop {
                let count = child_stderr.read(&mut buffer)?;
                if count == 0 {
                    break;
                }
                console.write_all(&buffer[..count])?;
                console.flush()?;
                stderr_log.write_all(&buffer[..count])?;
            }
            Ok(())
        });
        let status = child
            .wait()
            .with_context(|| format!("failed to wait for {display}"))?;
        stdout_thread
            .join()
            .map_err(|_| anyhow!("stdout forwarding thread panicked for {display}"))??;
        stderr_thread
            .join()
            .map_err(|_| anyhow!("stderr forwarding thread panicked for {display}"))??;
        return Ok(status);
    }
    let stdout = log.try_clone()?;
    let stderr = log.try_clone()?;
    command
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));
    command
        .status()
        .with_context(|| format!("failed to spawn {display}"))
}

fn log_tail(path: &Path, lines: usize) -> Result<String> {
    let body = fs::read_to_string(path)?;
    let values = body.lines().collect::<Vec<_>>();
    let start = values.len().saturating_sub(lines);
    Ok(values[start..].join("\n") + "\n")
}

/// Atomically publishes a completely-built path while retaining and restoring
/// the previous known-good output if publication itself fails.
pub(crate) fn atomic_replace_path(temp: &Path, destination: &Path) -> Result<()> {
    if !temp.symlink_metadata().is_ok() {
        bail!("temporary output is missing: {}", temp.display());
    }
    let parent = destination
        .parent()
        .ok_or_else(|| anyhow!("path has no parent: {}", destination.display()))?;
    fs::create_dir_all(parent)?;
    let backup = parent.join(format!(
        ".{}.previous-{}",
        destination
            .file_name()
            .and_then(OsStr::to_str)
            .unwrap_or("output"),
        std::process::id()
    ));
    remove_path_if_exists(&backup)?;
    let had_previous = destination.symlink_metadata().is_ok();
    if had_previous {
        fs::rename(destination, &backup).with_context(|| {
            format!("failed to retain previous output {}", destination.display())
        })?;
    }
    if let Err(error) = fs::rename(temp, destination) {
        if had_previous {
            let _ = fs::rename(&backup, destination);
        }
        return Err(error).with_context(|| {
            format!(
                "failed to publish validated output {}",
                destination.display()
            )
        });
    }
    remove_path_if_exists(&backup)
}

pub(crate) fn temporary_sibling(destination: &Path, label: &str) -> Result<PathBuf> {
    let parent = destination
        .parent()
        .ok_or_else(|| anyhow!("path has no parent: {}", destination.display()))?;
    let temp = parent.join(format!(
        ".{}.{}-{}",
        destination
            .file_name()
            .and_then(OsStr::to_str)
            .unwrap_or("output"),
        sanitize_identifier(label),
        std::process::id()
    ));
    remove_path_if_exists(&temp)?;
    Ok(temp)
}

fn remove_path_if_exists(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {
            fs::remove_dir_all(path)?
        }
        Ok(_) => fs::remove_file(path)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

fn persist_timing_report() -> Result<()> {
    TIMINGS.with(|slot| -> Result<()> {
        let borrowed = slot.borrow();
        let Some((repo_root, report)) = borrowed.as_ref() else {
            return Ok(());
        };
        let reports = repo_root.join("out/reports");
        fs::create_dir_all(&reports)?;
        atomic_write_json(&reports.join("build-timings.json"), report)?;
        let mut records = report.stages.clone();
        records.sort_by(|a, b| {
            b.wall_seconds
                .partial_cmp(&a.wall_seconds)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.stage.cmp(&b.stage))
        });
        let mut text = format!(
            "MattOS build timings\ncommand: {}\nstarted: {}\nended: {}\nresult: {}\n\n",
            report.command,
            report.started_at_utc,
            report.ended_at_utc.as_deref().unwrap_or("running"),
            report.result
        );
        text.push_str("seconds  cache  result   stage  reason\n");
        for record in records {
            text.push_str(&format!(
                "{:>7.3}  {:<5}  {:<7}  {}  {}\n",
                record.wall_seconds,
                record.cache_status,
                record.result,
                record.stage,
                record.reason.replace('\n', " ")
            ));
        }
        atomic_write(&reports.join("build-timings.txt"), text.as_bytes())
    })
}

pub(crate) fn show_latest_timings(repo_root: &Path) -> Result<()> {
    let path = repo_root.join("out/reports/build-timings.txt");
    let body = fs::read_to_string(&path)
        .with_context(|| format!("no timing report found at {}", path.display()))?;
    print!("{body}");
    Ok(())
}

fn print_timing_summary() -> Result<()> {
    TIMINGS.with(|slot| -> Result<()> {
        let borrowed = slot.borrow();
        let Some((_, report)) = borrowed.as_ref() else {
            return Ok(());
        };
        let mut records = report.stages.clone();
        records.sort_by(|a, b| {
            b.wall_seconds
                .partial_cmp(&a.wall_seconds)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        println!("\nMattOS stage timing summary (slowest first):");
        for record in &records {
            println!(
                "  {:>8.3}s  {:<5}  {:<7}  {}",
                record.wall_seconds, record.cache_status, record.result, record.stage
            );
        }
        Ok(())
    })
}

pub(crate) fn execute_cached_stage<F, V>(
    repo_root: &Path,
    spec: &StageSpec,
    validate_reuse: V,
    action: F,
) -> Result<()>
where
    F: FnOnce() -> Result<()>,
    V: Fn() -> Result<()>,
{
    let started_at = Utc::now();
    let timer = Instant::now();
    let evaluation = compute_stage_evaluation(repo_root, spec)?;
    let inputs = evaluation.inputs;
    let manifest_path = stage_manifest_path(repo_root, &spec.id);
    let mut reason;
    let mut reused_digest = None;

    if let Ok(manifest) = read_stage_manifest(repo_root, &spec.id) {
        reason = cache_miss_reason(repo_root, spec, &inputs, &manifest)?;
        if reason.is_empty() {
            match validate_reuse() {
                Ok(()) => {
                    reason = "full input digest matched; output inventory and lightweight validation passed"
                        .to_string();
                    reused_digest = Some(manifest.output_content_digest);
                }
                Err(error) => reason = format!("lightweight reuse validation failed: {error:#}"),
            }
        }
    } else {
        reason = format!("no valid stage manifest at {}", manifest_path.display());
    }

    if let Some(output_digest) = reused_digest {
        record_timing(TimingRecord {
            stage: spec.id.clone(),
            started_at_utc: started_at.to_rfc3339(),
            ended_at_utc: Utc::now().to_rfc3339(),
            wall_seconds: timer.elapsed().as_secs_f64(),
            result: "success".to_string(),
            cache_status: "hit".to_string(),
            reason: reason.clone(),
            input_digest: inputs.full_digest,
            output_digest: Some(output_digest),
        })?;
        println!("cache hit: {} ({reason})", spec.id);
        return Ok(());
    }

    println!("cache miss: {} ({reason})", spec.id);
    let result = with_stage_log(repo_root, &spec.id, action);
    let mut output_digest = None;
    if result.is_ok() {
        let inventory = output_inventory(repo_root, &spec.outputs)?;
        if inventory.is_empty() {
            bail!("stage {} succeeded without expected outputs", spec.id);
        }
        let digest = inventory_digest(&inventory)?;
        let manifest = StageManifest {
            schema_version: STAGE_MANIFEST_SCHEMA_VERSION,
            stage: spec.id.clone(),
            inputs: inputs.clone(),
            input_details: evaluation.details,
            expected_outputs: inventory,
            output_content_digest: digest.clone(),
        };
        write_stage_manifest(repo_root, &manifest)?;
        output_digest = Some(digest);
    }
    record_timing(TimingRecord {
        stage: spec.id.clone(),
        started_at_utc: started_at.to_rfc3339(),
        ended_at_utc: Utc::now().to_rfc3339(),
        wall_seconds: timer.elapsed().as_secs_f64(),
        result: if result.is_ok() { "success" } else { "failed" }.to_string(),
        cache_status: "miss".to_string(),
        reason,
        input_digest: inputs.full_digest,
        output_digest,
    })?;
    if result.is_ok() {
        println!(
            "[build] {}: complete in {:.3}s (full log: {})",
            spec.id,
            timer.elapsed().as_secs_f64(),
            repo_root
                .join("out/logs")
                .join(format!("{}.log", sanitize_identifier(&spec.id)))
                .display()
        );
    }
    result
}

pub(crate) fn record_virtual_stage(repo_root: &Path, spec: &StageSpec) -> Result<()> {
    execute_cached_stage(repo_root, spec, || Ok(()), || Ok(()))
}

fn cache_miss_reason(
    repo_root: &Path,
    spec: &StageSpec,
    current: &StageInputs,
    manifest: &StageManifest,
) -> Result<String> {
    if manifest.schema_version != STAGE_MANIFEST_SCHEMA_VERSION {
        return Ok(format!(
            "manifest schema changed from {} to {}",
            manifest.schema_version, STAGE_MANIFEST_SCHEMA_VERSION
        ));
    }
    if manifest.stage != spec.id {
        return Ok(format!(
            "manifest stage is {}, expected {}",
            manifest.stage, spec.id
        ));
    }
    if manifest.inputs.full_digest != current.full_digest {
        let mut changed = Vec::new();
        if manifest.inputs.source_digest != current.source_digest {
            changed.push("source");
        }
        if manifest.inputs.configuration_digest != current.configuration_digest {
            changed.push("configuration");
        }
        if manifest.inputs.tool_digest != current.tool_digest {
            changed.push("tool/version");
        }
        if manifest.inputs.environment_digest != current.environment_digest {
            changed.push("environment");
        }
        if manifest.inputs.dependency_digests != current.dependency_digests {
            changed.push("dependency output");
        }
        return Ok(format!("input digest changed ({})", changed.join(", ")));
    }
    let current_inventory = match output_inventory(repo_root, &spec.outputs) {
        Ok(inventory) if !inventory.is_empty() => inventory,
        Ok(_) => return Ok("expected output inventory is empty".to_string()),
        Err(error) => {
            return Ok(format!(
                "expected output is missing or unreadable: {error:#}"
            ));
        }
    };
    if current_inventory != manifest.expected_outputs {
        return Ok("output inventory/content/mode/symlink target changed".to_string());
    }
    let digest = inventory_digest(&current_inventory)?;
    if digest != manifest.output_content_digest {
        return Ok("output content digest mismatch".to_string());
    }
    Ok(String::new())
}

struct StageEvaluation {
    inputs: StageInputs,
    details: StageInputDetails,
}

pub(crate) fn compute_stage_inputs(repo_root: &Path, spec: &StageSpec) -> Result<StageInputs> {
    Ok(compute_stage_evaluation(repo_root, spec)?.inputs)
}

fn compute_stage_evaluation(repo_root: &Path, spec: &StageSpec) -> Result<StageEvaluation> {
    let source_digest = digest_source_inputs(repo_root, &spec.source_inputs)?;
    let mut source = BTreeMap::new();
    for path in &spec.source_inputs {
        source.insert(
            diagnostic_path(repo_root, path),
            digest_source_inputs(repo_root, std::slice::from_ref(path))?,
        );
    }
    let mut configuration = spec.configuration_inputs.clone();
    configuration.sort();
    let configuration_digest = digest_paths(repo_root, &configuration, false, &spec.recipe)?;
    let mut configuration_details = BTreeMap::new();
    for path in &configuration {
        configuration_details.insert(
            diagnostic_path(repo_root, path),
            digest_paths(
                repo_root,
                std::slice::from_ref(path),
                false,
                "configuration-input",
            )?,
        );
    }
    let tools = tool_identities(&spec.tools)?;
    let tool_digest = digest_serializable(&tools)?;
    let environment = normalized_build_environment();
    let environment_digest = digest_serializable(&environment)?;
    let mut dependency_digests = BTreeMap::new();
    let mut dependencies = BTreeMap::new();
    for dependency in &spec.dependencies {
        let identity = match read_stage_manifest(repo_root, dependency) {
            Ok(manifest) => DependencyIdentity {
                input_digest: manifest.inputs.full_digest,
                output_digest: manifest.output_content_digest,
            },
            Err(_) => DependencyIdentity {
                input_digest: "<missing>".to_string(),
                output_digest: "<missing>".to_string(),
            },
        };
        // Consumers depend on the bytes exposed by the dependency. A rebuild
        // that republishes byte-identical output must not create a false
        // cascade merely because the dependency's own input identity changed.
        dependency_digests.insert(dependency.clone(), identity.output_digest.clone());
        dependencies.insert(dependency.clone(), identity);
    }
    let full_digest = digest_serializable(&(
        STAGE_MANIFEST_SCHEMA_VERSION,
        &spec.id,
        &source_digest,
        &configuration_digest,
        &tool_digest,
        &environment_digest,
        &dependency_digests,
    ))?;
    Ok(StageEvaluation {
        inputs: StageInputs {
            source_digest,
            configuration_digest,
            tool_digest,
            environment_digest,
            dependency_digests,
            full_digest,
        },
        details: StageInputDetails {
            schema_version: STAGE_MANIFEST_SCHEMA_VERSION,
            recipe: spec.recipe.clone(),
            source,
            configuration: configuration_details,
            environment,
            tools,
            dependencies,
        },
    })
}

fn diagnostic_path(repo_root: &Path, path: &Path) -> String {
    normalize_path(path.strip_prefix(repo_root).unwrap_or(path))
}

pub(crate) fn stage_manifest_path(repo_root: &Path, stage: &str) -> PathBuf {
    repo_root
        .join("out/state/stages")
        .join(format!("{}.json", sanitize_identifier(stage)))
}

pub(crate) fn read_stage_manifest(repo_root: &Path, stage: &str) -> Result<StageManifest> {
    let path = stage_manifest_path(repo_root, stage);
    let body = fs::read(&path).with_context(|| format!("unable to read {}", path.display()))?;
    serde_json::from_slice(&body).with_context(|| format!("invalid manifest {}", path.display()))
}

pub(crate) fn write_stage_manifest(repo_root: &Path, manifest: &StageManifest) -> Result<()> {
    let path = stage_manifest_path(repo_root, &manifest.stage);
    atomic_write_json(&path, manifest)
}

pub(crate) fn output_path_digest(repo_root: &Path, path: &Path) -> Result<String> {
    let inventory = output_inventory(repo_root, &[path.to_path_buf()])?;
    inventory_digest(&inventory)
}

pub(crate) fn digest_value<T: Serialize>(value: &T) -> Result<String> {
    digest_serializable(value)
}

pub(crate) fn atomic_write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let mut body = serde_json::to_vec_pretty(value)?;
    body.push(b'\n');
    atomic_write(path, &body)
}

pub(crate) fn atomic_write(path: &Path, body: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("path has no parent: {}", path.display()))?;
    fs::create_dir_all(parent)?;
    let temp = parent.join(format!(
        ".{}.tmp-{}",
        path.file_name()
            .and_then(OsStr::to_str)
            .unwrap_or("manifest"),
        std::process::id()
    ));
    fs::write(&temp, body).with_context(|| format!("failed to write {}", temp.display()))?;
    fs::rename(&temp, path)
        .with_context(|| format!("failed to atomically publish {}", path.display()))
}

pub(crate) fn invalidate_manifest(repo_root: &Path, stage: &str) -> Result<bool> {
    let path = stage_manifest_path(repo_root, stage);
    if path.exists() {
        fs::remove_file(&path)?;
        return Ok(true);
    }
    Ok(false)
}

pub(crate) fn explain_stage(repo_root: &Path, spec: &StageSpec) -> Result<String> {
    let inputs = compute_stage_inputs(repo_root, spec)?;
    let manifest = match read_stage_manifest(repo_root, &spec.id) {
        Ok(manifest) => manifest,
        Err(error) => return Ok(format!("{}: rebuild: {error:#}", spec.id)),
    };
    let reason = cache_miss_reason(repo_root, spec, &inputs, &manifest)?;
    if reason.is_empty() {
        Ok(format!(
            "{}: reusable; input={} output={}",
            spec.id, inputs.full_digest, manifest.output_content_digest
        ))
    } else {
        Ok(format!("{}: rebuild: {reason}", spec.id))
    }
}

pub(crate) fn explain_stage_details(repo_root: &Path, spec: &StageSpec) -> Result<String> {
    let evaluation = compute_stage_evaluation(repo_root, spec)?;
    let manifest = match read_stage_manifest(repo_root, &spec.id) {
        Ok(manifest) => manifest,
        Err(error) => return Ok(format!("{}: rebuild: {error:#}", spec.id)),
    };
    let reason = cache_miss_reason(repo_root, spec, &evaluation.inputs, &manifest)?;
    let mut output = if reason.is_empty() {
        format!("{}: reusable\n", spec.id)
    } else {
        format!("{}: rebuild: {reason}\n", spec.id)
    };
    push_value_diff(
        &mut output,
        "schema",
        &manifest.schema_version.to_string(),
        &STAGE_MANIFEST_SCHEMA_VERSION.to_string(),
    );
    let stored_dependency_digest = digest_serializable(&manifest.inputs.dependency_digests)?;
    let current_dependency_digest = digest_serializable(&evaluation.inputs.dependency_digests)?;
    for (name, stored, current) in [
        (
            "source.digest",
            &manifest.inputs.source_digest,
            &evaluation.inputs.source_digest,
        ),
        (
            "configuration.digest",
            &manifest.inputs.configuration_digest,
            &evaluation.inputs.configuration_digest,
        ),
        (
            "environment.digest",
            &manifest.inputs.environment_digest,
            &evaluation.inputs.environment_digest,
        ),
        (
            "tools.digest",
            &manifest.inputs.tool_digest,
            &evaluation.inputs.tool_digest,
        ),
        (
            "dependencies.digest",
            &stored_dependency_digest,
            &current_dependency_digest,
        ),
        (
            "full.digest",
            &manifest.inputs.full_digest,
            &evaluation.inputs.full_digest,
        ),
    ] {
        push_value_diff(&mut output, name, stored, current);
    }
    if manifest.input_details.schema_version == 0 {
        output.push_str(
            "stored field details: unavailable in the pre-schema-3 manifest; one-time migration required\n",
        );
        return Ok(output);
    }
    push_value_diff(
        &mut output,
        "configuration.recipe",
        &manifest.input_details.recipe,
        &evaluation.details.recipe,
    );
    push_map_diff(
        &mut output,
        "source",
        &manifest.input_details.source,
        &evaluation.details.source,
    )?;
    push_map_diff(
        &mut output,
        "configuration",
        &manifest.input_details.configuration,
        &evaluation.details.configuration,
    )?;
    push_map_diff(
        &mut output,
        "environment",
        &manifest.input_details.environment,
        &evaluation.details.environment,
    )?;
    push_map_diff(
        &mut output,
        "tools",
        &manifest.input_details.tools,
        &evaluation.details.tools,
    )?;
    push_map_diff(
        &mut output,
        "dependencies",
        &manifest.input_details.dependencies,
        &evaluation.details.dependencies,
    )?;
    output.push_str("ordering-only differences: none (maps use canonical key ordering)\n");
    Ok(output)
}

fn push_value_diff(output: &mut String, field: &str, stored: &str, current: &str) {
    if stored == current {
        output.push_str(&format!("{field}: unchanged ({current})\n"));
    } else {
        output.push_str(&format!(
            "{field}:\n  stored: {stored}\n  current: {current}\n"
        ));
    }
}

fn push_map_diff<T>(
    output: &mut String,
    group: &str,
    stored: &BTreeMap<String, T>,
    current: &BTreeMap<String, T>,
) -> Result<()>
where
    T: Serialize + PartialEq,
{
    let added = current
        .keys()
        .filter(|key| !stored.contains_key(*key))
        .cloned()
        .collect::<Vec<_>>();
    let removed = stored
        .keys()
        .filter(|key| !current.contains_key(*key))
        .cloned()
        .collect::<Vec<_>>();
    if !added.is_empty() {
        output.push_str(&format!("{group}.added keys: {}\n", added.join(", ")));
    }
    if !removed.is_empty() {
        output.push_str(&format!("{group}.removed keys: {}\n", removed.join(", ")));
    }
    for (key, stored_value) in stored {
        let Some(current_value) = current.get(key) else {
            continue;
        };
        if stored_value != current_value {
            output.push_str(&format!(
                "{group}.{key}:\n  stored: {}\n  current: {}\n",
                serde_json::to_string(stored_value)?,
                serde_json::to_string(current_value)?
            ));
        }
    }
    if added.is_empty()
        && removed.is_empty()
        && stored
            .iter()
            .all(|(key, value)| current.get(key) == Some(value))
    {
        output.push_str(&format!("{group}: unchanged\n"));
    }
    Ok(())
}

fn digest_source_inputs(repo_root: &Path, roots: &[PathBuf]) -> Result<String> {
    tracked_source_digest(repo_root, roots, true)
}

pub(crate) fn tracked_source_digest(
    repo_root: &Path,
    roots: &[PathBuf],
    exclude_documentation: bool,
) -> Result<String> {
    if roots.is_empty() {
        return digest_serializable(&Vec::<String>::new());
    }
    let relative_roots = roots
        .iter()
        .map(|path| path.strip_prefix(repo_root).unwrap_or(path).to_path_buf())
        .collect::<Vec<_>>();
    let git_output = |arguments: &[&str]| -> Result<Vec<u8>> {
        let output = Command::new("git")
            .args(arguments)
            .args(&relative_roots)
            .current_dir(repo_root)
            .output()?;
        if !output.status.success() {
            bail!("git input inventory failed with {}", output.status)
        }
        Ok(output.stdout)
    };
    if let (Ok(index), Ok(modified), Ok(untracked)) = (
        git_output(&["ls-files", "--stage", "-z", "--"]),
        git_output(&["diff", "--name-only", "-z", "--"]),
        git_output(&["ls-files", "--others", "--exclude-standard", "-z", "--"]),
    ) {
        let modified = modified
            .split(|byte| *byte == 0)
            .filter(|bytes| !bytes.is_empty())
            .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
            .collect::<BTreeSet<_>>();
        let mut values = BTreeMap::new();
        for entry in index
            .split(|byte| *byte == 0)
            .filter(|bytes| !bytes.is_empty())
        {
            let Some(tab) = entry.iter().position(|byte| *byte == b'\t') else {
                continue;
            };
            let header = String::from_utf8_lossy(&entry[..tab]);
            let path = String::from_utf8_lossy(&entry[tab + 1..]).into_owned();
            let path_buf = PathBuf::from(&path);
            if exclude_documentation && is_irrelevant_documentation(&path_buf) {
                continue;
            }
            if modified.contains(&path) {
                let absolute = repo_root.join(&path_buf);
                if absolute.symlink_metadata().is_ok() {
                    let mut inventory = Vec::new();
                    collect_inventory(repo_root, &absolute, false, &mut inventory)?;
                    values.insert(
                        path,
                        format!("working:{}", digest_serializable(&inventory)?),
                    );
                } else {
                    values.insert(path, "working:<deleted>".to_string());
                }
            } else {
                values.insert(path, format!("index:{header}"));
            }
        }
        for bytes in untracked
            .split(|byte| *byte == 0)
            .filter(|bytes| !bytes.is_empty())
        {
            let path = PathBuf::from(String::from_utf8_lossy(bytes).into_owned());
            if exclude_documentation && is_irrelevant_documentation(&path) {
                continue;
            }
            let mut inventory = Vec::new();
            collect_inventory(repo_root, &repo_root.join(&path), false, &mut inventory)?;
            values.insert(
                normalize_path(&path),
                format!("untracked:{}", digest_serializable(&inventory)?),
            );
        }
        return digest_serializable(&("git-index-and-working-tree", values));
    }
    digest_paths(
        repo_root,
        roots,
        exclude_documentation,
        "filesystem-source-inputs",
    )
}

fn digest_paths(
    repo_root: &Path,
    paths: &[PathBuf],
    filter_docs: bool,
    seed: &str,
) -> Result<String> {
    let mut entries = Vec::new();
    for path in paths {
        let absolute = if path.is_absolute() {
            path.clone()
        } else {
            repo_root.join(path)
        };
        collect_inventory(repo_root, &absolute, filter_docs, &mut entries)?;
    }
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    digest_serializable(&(seed, entries))
}

fn output_inventory(repo_root: &Path, paths: &[PathBuf]) -> Result<Vec<InventoryEntry>> {
    let mut entries = Vec::new();
    for path in paths {
        let absolute = if path.is_absolute() {
            path.clone()
        } else {
            repo_root.join(path)
        };
        if !absolute.symlink_metadata().is_ok() {
            bail!("missing expected output {}", absolute.display());
        }
        collect_inventory(repo_root, &absolute, false, &mut entries)?;
    }
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    let mut seen = BTreeSet::new();
    entries.retain(|entry| seen.insert(entry.path.clone()));
    Ok(entries)
}

fn collect_inventory(
    repo_root: &Path,
    path: &Path,
    filter_docs: bool,
    entries: &mut Vec<InventoryEntry>,
) -> Result<()> {
    let relative = path.strip_prefix(repo_root).unwrap_or(path);
    if filter_docs && is_irrelevant_documentation(relative) {
        return Ok(());
    }
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("failed to inspect {}", path.display()))?;
    #[cfg(unix)]
    let (mode, uid, gid) = {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        (
            metadata.permissions().mode() & 0o7777,
            metadata.uid(),
            metadata.gid(),
        )
    };
    #[cfg(not(unix))]
    let (mode, uid, gid) = {
        let mode = if metadata.permissions().readonly() {
            0o444
        } else {
            0o666
        };
        (mode, 0, 0)
    };
    let normalized = normalize_path(relative);
    if metadata.file_type().is_symlink() {
        entries.push(InventoryEntry {
            path: normalized,
            kind: "symlink".to_string(),
            mode,
            uid,
            gid,
            size: 0,
            content: normalize_path(&fs::read_link(path)?),
        });
    } else if metadata.is_file() {
        entries.push(InventoryEntry {
            path: normalized,
            kind: "file".to_string(),
            mode,
            uid,
            gid,
            size: metadata.len(),
            content: sha256_file(path)?,
        });
    } else if metadata.is_dir() {
        entries.push(InventoryEntry {
            path: normalized,
            kind: "directory".to_string(),
            mode,
            uid,
            gid,
            size: 0,
            content: String::new(),
        });
        let mut children = fs::read_dir(path)?.collect::<std::io::Result<Vec<_>>>()?;
        children.sort_by_key(|entry| entry.file_name());
        for child in children {
            collect_inventory(repo_root, &child.path(), filter_docs, entries)?;
        }
    }
    Ok(())
}

fn is_irrelevant_documentation(path: &Path) -> bool {
    for component in path.components() {
        let value = component.as_os_str().to_string_lossy().to_ascii_lowercase();
        if matches!(value.as_str(), "doc" | "docs" | "documentation") {
            return true;
        }
    }
    let name = path
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or("")
        .to_ascii_lowercase();
    name.starts_with("readme")
        || name.starts_with("changelog")
        || name == "news"
        || name.starts_with("copying")
}

fn tool_identities(tools: &[String]) -> Result<BTreeMap<String, ToolIdentity>> {
    let mut values = BTreeMap::new();
    for tool in tools {
        let path = resolve_executable(tool)?;
        let version_output = stable_tool_output(&path, &["--version"])?;
        let target = if matches!(tool.as_str(), "gcc" | "g++" | "cc" | "c++") {
            stable_tool_output(&path, &["-dumpmachine"])?
        } else if tool == "rustc" {
            stable_tool_output(&path, &["-vV"])?
                .lines()
                .find_map(|line| line.strip_prefix("host: "))
                .unwrap_or("")
                .to_string()
        } else {
            String::new()
        };
        let identity = ToolIdentity {
            resolved_path: normalize_path(&path),
            executable_sha256: sha256_file(&path)?,
            version: version_output.lines().next().unwrap_or("").to_string(),
            target,
        };
        values.insert(tool.clone(), identity);
    }
    Ok(values)
}

fn resolve_executable(tool: &str) -> Result<PathBuf> {
    let path = std::env::var_os("PATH");
    resolve_executable_from(tool, path.as_deref())
}

fn resolve_executable_from(tool: &str, path: Option<&OsStr>) -> Result<PathBuf> {
    let supplied = Path::new(tool);
    if supplied.components().count() > 1 {
        return supplied
            .canonicalize()
            .with_context(|| format!("unable to resolve tool {tool}"));
    }
    for directory in std::env::split_paths(path.unwrap_or_default()) {
        let candidate = directory.join(tool);
        if candidate.is_file() {
            return candidate
                .canonicalize()
                .with_context(|| format!("unable to resolve tool {tool}"));
        }
    }
    bail!("tool {tool} was not found on PATH")
}

fn stable_tool_output(tool: &Path, arguments: &[&str]) -> Result<String> {
    let output = Command::new(tool)
        .args(arguments)
        .env("LC_ALL", "C")
        .env("LANG", "C")
        .env("TZ", "UTC")
        .output()
        .with_context(|| format!("failed to inspect tool {}", tool.display()))?;
    if !output.status.success() {
        bail!(
            "tool identity probe failed with {}: {} {}",
            output.status,
            tool.display(),
            arguments.join(" ")
        );
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let selected = if stdout.trim().is_empty() {
        stderr.trim()
    } else {
        stdout.trim()
    };
    Ok(selected.replace('\r', ""))
}

fn normalized_build_environment() -> BTreeMap<String, String> {
    normalized_build_environment_from(|name| std::env::var(name).ok())
}

fn normalized_build_environment_from(
    mut lookup: impl FnMut(&str) -> Option<String>,
) -> BTreeMap<String, String> {
    let mut values = BTreeMap::new();
    // Only caller-controlled values that can alter generated bytes or the
    // location from which an expected output is read belong here. Raw PATH is
    // intentionally excluded: selected tools are represented above by their
    // canonical executable, file digest, stable version, and target.
    for name in [
        "CC",
        "CXX",
        "AR",
        "AS",
        "LD",
        "NM",
        "RANLIB",
        "STRIP",
        "OBJCOPY",
        "PKG_CONFIG",
        "CFLAGS",
        "CXXFLAGS",
        "CPPFLAGS",
        "LDFLAGS",
        "RUSTFLAGS",
        "CARGO_TARGET_DIR",
        "PKG_CONFIG_PATH",
        "LIBRARY_PATH",
    ] {
        values.insert(name.to_string(), lookup(name).unwrap_or_default());
    }
    // These are enforced for build subprocesses. Host/user locale and timezone
    // presentation therefore cannot perturb a cache key.
    values.insert("LC_ALL".to_string(), "C".to_string());
    values.insert("TZ".to_string(), "UTC".to_string());
    values.insert("PYTHONDONTWRITEBYTECODE".to_string(), "1".to_string());
    values.insert(
        "SOURCE_DATE_EPOCH".to_string(),
        NORMALIZED_SOURCE_DATE_EPOCH.to_string(),
    );
    values
}

fn inventory_digest(inventory: &[InventoryEntry]) -> Result<String> {
    digest_serializable(inventory)
}

fn digest_serializable<T: Serialize + ?Sized>(value: &T) -> Result<String> {
    let body = serde_json::to_vec(value)?;
    Ok(format!("{:x}", Sha256::digest(body)))
}

pub(crate) fn sha256_file(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 1024 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn normalize_path(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

fn sanitize_identifier(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character
            } else {
                '-'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn stable_digest_ignores_timestamps_but_detects_content_and_modes() {
        let root = tempdir().unwrap();
        let file = root.path().join("source.c");
        fs::write(&file, "int value = 1;\n").unwrap();
        let first = digest_paths(root.path(), &[file.clone()], false, "test").unwrap();
        let now = filetime::FileTime::from_unix_time(1_700_000_000, 0);
        filetime::set_file_mtime(&file, now).unwrap();
        let timestamp_only = digest_paths(root.path(), &[file.clone()], false, "test").unwrap();
        assert_eq!(first, timestamp_only);
        fs::write(&file, "int value = 2;\n").unwrap();
        assert_ne!(
            first,
            digest_paths(root.path(), &[file], false, "test").unwrap()
        );
    }

    #[test]
    fn documentation_is_not_a_compilation_input() {
        let root = tempdir().unwrap();
        fs::create_dir(root.path().join("docs")).unwrap();
        fs::write(root.path().join("source.c"), "one").unwrap();
        fs::write(root.path().join("docs/README.md"), "first").unwrap();
        let first = digest_paths(root.path(), &[root.path().to_path_buf()], true, "test").unwrap();
        fs::write(root.path().join("docs/README.md"), "second").unwrap();
        let second = digest_paths(root.path(), &[root.path().to_path_buf()], true, "test").unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn inventory_detects_missing_corrupt_mode_and_symlink_outputs() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let root = tempdir().unwrap();
        let output = root.path().join("output");
        fs::create_dir(&output).unwrap();
        fs::write(output.join("file"), "good").unwrap();
        symlink("file", output.join("link")).unwrap();
        let first = output_inventory(root.path(), &[output.clone()]).unwrap();
        fs::write(output.join("file"), "bad!").unwrap();
        assert_ne!(
            first,
            output_inventory(root.path(), &[output.clone()]).unwrap()
        );
        fs::write(output.join("file"), "good").unwrap();
        fs::set_permissions(output.join("file"), fs::Permissions::from_mode(0o755)).unwrap();
        assert_ne!(
            first,
            output_inventory(root.path(), &[output.clone()]).unwrap()
        );
        fs::remove_file(output.join("link")).unwrap();
        symlink("missing", output.join("link")).unwrap();
        assert_ne!(first, output_inventory(root.path(), &[output]).unwrap());
    }

    #[test]
    fn failed_stage_never_publishes_success_manifest() {
        let root = tempdir().unwrap();
        let spec = StageSpec {
            id: "failure".to_string(),
            source_inputs: Vec::new(),
            configuration_inputs: Vec::new(),
            tools: Vec::new(),
            dependencies: Vec::new(),
            outputs: vec![PathBuf::from("out/result")],
            recipe: "test".to_string(),
        };
        let result = execute_cached_stage(root.path(), &spec, || Ok(()), || bail!("expected"));
        assert!(result.is_err());
        assert!(!stage_manifest_path(root.path(), "failure").exists());
    }

    #[test]
    fn atomic_manifest_replaces_old_document() {
        let root = tempdir().unwrap();
        let path = root.path().join("state.json");
        atomic_write(&path, b"old").unwrap();
        atomic_write(&path, b"new").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"new");
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
    }

    #[test]
    fn atomic_output_replacement_preserves_or_replaces_previous_output() {
        let root = tempdir().unwrap();
        let destination = root.path().join("layer");
        fs::create_dir(&destination).unwrap();
        fs::write(destination.join("value"), "old").unwrap();
        let missing = root.path().join("missing");
        assert!(atomic_replace_path(&missing, &destination).is_err());
        assert_eq!(
            fs::read_to_string(destination.join("value")).unwrap(),
            "old"
        );

        let replacement = root.path().join("replacement");
        fs::create_dir(&replacement).unwrap();
        fs::write(replacement.join("value"), "new").unwrap();
        atomic_replace_path(&replacement, &destination).unwrap();
        assert_eq!(
            fs::read_to_string(destination.join("value")).unwrap(),
            "new"
        );
    }

    #[test]
    fn quiet_stage_logging_keeps_complete_subprocess_output() {
        let root = tempdir().unwrap();
        with_stage_log(root.path(), "native-fixture", || {
            let mut command = Command::new("sh");
            command.args(["-c", "printf 'stdout-line\\n'; printf 'stderr-line\\n' >&2"]);
            let status = run_logged_command(&mut command, "fixture command")?;
            if !status.success() {
                bail!("fixture failed");
            }
            Ok(())
        })
        .unwrap();
        let log = fs::read_to_string(root.path().join("out/logs/native-fixture.log")).unwrap();
        assert!(log.contains("fixture command"));
        assert!(log.contains("stdout-line"));
        assert!(log.contains("stderr-line"));
    }

    #[test]
    fn verbose_stage_logging_streams_and_keeps_complete_subprocess_output() {
        let root = tempdir().unwrap();
        with_stage_log(root.path(), "verbose-fixture", || {
            let mut command = Command::new("sh");
            command.args([
                "-c",
                "printf 'verbose-stdout-line\\n'; printf 'verbose-stderr-line\\n' >&2",
            ]);
            let status = run_logged_command_mode(&mut command, "verbose fixture command", true)?;
            if !status.success() {
                bail!("verbose fixture failed");
            }
            Ok(())
        })
        .unwrap();
        let log = fs::read_to_string(root.path().join("out/logs/verbose-fixture.log")).unwrap();
        assert!(log.contains("verbose fixture command"));
        assert!(log.contains("verbose-stdout-line"));
        assert!(log.contains("verbose-stderr-line"));
    }

    #[test]
    fn source_configuration_and_dependency_changes_are_targeted() {
        use std::cell::Cell;
        let root = tempdir().unwrap();
        fs::write(root.path().join("a.source"), "one").unwrap();
        fs::write(root.path().join("b.source"), "one").unwrap();
        fs::write(root.path().join("c.source"), "one").unwrap();
        fs::write(root.path().join("config"), "one").unwrap();
        let make_spec = |id: &str, source: &str, dependency: &[&str]| StageSpec {
            id: id.to_string(),
            source_inputs: vec![source.into()],
            configuration_inputs: vec!["config".into()],
            tools: Vec::new(),
            dependencies: dependency.iter().map(|value| value.to_string()).collect(),
            outputs: vec![format!("out/{id}").into()],
            recipe: id.to_string(),
        };
        let a = make_spec("a", "a.source", &[]);
        let b = make_spec("b", "b.source", &["a"]);
        let c = make_spec("c", "c.source", &[]);
        let runs = Cell::new(0usize);
        let run = |spec: &StageSpec| {
            execute_cached_stage(
                root.path(),
                spec,
                || Ok(()),
                || {
                    runs.set(runs.get() + 1);
                    let output = root.path().join(&spec.outputs[0]);
                    fs::create_dir_all(output.parent().unwrap())?;
                    fs::write(output, format!("run {}", runs.get()))?;
                    Ok(())
                },
            )
        };
        run(&a).unwrap();
        run(&b).unwrap();
        run(&c).unwrap();
        assert_eq!(runs.get(), 3);
        run(&a).unwrap();
        run(&b).unwrap();
        run(&c).unwrap();
        assert_eq!(runs.get(), 3, "unchanged stages should all hit");

        fs::write(root.path().join("a.source"), "two").unwrap();
        run(&a).unwrap();
        run(&b).unwrap();
        run(&c).unwrap();
        assert_eq!(
            runs.get(),
            5,
            "owner and direct dependent rebuild; unrelated stage hits"
        );

        fs::write(root.path().join("config"), "two").unwrap();
        run(&c).unwrap();
        assert_eq!(
            runs.get(),
            6,
            "configure input changes invalidate the stage"
        );
    }

    #[test]
    fn tool_version_changes_stage_input_digest() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempdir().unwrap();
        let tool = root.path().join("tool");
        fs::write(&tool, "#!/bin/sh\necho version-one\n").unwrap();
        fs::set_permissions(&tool, fs::Permissions::from_mode(0o755)).unwrap();
        let spec = StageSpec {
            id: "tool-test".to_string(),
            source_inputs: Vec::new(),
            configuration_inputs: Vec::new(),
            tools: vec![tool.to_string_lossy().into_owned()],
            dependencies: Vec::new(),
            outputs: vec!["out/tool".into()],
            recipe: "tool-test".to_string(),
        };
        let first = compute_stage_inputs(root.path(), &spec).unwrap();
        fs::write(&tool, "#!/bin/sh\necho version-two\n").unwrap();
        let second = compute_stage_inputs(root.path(), &spec).unwrap();
        assert_ne!(first.tool_digest, second.tool_digest);
        assert_ne!(first.full_digest, second.full_digest);
    }

    #[test]
    fn path_noise_does_not_change_resolved_tool_identity() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let root = tempdir().unwrap();
        let bin = root.path().join("bin");
        let alias = root.path().join("alias");
        fs::create_dir(&bin).unwrap();
        fs::create_dir(&alias).unwrap();
        let tool = bin.join("fixture-cc");
        fs::write(&tool, "#!/bin/sh\necho fixture-cc 1.0\n").unwrap();
        fs::set_permissions(&tool, fs::Permissions::from_mode(0o755)).unwrap();
        symlink(&tool, alias.join("fixture-cc")).unwrap();
        let first_path = std::env::join_paths([root.path().join("missing"), bin]).unwrap();
        let second_path = std::env::join_paths([alias, root.path().join("other")]).unwrap();
        assert_eq!(
            resolve_executable_from("fixture-cc", Some(&first_path)).unwrap(),
            resolve_executable_from("fixture-cc", Some(&second_path)).unwrap()
        );
    }

    #[test]
    fn normalized_environment_ignores_presentation_and_launcher_state() {
        let first = BTreeMap::from([
            ("CFLAGS", "-O2"),
            ("PATH", "/first/launcher/path"),
            ("LC_ALL", "C.UTF-8"),
            ("LANG", "en_US.UTF-8"),
            ("TERM", "dumb"),
            ("COLORTERM", ""),
            ("COLUMNS", "80"),
            ("LINES", "24"),
            ("MATTOS_VERBOSE", "0"),
            ("QEMU_AUDIO_DRV", "none"),
            ("SOURCE_DATE_EPOCH", "1"),
        ]);
        let second = BTreeMap::from([
            ("CFLAGS", "-O2"),
            ("PATH", "/second/interactive/path"),
            ("LC_ALL", ""),
            ("LANG", "fr_FR.UTF-8"),
            ("TERM", "xterm-256color"),
            ("COLORTERM", "truecolor"),
            ("COLUMNS", "240"),
            ("LINES", "60"),
            ("MATTOS_VERBOSE", "1"),
            ("QEMU_AUDIO_DRV", "pa"),
            ("SOURCE_DATE_EPOCH", "999"),
        ]);
        let normalized = |values: &BTreeMap<&str, &str>| {
            normalized_build_environment_from(|name| {
                values.get(name).map(|value| (*value).to_string())
            })
        };
        assert_eq!(normalized(&first), normalized(&second));
        assert_eq!(normalized(&first)["LC_ALL"], "C");
        assert_eq!(
            normalized(&first)["SOURCE_DATE_EPOCH"],
            NORMALIZED_SOURCE_DATE_EPOCH
        );

        let relevant_change = BTreeMap::from([("CFLAGS", "-O3")]);
        assert_ne!(normalized(&first), normalized(&relevant_change));
    }

    #[test]
    fn identical_dependency_output_does_not_cascade() {
        let root = tempdir().unwrap();
        fs::write(root.path().join("a.source"), "one").unwrap();
        fs::write(root.path().join("b.source"), "one").unwrap();
        let make_spec = |id: &str, source: &str, dependencies: &[&str]| StageSpec {
            id: id.to_string(),
            source_inputs: vec![source.into()],
            configuration_inputs: Vec::new(),
            tools: Vec::new(),
            dependencies: dependencies.iter().map(|value| value.to_string()).collect(),
            outputs: vec![format!("out/{id}").into()],
            recipe: id.to_string(),
        };
        let upstream = make_spec("a", "a.source", &[]);
        let downstream = make_spec("b", "b.source", &["a"]);
        let publish = |spec: &StageSpec, body: &str| {
            execute_cached_stage(
                root.path(),
                spec,
                || Ok(()),
                || {
                    let output = root.path().join(&spec.outputs[0]);
                    fs::create_dir_all(output.parent().unwrap())?;
                    fs::write(output, body)?;
                    Ok(())
                },
            )
        };
        publish(&upstream, "stable bytes").unwrap();
        publish(&downstream, "downstream bytes").unwrap();
        let before = compute_stage_inputs(root.path(), &downstream).unwrap();

        fs::write(root.path().join("a.source"), "two").unwrap();
        publish(&upstream, "stable bytes").unwrap();
        let after = compute_stage_inputs(root.path(), &downstream).unwrap();
        assert_eq!(before.dependency_digests, after.dependency_digests);
        assert_eq!(before.full_digest, after.full_digest);
        execute_cached_stage(
            root.path(),
            &downstream,
            || Ok(()),
            || bail!("byte-identical upstream publication must remain a downstream hit"),
        )
        .unwrap();
    }

    #[test]
    fn timing_report_distinguishes_miss_and_hit() {
        let root = tempdir().unwrap();
        start_timing_run(root.path(), "test").unwrap();
        let spec = StageSpec {
            id: "timed".to_string(),
            source_inputs: Vec::new(),
            configuration_inputs: Vec::new(),
            tools: Vec::new(),
            dependencies: Vec::new(),
            outputs: vec!["out/timed".into()],
            recipe: "timed".to_string(),
        };
        execute_cached_stage(
            root.path(),
            &spec,
            || Ok(()),
            || {
                fs::create_dir_all(root.path().join("out"))?;
                fs::write(root.path().join("out/timed"), "result")?;
                Ok(())
            },
        )
        .unwrap();
        execute_cached_stage(root.path(), &spec, || Ok(()), || bail!("must hit")).unwrap();
        finish_timing_run(&Ok(())).unwrap();
        let body = fs::read_to_string(root.path().join("out/reports/build-timings.json")).unwrap();
        assert!(body.contains("\"cache_status\": \"miss\""));
        assert!(body.contains("\"cache_status\": \"hit\""));
    }
}
