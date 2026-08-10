//! Host resource discovery and deterministic launch-time build budgeting.
//!
//! This module deliberately separates observation from policy.  `discover()`
//! reads the host once at scheduler startup; tests use `ResourceSnapshot`
//! values directly.  A fixed snapshot always yields the same `ResourceBudget`.

use std::fs;
use std::io;
use std::time::{Duration, Instant};

const MIB: u64 = 1024 * 1024;
const GIB: u64 = 1024 * MIB;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ResourceSnapshot {
    pub(crate) logical_cpus: usize,
    pub(crate) cpuset_cpus: Option<usize>,
    pub(crate) cpu_quota_cpus: Option<usize>,
    pub(crate) physical_memory_bytes: u64,
    pub(crate) available_memory_bytes: u64,
    pub(crate) cgroup_memory_limit_bytes: Option<u64>,
    pub(crate) cgroup_memory_current_bytes: Option<u64>,
    pub(crate) swap_total_bytes: u64,
    pub(crate) swap_free_bytes: u64,
    /// Cumulative kernel counters. They are reported for diagnostics but are
    /// not treated as a "recent" rate from a single startup snapshot.
    pub(crate) swap_in_pages: u64,
    pub(crate) swap_out_pages: u64,
    pub(crate) psi_memory_some_avg10: Option<f64>,
    pub(crate) reserved_memory_bytes: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ResourceBudget {
    pub(crate) cpu_tokens: usize,
    /// Memory that new build actions may collectively reserve.  The configured
    /// headroom has already been removed from the observed available memory.
    pub(crate) build_memory_bytes: u64,
    pub(crate) reserved_memory_bytes: u64,
    pub(crate) available_memory_bytes: u64,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum PressureLevel {
    Healthy,
    Constrained,
    Critical,
}

impl PressureLevel {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Healthy => "healthy",
            Self::Constrained => "constrained",
            Self::Critical => "critical",
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ResourceEnvelope {
    pub(crate) budget: ResourceBudget,
    pub(crate) pressure: PressureLevel,
    pub(crate) swap_in_pages_per_second: f64,
    pub(crate) swap_out_pages_per_second: f64,
    pub(crate) psi_memory_some_avg10: Option<f64>,
    pub(crate) cgroup_memory_current_bytes: Option<u64>,
}

/// Stateful but deterministic pressure classifier.  Pressure increases
/// immediately; recovery requires two consecutive lower-pressure samples.
pub(crate) struct PressureTracker {
    previous: Option<ResourceSnapshot>,
    level: PressureLevel,
    recovery_samples: u8,
}

impl PressureTracker {
    pub(crate) fn new() -> Self {
        Self {
            previous: None,
            level: PressureLevel::Healthy,
            recovery_samples: 0,
        }
    }

    pub(crate) fn observe(
        &mut self,
        snapshot: ResourceSnapshot,
        elapsed: Duration,
    ) -> ResourceEnvelope {
        let seconds = elapsed.as_secs_f64().max(0.001);
        let (swap_in_pages_per_second, swap_out_pages_per_second) =
            self.previous.as_ref().map_or((0.0, 0.0), |previous| {
                (
                    snapshot
                        .swap_in_pages
                        .saturating_sub(previous.swap_in_pages) as f64
                        / seconds,
                    snapshot
                        .swap_out_pages
                        .saturating_sub(previous.swap_out_pages) as f64
                        / seconds,
                )
            });
        let budget = snapshot.budget();
        let psi_memory_some_avg10 = snapshot.psi_memory_some_avg10;
        let candidate = pressure_candidate(
            &budget,
            swap_in_pages_per_second,
            swap_out_pages_per_second,
            psi_memory_some_avg10,
        );
        if candidate > self.level {
            self.level = candidate;
            self.recovery_samples = 0;
        } else if candidate < self.level {
            self.recovery_samples += 1;
            if self.recovery_samples >= 2 {
                self.level = candidate;
                self.recovery_samples = 0;
            }
        } else {
            self.recovery_samples = 0;
        }
        self.previous = Some(snapshot.clone());
        ResourceEnvelope {
            budget,
            pressure: self.level,
            swap_in_pages_per_second,
            swap_out_pages_per_second,
            psi_memory_some_avg10,
            cgroup_memory_current_bytes: snapshot.cgroup_memory_current_bytes,
        }
    }
}

fn pressure_candidate(
    budget: &ResourceBudget,
    swap_in_rate: f64,
    swap_out_rate: f64,
    psi_some_avg10: Option<f64>,
) -> PressureLevel {
    if budget.build_memory_bytes == 0
        || swap_out_rate >= 8.0
        || psi_some_avg10.is_some_and(|value| value >= 0.20)
    {
        PressureLevel::Critical
    } else if budget.available_memory_bytes <= budget.reserved_memory_bytes.saturating_mul(2)
        || swap_in_rate > 0.0
        || swap_out_rate > 0.0
        || psi_some_avg10.is_some_and(|value| value >= 0.05)
    {
        PressureLevel::Constrained
    } else {
        PressureLevel::Healthy
    }
}

pub(crate) trait RuntimeResourceSampler {
    fn sample(&mut self) -> ResourceEnvelope;
}

pub(crate) struct HostResourceSampler {
    tracker: PressureTracker,
    last_sample: Instant,
}

impl HostResourceSampler {
    pub(crate) fn new() -> Self {
        Self {
            tracker: PressureTracker::new(),
            last_sample: Instant::now(),
        }
    }
}

impl RuntimeResourceSampler for HostResourceSampler {
    fn sample(&mut self) -> ResourceEnvelope {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_sample);
        self.last_sample = now;
        self.tracker.observe(discover(), elapsed)
    }
}

impl ResourceSnapshot {
    pub(crate) fn effective_cpu_count(&self) -> usize {
        [
            Some(self.logical_cpus),
            self.cpuset_cpus,
            self.cpu_quota_cpus,
        ]
        .into_iter()
        .flatten()
        .min()
        .unwrap_or(1)
        .max(1)
    }

    #[cfg(test)]
    pub(crate) fn effective_memory_limit_bytes(&self) -> u64 {
        self.cgroup_memory_limit_bytes
            .map(|limit| limit.min(self.physical_memory_bytes))
            .unwrap_or(self.physical_memory_bytes)
    }

    pub(crate) fn effective_available_memory_bytes(&self) -> u64 {
        let cgroup_available = self
            .cgroup_memory_limit_bytes
            .zip(self.cgroup_memory_current_bytes)
            .map(|(limit, current)| limit.saturating_sub(current));
        cgroup_available
            .map(|available| available.min(self.available_memory_bytes))
            .unwrap_or(self.available_memory_bytes)
    }

    pub(crate) fn budget(&self) -> ResourceBudget {
        ResourceBudget {
            cpu_tokens: self.effective_cpu_count(),
            build_memory_bytes: self
                .effective_available_memory_bytes()
                .saturating_sub(self.reserved_memory_bytes),
            reserved_memory_bytes: self.reserved_memory_bytes,
            available_memory_bytes: self.effective_available_memory_bytes(),
        }
    }
}

pub(crate) trait ResourceReader {
    fn read(&self, path: &str) -> io::Result<String>;
}

struct HostReader;

impl ResourceReader for HostReader {
    fn read(&self, path: &str) -> io::Result<String> {
        fs::read_to_string(path)
    }
}

pub(crate) fn discover() -> ResourceSnapshot {
    let logical_cpus = std::thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(1);
    discover_with(
        &HostReader,
        logical_cpus,
        reserved_memory_from_environment(),
    )
}

pub(crate) fn sample_now() -> ResourceEnvelope {
    PressureTracker::new().observe(discover(), Duration::from_secs(1))
}

pub(crate) fn discover_with(
    reader: &impl ResourceReader,
    logical_cpus: usize,
    reserved_memory_override: Option<u64>,
) -> ResourceSnapshot {
    let meminfo = reader.read("/proc/meminfo").unwrap_or_default();
    let vmstat = reader.read("/proc/vmstat").unwrap_or_default();
    let psi_memory = reader.read("/proc/pressure/memory").unwrap_or_default();
    let physical_memory_bytes = meminfo_value(&meminfo, "MemTotal").unwrap_or(0);
    let available_memory_bytes = meminfo_value(&meminfo, "MemAvailable")
        .or_else(|| meminfo_value(&meminfo, "MemFree"))
        .unwrap_or(0);
    let swap_total_bytes = meminfo_value(&meminfo, "SwapTotal").unwrap_or(0);
    let swap_free_bytes = meminfo_value(&meminfo, "SwapFree").unwrap_or(0);
    let cpuset_cpus = read_cgroup_first(reader, None, "cpuset.cpus.effective")
        .or_else(|| read_cgroup_first(reader, None, "cpuset.cpus"))
        .or_else(|| read_cgroup_first(reader, Some("cpuset"), "cpuset.cpus"))
        .and_then(|value| parse_cpuset(&value));
    let cpu_quota_cpus = read_cgroup_first(reader, None, "cpu.max")
        .and_then(|value| parse_cpu_max(&value))
        .or_else(|| {
            let quota = read_cgroup_first(reader, Some("cpu"), "cpu.cfs_quota_us")?;
            let period = read_cgroup_first(reader, Some("cpu"), "cpu.cfs_period_us")?;
            parse_v1_cpu_quota(&quota, &period)
        });
    let cgroup_memory_limit_bytes = read_cgroup_first(reader, None, "memory.max")
        .or_else(|| read_cgroup_first(reader, Some("memory"), "memory.limit_in_bytes"))
        .and_then(|value| parse_memory_limit(&value, physical_memory_bytes));
    let cgroup_memory_current_bytes = read_cgroup_first(reader, None, "memory.current")
        .or_else(|| read_cgroup_first(reader, Some("memory"), "memory.usage_in_bytes"))
        .and_then(|value| value.trim().parse().ok());
    let reserved_memory_bytes = reserved_memory_override.unwrap_or_else(|| {
        default_reserved_memory(physical_memory_bytes, cgroup_memory_limit_bytes)
    });
    ResourceSnapshot {
        logical_cpus: logical_cpus.max(1),
        cpuset_cpus,
        cpu_quota_cpus,
        physical_memory_bytes,
        available_memory_bytes,
        cgroup_memory_limit_bytes,
        cgroup_memory_current_bytes,
        swap_total_bytes,
        swap_free_bytes,
        swap_in_pages: vmstat_value(&vmstat, "pswpin").unwrap_or(0),
        swap_out_pages: vmstat_value(&vmstat, "pswpout").unwrap_or(0),
        psi_memory_some_avg10: psi_some_avg10(&psi_memory),
        reserved_memory_bytes,
    }
}

fn psi_some_avg10(contents: &str) -> Option<f64> {
    let line = contents.lines().find(|line| line.starts_with("some "))?;
    line.split_whitespace()
        .find_map(|field| field.strip_prefix("avg10=")?.parse().ok())
}

fn read_cgroup_first(
    reader: &impl ResourceReader,
    controller: Option<&str>,
    file: &str,
) -> Option<String> {
    let membership = reader.read("/proc/self/cgroup").ok();
    let relative = membership
        .as_deref()
        .and_then(|contents| cgroup_relative_path(contents, controller));
    let mut paths = Vec::new();
    if let Some(relative) = relative {
        let mount = controller.map_or("/sys/fs/cgroup".to_string(), |value| {
            format!("/sys/fs/cgroup/{value}")
        });
        paths.push(format!("{mount}/{}/{}", relative.trim_matches('/'), file));
    }
    // This fallback supports flat cgroup mounts and older container setups.
    let mount = controller.map_or("/sys/fs/cgroup".to_string(), |value| {
        format!("/sys/fs/cgroup/{value}")
    });
    paths.push(format!("{mount}/{file}"));
    paths.into_iter().find_map(|path| reader.read(&path).ok())
}

fn cgroup_relative_path<'a>(contents: &'a str, controller: Option<&str>) -> Option<&'a str> {
    contents.lines().find_map(|line| {
        let mut fields = line.splitn(3, ':');
        let hierarchy = fields.next()?;
        let controllers = fields.next()?;
        let path = fields.next()?;
        match controller {
            None if hierarchy == "0" && controllers.is_empty() => Some(path),
            Some(required) if controllers.split(',').any(|value| value == required) => Some(path),
            _ => None,
        }
    })
}

fn meminfo_value(contents: &str, key: &str) -> Option<u64> {
    contents.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        (name == key).then(|| {
            value
                .split_whitespace()
                .next()?
                .parse::<u64>()
                .ok()
                .map(|v| v * 1024)
        })?
    })
}

fn vmstat_value(contents: &str, key: &str) -> Option<u64> {
    contents.lines().find_map(|line| {
        let mut fields = line.split_whitespace();
        (fields.next()? == key).then(|| fields.next()?.parse().ok())?
    })
}

fn parse_cpuset(value: &str) -> Option<usize> {
    let mut count = 0usize;
    for span in value.trim().split(',').filter(|span| !span.is_empty()) {
        let (start, end) = span
            .split_once('-')
            .map_or_else(|| Some((span, span)), |pair| Some(pair))?;
        let start = start.parse::<usize>().ok()?;
        let end = end.parse::<usize>().ok()?;
        if end < start {
            return None;
        }
        count = count.checked_add(end - start + 1)?;
    }
    (count > 0).then_some(count)
}

fn parse_cpu_max(value: &str) -> Option<usize> {
    let mut fields = value.split_whitespace();
    let quota = fields.next()?;
    let period = fields.next()?.parse::<u64>().ok()?;
    if quota == "max" {
        return None;
    }
    let quota = quota.parse::<u64>().ok()?;
    Some(((quota + period - 1) / period).max(1) as usize)
}

fn parse_v1_cpu_quota(quota: &str, period: &str) -> Option<usize> {
    let quota = quota.trim().parse::<i64>().ok()?;
    let period = period.trim().parse::<u64>().ok()?;
    (quota >= 0).then(|| ((quota as u64 + period - 1) / period).max(1) as usize)
}

fn parse_memory_limit(value: &str, physical: u64) -> Option<u64> {
    let value = value.trim();
    if value == "max" {
        return None;
    }
    let limit = value.parse::<u64>().ok()?;
    // v1 reports a very large sentinel for an unlimited controller.
    (physical == 0 || limit < physical.saturating_mul(16)).then_some(limit)
}

fn default_reserved_memory(physical: u64, cgroup_limit: Option<u64>) -> u64 {
    let effective = cgroup_limit
        .map(|limit| limit.min(physical))
        .unwrap_or(physical);
    let baseline = (effective / 8).max(GIB);
    baseline.min(effective / 2)
}

fn reserved_memory_from_environment() -> Option<u64> {
    std::env::var("MATTOS_RESERVED_MEMORY_MIB")
        .ok()?
        .parse::<u64>()
        .ok()?
        .checked_mul(MIB)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    struct FakeReader(BTreeMap<String, String>);
    impl ResourceReader for FakeReader {
        fn read(&self, path: &str) -> io::Result<String> {
            self.0
                .get(path)
                .cloned()
                .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))
        }
    }

    #[test]
    fn cgroup_v2_limits_cpu_and_memory() {
        let reader = FakeReader(BTreeMap::from([
            ("/proc/meminfo".into(), "MemTotal:       16777216 kB\nMemAvailable:    8388608 kB\nSwapTotal:       1048576 kB\nSwapFree:        1048576 kB\n".into()),
            ("/proc/vmstat".into(), "pswpin 0\npswpout 0\n".into()),
            ("/sys/fs/cgroup/cpuset.cpus.effective".into(), "0-3".into()),
            ("/sys/fs/cgroup/cpu.max".into(), "250000 100000".into()),
            ("/sys/fs/cgroup/memory.max".into(), "8589934592".into()),
            ("/sys/fs/cgroup/memory.current".into(), "2147483648".into()),
        ]));
        let snapshot = discover_with(&reader, 12, Some(GIB));
        assert_eq!(snapshot.effective_cpu_count(), 3);
        assert_eq!(snapshot.effective_memory_limit_bytes(), 8 * GIB);
        assert_eq!(snapshot.effective_available_memory_bytes(), 6 * GIB);
        assert_eq!(snapshot.budget().build_memory_bytes, 5 * GIB);
    }

    #[test]
    fn discovery_uses_the_process_cgroup_subtree_before_controller_root() {
        let reader = FakeReader(BTreeMap::from([
            ("/proc/meminfo".into(), "MemTotal:       16777216 kB\nMemAvailable:    8388608 kB\nSwapTotal:             0 kB\nSwapFree:              0 kB\n".into()),
            ("/proc/vmstat".into(), "pswpin 0\npswpout 0\n".into()),
            ("/proc/self/cgroup".into(), "0::/ci/mattos\n".into()),
            ("/sys/fs/cgroup/ci/mattos/cpu.max".into(), "200000 100000".into()),
            ("/sys/fs/cgroup/ci/mattos/memory.max".into(), "4294967296".into()),
            ("/sys/fs/cgroup/ci/mattos/memory.current".into(), "1073741824".into()),
        ]));
        let snapshot = discover_with(&reader, 24, Some(GIB));
        assert_eq!(snapshot.effective_cpu_count(), 2);
        assert_eq!(snapshot.effective_available_memory_bytes(), 3 * GIB);
    }

    #[test]
    fn parses_cpuset_quota_and_swap_pressure() {
        assert_eq!(parse_cpuset("0-3,8,10-11"), Some(7));
        assert_eq!(parse_cpu_max("150000 100000"), Some(2));
        let snapshot = ResourceSnapshot {
            logical_cpus: 32,
            cpuset_cpus: Some(8),
            cpu_quota_cpus: Some(6),
            physical_memory_bytes: 64 * GIB,
            available_memory_bytes: 40 * GIB,
            cgroup_memory_limit_bytes: None,
            cgroup_memory_current_bytes: None,
            swap_total_bytes: GIB,
            swap_free_bytes: GIB / 2,
            swap_in_pages: 1,
            swap_out_pages: 2,
            psi_memory_some_avg10: None,
            reserved_memory_bytes: 4 * GIB,
        };
        assert_eq!(
            snapshot.budget(),
            ResourceBudget {
                cpu_tokens: 6,
                build_memory_bytes: 36 * GIB,
                reserved_memory_bytes: 4 * GIB,
                available_memory_bytes: 40 * GIB,
            }
        );
    }

    #[test]
    fn injected_host_sizes_and_pressure_states_produce_expected_budgets() {
        let snapshot = |cpus, total, available, reserve| ResourceSnapshot {
            logical_cpus: cpus,
            cpuset_cpus: None,
            cpu_quota_cpus: None,
            physical_memory_bytes: total,
            available_memory_bytes: available,
            cgroup_memory_limit_bytes: None,
            cgroup_memory_current_bytes: None,
            swap_total_bytes: 0,
            swap_free_bytes: 0,
            swap_in_pages: 0,
            swap_out_pages: 0,
            psi_memory_some_avg10: None,
            reserved_memory_bytes: reserve,
        };
        // 4-core/8-GiB laptop, current 12-thread/16-GiB host, 32/64-GiB
        // workstation, and a large build server are all ordinary snapshots.
        assert_eq!(snapshot(4, 8 * GIB, 6 * GIB, GIB).budget().cpu_tokens, 4);
        assert_eq!(
            snapshot(12, 16 * GIB, 12 * GIB, 2 * GIB)
                .budget()
                .build_memory_bytes,
            10 * GIB
        );
        assert_eq!(
            snapshot(32, 64 * GIB, 48 * GIB, 8 * GIB)
                .budget()
                .cpu_tokens,
            32
        );
        assert_eq!(
            snapshot(128, 256 * GIB, 220 * GIB, 32 * GIB)
                .budget()
                .build_memory_bytes,
            188 * GIB
        );
        // Low free RAM leaves no new-build memory budget even though CPUs are idle.
        assert_eq!(
            snapshot(12, 16 * GIB, GIB, 2 * GIB)
                .budget()
                .build_memory_bytes,
            0
        );
    }

    fn pressure_snapshot(available: u64, swap_in: u64, swap_out: u64) -> ResourceSnapshot {
        ResourceSnapshot {
            logical_cpus: 12,
            cpuset_cpus: None,
            cpu_quota_cpus: None,
            physical_memory_bytes: 16 * GIB,
            available_memory_bytes: available,
            cgroup_memory_limit_bytes: None,
            cgroup_memory_current_bytes: None,
            swap_total_bytes: 8 * GIB,
            swap_free_bytes: 2 * GIB,
            swap_in_pages: swap_in,
            swap_out_pages: swap_out,
            psi_memory_some_avg10: None,
            reserved_memory_bytes: 2 * GIB,
        }
    }

    #[test]
    fn old_swap_occupancy_without_counter_activity_is_healthy() {
        let mut tracker = PressureTracker::new();
        tracker.observe(
            pressure_snapshot(12 * GIB, 100, 200),
            Duration::from_secs(1),
        );
        let envelope = tracker.observe(
            pressure_snapshot(12 * GIB, 100, 200),
            Duration::from_secs(1),
        );
        assert_eq!(envelope.pressure, PressureLevel::Healthy);
    }

    #[test]
    fn active_swap_out_escalates_and_recovery_is_hysteretic() {
        let mut tracker = PressureTracker::new();
        tracker.observe(pressure_snapshot(12 * GIB, 0, 0), Duration::from_secs(1));
        let critical = tracker.observe(pressure_snapshot(12 * GIB, 0, 32), Duration::from_secs(1));
        assert_eq!(critical.pressure, PressureLevel::Critical);
        let first_recovery =
            tracker.observe(pressure_snapshot(12 * GIB, 0, 32), Duration::from_secs(1));
        assert_eq!(first_recovery.pressure, PressureLevel::Critical);
        let recovered = tracker.observe(pressure_snapshot(12 * GIB, 0, 32), Duration::from_secs(1));
        assert_eq!(recovered.pressure, PressureLevel::Healthy);
    }
}
