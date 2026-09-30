//! Per-stage memory containment and accounting.
//!
//! Each stage that runs its recipe gets a transient systemd user scope (a
//! cgroup) whose `memory.high` is the memory available when the stage starts.
//! Every command the stage runs moves itself into that cgroup before `exec`,
//! so when a stage outgrows the memory it was started with the kernel
//! reclaims and throttles that stage, instead of pushing the rest of the host
//! (and the user's desktop) into swap, and the scheduler sheds jobs as the
//! stage approaches that limit (`stage_pressure`).  The cgroup's
//! `memory.peak`, the stage's true aggregate peak, is recorded for diagnosis.
//!
//! Scopes need a systemd user manager with the memory controller delegated,
//! as on a normal desktop session.  Without one (CI, containers, root builds)
//! stages run uncontained and unmeasured, as before; set
//! `MATTOS_STAGE_CGROUPS=0` to opt out explicitly.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::ffi::CString;
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use crate::resources::PressureLevel;

pub(crate) const STAGE_MEMORY_HISTORY: &str = "out/reports/stage-memory.json";
const GIB: u64 = 1024 * 1024 * 1024;
/// A stage is never throttled below this, however little memory was free
/// when it started; the scheduler's admission remains the primary control.
const MINIMUM_MEMORY_HIGH_BYTES: u64 = GIB;

/// The stage's `memory.high`: three quarters of the memory available when it
/// started.  The remaining quarter is headroom for the desktop and the other
/// running stages; a limit of everything available let a stage at its limit
/// (LLVM) push idle desktop memory into swap through global reclaim.
pub(crate) fn stage_memory_high(available_memory_bytes: u64) -> u64 {
    (available_memory_bytes / 4 * 3).max(MINIMUM_MEMORY_HIGH_BYTES)
}

thread_local! {
    /// `cgroup.procs` of the running stage's cgroup, for `attach_command`.
    static ACTIVE_STAGE_CGROUP: RefCell<Option<CString>> = const { RefCell::new(None) };
}

static UNAVAILABLE_REPORTED: AtomicBool = AtomicBool::new(false);
/// Running stage cgroups, with the `memory.events` `high` count last seen.
static ACTIVE_CGROUPS: Mutex<BTreeMap<PathBuf, u64>> = Mutex::new(BTreeMap::new());
static SCOPE_COUNTER: AtomicUsize = AtomicUsize::new(0);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct StageMemoryRecord {
    /// `memory.peak` of the stage's cgroup: all of its processes together,
    /// page cache included.
    pub(crate) peak_bytes: u64,
    /// The child-job ceiling the stage ran under.  Under the shared
    /// jobserver this is the pool's ceiling, not the jobs the stage actually
    /// ran at once, so the peak cannot be divided into a per-job cost.
    pub(crate) child_jobs: usize,
}

/// A transient scope holding one stage's commands.  Dropping it ends the
/// scope once its processes have exited.
pub(crate) struct StageCgroup {
    holder: Child,
    directory: PathBuf,
    procs: CString,
}

impl StageCgroup {
    /// Creates a scope for `stage` limited (`memory.high`) to
    /// [`stage_memory_high`] of `available_memory_bytes`, or `None` when this
    /// host cannot provide one.
    pub(crate) fn create(stage: &str, available_memory_bytes: u64) -> Option<Self> {
        if cfg!(test) || std::env::var_os("MATTOS_STAGE_CGROUPS").is_some_and(|value| value == "0") {
            return None;
        }
        match Self::try_create(stage, stage_memory_high(available_memory_bytes)) {
            Ok(cgroup) => Some(cgroup),
            Err(error) => {
                if !UNAVAILABLE_REPORTED.swap(true, Ordering::Relaxed) {
                    println!("[build] per-stage memory containment unavailable: {error:#}");
                }
                None
            }
        }
    }

    fn try_create(stage: &str, memory_high: u64) -> Result<Self> {
        let unit = format!(
            "mattos-stage-{}-{}-{}",
            stage
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
                .collect::<String>(),
            std::process::id(),
            SCOPE_COUNTER.fetch_add(1, Ordering::Relaxed)
        );
        // `systemd-run --scope` moves itself into the new scope and then
        // execs the holder, so the holder's own cgroup is the scope.  The
        // parent-death signal survives that exec, so a crashed build cannot
        // leave the holder, its scope or the stage's commands behind (see
        // `SCOPE_HOLDER_SCRIPT`).
        let mut command = Command::new("systemd-run");
        command
            .args(["--user", "--scope", "--quiet", "--collect", "--unit", &unit])
            .arg(format!("--property=MemoryHigh={memory_high}"))
            .args(["sh", "-c", SCOPE_HOLDER_SCRIPT])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        // SAFETY: prctl is async-signal-safe.
        unsafe {
            command.pre_exec(|| {
                libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
                Ok(())
            });
        }
        let mut holder = command.spawn().context("systemd-run is not available")?;
        let deadline = Instant::now() + Duration::from_secs(10);
        let relative = loop {
            if let Some(status) = holder.try_wait()? {
                anyhow::bail!("systemd-run --user --scope exited with {status}");
            }
            let cgroup = fs::read_to_string(format!("/proc/{}/cgroup", holder.id())).unwrap_or_default();
            if let Some(path) = cgroup
                .lines()
                .find_map(|line| line.strip_prefix("0::"))
                .filter(|path| path.ends_with(&format!("/{unit}.scope")))
            {
                break path.to_string();
            }
            if Instant::now() >= deadline {
                let _ = holder.kill();
                let _ = holder.wait();
                anyhow::bail!("the {unit} scope did not appear");
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        let directory = Path::new("/sys/fs/cgroup").join(relative.trim_start_matches('/'));
        let procs = CString::new(directory.join("cgroup.procs").as_os_str().as_bytes())?;
        let throttled = throttle_count(&directory).unwrap_or(0);
        ACTIVE_CGROUPS.lock().unwrap_or_else(|poison| poison.into_inner()).insert(directory.clone(), throttled);
        Ok(Self { holder, directory, procs })
    }

    /// Runs `action` with every command it spawns through
    /// `attach_command` placed in this cgroup.
    pub(crate) fn enter<T>(cgroup: Option<&Self>, action: impl FnOnce() -> T) -> T {
        let previous = ACTIVE_STAGE_CGROUP.with(|slot| slot.replace(cgroup.map(|cgroup| cgroup.procs.clone())));
        let result = action();
        ACTIVE_STAGE_CGROUP.with(|slot| *slot.borrow_mut() = previous);
        result
    }

    pub(crate) fn peak_bytes(&self) -> Option<u64> {
        fs::read_to_string(self.directory.join("memory.peak")).ok()?.trim().parse().ok()
    }
}

impl Drop for StageCgroup {
    fn drop(&mut self) {
        ACTIVE_CGROUPS.lock().unwrap_or_else(|poison| poison.into_inner()).remove(&self.directory);
        let _ = self.holder.kill();
        let _ = self.holder.wait();
    }
}

/// The scope's holder process.  When the build dies, the parent-death
/// signal (SIGTERM) makes it kill the whole scope through `cgroup.kill`, so a
/// crashed or killed build cannot leave its `make` and compiler jobs running.
/// Normal teardown kills the holder with SIGKILL after the stage's commands
/// have exited.
const SCOPE_HOLDER_SCRIPT: &str = "trap 'echo 1 > \"/sys/fs/cgroup$(cut -d: -f3 /proc/self/cgroup)/cgroup.kill\"' TERM; sleep infinity & wait";

/// How many times the kernel has throttled the cgroup at its `memory.high`.
fn throttle_count(directory: &Path) -> Option<u64> {
    let events = fs::read_to_string(directory.join("memory.events")).ok()?;
    events.lines().find_map(|line| line.strip_prefix("high ")?.trim().parse().ok())
}

fn read_bytes(path: &Path) -> Option<u64> {
    fs::read_to_string(path).ok()?.trim().parse().ok()
}

/// Memory pressure inside the running stages' own cgroups.  A stage that was
/// throttled at its `memory.high` since the last sample is Critical: the
/// kernel is already reclaiming its pages, towards swap, so no new compile
/// job may start until it recovers.  A stage within 10% of its `memory.high`
/// is Constrained, which halves the job tokens before reclaim begins.
pub(crate) fn stage_pressure() -> PressureLevel {
    let mut active = ACTIVE_CGROUPS.lock().unwrap_or_else(|poison| poison.into_inner());
    let mut level = PressureLevel::Healthy;
    for (directory, seen) in active.iter_mut() {
        let throttled = throttle_count(directory).unwrap_or(*seen);
        let current = read_bytes(&directory.join("memory.current"));
        let high = read_bytes(&directory.join("memory.high"));
        level = level.max(cgroup_pressure(*seen, throttled, current, high));
        *seen = throttled;
    }
    level
}

fn cgroup_pressure(seen_throttles: u64, throttles: u64, current: Option<u64>, high: Option<u64>) -> PressureLevel {
    if throttles > seen_throttles {
        PressureLevel::Critical
    } else if current.zip(high).is_some_and(|(current, high)| current >= high / 10 * 9) {
        PressureLevel::Constrained
    } else {
        PressureLevel::Healthy
    }
}

/// Makes `command` join the running stage's cgroup, if it has one.  The
/// child writes `0` (itself) to `cgroup.procs` before exec; failing to move
/// only leaves it uncontained, never fails the command.
pub(crate) fn attach_command(command: &mut Command) {
    let Some(procs) = ACTIVE_STAGE_CGROUP.with(|slot| slot.borrow().clone()) else {
        return;
    };
    // SAFETY: the closure only calls the async-signal-safe open, write and
    // close on memory allocated before fork.
    unsafe {
        command.pre_exec(move || {
            let fd = libc::open(procs.as_ptr(), libc::O_WRONLY | libc::O_CLOEXEC);
            if fd >= 0 {
                libc::write(fd, b"0".as_ptr().cast(), 1);
                libc::close(fd);
            }
            Ok(())
        });
    }
}

pub(crate) fn read_history(repo_root: &Path) -> BTreeMap<String, StageMemoryRecord> {
    fs::read_to_string(repo_root.join(STAGE_MEMORY_HISTORY))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub(crate) fn record(repo_root: &Path, stage: &str, record: StageMemoryRecord) -> Result<()> {
    let mut history = read_history(repo_root);
    history.insert(stage.to_string(), record);
    let path = repo_root.join(STAGE_MEMORY_HISTORY);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    crate::performance::atomic_write(&path, serde_json::to_string_pretty(&history)?.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stage_memory_high_leaves_a_quarter_of_available_memory_as_headroom() {
        assert_eq!(stage_memory_high(8 * GIB), 6 * GIB);
        assert_eq!(stage_memory_high(GIB / 2), MINIMUM_MEMORY_HIGH_BYTES);
    }

    #[test]
    fn stage_cgroups_shed_jobs_before_and_while_they_are_throttled() {
        let high = Some(10 * GIB);
        assert_eq!(cgroup_pressure(5, 5, Some(4 * GIB), high), PressureLevel::Healthy);
        assert_eq!(cgroup_pressure(5, 5, Some(9 * GIB), high), PressureLevel::Constrained);
        assert_eq!(cgroup_pressure(5, 6, Some(4 * GIB), high), PressureLevel::Critical);
        // An unreadable cgroup reports nothing rather than pressure.
        assert_eq!(cgroup_pressure(5, 5, None, None), PressureLevel::Healthy);
    }

    #[test]
    fn history_round_trips_and_tolerates_corruption() {
        let temp = tempfile::tempdir().unwrap();
        record(temp.path(), "llvm", StageMemoryRecord { peak_bytes: 13 * GIB, child_jobs: 8 }).unwrap();
        record(temp.path(), "zlib", StageMemoryRecord { peak_bytes: GIB, child_jobs: 4 }).unwrap();
        record(temp.path(), "llvm", StageMemoryRecord { peak_bytes: 7 * GIB, child_jobs: 12 }).unwrap();
        let history = read_history(temp.path());
        assert_eq!(history.len(), 2);
        assert_eq!(history["llvm"], StageMemoryRecord { peak_bytes: 7 * GIB, child_jobs: 12 });
        // A missing or corrupt history is empty, not an error.
        fs::write(temp.path().join(STAGE_MEMORY_HISTORY), "not json").unwrap();
        assert!(read_history(temp.path()).is_empty());
    }

    /// On a host with a systemd user manager, a stage's commands really run
    /// in its scope and the scope reports their peak.
    #[test]
    fn stage_commands_join_the_stage_scope() {
        let Ok(cgroup) = StageCgroup::try_create("scope-test", 2 * GIB) else {
            eprintln!("skipped: no systemd user manager");
            return;
        };
        assert_eq!(
            fs::read_to_string(cgroup.directory.join("memory.high")).unwrap().trim(),
            (2 * GIB).to_string()
        );
        let output = StageCgroup::enter(Some(&cgroup), || {
            let mut command = Command::new("cat");
            command.arg("/proc/self/cgroup");
            attach_command(&mut command);
            command.output().unwrap()
        });
        let own = String::from_utf8(output.stdout).unwrap();
        assert!(own.trim().ends_with(".scope") && own.contains("mattos-stage-scope-test-"), "{own}");
        assert!(cgroup.peak_bytes().is_some_and(|peak| peak > 0));
        // Outside `enter`, commands stay where they are.
        let mut command = Command::new("cat");
        command.arg("/proc/self/cgroup");
        attach_command(&mut command);
        let outside = String::from_utf8(command.output().unwrap().stdout).unwrap();
        assert!(!outside.contains("mattos-stage-scope-test-"));
    }
}
