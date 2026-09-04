fn cache_impact(
    repo_root: &Path,
    specs: &[performance::StageSpec],
    requested: &str,
    json: bool,
) -> Result<()> {
    let selected = if requested == "all" {
        specs
            .iter()
            .map(|spec| spec.id.clone())
            .collect::<BTreeSet<_>>()
    } else {
        if !specs.iter().any(|spec| spec.id == requested) {
            bail!("unknown cache stage {requested}")
        }
        let mut selected = BTreeSet::from([requested.to_string()]);
        let mut changed = true;
        while changed {
            changed = false;
            for spec in specs {
                if spec
                    .dependencies
                    .iter()
                    .any(|dependency| selected.contains(dependency))
                    && selected.insert(spec.id.clone())
                {
                    changed = true;
                }
            }
        }
        selected
    };
    let historical_seconds = fs::read_to_string(repo_root.join("out/reports/build-timings.json"))
        .ok()
        .and_then(|body| serde_json::from_str::<serde_json::Value>(&body).ok())
        .and_then(|value| value.get("stages").cloned())
        .and_then(|value| value.as_array().cloned())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|record| {
            Some((
                record.get("stage")?.as_str()?.to_string(),
                record.get("wall_seconds")?.as_f64()?,
            ))
        })
        .collect::<BTreeMap<_, _>>();
    let mut estimated = 0.0;
    let mut required = 0usize;
    let mut suspicious = 0usize;
    let mut migrations = 0usize;
    performance::begin_read_only_integrity_cache();
    let result = (|| -> Result<()> {
        let impacts = specs
            .iter()
            .filter(|spec| selected.contains(&spec.id))
            .map(|spec| performance::explain_stage_impact(repo_root, spec))
            .collect::<Result<Vec<_>>>()?;
        let impact_by_stage = impacts
            .iter()
            .map(|impact| (impact.stage.as_str(), impact))
            .collect::<BTreeMap<_, _>>();
        if !json {
            println!("cache impact: {requested} (read-only; no build actions will run)");
            println!(
                "tool validity mode: {} (exact tool identities remain recorded as build provenance)",
                if stage_cache::strict_tool_identity_mode() {
                    "strict reproducible"
                } else {
                    "development artifact reuse"
                }
            );
        }
        for (spec, impact) in specs
            .iter()
            .filter(|spec| selected.contains(&spec.id))
            .zip(impacts.iter())
        {
            let previous = historical_seconds.get(&spec.id).copied().unwrap_or(0.0);
            if impact.status == "MIGRATE" {
                migrations += 1;
            } else if impact.status == "MISS" {
                estimated += previous;
                if impact.classification == "unexplained/unrelated invalidation" {
                    suspicious += 1;
                } else {
                    required += 1;
                }
            }
            let chain = spec
                .dependencies
                .iter()
                .filter(|dependency| {
                    impact.changes.iter().any(|change| {
                        change.category == "dependency-output" && change.key == dependency.as_str()
                    })
                })
                .filter_map(|dependency| {
                    impact_by_stage
                        .get(dependency.as_str())
                        .map(|_| dependency.clone())
                })
                .collect::<Vec<_>>();
            if json {
                let mut record = serde_json::to_value(impact)?;
                if let Some(object) = record.as_object_mut() {
                    object.insert(
                        "historical_seconds".to_string(),
                        serde_json::json!(previous),
                    );
                    object.insert("causal_chain".to_string(), serde_json::json!(chain));
                    object.insert(
                        "work_class".to_string(),
                        serde_json::json!(if impact.status == "MISS"
                            && impact.classification == "unexplained/unrelated invalidation"
                        {
                            "suspicious"
                        } else if impact.status == "MISS" {
                            "required"
                        } else if impact.status == "MIGRATE" {
                            "migration"
                        } else {
                            "none"
                        }),
                    );
                }
                println!("{}", serde_json::to_string(&record)?);
            } else if impact.status != "HIT" {
                println!(
                    "{:<7} {:<24} class={:<32} historical={previous:.1}s reason={} changes={} chain={}",
                    impact.status,
                    spec.id,
                    impact.classification,
                    impact.reason,
                    serde_json::to_string(&impact.changes)?,
                    serde_json::to_string(&chain)?,
                );
            }
        }
        if !json {
            println!(
                "totals: selected={} misses={} required={} suspicious={} migrations={} historical_estimate={estimated:.1}s",
                selected.len(),
                required + suspicious,
                required,
                suspicious,
                migrations
            );
        } else {
            println!(
                "{}",
                serde_json::json!({
                    "type": "summary",
                    "selected": selected.len(),
                    "misses": required + suspicious,
                    "required": required,
                    "suspicious": suspicious,
                    "migrations": migrations,
                    "historical_seconds": estimated,
                })
            );
        }
        Ok(())
    })();
    performance::end_read_only_integrity_cache();
    result?;
    println!(
        "downstream propagation: {} stage(s); historical estimated work: {:.1}s",
        selected.len(),
        estimated
    );
    Ok(())
}

fn cache_command(repo_root: &Path, command: CacheCommands) -> Result<()> {
    let specs = cacheable_stage_specs(repo_root)?;
    match command {
        CacheCommands::Status => {
            for spec in &specs {
                println!("{}", performance::explain_stage(repo_root, spec)?);
            }
            packaging::print_package_cache_status(repo_root)?;
            println!("{}", packaging::package_facts_status(repo_root)?);
            println!("{}", elf_cache::status(repo_root)?);
            println!(
                "rootfs-base: not materialized separately; live assembly currently consumes package staging directly"
            );
            Ok(())
        }
        CacheCommands::Explain { stage, details } => {
            if let Some(package) = stage.strip_prefix("package:") {
                return packaging::explain_package_cache(repo_root, package);
            }
            if stage == "elf-facts" {
                println!("{}", elf_cache::status(repo_root)?);
                return Ok(());
            }
            if stage == "package-audit" {
                println!("{}", packaging::package_facts_status(repo_root)?);
                return Ok(());
            }
            if stage == "rootfs-base" || stage == "rootfs-live" {
                let spec = resolve_cache_stage(&specs, "rootfs")?;
                if details {
                    print!("{}", performance::explain_stage_details(repo_root, spec)?);
                } else {
                    println!("{}", performance::explain_stage(repo_root, spec)?);
                }
                return Ok(());
            }
            let spec = resolve_cache_stage(&specs, &stage)?;
            if details {
                print!("{}", performance::explain_stage_details(repo_root, spec)?);
            } else {
                println!("{}", performance::explain_stage(repo_root, spec)?);
            }
            Ok(())
        }
        CacheCommands::Impact { json, stage } => cache_impact(repo_root, &specs, &stage, json),
        CacheCommands::Invalidate { dependents, stage } => {
            if let Some(package) = stage.strip_prefix("package:") {
                return packaging::invalidate_package_cache(repo_root, package);
            }
            if stage == "elf-facts" {
                println!(
                    "invalidated {} ELF fact record(s)",
                    elf_cache::invalidate(repo_root)?
                );
                return Ok(());
            }
            if stage == "package-audit" {
                println!(
                    "invalidated {} package fact/audit record(s)",
                    packaging::invalidate_package_facts(repo_root)?
                );
                return Ok(());
            }
            let stage = if stage == "rootfs-base" || stage == "rootfs-live" {
                "rootfs".to_string()
            } else {
                stage
            };
            let root = resolve_cache_stage(&specs, &stage)?.id.clone();
            let mut selected = BTreeSet::from([root.clone()]);
            if dependents {
                loop {
                    let before = selected.len();
                    for spec in &specs {
                        if spec
                            .dependencies
                            .iter()
                            .any(|dependency| selected.contains(dependency))
                        {
                            selected.insert(spec.id.clone());
                        }
                    }
                    if selected.len() == before {
                        break;
                    }
                }
            }
            for stage in selected {
                if performance::invalidate_manifest(repo_root, &stage)? {
                    println!("invalidated cache manifest: {stage}");
                } else {
                    println!("cache manifest was already absent: {stage}");
                }
            }
            println!(
                "build outputs were preserved; the next dependency-correct build will refresh them"
            );
            Ok(())
        }
    }
}

fn resolve_cache_stage<'a>(
    specs: &'a [performance::StageSpec],
    supplied: &str,
) -> Result<&'a performance::StageSpec> {
    let normalized = if supplied == "gcc-toolchain" {
        "gcc-compiler"
    } else if supplied == "kernel" {
        "linux"
    } else {
        supplied
    };
    specs
        .iter()
        .find(|spec| spec.id == normalized)
        .ok_or_else(|| anyhow!("unknown cache stage {supplied}"))
}
