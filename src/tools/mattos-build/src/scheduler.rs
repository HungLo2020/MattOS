use anyhow::{Result, anyhow, bail};
use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Instant;

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
    pub(crate) weight: usize,
    pub(crate) memory_heavy: bool,
    pub(crate) child_jobs: ChildJobPolicy,
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
    requested_weight: usize,
    memory_heavy: bool,
    child_jobs: ChildJobPolicy,
    events: Sender<Event>,
    permit: Receiver<bool>,
}

impl JobContext {
    pub(crate) fn acquire_build_resources(&self) -> Result<()> {
        self.events
            .send(Event::RequestBuildResources {
                id: self.id.clone(),
                weight: self.requested_weight,
                memory_heavy: self.memory_heavy,
            })
            .map_err(|_| anyhow!("scheduler stopped before {} could request resources", self.id))?;
        match self.permit.recv() {
            Ok(true) => {
                configure_child_jobs(self.requested_weight, self.child_jobs);
                Ok(())
            }
            Ok(false) | Err(_) => bail!("scheduler cancelled {} before its build action", self.id),
        }
    }
}

enum Event {
    RequestBuildResources {
        id: String,
        weight: usize,
        memory_heavy: bool,
    },
    Finished {
        id: String,
        result: Result<()>,
    },
}

struct RunningJob {
    tokens: usize,
    memory_heavy: bool,
    permit: Sender<bool>,
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

pub(crate) fn validate(nodes: &[SchedulerNode], token_budget: usize) -> Result<()> {
    if token_budget == 0 {
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
        if node.weight == 0 || node.weight > token_budget {
            bail!(
                "scheduler stage {} has invalid weight {} for budget {}",
                node.id,
                node.weight,
                token_budget
            );
        }
        if matches!(node.child_jobs, ChildJobPolicy::Capped(0)) {
            bail!("scheduler stage {} has a zero child-job cap", node.id);
        }
        for dependency in &node.dependencies {
            if !by_id.contains_key(dependency.as_str()) {
                bail!("scheduler stage {} depends on unknown {}", node.id, dependency);
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
    token_budget: usize,
) -> Result<SimulationReport> {
    validate(nodes, token_budget)?;
    for node in nodes {
        if !durations.get(&node.id).is_some_and(|duration| *duration >= 0.0) {
            bail!("scheduler simulation is missing a valid duration for {}", node.id);
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
    let mut running = BTreeMap::<String, (f64, usize, bool)>::new();
    let mut stable_nodes = nodes.iter().collect::<Vec<_>>();
    stable_nodes.sort_by(|left, right| left.id.cmp(&right.id));
    while complete.len() < nodes.len() {
        let used_tokens = running.values().map(|(_, tokens, _)| tokens).sum::<usize>();
        let heavy_jobs = running.values().filter(|(_, _, heavy)| *heavy).count();
        let mut available_tokens = token_budget - used_tokens;
        let mut available_heavy = 2usize.saturating_sub(heavy_jobs);
        for node in &stable_nodes {
            if complete.contains(&node.id)
                || running.contains_key(&node.id)
                || node.weight > available_tokens
                || (node.memory_heavy && available_heavy == 0)
                || !node
                    .dependencies
                    .iter()
                    .all(|dependency| complete.contains(dependency))
            {
                continue;
            }
            running.insert(
                node.id.clone(),
                (now + durations[&node.id], node.weight, node.memory_heavy),
            );
            available_tokens -= node.weight;
            available_heavy -= usize::from(node.memory_heavy);
        }
        let next_finish = running
            .values()
            .map(|(finish, _, _)| *finish)
            .min_by(f64::total_cmp)
            .ok_or_else(|| anyhow!("scheduler simulation made no progress"))?;
        now = next_finish;
        let finished = running
            .iter()
            .filter(|(_, (finish, _, _))| *finish == now)
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

pub(crate) fn execute<F>(nodes: Vec<SchedulerNode>, token_budget: usize, action: F) -> Result<()>
where
    F: Fn(&str, &JobContext) -> Result<()> + Sync,
{
    validate(&nodes, token_budget)?;
    let mut trace = SchedulerTrace::start()?;
    trace.event(&format!(
        "event=validated nodes={} token_budget={} heavy_limit=2",
        nodes.len(), token_budget
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
    let mut heavy_jobs = 0usize;
    let mut first_error = None;

    thread::scope(|scope| {
        loop {
            if first_error.is_none() {
                for (id, node) in &nodes {
                    if used_tokens == token_budget {
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
                            memory_heavy: false,
                            permit: permit_tx,
                            wait_started: None,
                            resource_wait_seconds: 0.0,
                            build_started: None,
                            last_accounted: None,
                            unused_token_seconds: 0.0,
                            minimum_unused_tokens: token_budget,
                        },
                    );
                    launched.insert(id.clone());
                    account_running_builds(
                        &mut running,
                        Instant::now(),
                        token_budget - used_tokens,
                    );
                    used_tokens += 1;
                    trace.event(&format!(
                        "event=evaluation-dispatch stage={id} used_tokens={used_tokens} heavy_jobs={heavy_jobs}"
                    ));
                    let id = id.clone();
                    let events = events_tx.clone();
                    let action = &action;
                    let requested_weight = node.weight;
                    let memory_heavy = node.memory_heavy;
                    let child_jobs = node.child_jobs;
                    scope.spawn(move || {
                        let context = JobContext {
                            id: id.clone(),
                            requested_weight,
                            memory_heavy,
                            child_jobs,
                            events: events.clone(),
                            permit: permit_rx,
                        };
                        let result = action(&id, &context);
                        let _ = events.send(Event::Finished { id, result });
                    });
                }
            }

            let waiting_ids = waiting.iter().cloned().collect::<Vec<_>>();
            for id in waiting_ids {
                let node = &nodes[&id];
                if first_error.is_some() {
                    if let Some(job) = running.get(&id) {
                        let _ = job.permit.send(false);
                    }
                    waiting.remove(&id);
                    trace.event(&format!(
                        "event=build-cancel stage={id} used_tokens={used_tokens} heavy_jobs={heavy_jobs}"
                    ));
                } else if used_tokens + node.weight <= token_budget
                    && (!node.memory_heavy || heavy_jobs < 2)
                {
                    let now = Instant::now();
                    account_running_builds(&mut running, now, token_budget - used_tokens);
                    let job = running.get_mut(&id).expect("waiting job is running");
                    job.tokens = node.weight;
                    job.memory_heavy = node.memory_heavy;
                    job.resource_wait_seconds = job
                        .wait_started
                        .take()
                        .map(|started| now.duration_since(started).as_secs_f64())
                        .unwrap_or(0.0);
                    job.build_started = Some(now);
                    job.last_accounted = Some(now);
                    used_tokens += node.weight;
                    heavy_jobs += usize::from(node.memory_heavy);
                    job.minimum_unused_tokens = token_budget - used_tokens;
                    let _ = job.permit.send(true);
                    waiting.remove(&id);
                    trace.event(&format!(
                        "event=build-start stage={id} weight={} memory_heavy={} used_tokens={used_tokens} heavy_jobs={heavy_jobs}",
                        node.weight, node.memory_heavy
                    ));
                }
            }

            if running.is_empty() {
                break;
            }

            match events_rx.recv().expect("scheduler workers retain event senders") {
                Event::RequestBuildResources {
                    id,
                    weight,
                    memory_heavy,
                } => {
                    let now = Instant::now();
                    account_running_builds(&mut running, now, token_budget - used_tokens);
                    let job = running.get_mut(&id).expect("resource requester is running");
                    used_tokens -= job.tokens;
                    job.tokens = 0;
                    job.wait_started = Some(now);
                    debug_assert_eq!(weight, nodes[&id].weight);
                    debug_assert_eq!(memory_heavy, nodes[&id].memory_heavy);
                    trace.event(&format!(
                        "event=build-wait stage={id} weight={weight} memory_heavy={memory_heavy} used_tokens={used_tokens} heavy_jobs={heavy_jobs}"
                    ));
                    waiting.insert(id);
                }
                Event::Finished { id, result } => {
                    let now = Instant::now();
                    account_running_builds(&mut running, now, token_budget - used_tokens);
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
                        token_budget.saturating_sub(used_tokens) as f64
                    };
                    let minimum_unused_tokens = if job.build_started.is_some() {
                        job.minimum_unused_tokens
                    } else {
                        token_budget.saturating_sub(used_tokens)
                    };
                    used_tokens -= job.tokens;
                    heavy_jobs -= usize::from(job.memory_heavy);
                    trace.event(&format!(
                        "event=stage-metrics stage={id} build_executed={} weight={} child_jobs={} resource_wait_seconds={:.3} action_seconds={action_seconds:.3} unused_tokens_avg={average_unused_tokens:.3} unused_tokens_min={minimum_unused_tokens} cpu_seconds=unavailable",
                        job.build_started.is_some(),
                        node.weight,
                        child_job_limit_for(node.weight, node.child_jobs),
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

    fn node(id: &str, dependencies: &[&str], weight: usize) -> SchedulerNode {
        SchedulerNode {
            id: id.to_string(),
            dependencies: dependencies.iter().map(|value| value.to_string()).collect(),
            outputs: vec![PathBuf::from(format!("out/{id}"))],
            weight,
            memory_heavy: false,
            child_jobs: ChildJobPolicy::SchedulerGrant,
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
                memory_heavy: false,
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
        assert!(validate(&[node("a", &["missing"], 1)], 12).is_err());
        assert!(validate(&[node("a", &["b"], 1), node("b", &["a"], 1)], 12).is_err());
        assert!(validate(&[node("a", &[], 13)], 12).is_err());
        let mut left = node("a", &[], 1);
        let mut right = node("b", &[], 1);
        left.outputs = vec!["out/shared".into()];
        right.outputs = vec!["out/shared/child".into()];
        assert!(validate(&[left, right], 12).is_err());
    }

    #[test]
    fn dependency_publication_precedes_consumer_and_order_is_stable() {
        let events = Arc::new(Mutex::new(Vec::new()));
        execute(
            vec![node("b", &[], 1), node("a", &[], 1), node("consumer", &["a", "b"], 1)],
            1,
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
        execute(vec![node("a", &[], 1), node("b", &[], 1)], 2, |_, _| {
            barrier.wait();
            Ok(())
        })
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
            node.memory_heavy = true;
        }
        execute(nodes, 12, |_, context| {
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
        })
        .unwrap();
        assert_eq!(*maximum.lock().unwrap(), (8, 2));
    }

    #[test]
    fn cache_hits_do_not_request_full_stage_weight() {
        let misses_started = Arc::new(Mutex::new(0usize));
        execute(vec![node("hit", &[], 12), node("miss", &[], 12)], 12, |id, context| {
            if id == "miss" {
                context.acquire_build_resources()?;
                *misses_started.lock().unwrap() += 1;
            }
            Ok(())
        })
        .unwrap();
        assert_eq!(*misses_started.lock().unwrap(), 1);
    }

    #[test]
    fn failure_stops_dispatch_and_drains_running_jobs() {
        let ran = Arc::new(Mutex::new(Vec::new()));
        let result = execute(
            vec![node("a-fail", &[], 1), node("b-running", &[], 1), node("z-blocked", &["a-fail"], 1)],
            2,
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
