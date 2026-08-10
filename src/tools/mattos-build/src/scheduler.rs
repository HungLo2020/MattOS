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
    pub(crate) minimum_cpu_grant: usize,
    pub(crate) useful_cpu_ceiling: Option<usize>,
    pub(crate) estimated_memory_bytes: u64,
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
            estimated_memory_bytes: 768 * Self::MIB,
            memory_heavy: false,
            may_borrow_idle_cpu: true,
            child_jobs: ChildJobPolicy::SchedulerGrant,
        }
    }

    pub(crate) fn memory_heavy() -> Self {
        Self {
            minimum_cpu_grant: 1,
            useful_cpu_ceiling: None,
            estimated_memory_bytes: 3 * 1024 * Self::MIB,
            memory_heavy: true,
            may_borrow_idle_cpu: true,
            child_jobs: ChildJobPolicy::SchedulerGrant,
        }
    }

    pub(crate) fn serial() -> Self {
        Self {
            minimum_cpu_grant: 1,
            useful_cpu_ceiling: Some(1),
            estimated_memory_bytes: 256 * Self::MIB,
            memory_heavy: false,
            may_borrow_idle_cpu: false,
            child_jobs: ChildJobPolicy::Serial,
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
                configure_child_jobs(allocation.cpu_tokens, self.profile.child_jobs);
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

struct SchedulerTrace {
    started: Instant,
    file: Option<File>,
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
        })
    }

    fn event(&mut self, event: &str) {
        let line = format!(
            "[scheduler] elapsed={:.3}s {event}",
            self.started.elapsed().as_secs_f64()
        );
        println!("{line}");
        if let Some(file) = &mut self.file {
            let _ = writeln!(file, "{line}");
            let _ = file.flush();
        }
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
    let maximum = profile.useful_cpu_ceiling.unwrap_or(budget.cpu_tokens);
    if profile.may_borrow_idle_cpu
        && envelope.pressure == PressureLevel::Healthy
        && budget.build_memory_bytes >= profile.estimated_memory_bytes
    {
        budget
            .cpu_tokens
            .min(maximum)
            .max(profile.minimum_cpu_grant)
    } else {
        profile.minimum_cpu_grant
    }
}

fn heavy_limit(pressure: PressureLevel) -> usize {
    match pressure {
        PressureLevel::Healthy => 2,
        PressureLevel::Constrained => 1,
        PressureLevel::Critical => 0,
    }
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
    if available_cpu < profile.minimum_cpu_grant
        || available_memory < profile.estimated_memory_bytes
    {
        return Err("insufficient-cpu-or-memory-budget");
    }
    let reserved_for_peers = waiting_profiles
        .filter(|peer| {
            peer.minimum_cpu_grant <= available_cpu
                && peer.estimated_memory_bytes <= available_memory
        })
        .map(|peer| peer.minimum_cpu_grant)
        .sum::<usize>();
    let maximum = profile.useful_cpu_ceiling.unwrap_or(budget.cpu_tokens);
    let borrowable = available_cpu
        .saturating_sub(profile.minimum_cpu_grant)
        .saturating_sub(reserved_for_peers);
    let cpu_tokens = if profile.may_borrow_idle_cpu && pressure == PressureLevel::Healthy {
        (profile.minimum_cpu_grant + borrowable).min(maximum)
    } else {
        profile.minimum_cpu_grant
    };
    Ok(Allocation { cpu_tokens })
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

    thread::scope(|scope| {
        loop {
            if first_error.is_none() {
                for (id, node) in &nodes {
                    if used_tokens == budget.cpu_tokens {
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
                        budget.cpu_tokens - used_tokens,
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
            let sampled = sampler.sample();
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
                    trace.event(&format!(
                        "event=build-deferred stage={id} reason=memory-heavy-limit pressure={} used_tokens={used_tokens} used_memory_bytes={used_memory}",
                        sampled.pressure.as_str()
                    ));
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
                            trace.event(&format!("event=build-deferred stage={id} reason={reason} pressure={} used_tokens={used_tokens} used_memory_bytes={used_memory}", sampled.pressure.as_str()));
                            continue;
                        }
                    };
                    let now = Instant::now();
                    account_running_builds(&mut running, now, budget.cpu_tokens - used_tokens);
                    let job = running.get_mut(&id).expect("waiting job is running");
                    job.tokens = allocation.cpu_tokens;
                    job.memory_bytes = node.profile.estimated_memory_bytes;
                    job.memory_heavy = node.profile.memory_heavy;
                    job.estimated_memory_bytes = node.profile.estimated_memory_bytes;
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
                    used_memory += node.profile.estimated_memory_bytes;
                    heavy_jobs += usize::from(node.profile.memory_heavy);
                    job.minimum_unused_tokens = budget.cpu_tokens - used_tokens;
                    let _ = job.permit.send(Some(allocation));
                    waiting.remove(&id);
                    trace.event(&format!(
                        "event=build-start stage={id} grant={} memory_bytes={} memory_heavy={} pressure={} available_memory_bytes={} used_tokens={used_tokens} used_memory_bytes={used_memory} heavy_jobs={heavy_jobs}",
                        allocation.cpu_tokens, node.profile.estimated_memory_bytes, node.profile.memory_heavy, sampled.pressure.as_str(), admission_budget.available_memory_bytes
                    ));
                }
            }

            if running.is_empty() {
                break;
            }

            match events_rx.recv_timeout(Duration::from_millis(200)) {
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => {
                    first_error = Some(anyhow!(
                        "scheduler workers disconnected before all stages completed"
                    ));
                    break;
                }
                Ok(event) => match event {
                    Event::RequestBuildResources { id } => {
                        let now = Instant::now();
                        account_running_builds(&mut running, now, budget.cpu_tokens - used_tokens);
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
                        let ending_sample = sampler.sample();
                        let now = Instant::now();
                        account_running_builds(&mut running, now, budget.cpu_tokens - used_tokens);
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
                        trace.event(&format!(
                        "event=stage-metrics stage={id} build_executed={} grant={} child_jobs={} estimated_memory_bytes={} observed_available_memory_start={} observed_available_memory_end={} observed_cgroup_memory_current_start={} observed_cgroup_memory_current_end={} observed_pressure_start={} observed_pressure_end={} resource_wait_seconds={:.3} action_seconds={action_seconds:.3} unused_tokens_avg={average_unused_tokens:.3} unused_tokens_min={minimum_unused_tokens} cpu_seconds=unavailable",
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
                estimated_memory_bytes: 256 * 1024 * 1024,
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
        let constrained = budget(12, 4 * GIB, false);
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
                heavy.estimated_memory_bytes,
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
            allocation.cpu_tokens, 10,
            "two ready standard peers retain their safe grants"
        );
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
}
