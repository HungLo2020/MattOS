use anyhow::{anyhow, bail, Context, Result};
use chrono::{DateTime, Utc};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::Arc;
#[cfg(not(test))]
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

use crate::cache_manifest::{InventoryEntry, ToolIdentity};
#[cfg(test)]
use crate::cache_manifest::{StageInputDetails, StageManifest};
pub(crate) use crate::cache_manifest::{StageSpec, STAGE_MANIFEST_SCHEMA_VERSION};
use crate::integrity_index::{self, FileFingerprint};
use crate::source_identity::{GitSourceSnapshot, SourceQuery};
#[cfg(test)]
pub(crate) use crate::stage_cache::{
    can_migrate_narrowed_manifest, changed_input_summary, compute_stage_evaluation,
    write_stage_manifest,
};
pub(crate) use crate::stage_cache::{
    compute_stage_inputs, execute_cached_stage, execute_cached_stage_with_resources, explain_stage,
    explain_stage_details, read_stage_manifest, record_virtual_stage, stage_manifest_path,
};
use crate::timing::{IntegrityCacheStats, TimingCategory, TimingRecord, TimingReport};

const TIMING_SCHEMA_VERSION: u32 = 2;
const NORMALIZED_SOURCE_DATE_EPOCH: &str = "1767225600";

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct SourceDigestKey {
    repo_root: PathBuf,
    query: SourceQuery,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct InventoryKey {
    repo_root: PathBuf,
    path: PathBuf,
    filter_docs: bool,
}

#[derive(Default)]
struct InvocationIntegrityCache {
    file_digests: BTreeMap<PathBuf, String>,
    inventories: BTreeMap<InventoryKey, Vec<InventoryEntry>>,
    source_digests: BTreeMap<SourceDigestKey, String>,
    source_digest_queries: BTreeMap<String, SourceQuery>,
    tool_identities: BTreeMap<String, ToolIdentity>,
    stats: BTreeMap<String, IntegrityCacheStats>,
    git_source_snapshot: Option<Arc<GitSourceSnapshot>>,
}

#[cfg(not(test))]
struct Shared<T>(Mutex<RefCell<T>>);

#[cfg(not(test))]
impl<T> Shared<T> {
    const fn new(value: T) -> Self {
        Self(Mutex::new(RefCell::new(value)))
    }

    fn with<R>(&self, action: impl FnOnce(&RefCell<T>) -> R) -> R {
        let guard = self
            .0
            .lock()
            .expect("shared invocation state mutex poisoned");
        action(&guard)
    }
}

thread_local! {
    static ACTIVE_BUILD_LOG: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
    static ACTIVE_STAGE_CPU: RefCell<Option<StageCpuUsage>> = const { RefCell::new(None) };
    #[cfg(test)]
    static TIMINGS: RefCell<Option<(PathBuf, TimingReport)>> = const { RefCell::new(None) };
    #[cfg(test)]
    static INTEGRITY_CACHE: RefCell<Option<InvocationIntegrityCache>> = const { RefCell::new(None) };
    #[cfg(test)]
    static TIMING_STARTED: RefCell<Option<Instant>> = const { RefCell::new(None) };
}

#[derive(Clone, Copy, Default)]
struct StageCpuUsage {
    user: Duration,
    system: Duration,
    commands: usize,
    complete: bool,
}

pub(crate) fn stage_cpu_usage_from_log(repo_root: &Path, stage: &str) -> Option<(f64, f64)> {
    let log = fs::read_to_string(repo_root.join("out/logs").join(format!("{stage}.log"))).ok()?;
    let line = log
        .lines()
        .rev()
        .find(|line| line.contains("stage-cpu-accounting") && line.contains("available=true"))?;
    let value = |key: &str| {
        line.split_whitespace()
            .find_map(|part| part.strip_prefix(key))?
            .parse::<f64>()
            .ok()
    };
    Some((value("user_seconds=")?, value("system_seconds=")?))
}

fn wait_with_tree_cpu(
    child: &mut std::process::Child,
) -> Result<(ExitStatus, Option<StageCpuUsage>)> {
    use std::os::unix::process::ExitStatusExt;

    let mut raw_status = 0;
    let mut usage = std::mem::MaybeUninit::<libc::rusage>::zeroed();
    // wait4() reaps exactly this command child and atomically returns its CPU
    // usage, including CPU charged to descendants which it waited for.  This is
    // more reliable than sampling a zombie's /proc stat file and cannot cross
    // attribute a concurrently running stage's process tree.
    loop {
        let result = unsafe {
            libc::wait4(
                child.id() as libc::pid_t,
                &mut raw_status,
                0,
                usage.as_mut_ptr(),
            )
        };
        if result >= 0 {
            let usage = unsafe { usage.assume_init() };
            let to_duration = |time: libc::timeval| {
                Duration::from_secs(time.tv_sec as u64)
                    + Duration::from_micros(time.tv_usec as u64)
            };
            return Ok((
                ExitStatus::from_raw(raw_status),
                Some(StageCpuUsage {
                    user: to_duration(usage.ru_utime),
                    system: to_duration(usage.ru_stime),
                    commands: 0,
                    complete: true,
                }),
            ));
        }
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() == Some(libc::EINTR) {
            continue;
        }
        // An unavailable kernel interface must not silently produce partial
        // stage totals.  Reap through std so normal child cleanup still occurs.
        return Ok((child.wait()?, None));
    }
}

fn record_active_tree_cpu(usage: Option<StageCpuUsage>) {
    ACTIVE_STAGE_CPU.with(|slot| {
        let mut slot = slot.borrow_mut();
        let total = slot.get_or_insert_default();
        total.complete = if total.commands == 0 {
            usage.is_some()
        } else {
            total.complete && usage.is_some()
        };
        total.commands += 1;
        if let Some(usage) = usage {
            total.user += usage.user;
            total.system += usage.system;
        }
    });
}
#[cfg(not(test))]
static TIMINGS: Shared<Option<(PathBuf, TimingReport)>> = Shared::new(None);
#[cfg(not(test))]
static INTEGRITY_CACHE: Shared<Option<InvocationIntegrityCache>> = Shared::new(None);
#[cfg(not(test))]
static TIMING_STARTED: Shared<Option<Instant>> = Shared::new(None);

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
                categories: BTreeMap::new(),
                integrity_cache: BTreeMap::new(),
            },
        ));
    });
    INTEGRITY_CACHE.with(|slot| *slot.borrow_mut() = Some(InvocationIntegrityCache::default()));
    TIMING_STARTED.with(|slot| *slot.borrow_mut() = Some(Instant::now()));
    let index_timer = Instant::now();
    integrity_index::start(repo_root);
    record_category("integrity_index_load", index_timer.elapsed());
    persist_timing_report()
}

pub(crate) fn finish_timing_run(result: &Result<()>) -> Result<()> {
    let index_timer = Instant::now();
    if result.is_ok() {
        persist_persistent_integrity_index()?;
    }
    record_category("integrity_index_store", index_timer.elapsed());
    let elapsed = TIMING_STARTED.with(|slot| slot.borrow().as_ref().map(Instant::elapsed));
    let integrity_cache = INTEGRITY_CACHE.with(|slot| {
        slot.borrow()
            .as_ref()
            .map(|cache| cache.stats.clone())
            .unwrap_or_default()
    });
    TIMINGS.with(|slot| {
        if let Some((_, report)) = slot.borrow_mut().as_mut() {
            report.ended_at_utc = Some(Utc::now().to_rfc3339());
            report.result = if result.is_ok() { "success" } else { "failed" }.to_string();
            report.integrity_cache = integrity_cache;
            if let Some(elapsed) = elapsed {
                let attributed = report
                    .categories
                    .iter()
                    .filter(|(name, _)| {
                        !matches!(
                            name.as_str(),
                            "integrity_index_lookup" | "integrity_fallback_hashing"
                        )
                    })
                    .map(|(_, category)| category.wall_seconds)
                    .sum::<f64>();
                report.categories.insert(
                    "orchestration_unattributed".to_string(),
                    TimingCategory {
                        wall_seconds: (elapsed.as_secs_f64() - attributed).max(0.0),
                        operations: 1,
                    },
                );
            }
        }
    });
    persist_timing_report()?;
    print_timing_summary()?;
    TIMINGS.with(|slot| *slot.borrow_mut() = None);
    INTEGRITY_CACHE.with(|slot| *slot.borrow_mut() = None);
    integrity_index::clear();
    TIMING_STARTED.with(|slot| *slot.borrow_mut() = None);
    Ok(())
}

fn persist_persistent_integrity_index() -> Result<()> {
    let Some((path, mut body)) = integrity_index::serialized_if_dirty()? else {
        return Ok(());
    };
    body.push(b'\n');
    atomic_write(&path, &body)
}

pub(crate) fn record_category(name: &str, elapsed: Duration) {
    TIMINGS.with(|slot| {
        if let Some((_, report)) = slot.borrow_mut().as_mut() {
            let category = report.categories.entry(name.to_string()).or_default();
            category.wall_seconds += elapsed.as_secs_f64();
            category.operations += 1;
        }
    });
}

pub(crate) fn measured<T>(category: &str, action: impl FnOnce() -> Result<T>) -> Result<T> {
    let timer = Instant::now();
    let result = action();
    record_category(category, timer.elapsed());
    result
}

pub(crate) fn measure_package_validation<T>(action: impl FnOnce() -> Result<T>) -> Result<T> {
    measured("package_validation", action)
}

pub(crate) fn measure_package_validation_step<T>(
    step: &str,
    action: impl FnOnce() -> Result<T>,
) -> Result<T> {
    measured(&format!("package_validation:{step}"), action)
}

pub(crate) fn record_timing(record: TimingRecord) -> Result<()> {
    TIMINGS.with(|slot| {
        if let Some((_, report)) = slot.borrow_mut().as_mut() {
            report.stages.push(record);
        }
    });
    Ok(())
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
    let result = if cache_status == "miss" || cache_status == "n/a" {
        measured("stage_actions", action)
    } else {
        action()
    };
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

pub(crate) fn with_stage_log<T, F>(repo_root: &Path, stage: &str, action: F) -> Result<T>
where
    F: FnOnce() -> Result<T>,
{
    trace_log_context("with_stage_log-before");
    let log = repo_root
        .join("out/logs")
        .join(format!("{}.log", sanitize_identifier(stage)));
    if let Some(parent) = log.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&log, format!("MattOS build log: {stage}\n"))?;
    ACTIVE_BUILD_LOG.with(|slot| *slot.borrow_mut() = Some(log.clone()));
    ACTIVE_STAGE_CPU.with(|slot| *slot.borrow_mut() = Some(StageCpuUsage::default()));
    trace_log_context("with_stage_log-after-set");
    println!("[build] {stage}: running (full log: {})", log.display());
    trace_log_context("with_stage_log-action-entry");
    let result = action();
    let cpu = ACTIVE_STAGE_CPU.with(|slot| slot.borrow_mut().take());
    if let Some(cpu) = cpu {
        let wall = 0.0; // Scheduler records action wall time at its ownership boundary.
        let _ = append_active_stage_log(&format!(
            "stage-cpu-accounting available={} user_seconds={:.6} system_seconds={:.6} total_seconds={:.6} average_cores={}",
            cpu.complete && cpu.commands > 0, cpu.user.as_secs_f64(), cpu.system.as_secs_f64(),
            (cpu.user + cpu.system).as_secs_f64(),
            if wall == 0.0 { "recorded-by-scheduler".to_string() } else { "unavailable".to_string() }
        ));
    }
    trace_log_context("with_stage_log-action-return");
    ACTIVE_BUILD_LOG.with(|slot| *slot.borrow_mut() = None);
    trace_log_context("with_stage_log-after-clear");
    if let Err(error) = &result {
        eprintln!("[build] {stage}: failed; full log: {}", log.display());
        if let Ok(tail) = log_tail(&log, 40) {
            eprintln!("--- last build output ---\n{tail}--- end build output ---\n");
        }
        eprintln!("{error:#}");
    }
    result
}

pub(crate) fn trace_log_context(boundary: &str) {
    let Some(path) = std::env::var_os("MATTOS_LOG_CONTEXT_TRACE") else {
        return;
    };
    let active = ACTIVE_BUILD_LOG.with(|slot| slot.borrow().clone());
    let line = format!(
        "boundary={boundary} thread={:?} active={}\n",
        std::thread::current().id(),
        active
            .as_ref()
            .map_or_else(|| "None".to_string(), |path| path.display().to_string())
    );
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = file.write_all(line.as_bytes());
    }
}

/// Appends an internal diagnostic to the log of the currently executing stage.
/// It deliberately does nothing outside `with_stage_log`, so normal callers
/// do not create logs or change build output.
pub(crate) fn append_active_stage_log(line: &str) -> Result<()> {
    let active = ACTIVE_BUILD_LOG.with(|slot| slot.borrow().clone());
    if let Some(path) = active {
        let mut log = OpenOptions::new().append(true).open(&path)?;
        writeln!(log, "[mattos] {line}")?;
    }
    Ok(())
}

#[cfg(test)]
pub(crate) fn active_stage_log_path_for_test() -> Option<PathBuf> {
    ACTIVE_BUILD_LOG.with(|slot| slot.borrow().clone())
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
        let (status, usage) = wait_with_tree_cpu(&mut child)
            .with_context(|| format!("failed to wait for {display}"))?;
        record_active_tree_cpu(usage);
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
    let mut child = command
        .spawn()
        .with_context(|| format!("failed to spawn {display}"))?;
    let (status, usage) =
        wait_with_tree_cpu(&mut child).with_context(|| format!("failed to wait for {display}"))?;
    record_active_tree_cpu(usage);
    Ok(status)
}

fn log_tail(path: &Path, lines: usize) -> Result<String> {
    let body = fs::read_to_string(path)?;
    let values = body.lines().collect::<Vec<_>>();
    let start = values.len().saturating_sub(lines);
    Ok(values[start..].join("\n") + "\n")
}

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
        text.push_str("\nTiming categories\nseconds  operations  category\n");
        for (name, category) in &report.categories {
            text.push_str(&format!(
                "{:>7.3}  {:>10}  {}\n",
                category.wall_seconds, category.operations, name
            ));
        }
        text.push_str("\nInvocation integrity cache\nhits  misses  cache\n");
        for (name, stats) in &report.integrity_cache {
            text.push_str(&format!(
                "{:>4}  {:>6}  {}\n",
                stats.hits, stats.misses, name
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
        println!("\nMattOS timing categories:");
        for (name, category) in &report.categories {
            println!(
                "  {:>8.3}s  {:>5} operation(s)  {}",
                category.wall_seconds, category.operations, name
            );
        }
        println!("\nMattOS invocation integrity cache:");
        for (name, stats) in &report.integrity_cache {
            println!(
                "  {:>5} hit(s)  {:>5} miss(es)  {}",
                stats.hits, stats.misses, name
            );
        }
        Ok(())
    })
}

pub(crate) fn diagnostic_path(repo_root: &Path, path: &Path) -> String {
    normalize_path(path.strip_prefix(repo_root).unwrap_or(path))
}

fn absolute_cache_path(repo_root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        repo_root.join(path)
    }
}

fn paths_overlap(left: &Path, right: &Path) -> bool {
    left.starts_with(right) || right.starts_with(left)
}

pub(crate) fn invalidate_integrity_paths(repo_root: &Path, paths: &[PathBuf]) {
    let paths = paths
        .iter()
        .map(|path| absolute_cache_path(repo_root, path))
        .collect::<Vec<_>>();
    INTEGRITY_CACHE.with(|slot| {
        let mut borrowed = slot.borrow_mut();
        let Some(cache) = borrowed.as_mut() else {
            return;
        };
        cache
            .file_digests
            .retain(|path, _| !paths.iter().any(|changed| paths_overlap(path, changed)));
        cache.inventories.retain(|key, _| {
            !paths
                .iter()
                .any(|changed| paths_overlap(&key.path, changed))
        });
        cache.source_digests.retain(|key, _| {
            !key.query
                .roots
                .iter()
                .any(|root| paths.iter().any(|changed| paths_overlap(root, changed)))
        });
        cache.tool_identities.retain(|_, identity| {
            !paths
                .iter()
                .any(|changed| paths_overlap(Path::new(&identity.resolved_path), changed))
        });
        if paths
            .iter()
            .any(|path| path.starts_with(repo_root) && !path.starts_with(repo_root.join("out")))
        {
            cache.git_source_snapshot = None;
        }
    });
    integrity_index::invalidate(&paths);
}

fn record_integrity_cache_access(cache: &mut InvocationIntegrityCache, name: &str, hit: bool) {
    let stats = cache.stats.entry(name.to_string()).or_default();
    if hit {
        stats.hits += 1;
    } else {
        stats.misses += 1;
    }
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

pub(crate) fn digest_source_inputs(repo_root: &Path, roots: &[PathBuf]) -> Result<String> {
    tracked_source_digest(repo_root, roots, true)
}

pub(crate) fn tracked_source_digest(
    repo_root: &Path,
    roots: &[PathBuf],
    exclude_documentation: bool,
) -> Result<String> {
    let key = SourceDigestKey {
        repo_root: repo_root.to_path_buf(),
        query: SourceQuery::new(
            &roots
                .iter()
                .map(|path| absolute_cache_path(repo_root, path))
                .collect::<Vec<_>>(),
            exclude_documentation,
        ),
    };
    if let Some(digest) = INTEGRITY_CACHE.with(|slot| {
        let mut borrowed = slot.borrow_mut();
        let cache = borrowed.as_mut()?;
        let digest = cache.source_digests.get(&key).cloned();
        record_integrity_cache_access(cache, "source_digest", digest.is_some());
        digest
    }) {
        return Ok(digest);
    }
    let timer = Instant::now();
    record_category(
        if exclude_documentation {
            "input_source_query:miss:exclude_docs"
        } else {
            "input_source_query:miss:include_docs"
        },
        Duration::ZERO,
    );
    let digest = tracked_source_digest_uncached(repo_root, roots, exclude_documentation)?;
    let label = roots
        .iter()
        .map(|path| diagnostic_path(repo_root, path))
        .collect::<Vec<_>>()
        .join("+");
    record_category(&format!("input_source_root:{label}"), timer.elapsed());
    INTEGRITY_CACHE.with(|slot| {
        if let Some(cache) = slot.borrow_mut().as_mut() {
            let reuse = if cache.source_digest_queries.contains_key(&digest) {
                "equivalent"
            } else {
                "unique"
            };
            record_category(
                &format!("input_source_query:canonical_digest:{reuse}"),
                Duration::ZERO,
            );
            cache
                .source_digest_queries
                .entry(digest.clone())
                .or_insert_with(|| key.query.clone());
            cache.source_digests.insert(key, digest.clone());
        }
    });
    Ok(digest)
}

fn tracked_source_digest_uncached(
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
    if let Some(snapshot) = invocation_git_source_snapshot(repo_root)? {
        let query = SourceQuery::new(&relative_roots, exclude_documentation);
        let label = query
            .roots
            .iter()
            .map(|path| path.to_string_lossy())
            .collect::<Vec<_>>()
            .join("+");
        return snapshot.digest_query(
            repo_root,
            &query,
            |absolute| {
                if !absolute.symlink_metadata().is_ok() {
                    return Ok(None);
                }
                let mut inventory = Vec::new();
                collect_inventory(repo_root, absolute, false, &mut inventory)?;
                Ok(Some(digest_serializable(&inventory)?))
            },
            |phase, elapsed| {
                record_category(&format!("input_source_profile:{label}:{phase}"), elapsed);
            },
        );
    }
    digest_paths(
        repo_root,
        roots,
        exclude_documentation,
        "filesystem-source-inputs",
    )
}

#[cfg(test)]
fn populate_git_source_values<'a>(
    repo_root: &Path,
    relative_roots: &[PathBuf],
    exclude_documentation: bool,
    snapshot: &'a GitSourceSnapshot,
    index_entries: BTreeMap<&'a str, &'a str>,
    values: &mut BTreeMap<&'a str, String>,
) -> Result<()> {
    for (path, header) in index_entries {
        let path_buf = PathBuf::from(path);
        if exclude_documentation && is_irrelevant_documentation(&path_buf) {
            continue;
        }
        if snapshot.is_modified(path) {
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
    for path in snapshot.untracked_paths(&relative_roots) {
        let path_buf = PathBuf::from(path);
        if exclude_documentation && is_irrelevant_documentation(&path_buf) {
            continue;
        }
        let mut inventory = Vec::new();
        collect_inventory(repo_root, &repo_root.join(&path_buf), false, &mut inventory)?;
        values.insert(
            path,
            format!("untracked:{}", digest_serializable(&inventory)?),
        );
    }
    Ok(())
}

#[cfg(test)]
fn tracked_source_digest_full_scan_reference(
    repo_root: &Path,
    roots: &[PathBuf],
    exclude_documentation: bool,
) -> Result<String> {
    let relative_roots = roots
        .iter()
        .map(|path| path.strip_prefix(repo_root).unwrap_or(path).to_path_buf())
        .collect::<Vec<_>>();
    if let Ok(snapshot) = GitSourceSnapshot::capture(repo_root, |_, _| {}) {
        let mut values = BTreeMap::new();
        let index_entries = snapshot.index_entries_full_scan(&relative_roots);
        populate_git_source_values(
            repo_root,
            &relative_roots,
            exclude_documentation,
            &snapshot,
            index_entries,
            &mut values,
        )?;
        return digest_serializable(&("git-index-and-working-tree", values));
    }
    digest_paths(
        repo_root,
        roots,
        exclude_documentation,
        "filesystem-source-inputs",
    )
}

#[cfg(test)]
fn tracked_source_canonical_bytes(
    repo_root: &Path,
    roots: &[PathBuf],
    exclude_documentation: bool,
) -> Result<(Vec<u8>, Vec<u8>)> {
    let relative_roots = roots
        .iter()
        .map(|path| path.strip_prefix(repo_root).unwrap_or(path).to_path_buf())
        .collect::<Vec<_>>();
    let snapshot = GitSourceSnapshot::capture(repo_root, |_, _| {})?;
    let query = SourceQuery::new(&relative_roots, exclude_documentation);
    let mut digest_working = |absolute: &Path| -> Result<Option<String>> {
        if !absolute.symlink_metadata().is_ok() {
            return Ok(None);
        }
        let mut inventory = Vec::new();
        collect_inventory(repo_root, absolute, false, &mut inventory)?;
        Ok(Some(digest_serializable(&inventory)?))
    };
    let streamed = snapshot.canonical_query_bytes(repo_root, &query, &mut digest_working)?;
    let mut values = BTreeMap::new();
    populate_git_source_values(
        repo_root,
        &relative_roots,
        exclude_documentation,
        &snapshot,
        snapshot.index_entries_full_scan(&relative_roots),
        &mut values,
    )?;
    let legacy = serde_json::to_vec(&("git-index-and-working-tree", values))?;
    Ok((streamed, legacy))
}

fn invocation_git_source_snapshot(repo_root: &Path) -> Result<Option<Arc<GitSourceSnapshot>>> {
    if let Some(snapshot) = INTEGRITY_CACHE.with(|slot| {
        slot.borrow()
            .as_ref()
            .and_then(|cache| cache.git_source_snapshot.clone())
    }) {
        return Ok(Some(snapshot));
    }
    if let Ok(snapshot) = GitSourceSnapshot::capture(repo_root, |command, elapsed| {
        if command == "snapshot-map-construction" {
            record_category("input_source_profile:snapshot_map_construction", elapsed);
        } else {
            record_category(&format!("input_git:{command}"), elapsed);
        }
    }) {
        let snapshot = Arc::new(snapshot);
        INTEGRITY_CACHE.with(|slot| {
            if let Some(cache) = slot.borrow_mut().as_mut() {
                cache.git_source_snapshot = Some(snapshot.clone());
            }
        });
        return Ok(Some(snapshot));
    }
    Ok(None)
}

pub(crate) fn digest_paths(
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
        entries.extend(inventory_for_path(repo_root, &absolute, filter_docs)?);
    }
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    digest_serializable(&(seed, entries))
}

pub(crate) fn output_inventory(repo_root: &Path, paths: &[PathBuf]) -> Result<Vec<InventoryEntry>> {
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
        entries.extend(inventory_for_path(repo_root, &absolute, false)?);
    }
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    let mut seen = BTreeSet::new();
    entries.retain(|entry| seen.insert(entry.path.clone()));
    Ok(entries)
}

fn inventory_for_path(
    repo_root: &Path,
    path: &Path,
    filter_docs: bool,
) -> Result<Vec<InventoryEntry>> {
    let key = InventoryKey {
        repo_root: repo_root.to_path_buf(),
        path: absolute_cache_path(repo_root, path),
        filter_docs,
    };
    if let Some(inventory) = INTEGRITY_CACHE.with(|slot| {
        let mut borrowed = slot.borrow_mut();
        let cache = borrowed.as_mut()?;
        let inventory = cache.inventories.get(&key).cloned();
        record_integrity_cache_access(cache, "path_inventory", inventory.is_some());
        inventory
    }) {
        return Ok(inventory);
    }
    let mut inventory = Vec::new();
    collect_inventory(repo_root, path, filter_docs, &mut inventory)?;
    INTEGRITY_CACHE.with(|slot| {
        if let Some(cache) = slot.borrow_mut().as_mut() {
            cache.inventories.insert(key, inventory.clone());
        }
    });
    Ok(inventory)
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
        let fingerprint = integrity_index::fingerprint(&metadata);
        entries.push(InventoryEntry {
            path: normalized,
            kind: "file".to_string(),
            mode,
            uid,
            gid,
            size: metadata.len(),
            content: sha256_file_with_fingerprint(path, fingerprint.as_ref())?,
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

pub(crate) fn tool_identities(tools: &[String]) -> Result<BTreeMap<String, ToolIdentity>> {
    let mut values = BTreeMap::new();
    for tool in tools {
        if let Some(identity) = INTEGRITY_CACHE.with(|slot| {
            let mut borrowed = slot.borrow_mut();
            let cache = borrowed.as_mut()?;
            let identity = cache.tool_identities.get(tool).cloned();
            record_integrity_cache_access(cache, "tool_identity", identity.is_some());
            identity
        }) {
            values.insert(tool.clone(), identity);
            continue;
        }
        let identity = crate::tool_identity::inspect(tool, sha256_file)?;
        INTEGRITY_CACHE.with(|slot| {
            if let Some(cache) = slot.borrow_mut().as_mut() {
                cache.tool_identities.insert(tool.clone(), identity.clone());
            }
        });
        values.insert(tool.clone(), identity);
    }
    Ok(values)
}

pub(crate) fn normalized_build_environment() -> BTreeMap<String, String> {
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

pub(crate) fn inventory_digest(inventory: &[InventoryEntry]) -> Result<String> {
    digest_serializable(inventory)
}

pub(crate) fn digest_serializable<T: Serialize + ?Sized>(value: &T) -> Result<String> {
    let body = serde_json::to_vec(value)?;
    Ok(format!("{:x}", Sha256::digest(body)))
}

pub(crate) fn sha256_file(path: &Path) -> Result<String> {
    sha256_file_with_fingerprint(path, None)
}

fn sha256_file_with_fingerprint(
    path: &Path,
    expected_fingerprint: Option<&FileFingerprint>,
) -> Result<String> {
    let cache_path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    if let Some(digest) = INTEGRITY_CACHE.with(|slot| {
        let mut borrowed = slot.borrow_mut();
        let cache = borrowed.as_mut()?;
        let digest = cache.file_digests.get(&cache_path).cloned();
        record_integrity_cache_access(cache, "file_digest", digest.is_some());
        digest
    }) {
        return Ok(digest);
    }

    let path_metadata = fs::symlink_metadata(path)?;
    let path_fingerprint = if path_metadata.is_file() {
        integrity_index::fingerprint(&path_metadata)
    } else {
        None
    };
    let mut file = fs::File::open(path)?;
    let opened_fingerprint = integrity_index::fingerprint(&file.metadata()?);
    if expected_fingerprint.is_some() && opened_fingerprint.as_ref() != expected_fingerprint {
        bail!(
            "{} changed while its inventory was collected",
            path.display()
        )
    }
    let stable_regular_path = path_fingerprint.is_some() && path_fingerprint == opened_fingerprint;
    let persistent_eligible = stable_regular_path && integrity_index::eligible(path);
    if persistent_eligible {
        if let Some(fingerprint) = opened_fingerprint.as_ref() {
            let lookup_timer = Instant::now();
            let digest = integrity_index::lookup(path, fingerprint);
            record_category("integrity_index_lookup", lookup_timer.elapsed());
            INTEGRITY_CACHE.with(|slot| {
                if let Some(cache) = slot.borrow_mut().as_mut() {
                    record_integrity_cache_access(
                        cache,
                        "persistent_file_digest",
                        digest.is_some(),
                    );
                }
            });
            if let Some(digest) = digest {
                verify_unchanged_open_file(path, &file, fingerprint)?;
                INTEGRITY_CACHE.with(|slot| {
                    if let Some(cache) = slot.borrow_mut().as_mut() {
                        cache.file_digests.insert(cache_path, digest.clone());
                    }
                });
                return Ok(digest);
            }
        }
    }

    let timer = Instant::now();
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 1024 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    let digest = format!("{:x}", digest.finalize());
    if let Some(fingerprint) = opened_fingerprint.as_ref() {
        verify_unchanged_open_file(path, &file, fingerprint)?;
    }
    if persistent_eligible {
        record_category("integrity_fallback_hashing", timer.elapsed());
        if let Some(fingerprint) = opened_fingerprint {
            integrity_index::store(path, fingerprint, digest.clone());
        }
    }
    INTEGRITY_CACHE.with(|slot| {
        if let Some(cache) = slot.borrow_mut().as_mut() {
            cache.file_digests.insert(cache_path, digest.clone());
        }
    });
    Ok(digest)
}

fn verify_unchanged_open_file(
    path: &Path,
    file: &fs::File,
    expected: &FileFingerprint,
) -> Result<()> {
    let opened = integrity_index::fingerprint(&file.metadata()?);
    let current_path = fs::symlink_metadata(path)?;
    let current_path = if current_path.is_file() {
        integrity_index::fingerprint(&current_path)
    } else {
        None
    };
    if opened.as_ref() != Some(expected) || current_path.as_ref() != Some(expected) {
        bail!("{} changed while its digest was validated", path.display())
    }
    Ok(())
}

fn normalize_path(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

pub(crate) fn sanitize_identifier(value: &str) -> String {
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

    fn begin_test_integrity_cache() {
        INTEGRITY_CACHE.with(|slot| {
            *slot.borrow_mut() = Some(InvocationIntegrityCache::default());
        });
    }

    #[test]
    fn append_active_stage_log_writes_only_to_the_active_stage_log() {
        let root = tempdir().unwrap();
        with_stage_log(root.path(), "diagnostic", || {
            append_active_stage_log("normalization boundary").unwrap();
            Ok(())
        })
        .unwrap();
        assert!(
            fs::read_to_string(root.path().join("out/logs/diagnostic.log"))
                .unwrap()
                .contains("[mattos] normalization boundary")
        );
    }

    fn end_test_integrity_cache() {
        INTEGRITY_CACHE.with(|slot| *slot.borrow_mut() = None);
    }

    fn begin_test_integrity_session(repo_root: &Path) {
        begin_test_integrity_cache();
        integrity_index::start(repo_root);
    }

    fn end_test_integrity_session(persist: bool) {
        if persist {
            persist_persistent_integrity_index().unwrap();
        }
        integrity_index::clear();
        end_test_integrity_cache();
    }

    fn inventory_file_digest(inventory: &[InventoryEntry], suffix: &str) -> String {
        inventory
            .iter()
            .find(|entry| entry.path.ends_with(suffix))
            .unwrap()
            .content
            .clone()
    }

    fn integrity_cache_stats(name: &str) -> IntegrityCacheStats {
        INTEGRITY_CACHE.with(|slot| {
            slot.borrow()
                .as_ref()
                .and_then(|cache| cache.stats.get(name))
                .cloned()
                .unwrap_or_default()
        })
    }

    fn initialize_git_fixture(root: &Path) {
        for arguments in [
            vec!["init", "-q"],
            vec!["config", "user.email", "tests@mattos.invalid"],
            vec!["config", "user.name", "MattOS Tests"],
        ] {
            assert!(Command::new("git")
                .args(arguments)
                .current_dir(root)
                .status()
                .unwrap()
                .success());
        }
    }

    fn assert_source_digest_matches_full_scan(
        repo_root: &Path,
        roots: &[PathBuf],
        exclude_documentation: bool,
    ) {
        begin_test_integrity_cache();
        let optimized = tracked_source_digest(repo_root, roots, exclude_documentation).unwrap();
        end_test_integrity_cache();
        let reference =
            tracked_source_digest_full_scan_reference(repo_root, roots, exclude_documentation)
                .unwrap();
        assert_eq!(optimized, reference);
        if let Ok((streamed, legacy)) =
            tracked_source_canonical_bytes(repo_root, roots, exclude_documentation)
        {
            assert_eq!(streamed, legacy, "streamed canonical JSON changed");
        }
    }

    #[test]
    fn invocation_inventory_reuses_verified_results() {
        let root = tempdir().unwrap();
        let output = root.path().join("output");
        fs::create_dir(&output).unwrap();
        fs::write(output.join("file"), "stable").unwrap();
        begin_test_integrity_cache();
        let first = output_inventory(root.path(), &[output.clone()]).unwrap();
        let second = output_inventory(root.path(), &[output]).unwrap();
        assert_eq!(first, second);
        assert_eq!(
            sha256_file(&root.path().join("output/file")).unwrap(),
            first
                .iter()
                .find(|entry| entry.path.ends_with("output/file"))
                .unwrap()
                .content
        );
        assert_eq!(integrity_cache_stats("path_inventory").hits, 1);
        assert_eq!(integrity_cache_stats("path_inventory").misses, 1);
        assert_eq!(integrity_cache_stats("file_digest").hits, 1);
        end_test_integrity_cache();
    }

    #[test]
    fn invocation_source_and_tool_identity_are_memoized() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempdir().unwrap();
        let source = root.path().join("source");
        let tool = root.path().join("tool");
        fs::write(&source, "stable").unwrap();
        fs::write(&tool, "#!/bin/sh\necho fixture-tool 1.0\n").unwrap();
        fs::set_permissions(&tool, fs::Permissions::from_mode(0o755)).unwrap();
        begin_test_integrity_cache();
        let roots = vec![source];
        assert_eq!(
            tracked_source_digest(root.path(), &roots, false).unwrap(),
            tracked_source_digest(root.path(), &roots, false).unwrap()
        );
        let tools = vec![tool.to_string_lossy().into_owned()];
        assert_eq!(
            tool_identities(&tools).unwrap(),
            tool_identities(&tools).unwrap()
        );
        assert_eq!(integrity_cache_stats("source_digest").hits, 1);
        assert_eq!(integrity_cache_stats("source_digest").misses, 1);
        assert_eq!(integrity_cache_stats("tool_identity").hits, 1);
        assert_eq!(integrity_cache_stats("tool_identity").misses, 1);
        end_test_integrity_cache();
    }

    #[test]
    fn invocation_source_snapshot_detects_same_size_replacement_and_symlink_edits() {
        use std::os::unix::fs::symlink;

        let root = tempdir().unwrap();
        initialize_git_fixture(root.path());
        fs::create_dir(root.path().join("source")).unwrap();
        let file = root.path().join("source/file");
        let link = root.path().join("source/link");
        fs::write(&file, "good").unwrap();
        symlink("file", &link).unwrap();
        assert!(Command::new("git")
            .args(["add", "source"])
            .current_dir(root.path())
            .status()
            .unwrap()
            .success());
        assert!(Command::new("git")
            .args(["commit", "-qm", "fixture"])
            .current_dir(root.path())
            .status()
            .unwrap()
            .success());
        let roots = vec![PathBuf::from("source")];

        begin_test_integrity_cache();
        let original = tracked_source_digest(root.path(), &roots, true).unwrap();
        fs::write(&file, "evil").unwrap();
        invalidate_integrity_paths(root.path(), std::slice::from_ref(&file));
        let same_size = tracked_source_digest(root.path(), &roots, true).unwrap();
        assert_ne!(original, same_size);

        let replacement = root.path().join("source/replacement");
        fs::write(&replacement, "next").unwrap();
        fs::rename(&replacement, &file).unwrap();
        invalidate_integrity_paths(root.path(), std::slice::from_ref(&file));
        let replaced = tracked_source_digest(root.path(), &roots, true).unwrap();
        assert_ne!(same_size, replaced);

        fs::remove_file(&link).unwrap();
        symlink("missing", &link).unwrap();
        invalidate_integrity_paths(root.path(), std::slice::from_ref(&link));
        let relinked = tracked_source_digest(root.path(), &roots, true).unwrap();
        assert_ne!(replaced, relinked);
        end_test_integrity_cache();
    }

    #[test]
    fn git_assisted_source_identity_detects_all_worktree_and_index_changes() {
        use std::os::unix::fs::{symlink, PermissionsExt};

        let root = tempdir().unwrap();
        initialize_git_fixture(root.path());
        fs::create_dir(root.path().join("source")).unwrap();
        let file = root.path().join("source/file");
        fs::write(&file, "base").unwrap();
        assert!(Command::new("git")
            .args(["add", "source/file"])
            .current_dir(root.path())
            .status()
            .unwrap()
            .success());
        assert!(Command::new("git")
            .args(["commit", "-qm", "fixture"])
            .current_dir(root.path())
            .status()
            .unwrap()
            .success());
        let roots = vec![PathBuf::from("source")];
        let digest = || tracked_source_digest(root.path(), &roots, true).unwrap();
        let refresh = || invalidate_integrity_paths(root.path(), std::slice::from_ref(&file));

        begin_test_integrity_cache();
        let clean = digest();
        let original_mtime =
            filetime::FileTime::from_last_modification_time(&fs::metadata(&file).unwrap());

        fs::write(&file, "ordinary edit").unwrap();
        refresh();
        let ordinary = digest();
        assert_ne!(clean, ordinary, "ordinary unstaged edit must invalidate");

        fs::write(&file, "same").unwrap();
        filetime::set_file_mtime(&file, original_mtime).unwrap();
        refresh();
        let same_size_restored_time = digest();
        assert_ne!(clean, same_size_restored_time);
        assert_ne!(ordinary, same_size_restored_time);

        fs::set_permissions(&file, fs::Permissions::from_mode(0o755)).unwrap();
        refresh();
        let chmod = digest();
        assert_ne!(same_size_restored_time, chmod, "chmod must invalidate");

        fs::write(&file, "staged").unwrap();
        assert!(Command::new("git")
            .args(["add", "source/file"])
            .current_dir(root.path())
            .status()
            .unwrap()
            .success());
        refresh();
        let staged = digest();
        assert_ne!(clean, staged, "staged blob/mode identity must invalidate");

        fs::write(&file, "unstaged after index").unwrap();
        refresh();
        let unstaged = digest();
        assert_ne!(
            staged, unstaged,
            "unstaged bytes must override index identity"
        );

        let replacement = root.path().join("source/replacement");
        fs::write(&replacement, "rename replacement").unwrap();
        fs::rename(&replacement, &file).unwrap();
        refresh();
        let renamed = digest();
        assert_ne!(unstaged, renamed, "rename replacement must invalidate");

        fs::remove_file(&file).unwrap();
        symlink("replacement-target", &file).unwrap();
        refresh();
        let symlinked = digest();
        assert_ne!(renamed, symlinked, "symlink replacement must invalidate");
        end_test_integrity_cache();
    }

    #[test]
    fn optimized_source_identity_matches_full_scan_across_adversarial_states() {
        use std::os::unix::fs::{symlink, PermissionsExt};

        let root = tempdir().unwrap();
        initialize_git_fixture(root.path());
        fs::create_dir_all(root.path().join("source/nested/docs")).unwrap();
        fs::create_dir_all(root.path().join("sourced")).unwrap();
        let file = root.path().join("source/file");
        fs::write(&file, "base").unwrap();
        fs::write(root.path().join("source/nested/value"), "nested").unwrap();
        fs::write(root.path().join("source/nested/docs/readme"), "docs").unwrap();
        fs::write(root.path().join("sourced/file"), "boundary").unwrap();
        assert!(Command::new("git")
            .args(["add", "."])
            .current_dir(root.path())
            .status()
            .unwrap()
            .success());
        assert!(Command::new("git")
            .args(["commit", "-qm", "fixture"])
            .current_dir(root.path())
            .status()
            .unwrap()
            .success());
        let roots = [PathBuf::from("source"), PathBuf::from("source/nested")];
        let assert_matches = || {
            assert_source_digest_matches_full_scan(root.path(), &roots, false);
            assert_source_digest_matches_full_scan(root.path(), &roots, true);
        };

        assert_matches();
        let original_mtime =
            filetime::FileTime::from_last_modification_time(&fs::metadata(&file).unwrap());
        fs::write(&file, "edit").unwrap();
        assert_matches();
        fs::write(&file, "same").unwrap();
        filetime::set_file_mtime(&file, original_mtime).unwrap();
        assert_matches();
        fs::set_permissions(&file, fs::Permissions::from_mode(0o755)).unwrap();
        assert_matches();
        assert!(Command::new("git")
            .args(["add", "source/file"])
            .current_dir(root.path())
            .status()
            .unwrap()
            .success());
        assert_matches();
        fs::write(&file, "unstaged").unwrap();
        assert_matches();
        let replacement = root.path().join("source/replacement");
        fs::write(&replacement, "renamed").unwrap();
        fs::rename(&replacement, &file).unwrap();
        assert_matches();
        fs::remove_file(&file).unwrap();
        symlink("missing-target", &file).unwrap();
        assert_matches();
        fs::remove_file(&file).unwrap();
        assert_matches();
        fs::write(root.path().join("source/untracked"), "new").unwrap();
        assert_matches();

        let conflict = tempdir().unwrap();
        initialize_git_fixture(conflict.path());
        fs::write(conflict.path().join("file"), "base").unwrap();
        for arguments in [
            ["add", "file"].as_slice(),
            ["commit", "-qm", "base"].as_slice(),
            ["checkout", "-qb", "side"].as_slice(),
        ] {
            assert!(Command::new("git")
                .args(arguments)
                .current_dir(conflict.path())
                .status()
                .unwrap()
                .success());
        }
        fs::write(conflict.path().join("file"), "side").unwrap();
        assert!(Command::new("git")
            .args(["commit", "-qam", "side"])
            .current_dir(conflict.path())
            .status()
            .unwrap()
            .success());
        assert!(Command::new("git")
            .args(["checkout", "-q", "master"])
            .current_dir(conflict.path())
            .status()
            .unwrap()
            .success());
        fs::write(conflict.path().join("file"), "main").unwrap();
        assert!(Command::new("git")
            .args(["commit", "-qam", "main"])
            .current_dir(conflict.path())
            .status()
            .unwrap()
            .success());
        assert!(!Command::new("git")
            .args(["merge", "side"])
            .current_dir(conflict.path())
            .status()
            .unwrap()
            .success());
        assert_source_digest_matches_full_scan(conflict.path(), &[PathBuf::from("file")], false);
    }

    #[test]
    fn invocation_configuration_cache_detects_edits_replacements_and_symlinks() {
        use std::os::unix::fs::symlink;

        let root = tempdir().unwrap();
        let config = root.path().join("config");
        let link = root.path().join("config-link");
        fs::write(&config, "good").unwrap();
        symlink("config", &link).unwrap();
        let paths = vec![config.clone(), link.clone()];
        begin_test_integrity_cache();
        let original = digest_paths(root.path(), &paths, false, "config").unwrap();

        fs::write(&config, "evil").unwrap();
        invalidate_integrity_paths(root.path(), std::slice::from_ref(&config));
        let edited = digest_paths(root.path(), &paths, false, "config").unwrap();
        assert_ne!(original, edited);

        let replacement = root.path().join("replacement");
        fs::write(&replacement, "next").unwrap();
        fs::rename(&replacement, &config).unwrap();
        invalidate_integrity_paths(root.path(), std::slice::from_ref(&config));
        let replaced = digest_paths(root.path(), &paths, false, "config").unwrap();
        assert_ne!(edited, replaced);

        fs::remove_file(&link).unwrap();
        symlink("missing", &link).unwrap();
        invalidate_integrity_paths(root.path(), std::slice::from_ref(&link));
        let relinked = digest_paths(root.path(), &paths, false, "config").unwrap();
        assert_ne!(replaced, relinked);
        end_test_integrity_cache();
    }

    #[test]
    fn fresh_process_source_and_configuration_inputs_detect_changes() {
        const ROOT_ENV: &str = "MATTOS_TEST_INPUT_CACHE_ROOT";
        const SOURCE_ENV: &str = "MATTOS_TEST_INPUT_CACHE_SOURCE";
        const CONFIG_ENV: &str = "MATTOS_TEST_INPUT_CACHE_CONFIG";
        if let (Ok(root), Ok(source), Ok(config)) = (
            std::env::var(ROOT_ENV),
            std::env::var(SOURCE_ENV),
            std::env::var(CONFIG_ENV),
        ) {
            let root = PathBuf::from(root);
            begin_test_integrity_cache();
            let current = tracked_source_digest(&root, &[PathBuf::from("source")], true).unwrap();
            assert_ne!(current, source);
            assert_eq!(
                current,
                tracked_source_digest_full_scan_reference(&root, &[PathBuf::from("source")], true)
                    .unwrap()
            );
            let (streamed, legacy) =
                tracked_source_canonical_bytes(&root, &[PathBuf::from("source")], true).unwrap();
            assert_eq!(streamed, legacy);
            assert_ne!(
                digest_paths(&root, &[PathBuf::from("config")], false, "config").unwrap(),
                config
            );
            end_test_integrity_cache();
            return;
        }

        let root = tempdir().unwrap();
        initialize_git_fixture(root.path());
        fs::create_dir(root.path().join("source")).unwrap();
        fs::write(root.path().join("source/file"), "good").unwrap();
        fs::write(root.path().join("config"), "good").unwrap();
        assert!(Command::new("git")
            .args(["add", "source", "config"])
            .current_dir(root.path())
            .status()
            .unwrap()
            .success());
        assert!(Command::new("git")
            .args(["commit", "-qm", "fixture"])
            .current_dir(root.path())
            .status()
            .unwrap()
            .success());
        begin_test_integrity_cache();
        let source = tracked_source_digest(root.path(), &[PathBuf::from("source")], true).unwrap();
        let config =
            digest_paths(root.path(), &[PathBuf::from("config")], false, "config").unwrap();
        end_test_integrity_cache();
        fs::write(root.path().join("source/file"), "evil").unwrap();
        fs::write(root.path().join("config"), "evil").unwrap();

        let status = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "performance::tests::fresh_process_source_and_configuration_inputs_detect_changes",
                "--nocapture",
            ])
            .env(ROOT_ENV, root.path())
            .env(SOURCE_ENV, source)
            .env(CONFIG_ENV, config)
            .status()
            .unwrap();
        assert!(status.success());
    }

    #[test]
    fn persistent_index_reuses_unchanged_output_bytes() {
        let root = tempdir().unwrap();
        let output = root.path().join("out/result");
        fs::create_dir_all(&output).unwrap();
        fs::write(output.join("file"), "stable").unwrap();

        begin_test_integrity_session(root.path());
        let first = output_inventory(root.path(), &[output.clone()]).unwrap();
        assert_eq!(integrity_cache_stats("persistent_file_digest").misses, 1);
        end_test_integrity_session(true);

        begin_test_integrity_session(root.path());
        let second = output_inventory(root.path(), &[output]).unwrap();
        assert_eq!(first, second);
        assert_eq!(integrity_cache_stats("persistent_file_digest").hits, 1);
        assert_eq!(integrity_cache_stats("persistent_file_digest").misses, 0);
        end_test_integrity_session(false);
    }

    #[test]
    fn persistent_index_rehashes_same_size_change_with_restored_mtime() {
        let root = tempdir().unwrap();
        let output = root.path().join("out/result");
        let file = output.join("file");
        fs::create_dir_all(&output).unwrap();
        fs::write(&file, "good").unwrap();
        let original_mtime =
            filetime::FileTime::from_last_modification_time(&fs::metadata(&file).unwrap());

        begin_test_integrity_session(root.path());
        let first = output_inventory(root.path(), &[output.clone()]).unwrap();
        end_test_integrity_session(true);

        fs::write(&file, "evil").unwrap();
        filetime::set_file_mtime(&file, original_mtime).unwrap();
        begin_test_integrity_session(root.path());
        let second = output_inventory(root.path(), &[output]).unwrap();
        assert_ne!(
            inventory_file_digest(&first, "result/file"),
            inventory_file_digest(&second, "result/file")
        );
        assert_eq!(integrity_cache_stats("persistent_file_digest").hits, 0);
        assert_eq!(integrity_cache_stats("persistent_file_digest").misses, 1);
        end_test_integrity_session(false);
    }

    #[test]
    fn persistent_index_rehashes_rename_replacement() {
        let root = tempdir().unwrap();
        let output = root.path().join("out/result");
        let file = output.join("file");
        fs::create_dir_all(&output).unwrap();
        fs::write(&file, "good").unwrap();
        let original_mtime =
            filetime::FileTime::from_last_modification_time(&fs::metadata(&file).unwrap());

        begin_test_integrity_session(root.path());
        let first = output_inventory(root.path(), &[output.clone()]).unwrap();
        end_test_integrity_session(true);

        let replacement = output.join("replacement");
        fs::write(&replacement, "evil").unwrap();
        filetime::set_file_mtime(&replacement, original_mtime).unwrap();
        fs::rename(&replacement, &file).unwrap();
        begin_test_integrity_session(root.path());
        let second = output_inventory(root.path(), &[output]).unwrap();
        assert_ne!(
            inventory_file_digest(&first, "result/file"),
            inventory_file_digest(&second, "result/file")
        );
        assert_eq!(integrity_cache_stats("persistent_file_digest").misses, 1);
        end_test_integrity_session(false);
    }

    #[test]
    fn persistent_index_still_validates_symlinks_and_directory_entries() {
        use std::os::unix::fs::symlink;

        let root = tempdir().unwrap();
        let output = root.path().join("out/result");
        fs::create_dir_all(&output).unwrap();
        fs::write(output.join("file"), "stable").unwrap();
        symlink("file", output.join("link")).unwrap();
        begin_test_integrity_session(root.path());
        let first = output_inventory(root.path(), &[output.clone()]).unwrap();
        end_test_integrity_session(true);

        fs::remove_file(output.join("link")).unwrap();
        symlink("other", output.join("link")).unwrap();
        fs::write(output.join("added"), "new").unwrap();
        begin_test_integrity_session(root.path());
        let second = output_inventory(root.path(), &[output]).unwrap();
        assert_ne!(first, second);
        assert_eq!(integrity_cache_stats("persistent_file_digest").hits, 1);
        assert_eq!(integrity_cache_stats("persistent_file_digest").misses, 1);
        end_test_integrity_session(false);
    }

    #[test]
    fn malformed_persistent_index_falls_back_to_hashing() {
        let root = tempdir().unwrap();
        let output = root.path().join("out/result");
        fs::create_dir_all(root.path().join("out/state")).unwrap();
        fs::create_dir_all(&output).unwrap();
        fs::write(output.join("file"), "stable").unwrap();
        fs::write(integrity_index::path(root.path()), b"not valid json").unwrap();

        begin_test_integrity_session(root.path());
        output_inventory(root.path(), &[output]).unwrap();
        assert_eq!(integrity_cache_stats("persistent_file_digest").misses, 1);
        end_test_integrity_session(false);
    }

    #[test]
    fn checksum_corrupted_persistent_index_falls_back_to_hashing() {
        let root = tempdir().unwrap();
        let output = root.path().join("out/result");
        fs::create_dir_all(&output).unwrap();
        fs::write(output.join("file"), "stable").unwrap();
        begin_test_integrity_session(root.path());
        output_inventory(root.path(), &[output.clone()]).unwrap();
        end_test_integrity_session(true);

        let index_path = integrity_index::path(root.path());
        let mut index: serde_json::Value =
            serde_json::from_slice(&fs::read(&index_path).unwrap()).unwrap();
        let entry = index["entries"]
            .as_object_mut()
            .unwrap()
            .values_mut()
            .next()
            .unwrap();
        entry["sha256"] = serde_json::Value::String("0".repeat(64));
        fs::write(&index_path, serde_json::to_vec_pretty(&index).unwrap()).unwrap();

        begin_test_integrity_session(root.path());
        output_inventory(root.path(), &[output]).unwrap();
        assert_eq!(integrity_cache_stats("persistent_file_digest").misses, 1);
        end_test_integrity_session(false);
    }

    #[test]
    fn persistent_index_fresh_process_rejects_same_size_corruption() {
        const ROOT_ENV: &str = "MATTOS_TEST_INTEGRITY_INDEX_ROOT";
        const EXPECTED_ENV: &str = "MATTOS_TEST_INTEGRITY_INDEX_EXPECTED";
        if let (Ok(root), Ok(expected)) = (std::env::var(ROOT_ENV), std::env::var(EXPECTED_ENV)) {
            let root = PathBuf::from(root);
            begin_test_integrity_session(&root);
            let inventory = output_inventory(&root, &[root.join("out/result")]).unwrap();
            assert_ne!(inventory_file_digest(&inventory, "result/file"), expected);
            assert_eq!(integrity_cache_stats("persistent_file_digest").misses, 1);
            end_test_integrity_session(false);
            return;
        }

        let root = tempdir().unwrap();
        let output = root.path().join("out/result");
        let file = output.join("file");
        fs::create_dir_all(&output).unwrap();
        fs::write(&file, "good").unwrap();
        let original_mtime =
            filetime::FileTime::from_last_modification_time(&fs::metadata(&file).unwrap());
        begin_test_integrity_session(root.path());
        let inventory = output_inventory(root.path(), &[output]).unwrap();
        let expected = inventory_file_digest(&inventory, "result/file");
        end_test_integrity_session(true);
        fs::write(&file, "evil").unwrap();
        filetime::set_file_mtime(&file, original_mtime).unwrap();

        let status = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "performance::tests::persistent_index_fresh_process_rejects_same_size_corruption",
                "--nocapture",
            ])
            .env(ROOT_ENV, root.path())
            .env(EXPECTED_ENV, expected)
            .status()
            .unwrap();
        assert!(status.success());
    }

    #[test]
    fn invalidated_changed_file_is_rehashed() {
        let root = tempdir().unwrap();
        let input = root.path().join("input");
        fs::write(&input, "one").unwrap();
        begin_test_integrity_cache();
        let first = digest_paths(root.path(), std::slice::from_ref(&input), false, "test").unwrap();
        fs::write(&input, "two").unwrap();
        invalidate_integrity_paths(root.path(), std::slice::from_ref(&input));
        let second =
            digest_paths(root.path(), std::slice::from_ref(&input), false, "test").unwrap();
        assert_ne!(first, second);
        assert_eq!(integrity_cache_stats("path_inventory").misses, 2);
        end_test_integrity_cache();
    }

    #[test]
    fn fresh_invocation_detects_corrupted_cached_output() {
        let root = tempdir().unwrap();
        let spec = StageSpec {
            id: "corruption".to_string(),
            source_inputs: Vec::new(),
            configuration_inputs: Vec::new(),
            tools: Vec::new(),
            dependencies: Vec::new(),
            outputs: vec![PathBuf::from("out/result")],
            recipe: "test".to_string(),
        };
        begin_test_integrity_cache();
        execute_cached_stage(
            root.path(),
            &spec,
            || Ok(()),
            || {
                fs::create_dir_all(root.path().join("out"))?;
                fs::write(root.path().join("out/result"), "good")?;
                Ok(())
            },
        )
        .unwrap();
        end_test_integrity_cache();
        fs::write(root.path().join("out/result"), "evil").unwrap();
        begin_test_integrity_cache();
        let result = execute_cached_stage(
            root.path(),
            &spec,
            || Ok(()),
            || bail!("corrupted output forced a cache miss"),
        );
        assert!(result.is_err());
        end_test_integrity_cache();
    }

    #[test]
    fn semantic_validation_runs_when_inventory_is_reused() {
        use std::cell::Cell;
        let root = tempdir().unwrap();
        let spec = StageSpec {
            id: "semantic".to_string(),
            source_inputs: Vec::new(),
            configuration_inputs: Vec::new(),
            tools: Vec::new(),
            dependencies: Vec::new(),
            outputs: vec![PathBuf::from("out/result")],
            recipe: "test".to_string(),
        };
        begin_test_integrity_cache();
        execute_cached_stage(
            root.path(),
            &spec,
            || Ok(()),
            || {
                fs::create_dir_all(root.path().join("out"))?;
                fs::write(root.path().join("out/result"), "good")?;
                Ok(())
            },
        )
        .unwrap();
        let semantic_runs = Cell::new(0);
        execute_cached_stage(
            root.path(),
            &spec,
            || {
                semantic_runs.set(semantic_runs.get() + 1);
                Ok(())
            },
            || bail!("unchanged output must hit"),
        )
        .unwrap();
        assert_eq!(semantic_runs.get(), 1);
        assert!(integrity_cache_stats("path_inventory").hits >= 1);
        end_test_integrity_cache();
    }

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
        use std::os::unix::fs::{symlink, PermissionsExt};
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
    fn cache_decision_matrix_uses_real_manifests_and_outputs() {
        use std::cell::Cell;

        let root = tempdir().unwrap();
        for stage in ["owner", "unrelated"] {
            fs::create_dir_all(root.path().join(format!("src/{stage}/docs"))).unwrap();
            fs::write(root.path().join(format!("src/{stage}/code")), "one").unwrap();
            fs::write(
                root.path().join(format!("src/{stage}/docs/README.md")),
                "one",
            )
            .unwrap();
            fs::write(root.path().join(format!("{stage}.config")), "one").unwrap();
        }
        let spec = |stage: &str, recipe: &str| StageSpec {
            id: stage.to_string(),
            source_inputs: vec![format!("src/{stage}").into()],
            configuration_inputs: vec![format!("{stage}.config").into()],
            tools: Vec::new(),
            dependencies: Vec::new(),
            outputs: vec![format!("out/{stage}").into()],
            recipe: recipe.to_string(),
        };
        let owner_runs = Cell::new(0usize);
        let unrelated_runs = Cell::new(0usize);
        let run = |spec: &StageSpec, runs: &Cell<usize>| {
            execute_cached_stage(
                root.path(),
                spec,
                || Ok(()),
                || {
                    runs.set(runs.get() + 1);
                    fs::create_dir_all(root.path().join("out"))?;
                    fs::write(
                        root.path().join(&spec.outputs[0]),
                        format!("{}:{}", spec.id, runs.get()),
                    )?;
                    Ok(())
                },
            )
        };
        let mut owner = spec("owner", "recipe-1");
        let unrelated = spec("unrelated", "recipe-1");

        run(&owner, &owner_runs).unwrap();
        run(&unrelated, &unrelated_runs).unwrap();
        run(&owner, &owner_runs).unwrap();
        run(&unrelated, &unrelated_runs).unwrap();
        assert_eq!((owner_runs.get(), unrelated_runs.get()), (1, 1));

        fs::write(root.path().join("src/owner/docs/README.md"), "two").unwrap();
        run(&owner, &owner_runs).unwrap();
        run(&unrelated, &unrelated_runs).unwrap();
        assert_eq!((owner_runs.get(), unrelated_runs.get()), (1, 1));

        fs::write(root.path().join("src/owner/code"), "two").unwrap();
        run(&owner, &owner_runs).unwrap();
        run(&unrelated, &unrelated_runs).unwrap();
        assert_eq!((owner_runs.get(), unrelated_runs.get()), (2, 1));

        fs::write(root.path().join("owner.config"), "two").unwrap();
        run(&owner, &owner_runs).unwrap();
        run(&unrelated, &unrelated_runs).unwrap();
        assert_eq!((owner_runs.get(), unrelated_runs.get()), (3, 1));

        owner.recipe = "recipe-2".to_string();
        run(&owner, &owner_runs).unwrap();
        run(&unrelated, &unrelated_runs).unwrap();
        assert_eq!((owner_runs.get(), unrelated_runs.get()), (4, 1));

        fs::remove_file(root.path().join("out/owner")).unwrap();
        run(&owner, &owner_runs).unwrap();
        run(&unrelated, &unrelated_runs).unwrap();
        assert_eq!((owner_runs.get(), unrelated_runs.get()), (5, 1));

        fs::write(root.path().join("out/owner"), "corrupt").unwrap();
        run(&owner, &owner_runs).unwrap();
        run(&unrelated, &unrelated_runs).unwrap();
        assert_eq!((owner_runs.get(), unrelated_runs.get()), (6, 1));
    }

    #[test]
    fn changed_input_summary_names_concrete_inputs() {
        let mut stored = StageInputDetails {
            recipe: "recipe-1".to_string(),
            ..StageInputDetails::default()
        };
        stored
            .source
            .insert("src/component".to_string(), "old".to_string());
        let mut current = stored.clone();
        current.recipe = "recipe-2".to_string();
        current
            .source
            .insert("src/component".to_string(), "new".to_string());
        current
            .configuration
            .insert("component/config.toml".to_string(), "digest".to_string());

        assert_eq!(
            changed_input_summary(&stored, &current),
            "recipe, source:src/component, configuration:component/config.toml"
        );
    }

    #[test]
    fn narrowed_manifest_migration_rejects_changed_removed_source_root() {
        let root = tempdir().unwrap();
        fs::create_dir_all(root.path().join("tree/subdir")).unwrap();
        fs::write(root.path().join("tree/subdir/input"), "old").unwrap();
        let old_spec = StageSpec {
            id: "migration".to_string(),
            source_inputs: vec!["tree".into()],
            configuration_inputs: Vec::new(),
            tools: Vec::new(),
            dependencies: Vec::new(),
            outputs: Vec::new(),
            recipe: format!("mattos-build-stage:migration:schema={STAGE_MANIFEST_SCHEMA_VERSION}"),
        };
        let old = compute_stage_evaluation(root.path(), &old_spec).unwrap();
        let manifest = StageManifest {
            schema_version: STAGE_MANIFEST_SCHEMA_VERSION,
            stage: "migration".to_string(),
            inputs: old.inputs,
            input_details: old.details,
            expected_outputs: Vec::new(),
            output_content_digest: String::new(),
        };
        fs::write(root.path().join("tree/subdir/input"), "new").unwrap();
        let new_spec = StageSpec {
            source_inputs: vec!["tree/subdir".into()],
            recipe: format!(
                "mattos-build-stage:migration:recipe=1:schema={STAGE_MANIFEST_SCHEMA_VERSION}"
            ),
            ..old_spec
        };
        let current = compute_stage_evaluation(root.path(), &new_spec).unwrap();

        assert!(!can_migrate_narrowed_manifest(root.path(), &current, &manifest).unwrap());
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
        use std::os::unix::fs::{symlink, PermissionsExt};
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
            crate::tool_identity::resolve_executable_from("fixture-cc", Some(&first_path)).unwrap(),
            crate::tool_identity::resolve_executable_from("fixture-cc", Some(&second_path))
                .unwrap()
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

    #[cfg(target_os = "linux")]
    fn accounting_fixture() -> &'static Path {
        use std::sync::OnceLock;

        static FIXTURE: OnceLock<PathBuf> = OnceLock::new();
        FIXTURE.get_or_init(|| {
            let destination = std::env::temp_dir().join(format!(
                "mattos-cpu-accounting-fixture-{}",
                std::process::id()
            ));
            let source = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/cpu_accounting_fixture.c");
            let status = Command::new("cc")
                .args(["-O2", "-std=c11"])
                .arg(&source)
                .args(["-o"])
                .arg(&destination)
                .status()
                .expect("C compiler is required for Linux CPU-accounting tests");
            assert!(status.success(), "failed to compile CPU accounting fixture");
            destination
        })
    }

    #[cfg(target_os = "linux")]
    fn accounted_fixture(mode: &str, milliseconds: u32) -> (ExitStatus, StageCpuUsage) {
        let mut child = Command::new(accounting_fixture())
            .args([mode, &milliseconds.to_string()])
            .spawn()
            .unwrap();
        let (status, usage) = wait_with_tree_cpu(&mut child).unwrap();
        (
            status,
            usage.expect("/proc zombie accounting must be available on Linux"),
        )
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn wait4_accounts_direct_sequential_and_parallel_waited_children() {
        let (_, direct) = accounted_fixture("direct", 150);
        let (_, one) = accounted_fixture("one", 150);
        let (_, sequential) = accounted_fixture("sequential", 150);
        let (_, parallel) = accounted_fixture("parallel", 150);
        let cpu = |usage: StageCpuUsage| (usage.user + usage.system).as_secs_f64();
        assert!(cpu(direct) >= 0.12, "direct CPU was not measured: {}", cpu(direct));
        assert!(cpu(one) >= 0.12, "one waited child was not measured: {}", cpu(one));
        assert!(
            cpu(sequential) >= 0.24 && cpu(parallel) >= 0.24,
            "two waited children were not cumulatively measured: sequential={} parallel={}",
            cpu(sequential), cpu(parallel)
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn wait4_accounts_nested_short_lived_and_failed_commands_before_reap() {
        let (_, nested) = accounted_fixture("nested", 150);
        let (status, failed) = accounted_fixture("fail", 150);
        assert_eq!(status.code(), Some(7));
        assert!((nested.user + nested.system) >= Duration::from_millis(240));
        assert!((failed.user + failed.system) >= Duration::from_millis(120));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn wait4_idle_command_is_near_zero_and_reaped() {
        let (status, usage) = accounted_fixture("idle", 100);
        assert!(status.success());
        assert!((usage.user + usage.system) < Duration::from_millis(100));
    }
}
