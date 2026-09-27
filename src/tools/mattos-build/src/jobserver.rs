//! GNU make jobserver shared by every build stage of one `mattos-build`
//! invocation.
//!
//! A stage's parallelism used to be fixed when it started: a large stage that
//! launched while other stages held the CPU budget kept `-j1`/`-j2` for its
//! whole run, even after every other stage finished.  With one shared token
//! pool, GNU Make (4.4+), Ninja (1.13+, and therefore CMake and Meson) and
//! Cargo acquire tokens dynamically, so a running stage grows into CPUs as
//! soon as they are released.  The scheduler adjusts the number of tokens in
//! circulation from live memory pressure.
//!
//! Protocol: a named FIFO holding one byte per available token.  Each client
//! also owns one implicit token, so the pool target excludes one CPU per
//! running stage.

use anyhow::{Context, Result};
use std::fs::{File, OpenOptions};
use std::io::{ErrorKind, Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
#[cfg(test)]
use std::cell::RefCell;
#[cfg(not(test))]
use std::sync::Mutex;

pub(crate) struct Jobserver {
    path: PathBuf,
    fifo: File,
    /// Upper bound on concurrent jobs, advertised to clients as `-jN`.
    ceiling: usize,
    /// Tokens this process has placed into circulation (in the FIFO or held
    /// by clients).
    outstanding: usize,
}

// Stage worker threads inherit the invocation's jobserver; unit tests keep
// the published state per thread so parallel tests cannot observe it.
#[cfg(not(test))]
static ACTIVE: Mutex<Option<(PathBuf, usize)>> = Mutex::new(None);
#[cfg(test)]
thread_local! {
    static ACTIVE: RefCell<Option<(PathBuf, usize)>> = const { RefCell::new(None) };
}

#[cfg(not(test))]
fn with_active<R>(action: impl FnOnce(&mut Option<(PathBuf, usize)>) -> R) -> R {
    action(&mut ACTIVE.lock().expect("jobserver mutex poisoned"))
}

#[cfg(test)]
fn with_active<R>(action: impl FnOnce(&mut Option<(PathBuf, usize)>) -> R) -> R {
    ACTIVE.with(|slot| action(&mut slot.borrow_mut()))
}

impl Jobserver {
    pub(crate) fn create(directory: &Path, ceiling: usize) -> Result<Self> {
        // Clients open the FIFO from their own working directories (build
        // trees), so the advertised path must be absolute.
        let directory = &std::path::absolute(directory)
            .with_context(|| format!("failed to resolve {}", directory.display()))?;
        std::fs::create_dir_all(directory)?;
        remove_stale_fifos(directory);
        let path = directory.join(format!("mattos-jobserver-{}.fifo", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let c_path = std::ffi::CString::new(path.as_os_str().as_encoded_bytes())
            .context("jobserver path contains NUL")?;
        // SAFETY: `c_path` is a valid NUL-terminated path for the call.
        if unsafe { libc::mkfifo(c_path.as_ptr(), 0o600) } != 0 {
            return Err(std::io::Error::last_os_error())
                .with_context(|| format!("failed to create jobserver FIFO {}", path.display()));
        }
        // Read-write keeps the FIFO open (never EOF for clients) and lets the
        // controller both add and reclaim tokens without blocking.
        let fifo = OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC)
            .open(&path)
            .with_context(|| format!("failed to open jobserver FIFO {}", path.display()))?;
        Ok(Self {
            path,
            fifo,
            ceiling: ceiling.max(1),
            outstanding: 0,
        })
    }

    /// Publishes this jobserver to child processes of the current invocation.
    pub(crate) fn activate(&self) {
        with_active(|active| *active = Some((self.path.clone(), self.ceiling)));
    }

    /// Moves the number of circulating tokens toward `target`.  Tokens held by
    /// clients cannot be revoked; they are reclaimed as they are returned.
    pub(crate) fn rebalance(&mut self, target: usize) -> Result<()> {
        let target = target.min(self.ceiling);
        if target > self.outstanding {
            let tokens = vec![b'+'; target - self.outstanding];
            let mut written = 0;
            while written < tokens.len() {
                match self.fifo.write(&tokens[written..]) {
                    Ok(0) => break,
                    Ok(count) => written += count,
                    Err(error) if error.kind() == ErrorKind::Interrupted => continue,
                    Err(error) if error.kind() == ErrorKind::WouldBlock => break,
                    Err(error) => return Err(error).context("failed to add jobserver tokens"),
                }
            }
            self.outstanding += written;
        } else if target < self.outstanding {
            let mut buffer = vec![0u8; self.outstanding - target];
            loop {
                match self.fifo.read(&mut buffer) {
                    Ok(count) => {
                        self.outstanding -= count;
                        break;
                    }
                    Err(error) if error.kind() == ErrorKind::Interrupted => continue,
                    Err(error) if error.kind() == ErrorKind::WouldBlock => break,
                    Err(error) => return Err(error).context("failed to reclaim jobserver tokens"),
                }
            }
        }
        Ok(())
    }

    /// Tokens currently held by running jobs (in circulation but not
    /// waiting in the FIFO).
    pub(crate) fn held(&self) -> usize {
        use std::os::fd::AsRawFd;
        let mut queued: libc::c_int = 0;
        // SAFETY: FIONREAD writes the number of readable bytes into `queued`.
        let result = unsafe { libc::ioctl(self.fifo.as_raw_fd(), libc::FIONREAD, &mut queued) };
        if result != 0 {
            return 0;
        }
        self.outstanding.saturating_sub(usize::try_from(queued).unwrap_or(0))
    }

    #[cfg(test)]
    pub(crate) fn outstanding(&self) -> usize {
        self.outstanding
    }
}

impl Drop for Jobserver {
    fn drop(&mut self) {
        with_active(|active| {
            if active.as_ref().is_some_and(|(path, _)| *path == self.path) {
                *active = None;
            }
        });
        let _ = std::fs::remove_file(&self.path);
    }
}

/// An interrupted invocation cannot run `Drop`; remove FIFOs whose owning
/// process no longer exists.
fn remove_stale_fifos(directory: &Path) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(pid) = name
            .to_str()
            .and_then(|name| name.strip_prefix("mattos-jobserver-"))
            .and_then(|rest| rest.strip_suffix(".fifo"))
            .and_then(|pid| pid.parse::<u32>().ok())
        else {
            continue;
        };
        if pid != std::process::id() && !Path::new("/proc").join(pid.to_string()).exists() {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// `MAKEFLAGS` for a child build when a shared jobserver is active.
pub(crate) fn makeflags() -> Option<String> {
    with_active(|active| {
        active
            .as_ref()
            .map(|(path, ceiling)| format!("-j{ceiling} --jobserver-auth=fifo:{}", path.display()))
    })
}

/// The job ceiling of the active jobserver, if any.
pub(crate) fn ceiling() -> Option<usize> {
    with_active(|active| active.as_ref().map(|(_, ceiling)| *ceiling))
}

/// Pool size for the scheduler's current state: every CPU not already covered
/// by a building stage's implicit job slot while memory is healthy, half of it
/// under constrained pressure, none when critical.
pub(crate) fn target_tokens(cpu_budget: usize, building_stages: usize, pressure: &str) -> usize {
    let idle = cpu_budget.saturating_sub(building_stages);
    match pressure {
        "healthy" => idle,
        "constrained" => idle / 2,
        _ => 0,
    }
}

/// Additional jobs are admitted only while their estimated memory fits the
/// current build-memory headroom; jobs already holding tokens are running and
/// their memory is already accounted for in that headroom.
pub(crate) fn memory_capped_target(
    cpu_target: usize,
    held_tokens: usize,
    build_memory_bytes: u64,
    per_job_memory_bytes: u64,
) -> usize {
    let affordable = build_memory_bytes / per_job_memory_bytes.max(1);
    cpu_target.min(held_tokens.saturating_add(usize::try_from(affordable).unwrap_or(usize::MAX)))
}

/// Memory pressure readings flap between levels within seconds.  Shrink
/// immediately so pressure is relieved, but grow by at most one token per
/// scheduler pass so a transient healthy sample cannot flood the host.
pub(crate) fn smoothed_target(current: usize, desired: usize) -> usize {
    if desired <= current {
        desired
    } else {
        current + 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_added_and_reclaimed_through_the_fifo() {
        let temporary = tempfile::tempdir().unwrap();
        let mut server = Jobserver::create(temporary.path(), 8).unwrap();
        server.rebalance(6).unwrap();
        assert_eq!(server.outstanding(), 6);
        // A client takes two tokens.
        let mut client = OpenOptions::new().read(true).open(&server.path).unwrap();
        let mut held = [0u8; 2];
        client.read_exact(&mut held).unwrap();
        // Shrinking reclaims only the four tokens still in the FIFO.
        server.rebalance(0).unwrap();
        assert_eq!(server.outstanding(), 2);
        // Returned tokens are reclaimed on the next rebalance.
        let mut writer = OpenOptions::new().write(true).open(&server.path).unwrap();
        writer.write_all(&held).unwrap();
        server.rebalance(0).unwrap();
        assert_eq!(server.outstanding(), 0);
        // The ceiling bounds growth.
        server.rebalance(100).unwrap();
        assert_eq!(server.outstanding(), 8);
    }

    #[test]
    fn stale_fifos_from_dead_processes_are_removed() {
        let temporary = tempfile::tempdir().unwrap();
        let stale = temporary.path().join("mattos-jobserver-999999999.fifo");
        let unrelated = temporary.path().join("other.fifo");
        std::fs::write(&stale, "").unwrap();
        std::fs::write(&unrelated, "").unwrap();
        let _server = Jobserver::create(temporary.path(), 2).unwrap();
        assert!(!stale.exists());
        assert!(unrelated.exists());
    }

    #[test]
    fn makeflags_advertise_the_fifo_while_active() {
        let temporary = tempfile::tempdir().unwrap();
        let server = Jobserver::create(temporary.path(), 4).unwrap();
        server.activate();
        let flags = makeflags().unwrap();
        assert!(flags.starts_with("-j4 --jobserver-auth=fifo:"));
        assert!(flags.ends_with(".fifo"));
        let advertised = flags.split_once("fifo:").unwrap().1;
        assert!(Path::new(advertised).is_absolute(), "{advertised}");
        drop(server);
        assert!(makeflags().is_none());
    }

    #[test]
    fn pool_is_capped_by_memory_headroom() {
        const GIB: u64 = 1024 * 1024 * 1024;
        // Plenty of memory: CPU-bound.
        assert_eq!(memory_capped_target(10, 0, 32 * GIB, GIB), 10);
        // 3 GiB of headroom at 1 GiB per job: three more jobs beyond the held.
        assert_eq!(memory_capped_target(10, 4, 3 * GIB, GIB), 7);
        // No headroom: only jobs already running keep their tokens.
        assert_eq!(memory_capped_target(10, 4, 0, GIB), 4);
    }

    #[test]
    fn held_tokens_exclude_those_waiting_in_the_fifo() {
        let temporary = tempfile::tempdir().unwrap();
        let mut server = Jobserver::create(temporary.path(), 8).unwrap();
        server.rebalance(5).unwrap();
        assert_eq!(server.held(), 0);
        let mut client = OpenOptions::new().read(true).open(&server.path).unwrap();
        let mut taken = [0u8; 3];
        client.read_exact(&mut taken).unwrap();
        assert_eq!(server.held(), 3);
    }

    #[test]
    fn pool_shrinks_immediately_and_grows_gradually() {
        assert_eq!(smoothed_target(10, 0), 0);
        assert_eq!(smoothed_target(10, 4), 4);
        assert_eq!(smoothed_target(0, 10), 1);
        let mut current = 0;
        for _ in 0..3 {
            current = smoothed_target(current, 10);
        }
        assert_eq!(current, 3);
        assert_eq!(smoothed_target(10, 10), 10);
    }

    #[test]
    fn pool_target_follows_idle_cpus_and_memory_pressure() {
        assert_eq!(target_tokens(12, 3, "healthy"), 9);
        assert_eq!(target_tokens(12, 3, "constrained"), 4);
        assert_eq!(target_tokens(12, 3, "critical"), 0);
        assert_eq!(target_tokens(4, 6, "healthy"), 0);
    }
}
