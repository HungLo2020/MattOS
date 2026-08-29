use crate::performance;
use crate::resources::{
    HostResourceSampler, PressureLevel, ResourceBudget, ResourceEnvelope, RuntimeResourceSampler,
};
use anyhow::{Result, anyhow, bail};
use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

// Resource release and ready-set events wake admission immediately.  This is
// only a recovery/safety poll: it must never be the normal scheduling clock.
const PRESSURE_RECOVERY_POLL: Duration = Duration::from_secs(1);

thread_local! {
    static GRANTED_TOKENS: Cell<usize> = const { Cell::new(4) };
    static CHILD_JOB_POLICY: Cell<ChildJobPolicy> = const { Cell::new(ChildJobPolicy::SchedulerGrant) };
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ChildJobPolicy {
    Serial,
    #[allow(dead_code)]
    Capped(usize),
    SchedulerGrant,
}

/// Generic launch-time resource declaration.  These are classes, not a table
/// of machine-specific `-j` values: a larger safe host can grant more CPUs to
/// exactly the same profile.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct StageResourceProfile {
    /// Capacity charged to global admission. This is deliberately distinct
    /// from the process-level parallelism passed to Make/Cargo/Ninja.
    pub(crate) minimum_cpu_grant: usize,
    pub(crate) useful_cpu_ceiling: Option<usize>,
    /// Preferred launch baseline when the host envelope permits it.  This is
    /// distinct from the absolute safe minimum below so constrained hosts can
    /// still make forward progress without abandoning their RAM reserve.
    pub(crate) preferred_child_jobs: usize,
    pub(crate) minimum_child_jobs: usize,
    pub(crate) useful_child_job_ceiling: Option<usize>,
    pub(crate) estimated_memory_bytes: u64,
    /// Incremental reservation for each granted child job.  Admission searches
    /// the useful range downward, so an otherwise-safe stage is never made
    /// permanently impossible merely because its maximum configuration will
    /// not fit this host's current safe envelope.
    pub(crate) memory_per_child_job_bytes: u64,
    pub(crate) memory_heavy: bool,
    pub(crate) may_borrow_idle_cpu: bool,
    pub(crate) child_jobs: ChildJobPolicy,
}

impl StageResourceProfile {
    const MIB: u64 = 1024 * 1024;

    pub(crate) fn standard() -> Self {
        Self {
            minimum_cpu_grant: 1,
            useful_cpu_ceiling: None,
            preferred_child_jobs: 1,
            minimum_child_jobs: 1,
            useful_child_job_ceiling: None,
            estimated_memory_bytes: 768 * Self::MIB,
            memory_per_child_job_bytes: 0,
            memory_heavy: false,
            may_borrow_idle_cpu: true,
            child_jobs: ChildJobPolicy::SchedulerGrant,
        }
    }

    pub(crate) fn memory_heavy() -> Self {
        Self {
            minimum_cpu_grant: 1,
            useful_cpu_ceiling: Some(6),
            preferred_child_jobs: 4,
            minimum_child_jobs: 1,
            useful_child_job_ceiling: Some(6),
            // Four safe jobs reserve 2 GiB; six useful jobs reserve 2.75 GiB.
            // This is a scalable launch model, not a host-specific stage cap.
            estimated_memory_bytes: 512 * Self::MIB,
            memory_per_child_job_bytes: 384 * Self::MIB,
            memory_heavy: true,
            may_borrow_idle_cpu: true,
            child_jobs: ChildJobPolicy::SchedulerGrant,
        }
    }

    pub(crate) fn serial() -> Self {
        Self {
            minimum_cpu_grant: 1,
            useful_cpu_ceiling: Some(1),
            preferred_child_jobs: 1,
            minimum_child_jobs: 1,
            useful_child_job_ceiling: Some(1),
            estimated_memory_bytes: 256 * Self::MIB,
            memory_per_child_job_bytes: 0,
            memory_heavy: false,
            may_borrow_idle_cpu: false,
            child_jobs: ChildJobPolicy::Serial,
        }
    }

    /// Large C++/Rust link steps need an explicit reservation and a modest
    /// compiler cap. Four children keep incremental builds useful while a
    /// conservative 6 GiB reservation prevents admission beside competing
    /// heavy work while still leaving headroom when the invocation itself is
    /// enclosed by the documented 10 GiB systemd memory ceiling.
    pub(crate) fn high_memory_parallel() -> Self {
        Self {
            minimum_cpu_grant: 1,
            useful_cpu_ceiling: Some(4),
            preferred_child_jobs: 4,
            minimum_child_jobs: 1,
            useful_child_job_ceiling: Some(4),

            // These stages are expensive, but the reservation must scale down
            // with the granted compiler parallelism. A fixed 6 GiB reservation
            // made them impossible to admit whenever the scheduler's safe build
            // budget was below 6 GiB, causing permanent starvation.
            //
            // 1 child: 1.75 GiB
            // 2 children: 2.50 GiB
            // 3 children: 3.25 GiB
            // 4 children: 4.00 GiB
            estimated_memory_bytes: 1024 * Self::MIB,
            memory_per_child_job_bytes: 768 * Self::MIB,

            memory_heavy: true,
            may_borrow_idle_cpu: false,
            child_jobs: ChildJobPolicy::Capped(4),
        }
    }
}
pub(crate) fn configure_child_jobs(granted_tokens: usize, policy: ChildJobPolicy) {
    GRANTED_TOKENS.with(|granted| granted.set(granted_tokens));
    CHILD_JOB_POLICY.with(|current| current.set(policy));
}

pub(crate) fn child_job_limit() -> usize {
    let granted = GRANTED_TOKENS.with(Cell::get);
    CHILD_JOB_POLICY.with(|policy| child_job_limit_for(granted, policy.get()))
}

fn child_job_limit_for(granted: usize, policy: ChildJobPolicy) -> usize {
    match policy {
        ChildJobPolicy::Serial => 1,
        ChildJobPolicy::Capped(limit) => granted.min(limit),
        ChildJobPolicy::SchedulerGrant => granted,
    }
}

#[cfg(test)]
pub(crate) fn set_child_jobs_for_test(tokens: usize, policy: ChildJobPolicy) {
    configure_child_jobs(tokens, policy);
}

#[derive(Clone, Debug)]
pub(crate) struct SchedulerNode {
    pub(crate) id: String,
    pub(crate) dependencies: Vec<String>,
    pub(crate) outputs: Vec<PathBuf>,
    pub(crate) profile: StageResourceProfile,
}

#[cfg(test)]
#[derive(Clone, Debug)]
pub(crate) struct SimulationReport {
    pub(crate) serial_seconds: f64,
    pub(crate) scheduled_seconds: f64,
    pub(crate) critical_path_seconds: f64,
}

pub(crate) struct JobContext {
    id: String,
    profile: StageResourceProfile,
    events: Sender<Event>,
    permit: Receiver<Option<Allocation>>,
}

#[derive(Clone, Copy)]
struct Allocation {
    cpu_tokens: usize,
    child_jobs: usize,
}

impl JobContext {
    pub(crate) fn acquire_build_resources(&self) -> Result<()> {
        self.events
            .send(Event::RequestBuildResources {
                id: self.id.clone(),
            })
            .map_err(|_| {
                anyhow!(
                    "scheduler stopped before {} could request resources",
                    self.id
                )
            })?;
        match self.permit.recv() {
            Ok(Some(allocation)) => {
                configure_child_jobs(allocation.child_jobs, self.profile.child_jobs);
                Ok(())
            }
            Ok(None) | Err(_) => bail!("scheduler cancelled {} before its build action", self.id),
        }
    }
}

enum Event {
    RequestBuildResources { id: String },
    Finished { id: String, result: Result<()> },
}

struct RunningJob {
    tokens: usize,
    memory_bytes: u64,
    memory_heavy: bool,
    estimated_memory_bytes: u64,
    observed_available_memory_start: Option<u64>,
    observed_cgroup_memory_current_start: Option<u64>,
    observed_pressure_start: Option<PressureLevel>,
    permit: Sender<Option<Allocation>>,
    wait_started: Option<Instant>,
    resource_wait_seconds: f64,
    build_started: Option<Instant>,
    last_accounted: Option<Instant>,
    unused_token_seconds: f64,
    minimum_unused_tokens: usize,
}

fn account_running_builds(
    running: &mut BTreeMap<String, RunningJob>,
    now: Instant,
    unused_tokens: usize,
) {
    for job in running.values_mut() {
        let Some(last_accounted) = job.last_accounted else {
            continue;
        };
        job.unused_token_seconds +=
            now.duration_since(last_accounted).as_secs_f64() * unused_tokens as f64;
        job.minimum_unused_tokens = job.minimum_unused_tokens.min(unused_tokens);
        job.last_accounted = Some(now);
    }
}

fn remaining_cpu_tokens(cpu_budget: usize, used_tokens: usize) -> usize {
    cpu_budget.checked_sub(used_tokens).unwrap_or_else(|| {
        panic!(
            "scheduler CPU reservation invariant violated: used {used_tokens} tokens with a {cpu_budget}-token budget"
        )
    })
}

struct SchedulerTrace {
    started: Instant,
    file: Option<File>,
    writes: usize,
    write_time: Duration,
}

impl SchedulerTrace {
    fn start() -> Result<Self> {
        let file = std::env::var_os("MATTOS_SCHEDULER_TRACE")
            .map(PathBuf::from)
            .map(|path| {
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                OpenOptions::new()
                    .create(true)
                    .truncate(true)
                    .write(true)
                    .open(path)
            })
            .transpose()?;
        Ok(Self {
            started: Instant::now(),
            file,
            writes: 0,
            write_time: Duration::ZERO,
        })
    }

    fn event(&mut self, event: &str) {
        let write_started = Instant::now();
        let line = format!(
            "[scheduler] elapsed={:.3}s {event}",
            self.started.elapsed().as_secs_f64()
        );
        println!("{line}");
        if let Some(file) = &mut self.file {
            let _ = writeln!(file, "{line}");
            let _ = file.flush();
        }
        self.writes += 1;
        self.write_time += write_started.elapsed();
    }
}

#[derive(Clone)]
struct DeferredStage {
    reason: String,
    pressure: PressureLevel,
    started: Instant,
}

fn record_defer(
    trace: &mut SchedulerTrace,
    deferred: &mut BTreeMap<String, DeferredStage>,
    id: &str,
    reason: &str,
    pressure: PressureLevel,
    used_tokens: usize,
    used_memory: u64,
) {
    let changed = deferred
        .get(id)
        .is_none_or(|state| state.reason != reason || state.pressure != pressure);
    if changed {
        trace.event(&format!(
            "event=build-deferred stage={id} reason={reason} pressure={} used_tokens={used_tokens} used_memory_bytes={used_memory}",
            pressure.as_str()
        ));
        deferred.insert(
            id.to_string(),
            DeferredStage {
                reason: reason.to_string(),
                pressure,
                started: Instant::now(),
            },
        );
    }
}

pub(crate) fn validate(nodes: &[SchedulerNode], budget: ResourceBudget) -> Result<()> {
    if budget.cpu_tokens == 0 {
        bail!("scheduler token budget must be positive");
    }
    let by_id = nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect::<BTreeMap<_, _>>();
    if by_id.len() != nodes.len() {
        bail!("scheduler stage identifiers must be unique");
    }
    for node in nodes {
        if node.profile.minimum_cpu_grant == 0 || node.profile.minimum_cpu_grant > budget.cpu_tokens
        {
            bail!(
                "scheduler stage {} has invalid minimum CPU grant {} for budget {}",
                node.id,
                node.profile.minimum_cpu_grant,
                budget.cpu_tokens
            );
        }
        if matches!(node.profile.child_jobs, ChildJobPolicy::Capped(0)) {
            bail!("scheduler stage {} has a zero child-job cap", node.id);
        }
        for dependency in &node.dependencies {
            if !by_id.contains_key(dependency.as_str()) {
                bail!(
                    "scheduler stage {} depends on unknown {}",
                    node.id,
                    dependency
                );
            }
        }
    }

    let mut complete = BTreeSet::new();
    loop {
        let before = complete.len();
        for node in nodes {
            if node
                .dependencies
                .iter()
                .all(|dependency| complete.contains(dependency.as_str()))
            {
                complete.insert(node.id.as_str());
            }
        }
        if complete.len() == nodes.len() {
            break;
        }
        if complete.len() == before {
            let blocked = by_id
                .keys()
                .filter(|id| !complete.contains(**id))
                .copied()
                .collect::<Vec<_>>()
                .join(", ");
            bail!("scheduler graph contains a cycle involving: {blocked}");
        }
    }

    for (index, left) in nodes.iter().enumerate() {
        for right in &nodes[index + 1..] {
            if depends_on(&by_id, left.id.as_str(), right.id.as_str())
                || depends_on(&by_id, right.id.as_str(), left.id.as_str())
            {
                continue;
            }
            for left_output in &left.outputs {
                for right_output in &right.outputs {
                    if paths_overlap(left_output, right_output) {
                        bail!(
                            "concurrently runnable stages {} and {} have overlapping outputs {} and {}",
                            left.id,
                            right.id,
                            left_output.display(),
                            right_output.display()
                        );
                    }
                }
            }
        }
    }
    Ok(())
}

fn depends_on(
    nodes: &BTreeMap<&str, &SchedulerNode>,
    stage: &str,
    possible_dependency: &str,
) -> bool {
    let mut pending = vec![stage];
    let mut visited = BTreeSet::new();
    while let Some(current) = pending.pop() {
        if !visited.insert(current) {
            continue;
        }
        for dependency in &nodes[current].dependencies {
            if dependency == possible_dependency {
                return true;
            }
            pending.push(dependency);
        }
    }
    false
}

fn paths_overlap(left: &Path, right: &Path) -> bool {
    left.starts_with(right) || right.starts_with(left)
}

#[cfg(test)]
pub(crate) fn simulate(
    nodes: &[SchedulerNode],
    durations: &BTreeMap<String, f64>,
    budget: ResourceBudget,
) -> Result<SimulationReport> {
    validate(nodes, budget)?;
    for node in nodes {
        if !durations
            .get(&node.id)
            .is_some_and(|duration| *duration >= 0.0)
        {
            bail!(
                "scheduler simulation is missing a valid duration for {}",
                node.id
            );
        }
    }
    let serial_seconds = nodes.iter().map(|node| durations[&node.id]).sum();
    let mut critical_finish = BTreeMap::<String, f64>::new();
    while critical_finish.len() < nodes.len() {
        let before = critical_finish.len();
        for node in nodes {
            if critical_finish.contains_key(&node.id)
                || !node
                    .dependencies
                    .iter()
                    .all(|dependency| critical_finish.contains_key(dependency))
            {
                continue;
            }
            let dependency_finish = node
                .dependencies
                .iter()
                .map(|dependency| critical_finish[dependency])
                .fold(0.0, f64::max);
            critical_finish.insert(node.id.clone(), dependency_finish + durations[&node.id]);
        }
        debug_assert!(critical_finish.len() > before);
    }

    let mut now = 0.0f64;
    let mut complete = BTreeSet::new();
    let mut running = BTreeMap::<String, (f64, usize, u64, bool)>::new();
    let mut stable_nodes = nodes.iter().collect::<Vec<_>>();
    stable_nodes.sort_by(|left, right| left.id.cmp(&right.id));
    while complete.len() < nodes.len() {
        let used_tokens = running
            .values()
            .map(|(_, tokens, _, _)| tokens)
            .sum::<usize>();
        let used_memory = running
            .values()
            .map(|(_, _, memory, _)| memory)
            .sum::<u64>();
        let heavy_jobs = running.values().filter(|(_, _, _, heavy)| *heavy).count();
        let mut available_tokens = budget.cpu_tokens - used_tokens;
        let mut available_memory = budget.build_memory_bytes.saturating_sub(used_memory);
        let mut available_heavy = heavy_limit(PressureLevel::Healthy).saturating_sub(heavy_jobs);
        for node in &stable_nodes {
            if complete.contains(&node.id)
                || running.contains_key(&node.id)
                || node.profile.minimum_cpu_grant > available_tokens
                || node.profile.estimated_memory_bytes > available_memory
                || (node.profile.memory_heavy && available_heavy == 0)
                || !node
                    .dependencies
                    .iter()
                    .all(|dependency| complete.contains(dependency))
            {
                continue;
            }
            running.insert(
                node.id.clone(),
                (
                    now + durations[&node.id],
                    node.profile.minimum_cpu_grant,
                    node.profile.estimated_memory_bytes,
                    node.profile.memory_heavy,
                ),
            );
            available_tokens -= node.profile.minimum_cpu_grant;
            available_memory -= node.profile.estimated_memory_bytes;
            available_heavy -= usize::from(node.profile.memory_heavy);
        }
        let next_finish = running
            .values()
            .map(|(finish, _, _, _)| *finish)
            .min_by(f64::total_cmp)
            .ok_or_else(|| anyhow!("scheduler simulation made no progress"))?;
        now = next_finish;
        let finished = running
            .iter()
            .filter(|(_, (finish, _, _, _))| *finish == now)
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        for id in finished {
            running.remove(&id);
            complete.insert(id);
        }
    }
    Ok(SimulationReport {
        serial_seconds,
        scheduled_seconds: now,
        critical_path_seconds: critical_finish.values().copied().fold(0.0, f64::max),
    })
}

pub(crate) fn standalone_grant(
    profile: StageResourceProfile,
    envelope: &ResourceEnvelope,
) -> usize {
    let budget = envelope.budget;
    let maximum = profile.useful_cpu_ceiling.unwrap_or(budget.cpu_tokens).min(
        profile
            .useful_child_job_ceiling
            .unwrap_or(budget.cpu_tokens),
    );
    let minimum = profile
        .minimum_cpu_grant
        .max(profile.minimum_child_jobs)
        .min(budget.cpu_tokens);
    let preferred = if profile.may_borrow_idle_cpu && envelope.pressure == PressureLevel::Healthy {
        budget.cpu_tokens.min(maximum)
    } else {
        profile
            .minimum_cpu_grant
            .max(profile.preferred_child_jobs)
            .min(maximum)
            .min(budget.cpu_tokens)
    };
    (minimum..=preferred)
        .rev()
        .find(|jobs| memory_reservation(profile, *jobs) <= budget.build_memory_bytes)
        .unwrap_or(0)
}

fn heavy_limit(pressure: PressureLevel) -> usize {
    match pressure {
        PressureLevel::Healthy => 2,
        PressureLevel::Constrained => 1,
        PressureLevel::Critical => 0,
    }
}

fn memory_reservation(profile: StageResourceProfile, child_jobs: usize) -> u64 {
    profile.estimated_memory_bytes.saturating_add(
        profile
            .memory_per_child_job_bytes
            .saturating_mul(child_jobs as u64),
    )
}

fn admission_grant(
    profile: StageResourceProfile,
    budget: ResourceBudget,
    pressure: PressureLevel,
    used_tokens: usize,
    used_memory: u64,
    waiting_profiles: impl Iterator<Item = StageResourceProfile>,
) -> Result<Allocation, &'static str> {
    if profile.memory_heavy && pressure == PressureLevel::Critical {
        return Err("critical-pressure");
    }
    let available_cpu = budget
        .cpu_tokens
        .checked_sub(used_tokens)
        .ok_or("cpu-budget")?;
    let available_memory = budget
        .build_memory_bytes
        .checked_sub(used_memory)
        .ok_or("memory-budget")?;
    let minimum = profile.minimum_cpu_grant.max(profile.minimum_child_jobs);
    if available_cpu < minimum {
        return Err("insufficient-cpu-or-memory-budget");
    }
    let reserved_for_peers = waiting_profiles
        .filter(|peer| {
            peer.minimum_cpu_grant.max(peer.minimum_child_jobs) <= available_cpu
                && memory_reservation(*peer, peer.minimum_child_jobs) <= available_memory
        })
        .map(|peer| peer.minimum_cpu_grant.max(peer.minimum_child_jobs))
        .sum::<usize>();
    let maximum = profile.useful_cpu_ceiling.unwrap_or(budget.cpu_tokens).min(
        profile
            .useful_child_job_ceiling
            .unwrap_or(budget.cpu_tokens),
    );
    let preferred_baseline = profile
        .minimum_cpu_grant
        .max(profile.preferred_child_jobs)
        .min(maximum);
    let borrowable = available_cpu
        .saturating_sub(preferred_baseline)
        .saturating_sub(reserved_for_peers);
    let preferred = if profile.may_borrow_idle_cpu && pressure == PressureLevel::Healthy {
        (preferred_baseline + borrowable).min(maximum)
    } else {
        preferred_baseline
    }
    .min(available_cpu);
    let cpu_tokens = (minimum..=preferred)
        .rev()
        .find(|jobs| memory_reservation(profile, *jobs) <= available_memory)
        .ok_or("insufficient-cpu-or-memory-budget")?;
    Ok(Allocation {
        cpu_tokens,
        child_jobs: cpu_tokens,
    })
}

pub(crate) fn execute<F>(nodes: Vec<SchedulerNode>, budget: ResourceBudget, action: F) -> Result<()>
where
    F: Fn(&str, &JobContext) -> Result<()> + Sync,
{
    let mut sampler = HostResourceSampler::new();
    execute_with_sampler(nodes, budget, &mut sampler, action)
}

fn execute_with_sampler<F, S>(
    nodes: Vec<SchedulerNode>,
    budget: ResourceBudget,
    sampler: &mut S,
    action: F,
) -> Result<()>
where
    F: Fn(&str, &JobContext) -> Result<()> + Sync,
    S: RuntimeResourceSampler,
{
    validate(&nodes, budget)?;
    let mut trace = SchedulerTrace::start()?;
    trace.event(&format!(
        "event=validated nodes={} cpu_budget={} memory_budget_bytes={} reserved_memory_bytes={}",
        nodes.len(),
        budget.cpu_tokens,
        budget.build_memory_bytes,
        budget.reserved_memory_bytes
    ));
    let nodes = nodes
        .into_iter()
        .map(|node| (node.id.clone(), node))
        .collect::<BTreeMap<_, _>>();
    let (events_tx, events_rx) = mpsc::channel::<Event>();
    let mut complete = BTreeSet::new();
    let mut launched = BTreeSet::new();
    let mut running = BTreeMap::<String, RunningJob>::new();
    let mut waiting = BTreeSet::<String>::new();
    let mut used_tokens = 0usize;
    let mut used_memory = 0u64;
    let mut heavy_jobs = 0usize;
    let mut first_error = None;
    let mut deferred = BTreeMap::<String, DeferredStage>::new();
    let mut admission_passes = 0usize;
    let mut sampler_calls = 0usize;
    let mut sampler_time = Duration::ZERO;
    let mut admission_time = Duration::ZERO;
    let mut next_pressure_poll = Instant::now();

    thread::scope(|scope| {
        loop {
            if first_error.is_none() {
                for (id, node) in &nodes {
                    if used_tokens >= budget.cpu_tokens {
                        break;
                    }
                    if launched.contains(id)
                        || !node
                            .dependencies
                            .iter()
                            .all(|dependency| complete.contains(dependency))
                    {
                        continue;
                    }
                    let (permit_tx, permit_rx) = mpsc::channel();
                    running.insert(
                        id.clone(),
                        RunningJob {
                            tokens: 1,
                            memory_bytes: 0,
                            memory_heavy: false,
                            estimated_memory_bytes: 0,
                            observed_available_memory_start: None,
                            observed_cgroup_memory_current_start: None,
                            observed_pressure_start: None,
                            permit: permit_tx,
                            wait_started: None,
                            resource_wait_seconds: 0.0,
                            build_started: None,
                            last_accounted: None,
                            unused_token_seconds: 0.0,
                            minimum_unused_tokens: budget.cpu_tokens,
                        },
                    );
                    launched.insert(id.clone());
                    account_running_builds(
                        &mut running,
                        Instant::now(),
                        remaining_cpu_tokens(budget.cpu_tokens, used_tokens),
                    );
                    used_tokens += 1;
                    trace.event(&format!(
                        "event=evaluation-dispatch stage={id} used_tokens={used_tokens} heavy_jobs={heavy_jobs}"
                    ));
                    let id = id.clone();
                    let events = events_tx.clone();
                    let action = &action;
                    let profile = node.profile;
                    scope.spawn(move || {
                        let context = JobContext {
                            id: id.clone(),
                            profile,
                            events: events.clone(),
                            permit: permit_rx,
                        };
                        let result = action(&id, &context);
                        let _ = events.send(Event::Finished { id, result });
                    });
                }
            }

            let waiting_ids = waiting.iter().cloned().collect::<Vec<_>>();
            let sample_started = Instant::now();
            let sampled = sampler.sample();
            sampler_time += sample_started.elapsed();
            sampler_calls += 1;
            admission_passes += 1;
            let admission_started = Instant::now();
            // CPU capacity is fixed for this invocation; current memory and
            // pressure are refreshed for every launch decision.
            let admission_budget = ResourceBudget {
                cpu_tokens: budget.cpu_tokens,
                ..sampled.budget
            };
            trace.event(&format!(
                "event=resource-envelope pressure={} available_memory_bytes={} build_memory_bytes={} cgroup_memory_current_bytes={} swap_in_pages_per_second={:.3} swap_out_pages_per_second={:.3} psi_memory_some_avg10={}",
                sampled.pressure.as_str(), admission_budget.available_memory_bytes, admission_budget.build_memory_bytes,
                sampled.cgroup_memory_current_bytes.map_or_else(|| "unavailable".to_string(), |value| value.to_string()), sampled.swap_in_pages_per_second, sampled.swap_out_pages_per_second,
                sampled.psi_memory_some_avg10.map_or_else(|| "unavailable".to_string(), |value| format!("{value:.3}")),
            ));
            for id in waiting_ids {
                let node = &nodes[&id];
                if first_error.is_some() {
                    if let Some(job) = running.get(&id) {
                        let _ = job.permit.send(None);
                    }
                    waiting.remove(&id);
                    trace.event(&format!(
                        "event=build-cancel stage={id} used_tokens={used_tokens} heavy_jobs={heavy_jobs}"
                    ));
                } else if node.profile.memory_heavy && heavy_jobs >= heavy_limit(sampled.pressure) {
                    record_defer(
                        &mut trace,
                        &mut deferred,
                        &id,
                        "memory-heavy-limit",
                        sampled.pressure,
                        used_tokens,
                        used_memory,
                    );
                } else {
                    let peers = waiting
                        .iter()
                        .filter(|other| *other != &id)
                        .map(|other| nodes[other].profile);
                    let allocation = match admission_grant(
                        node.profile,
                        admission_budget,
                        sampled.pressure,
                        used_tokens,
                        used_memory,
                        peers,
                    ) {
                        Ok(allocation) => allocation,
                        Err(reason) => {
                            record_defer(
                                &mut trace,
                                &mut deferred,
                                &id,
                                reason,
                                sampled.pressure,
                                used_tokens,
                                used_memory,
                            );
                            continue;
                        }
                    };
                    let now = Instant::now();
                    account_running_builds(
                        &mut running,
                        now,
                        remaining_cpu_tokens(budget.cpu_tokens, used_tokens),
                    );
                    let job = running.get_mut(&id).expect("waiting job is running");
                    job.tokens = allocation.cpu_tokens;
                    job.memory_bytes = memory_reservation(node.profile, allocation.child_jobs);
                    job.memory_heavy = node.profile.memory_heavy;
                    job.estimated_memory_bytes =
                        memory_reservation(node.profile, allocation.child_jobs);
                    job.observed_available_memory_start =
                        Some(admission_budget.available_memory_bytes);
                    job.observed_cgroup_memory_current_start = sampled.cgroup_memory_current_bytes;
                    job.observed_pressure_start = Some(sampled.pressure);
                    job.resource_wait_seconds = job
                        .wait_started
                        .take()
                        .map(|started| now.duration_since(started).as_secs_f64())
                        .unwrap_or(0.0);
                    job.build_started = Some(now);
                    job.last_accounted = Some(now);
                    used_tokens += allocation.cpu_tokens;
                    used_memory += memory_reservation(node.profile, allocation.child_jobs);
                    heavy_jobs += usize::from(node.profile.memory_heavy);
                    job.minimum_unused_tokens =
                        remaining_cpu_tokens(budget.cpu_tokens, used_tokens);
                    let _ = job.permit.send(Some(allocation));
                    waiting.remove(&id);
                    if let Some(state) = deferred.remove(&id) {
                        trace.event(&format!(
                            "event=build-unblocked stage={id} prior_reason={} prior_pressure={} deferred_seconds={:.3}",
                            state.reason, state.pressure.as_str(), state.started.elapsed().as_secs_f64()
                        ));
                    }
                    trace.event(&format!(
                        "event=build-start stage={id} grant={} memory_bytes={} memory_heavy={} pressure={} available_memory_bytes={} used_tokens={used_tokens} used_memory_bytes={used_memory} heavy_jobs={heavy_jobs}",
                        allocation.cpu_tokens, memory_reservation(node.profile, allocation.child_jobs), node.profile.memory_heavy, sampled.pressure.as_str(), admission_budget.available_memory_bytes
                    ));
                }
            }
            admission_time += admission_started.elapsed();

            if running.is_empty() {
                break;
            }

            let now = Instant::now();
            let timeout = next_pressure_poll.saturating_duration_since(now);
            next_pressure_poll = now + PRESSURE_RECOVERY_POLL;
            match events_rx.recv_timeout(timeout) {
                Err(RecvTimeoutError::Timeout) => {
                    trace.event("event=admission-wake reason=pressure-recovery-poll");
                    continue;
                }
                Err(RecvTimeoutError::Disconnected) => {
                    first_error = Some(anyhow!(
                        "scheduler workers disconnected before all stages completed"
                    ));
                    break;
                }
                Ok(event) => match event {
                    Event::RequestBuildResources { id } => {
                        trace.event("event=admission-wake reason=resource-request");
                        let now = Instant::now();
                        account_running_builds(
                            &mut running,
                            now,
                            remaining_cpu_tokens(budget.cpu_tokens, used_tokens),
                        );
                        let job = running.get_mut(&id).expect("resource requester is running");
                        used_tokens -= job.tokens;
                        job.tokens = 0;
                        job.wait_started = Some(now);
                        trace.event(&format!(
                        "event=build-wait stage={id} min_grant={} memory_bytes={} memory_heavy={} used_tokens={used_tokens} heavy_jobs={heavy_jobs}",
                        nodes[&id].profile.minimum_cpu_grant, nodes[&id].profile.estimated_memory_bytes, nodes[&id].profile.memory_heavy
                    ));
                        waiting.insert(id);
                    }
                    Event::Finished { id, result } => {
                        trace.event("event=admission-wake reason=stage-finished");
                        let ending_sample = sampler.sample();
                        let now = Instant::now();
                        account_running_builds(
                            &mut running,
                            now,
                            remaining_cpu_tokens(budget.cpu_tokens, used_tokens),
                        );
                        waiting.remove(&id);
                        let job = running.remove(&id).expect("finished job is running");
                        let node = &nodes[&id];
                        let action_seconds = job
                            .build_started
                            .map(|started| now.duration_since(started).as_secs_f64())
                            .unwrap_or(0.0);
                        let average_unused_tokens = if action_seconds > 0.0 {
                            job.unused_token_seconds / action_seconds
                        } else {
                            budget.cpu_tokens.saturating_sub(used_tokens) as f64
                        };
                        let minimum_unused_tokens = if job.build_started.is_some() {
                            job.minimum_unused_tokens
                        } else {
                            budget.cpu_tokens.saturating_sub(used_tokens)
                        };
                        used_tokens -= job.tokens;
                        used_memory -= job.memory_bytes;
                        heavy_jobs -= usize::from(job.memory_heavy);
                        let cpu = performance::stage_cpu_usage_from_log(Path::new("."), &id);
                        let (cpu_user, cpu_system, cpu_total, cpu_cores) = cpu
                            .map(|(user, system)| {
                                let total = user + system;
                                (
                                    format!("{user:.6}"),
                                    format!("{system:.6}"),
                                    format!("{total:.6}"),
                                    format!("{:.3}", total / action_seconds.max(f64::MIN_POSITIVE)),
                                )
                            })
                            .unwrap_or_else(|| {
                                (
                                    "unavailable".into(),
                                    "unavailable".into(),
                                    "unavailable".into(),
                                    "unavailable".into(),
                                )
                            });
                        trace.event(&format!(
                        "event=stage-metrics stage={id} build_executed={} grant={} child_jobs={} estimated_memory_bytes={} observed_available_memory_start={} observed_available_memory_end={} observed_cgroup_memory_current_start={} observed_cgroup_memory_current_end={} observed_pressure_start={} observed_pressure_end={} resource_wait_seconds={:.3} action_seconds={action_seconds:.3} unused_tokens_avg={average_unused_tokens:.3} unused_tokens_min={minimum_unused_tokens} cpu_user_seconds={cpu_user} cpu_system_seconds={cpu_system} cpu_seconds={cpu_total} cpu_cores_avg={cpu_cores}",
                        job.build_started.is_some(),
                        job.tokens,
                        child_job_limit_for(job.tokens, node.profile.child_jobs),
                        job.estimated_memory_bytes,
                        job.observed_available_memory_start.map_or_else(|| "unavailable".to_string(), |value| value.to_string()),
                        ending_sample.budget.available_memory_bytes,
                        job.observed_cgroup_memory_current_start.map_or_else(|| "unavailable".to_string(), |value| value.to_string()),
                        ending_sample.cgroup_memory_current_bytes.map_or_else(|| "unavailable".to_string(), |value| value.to_string()),
                        job.observed_pressure_start.map_or("unavailable", PressureLevel::as_str),
                        ending_sample.pressure.as_str(),
                        job.resource_wait_seconds,
                    ));
                        trace.event(&format!(
                        "event=stage-end stage={id} result={} used_tokens={used_tokens} heavy_jobs={heavy_jobs}",
                        if result.is_ok() { "success" } else { "failed" }
                    ));
                        match result {
                            Ok(()) if first_error.is_none() => {
                                complete.insert(id);
                            }
                            Ok(()) => {}
                            Err(error) if first_error.is_none() => first_error = Some(error),
                            Err(_) => {}
                        }
                    }
                },
            }
        }
    });

    if let Some(error) = first_error {
        trace.event("event=scheduler-end result=failed");
        Err(error)
    } else if complete.len() != nodes.len() {
        trace.event("event=scheduler-end result=incomplete");
        bail!("scheduler stopped before all stages completed")
    } else {
        trace.event(&format!(
            "event=scheduler-metrics admission_passes={admission_passes} sampler_calls={sampler_calls} sampler_seconds={:.6} admission_seconds={:.6} trace_writes={} trace_seconds={:.6}",
            sampler_time.as_secs_f64(), admission_time.as_secs_f64(), trace.writes, trace.write_time.as_secs_f64()
        ));
        trace.event("event=scheduler-end result=success");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Barrier, Mutex};
    use std::time::Duration;

    const GIB: u64 = 1024 * 1024 * 1024;

    fn budget(cpus: usize, memory: u64, _swap_in_use: bool) -> ResourceBudget {
        ResourceBudget {
            cpu_tokens: cpus,
            build_memory_bytes: memory,
            reserved_memory_bytes: GIB,
            available_memory_bytes: memory + GIB,
        }
    }

    fn envelope(cpus: usize, memory: u64, pressure: PressureLevel) -> ResourceEnvelope {
        ResourceEnvelope {
            budget: budget(cpus, memory, false),
            pressure,
            swap_in_pages_per_second: 0.0,
            swap_out_pages_per_second: 0.0,
            psi_memory_some_avg10: None,
            cgroup_memory_current_bytes: None,
        }
    }

    struct StaticSampler(ResourceEnvelope);

    impl RuntimeResourceSampler for StaticSampler {
        fn sample(&mut self) -> ResourceEnvelope {
            self.0.clone()
        }
    }

    fn node(id: &str, dependencies: &[&str], minimum_cpu_grant: usize) -> SchedulerNode {
        SchedulerNode {
            id: id.to_string(),
            dependencies: dependencies.iter().map(|value| value.to_string()).collect(),
            outputs: vec![PathBuf::from(format!("out/{id}"))],
            profile: StageResourceProfile {
                minimum_cpu_grant,
                useful_cpu_ceiling: Some(minimum_cpu_grant),
                preferred_child_jobs: minimum_cpu_grant,
                minimum_child_jobs: minimum_cpu_grant,
                useful_child_job_ceiling: Some(minimum_cpu_grant),
                estimated_memory_bytes: 256 * 1024 * 1024,
                memory_per_child_job_bytes: 0,
                memory_heavy: false,
                may_borrow_idle_cpu: true,
                child_jobs: ChildJobPolicy::SchedulerGrant,
            },
        }
    }

    #[test]
    fn running_build_accounting_integrates_unused_tokens() {
        let (permit, _) = mpsc::channel();
        let started = Instant::now();
        let mut running = BTreeMap::from([(
            "stage".to_string(),
            RunningJob {
                tokens: 4,
                memory_bytes: 0,
                memory_heavy: false,
                estimated_memory_bytes: 0,
                observed_available_memory_start: None,
                observed_cgroup_memory_current_start: None,
                observed_pressure_start: None,
                permit,
                wait_started: None,
                resource_wait_seconds: 0.0,
                build_started: Some(started),
                last_accounted: Some(started),
                unused_token_seconds: 0.0,
                minimum_unused_tokens: 8,
            },
        )]);

        account_running_builds(&mut running, started + Duration::from_secs(2), 3);

        let job = &running["stage"];
        assert_eq!(job.unused_token_seconds, 6.0);
        assert_eq!(job.minimum_unused_tokens, 3);
    }

    #[test]
    fn rejects_unknown_dependencies_cycles_invalid_weights_and_overlaps() {
        assert!(validate(&[node("a", &["missing"], 1)], budget(12, 8 * GIB, false)).is_err());
        assert!(
            validate(
                &[node("a", &["b"], 1), node("b", &["a"], 1)],
                budget(12, 8 * GIB, false)
            )
            .is_err()
        );
        assert!(validate(&[node("a", &[], 13)], budget(12, 8 * GIB, false)).is_err());
        let mut left = node("a", &[], 1);
        let mut right = node("b", &[], 1);
        left.outputs = vec!["out/shared".into()];
        right.outputs = vec!["out/shared/child".into()];
        assert!(validate(&[left, right], budget(12, 8 * GIB, false)).is_err());
    }

    #[test]
    fn dependency_publication_precedes_consumer_and_order_is_stable() {
        let events = Arc::new(Mutex::new(Vec::new()));
        execute(
            vec![
                node("b", &[], 1),
                node("a", &[], 1),
                node("consumer", &["a", "b"], 1),
            ],
            budget(1, 8 * GIB, false),
            |id, _| {
                events.lock().unwrap().push(id.to_string());
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(*events.lock().unwrap(), ["a", "b", "consumer"]);
    }

    #[test]
    fn independent_nodes_run_concurrently() {
        let barrier = Arc::new(Barrier::new(2));
        execute(
            vec![node("a", &[], 1), node("b", &[], 1)],
            budget(2, 8 * GIB, false),
            |_, _| {
                barrier.wait();
                Ok(())
            },
        )
        .unwrap();
    }

    #[test]
    fn build_tokens_and_heavy_job_limit_are_enforced() {
        let active = Arc::new(Mutex::new((0usize, 0usize)));
        let maximum = Arc::new(Mutex::new((0usize, 0usize)));
        let mut nodes = (0..4)
            .map(|index| node(&format!("heavy-{index}"), &[], 4))
            .collect::<Vec<_>>();
        for node in &mut nodes {
            node.profile.memory_heavy = true;
            node.profile.estimated_memory_bytes = 2 * GIB;
        }
        let mut sampler = StaticSampler(envelope(12, 8 * GIB, PressureLevel::Healthy));
        execute_with_sampler(
            nodes,
            budget(12, 8 * GIB, false),
            &mut sampler,
            |_, context| {
                context.acquire_build_resources()?;
                {
                    let mut active = active.lock().unwrap();
                    active.0 += 4;
                    active.1 += 1;
                    let mut maximum = maximum.lock().unwrap();
                    maximum.0 = maximum.0.max(active.0);
                    maximum.1 = maximum.1.max(active.1);
                }
                thread::sleep(Duration::from_millis(10));
                let mut active = active.lock().unwrap();
                active.0 -= 4;
                active.1 -= 1;
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(*maximum.lock().unwrap(), (8, 2));
    }

    #[test]
    fn cache_hits_do_not_request_full_stage_weight() {
        let misses_started = Arc::new(Mutex::new(0usize));
        execute(
            vec![node("hit", &[], 12), node("miss", &[], 12)],
            budget(12, 8 * GIB, false),
            |id, context| {
                if id == "miss" {
                    context.acquire_build_resources()?;
                    *misses_started.lock().unwrap() += 1;
                }
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(*misses_started.lock().unwrap(), 1);
    }

    #[test]
    fn memory_pressure_reduces_admission_while_cpu_capacity_remains() {
        let heavy = StageResourceProfile::memory_heavy();
        let constrained = budget(12, 2 * GIB + 768 * 1024 * 1024, false);
        let first = admission_grant(
            heavy,
            constrained,
            PressureLevel::Constrained,
            0,
            0,
            std::iter::empty(),
        )
        .unwrap();
        assert!(first.cpu_tokens > 0);
        // Twelve CPUs remain available, but a second 3-GiB action would cross
        // the startup headroom budget and is not admitted.
        assert!(
            admission_grant(
                heavy,
                constrained,
                PressureLevel::Constrained,
                first.cpu_tokens,
                memory_reservation(heavy, first.child_jobs),
                std::iter::empty()
            )
            .is_err()
        );
    }

    #[test]
    fn generic_profiles_scale_with_host_capacity_without_stage_job_tables() {
        let profile = StageResourceProfile::standard();
        assert_eq!(
            standalone_grant(profile, &envelope(4, 6 * GIB, PressureLevel::Healthy)),
            4
        );
        assert_eq!(
            standalone_grant(profile, &envelope(32, 56 * GIB, PressureLevel::Healthy)),
            32
        );
        assert_eq!(
            standalone_grant(profile, &envelope(32, 56 * GIB, PressureLevel::Constrained)),
            1
        );
    }

    #[test]
    fn serial_profile_never_borrows_idle_cpu() {
        assert_eq!(
            standalone_grant(
                StageResourceProfile::serial(),
                &envelope(64, 128 * GIB, PressureLevel::Healthy)
            ),
            1
        );
    }

    #[test]
    fn standard_peers_reserve_cpu_before_a_heavy_stage_borrows() {
        let heavy = StageResourceProfile::memory_heavy();
        let peers = [
            StageResourceProfile::standard(),
            StageResourceProfile::standard(),
        ];
        let allocation = admission_grant(
            heavy,
            budget(12, 12 * GIB, false),
            PressureLevel::Healthy,
            0,
            0,
            peers.into_iter(),
        )
        .unwrap();
        assert_eq!(
            allocation.cpu_tokens, 6,
            "the generic measured heavy-work ceiling leaves capacity for peers"
        );
    }

    #[test]
    fn healthy_borrowing_never_exceeds_remaining_cpu_capacity() {
        let allocation = admission_grant(
            StageResourceProfile::memory_heavy(),
            budget(12, 12 * GIB, false),
            PressureLevel::Healthy,
            8,
            0,
            std::iter::empty(),
        )
        .expect("the four remaining CPU tokens can admit the heavy stage baseline");
        assert_eq!(allocation.cpu_tokens, 4);
        assert_eq!(allocation.child_jobs, 4);
    }

    #[test]
    #[should_panic(expected = "scheduler CPU reservation invariant violated")]
    fn remaining_cpu_token_accounting_fails_closed_on_overcommit() {
        let _ = remaining_cpu_tokens(12, 13);
    }

    #[test]
    fn constrained_heavy_stage_keeps_its_safe_parallel_baseline() {
        let allocation = admission_grant(
            StageResourceProfile::memory_heavy(),
            budget(12, 8 * GIB, false),
            PressureLevel::Constrained,
            0,
            0,
            std::iter::empty(),
        )
        .unwrap();
        assert_eq!(allocation.cpu_tokens, 4);
        assert_eq!(allocation.child_jobs, 4);
    }

    #[test]
    fn heavy_stage_progresses_in_the_failed_two_gib_envelope() {
        let allocation = admission_grant(
            StageResourceProfile::memory_heavy(),
            budget(12, 2_130 * 1024 * 1024, false),
            PressureLevel::Healthy,
            0,
            0,
            [StageResourceProfile::memory_heavy()].into_iter(),
        )
        .expect("one individually fitting heavy stage must make deterministic progress");
        assert_eq!(allocation.child_jobs, 4);
        assert_eq!(allocation.cpu_tokens, 4);
    }

    #[test]
    fn constrained_heavy_stage_scales_below_preferred_baseline_to_make_progress() {
        let allocation = admission_grant(
            StageResourceProfile::memory_heavy(),
            budget(12, 1_650 * 1024 * 1024, false),
            PressureLevel::Constrained,
            0,
            0,
            [StageResourceProfile::memory_heavy()].into_iter(),
        )
        .expect("a constrained host must use the highest safe sub-baseline grant");
        assert_eq!(allocation.cpu_tokens, 2);
        assert_eq!(allocation.child_jobs, 2);
        assert!(memory_reservation(StageResourceProfile::memory_heavy(), 2) <= 1_650 * 1024 * 1024);
    }

    #[test]
    fn critical_pressure_blocks_new_heavy_work_but_not_safe_standard_work() {
        let critical = PressureLevel::Critical;
        assert!(
            admission_grant(
                StageResourceProfile::memory_heavy(),
                budget(32, 32 * GIB, false),
                critical,
                0,
                0,
                std::iter::empty()
            )
            .is_err()
        );
        assert!(
            admission_grant(
                StageResourceProfile::standard(),
                budget(32, 32 * GIB, false),
                critical,
                0,
                0,
                std::iter::empty()
            )
            .is_ok()
        );
    }

    #[test]
    fn large_ready_sets_gain_concurrency_on_large_hosts() {
        let nodes = (0..64)
            .map(|index| node(&format!("desktop-{index:02}"), &[], 1))
            .collect::<Vec<_>>();
        let durations = nodes
            .iter()
            .map(|node| (node.id.clone(), 1.0))
            .collect::<BTreeMap<_, _>>();
        let workstation = simulate(&nodes, &durations, budget(32, 64 * GIB, false)).unwrap();
        let server = simulate(&nodes, &durations, budget(128, 256 * GIB, false)).unwrap();
        assert_eq!(workstation.scheduled_seconds, 2.0);
        assert_eq!(server.scheduled_seconds, 1.0);
    }

    #[test]
    fn overlarge_memory_estimate_cannot_cross_the_reserved_budget() {
        let mut profile = StageResourceProfile::memory_heavy();
        profile.estimated_memory_bytes = 8 * GIB;
        assert!(
            admission_grant(
                profile,
                budget(32, 4 * GIB, false),
                PressureLevel::Healthy,
                0,
                0,
                std::iter::empty()
            )
            .is_err()
        );
    }

    #[test]
    fn failure_stops_dispatch_and_drains_running_jobs() {
        let ran = Arc::new(Mutex::new(Vec::new()));
        let result = execute(
            vec![
                node("a-fail", &[], 1),
                node("b-running", &[], 1),
                node("z-blocked", &["a-fail"], 1),
            ],
            budget(2, 8 * GIB, false),
            |id, _| {
                ran.lock().unwrap().push(id.to_string());
                if id == "a-fail" {
                    bail!("expected failure");
                }
                Ok(())
            },
        );
        assert!(result.is_err());
        let ran = ran.lock().unwrap();
        assert!(ran.contains(&"b-running".to_string()));
        assert!(!ran.contains(&"z-blocked".to_string()));
    }

    #[test]
    fn identical_deferrals_are_coalesced_until_the_reason_or_pressure_changes() {
        let mut trace = SchedulerTrace::start().unwrap();
        let mut deferred = BTreeMap::new();
        record_defer(
            &mut trace,
            &mut deferred,
            "ready",
            "memory-budget",
            PressureLevel::Healthy,
            4,
            GIB,
        );
        let writes_after_first = trace.writes;
        for _ in 0..100 {
            record_defer(
                &mut trace,
                &mut deferred,
                "ready",
                "memory-budget",
                PressureLevel::Healthy,
                4,
                GIB,
            );
        }
        assert_eq!(trace.writes, writes_after_first);
        record_defer(
            &mut trace,
            &mut deferred,
            "ready",
            "memory-heavy-limit",
            PressureLevel::Constrained,
            4,
            GIB,
        );
        assert_eq!(trace.writes, writes_after_first + 1);
    }
}
