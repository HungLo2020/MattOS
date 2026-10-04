use anyhow::{Context, Result, bail};
use chrono::Utc;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::cache_manifest::{
    DependencyIdentity, STAGE_MANIFEST_SCHEMA_VERSION, StageInputDetails, StageInputs,
    StageManifest, StageSpec,
};
use crate::performance::{
    atomic_write_json, diagnostic_path, digest_paths, digest_serializable, digest_source_inputs,
    invalidate_integrity_paths, inventory_digest, measured, normalized_build_environment,
    output_inventory, record_category, record_timing, sanitize_identifier, tool_identities,
    with_stage_log,
};
use crate::timing::TimingRecord;

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
    execute_cached_stage_with_resources(repo_root, spec, validate_reuse, || Ok(()), action)
}

pub(crate) fn execute_cached_stage_with_resources<F, V, R>(
    repo_root: &Path,
    spec: &StageSpec,
    validate_reuse: V,
    acquire_resources: R,
    action: F,
) -> Result<()>
where
    F: FnOnce() -> Result<()>,
    V: Fn() -> Result<()>,
    R: FnOnce() -> Result<()>,
{
    let started_at = Utc::now();
    let timer = Instant::now();
    let input_timer = Instant::now();
    let evaluation = measured("input_hashing", || {
        compute_stage_evaluation(repo_root, spec)
    })?;
    record_category(&format!("input_stage:{}", spec.id), input_timer.elapsed());
    let inputs = evaluation.inputs.clone();
    let manifest_path = stage_manifest_path(repo_root, &spec.id);
    let mut reason;
    let mut reused_digest = None;

    if let Ok(mut manifest) = read_stage_manifest(repo_root, &spec.id) {
        // A manifest rekey is safe only when the *published* output still
        // matches the manifest as well as the narrowed input contract.  The
        // build-directory stamps used by Autotools, CMake, and Meson are
        // private implementation state; they may run only after this layer
        // has already made a real cache-miss decision.  In particular, never
        // label a changed install tree as a migration and then enter a helper
        // which has to configure or compile it.
        if can_migrate_narrowed_manifest(repo_root, &evaluation, &manifest)?
            && cached_output_miss_reason(repo_root, spec, &manifest)?.is_empty()
        {
            // A rekey does not rebuild anything: the recorded tool identities
            // still describe the tools that built the published output.
            let built_with = std::mem::take(&mut manifest.input_details.tools);
            manifest.inputs = inputs.clone();
            manifest.input_details = evaluation.details.clone();
            manifest.input_details.tools = built_with;
            write_stage_manifest(repo_root, &manifest)?;
        }
        reason = measured("output_inventory_hashing", || {
            cache_miss_reason(repo_root, spec, &inputs, &manifest)
        })?;
        if !reason.is_empty() {
            let details = changed_input_summary(&manifest.input_details, &evaluation.details);
            if !details.is_empty() {
                reason.push_str(&format!("; changed inputs: {details}"));
            }
        }
        if reason.is_empty() {
            match measured("semantic_validation", validate_reuse) {
                Ok(()) => {
                    reason = "full input digest matched; output inventory and lightweight validation passed"
                        .to_string();
                    record_tool_drift(&spec.id, &manifest.input_details.tools, &evaluation.details.tools);
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
    if let Ok(manifest) = read_stage_manifest(repo_root, &spec.id) {
        warn_host_built_toolchain_drift(&spec.id, &manifest.input_details.tools, &evaluation.details.tools);
    }
    acquire_resources()?;
    invalidate_integrity_paths(repo_root, &spec.outputs);
    crate::performance::trace_log_context("cached-stage-before-with-stage-log");
    let child_jobs = crate::scheduler::child_job_limit();
    let cgroup = crate::stage_memory::StageCgroup::create(
        &spec.id,
        crate::resources::discover().effective_available_memory_bytes(),
    );
    let result = measured("stage_actions", || {
        crate::stage_memory::StageCgroup::enter(cgroup.as_ref(), || with_stage_log(repo_root, &spec.id, action))
    });
    if let Some(peak_bytes) = cgroup.as_ref().and_then(crate::stage_memory::StageCgroup::peak_bytes)
        && result.is_ok()
    {
        crate::stage_memory::record(
            repo_root,
            &spec.id,
            crate::stage_memory::StageMemoryRecord { peak_bytes, child_jobs },
        )?;
    }
    drop(cgroup);
    let mut output_digest = None;
    if result.is_ok() {
        finalize_stage_outputs(repo_root, &spec.outputs)?;
        let inventory = measured("output_inventory_hashing", || {
            output_inventory(repo_root, &spec.outputs)
        })?;
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
    cached_output_miss_reason(repo_root, spec, manifest)
}

/// Validate only the published-output half of the cache contract.
///
/// This is deliberately shared by the planner and executor.  A migration is
/// a metadata-only rekey of a proven published artifact; if this check fails,
/// the stage is a real miss and its private build helper is allowed to run.
fn cached_output_miss_reason(
    repo_root: &Path,
    spec: &StageSpec,
    manifest: &StageManifest,
) -> Result<String> {
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

pub(crate) fn can_migrate_narrowed_manifest(
    repo_root: &Path,
    current: &StageEvaluation,
    manifest: &StageManifest,
) -> Result<bool> {
    if manifest.schema_version != STAGE_MANIFEST_SCHEMA_VERSION
        || manifest.input_details.schema_version == 0
        || (strict_tool_identity_mode()
            && !tool_details_compatible(&manifest.input_details.tools, &current.details.tools)
            && manifest.inputs.tool_digest != current.inputs.tool_digest)
        || (manifest_exposes_compiler_private_output(manifest)
            && !tool_details_compatible(&manifest.input_details.tools, &current.details.tools)
            && manifest.inputs.tool_digest != current.inputs.tool_digest)
        || manifest.inputs.environment_digest != current.inputs.environment_digest
        || !current
            .inputs
            .dependency_digests
            .iter()
            .all(|(stage, digest)| {
                manifest.inputs.dependency_digests.get(stage) == Some(digest)
                    || (stage == crate::TARGET_TOOLCHAIN_CACHE_KEY
                        && !manifest.inputs.dependency_digests.contains_key(stage))
            })
    {
        return Ok(false);
    }
    // Manifests from before the target toolchain was a cache key adopt it
    // only when the workspace guard recorded that toolchain for this stage.
    if current
        .inputs
        .dependency_digests
        .contains_key(crate::TARGET_TOOLCHAIN_CACHE_KEY)
        && !manifest
            .inputs
            .dependency_digests
            .contains_key(crate::TARGET_TOOLCHAIN_CACHE_KEY)
        && !crate::stage_built_by_current_target_toolchain(repo_root, &manifest.stage)?
    {
        return Ok(false);
    }
    let legacy_recipe = format!(
        "mattos-build-stage:{}:schema={}",
        manifest.stage, STAGE_MANIFEST_SCHEMA_VERSION
    );
    if manifest.input_details.recipe != current.details.recipe
        && manifest.input_details.recipe != legacy_recipe
    {
        return Ok(false);
    }
    // Manifests from before recipe projection recorded whole-file digests,
    // and v1 projections hashed a wider view than v2: in both cases an
    // unchanged wider view proves the narrower current view unchanged too.
    let mut current_source = current.details.source.clone();
    for (path, digest) in current_source.iter_mut() {
        let Some(stored) = manifest.input_details.source.get(path) else {
            continue;
        };
        if digest == stored {
            continue;
        }
        let unchanged_whole_file = !crate::recipe_projection::is_projection_digest(stored)
            && crate::recipe_projection::is_projection_digest(digest)
            && digest_source_inputs(repo_root, &[PathBuf::from(path)])? == *stored;
        if unchanged_whole_file
            || crate::recipe_projection::legacy_projection_matches(
                repo_root,
                &manifest.stage,
                Path::new(path),
                stored,
            )?
        {
            digest.clone_from(stored);
        }
    }
    // The coverage marker's version itself is adopted below, with the files
    // the newer coverage adds; it is not a change to an existing input.
    if let (Some(stored), Some(current)) = (
        manifest
            .input_details
            .source
            .get(crate::recipe_projection::IMPLICIT_RECIPE_COVERAGE_KEY),
        current_source.get_mut(crate::recipe_projection::IMPLICIT_RECIPE_COVERAGE_KEY),
    ) {
        current.clone_from(stored);
    }
    if !shared_values_match(&manifest.input_details.source, &current_source)
        || !shared_values_match(
            &manifest.input_details.configuration,
            &current.details.configuration,
        )
    {
        return Ok(false);
    }
    let removed_sources = manifest
        .input_details
        .source
        .keys()
        .filter(|path| !current.details.source.contains_key(*path))
        .collect::<Vec<_>>();
    // A manifest recorded before implicit recipe coverage adopts the recipe
    // files its stage reaches but did not list, once: those files were
    // already the code this output was built by.  Stages known to be stale
    // relative to such a file are rebuilt with a `recipe_revision` bump.
    // A newer coverage version (v2 added `stages/helpers/`) is adopted the
    // same way, once.
    let coverage_key = crate::recipe_projection::IMPLICIT_RECIPE_COVERAGE_KEY;
    let adopts_implicit_coverage = current.details.source.get(coverage_key).is_some_and(|current| {
        manifest.input_details.source.get(coverage_key) != Some(current)
    });
    for added in current
        .details
        .source
        .keys()
        .filter(|path| !manifest.input_details.source.contains_key(*path))
    {
        if adopts_implicit_coverage
            && (added == crate::recipe_projection::IMPLICIT_RECIPE_COVERAGE_KEY
                || added.starts_with("src/tools/mattos-build/src/stages/"))
        {
            continue;
        }
        if !removed_sources
            .iter()
            .any(|root| Path::new(added).starts_with(root))
        {
            return Ok(false);
        }
    }
    for path in removed_sources {
        let current_digest = digest_source_inputs(repo_root, &[PathBuf::from(path)])?;
        if manifest.input_details.source.get(path) != Some(&current_digest) {
            return Ok(false);
        }
    }
    Ok(true)
}

fn manifest_exposes_compiler_private_output(manifest: &StageManifest) -> bool {
    // Installed Rust compiler-private artifacts are validated against the
    // installed MattOS rustc at reuse time. The host/bootstrap rustc is not
    // their compatibility authority, so it is provenance-only in development
    // mode.
    let _ = manifest;
    false
}

fn tool_details_compatible(
    stored: &BTreeMap<String, crate::cache_manifest::ToolIdentity>,
    current: &BTreeMap<String, crate::cache_manifest::ToolIdentity>,
) -> bool {
    if stored.len() != current.len() {
        return false;
    }
    stored.iter().all(|(name, old)| {
        let Some(now) = current.get(name) else {
            return false;
        };
        if old == now {
            return true;
        }
        if name != "cargo" {
            return false;
        }
        let old_path = old.resolved_path.as_str();
        let old_is_known_proxy =
            old_path.ends_with("/rustup") || old_path.ends_with("/out/source-ownership/bin/cargo");
        if !old_is_known_proxy || !now.version.starts_with("cargo ") {
            return false;
        }
        if old.version.starts_with("cargo ") {
            old.version == now.version && old.target == now.target
        } else {
            // A rustup proxy's own version does not identify Cargo. It is
            // equivalent only when the current active rustup toolchain
            // resolves to the exact Cargo binary now being fingerprinted.
            std::process::Command::new("rustup")
                .args(["which", "cargo"])
                .output()
                .ok()
                .filter(|output| output.status.success())
                .map(|output| {
                    let resolved = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    Path::new(&resolved)
                        .canonicalize()
                        .ok()
                        .map(|path| path.to_string_lossy().replace('\\', "/"))
                        == Path::new(&now.resolved_path)
                            .canonicalize()
                            .ok()
                            .map(|path| path.to_string_lossy().replace('\\', "/"))
                })
                .unwrap_or(false)
        }
    })
}

fn shared_values_match<T: PartialEq>(
    stored: &BTreeMap<String, T>,
    current: &BTreeMap<String, T>,
) -> bool {
    stored
        .iter()
        .all(|(key, value)| current.get(key).is_none_or(|current| current == value))
}

pub(crate) fn changed_input_summary(
    stored: &StageInputDetails,
    current: &StageInputDetails,
) -> String {
    let mut changes = Vec::new();
    if stored.recipe != current.recipe {
        changes.push("recipe".to_string());
    }
    collect_changed_keys("source", &stored.source, &current.source, &mut changes);
    collect_changed_keys(
        "configuration",
        &stored.configuration,
        &current.configuration,
        &mut changes,
    );
    collect_changed_keys(
        "environment",
        &stored.environment,
        &current.environment,
        &mut changes,
    );
    collect_changed_keys("tool", &stored.tools, &current.tools, &mut changes);
    collect_changed_keys(
        "dependency",
        &stored.dependencies,
        &current.dependencies,
        &mut changes,
    );
    changes.join(", ")
}

fn collect_changed_keys<T: PartialEq>(
    group: &str,
    stored: &BTreeMap<String, T>,
    current: &BTreeMap<String, T>,
    changes: &mut Vec<String>,
) {
    for key in stored.keys().chain(current.keys()).collect::<BTreeSet<_>>() {
        if stored.get(key) != current.get(key) {
            changes.push(format!("{group}:{key}"));
        }
    }
}

pub(crate) struct StageEvaluation {
    pub(crate) inputs: StageInputs,
    pub(crate) details: StageInputDetails,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct StageInputChange {
    pub(crate) category: String,
    pub(crate) key: String,
    pub(crate) stored: serde_json::Value,
    pub(crate) current: serde_json::Value,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct StageImpact {
    pub(crate) stage: String,
    pub(crate) status: String,
    pub(crate) classification: String,
    pub(crate) reason: String,
    pub(crate) stored_input_digest: Option<String>,
    pub(crate) current_input_digest: String,
    pub(crate) stored_output_digest: Option<String>,
    pub(crate) changes: Vec<StageInputChange>,
    pub(crate) migration_eligible: bool,
}

pub(crate) fn explain_stage_impact(repo_root: &Path, spec: &StageSpec) -> Result<StageImpact> {
    let evaluation = compute_stage_evaluation(repo_root, spec)?;
    let manifest = match read_stage_manifest(repo_root, &spec.id) {
        Ok(manifest) => manifest,
        Err(error) => {
            return Ok(StageImpact {
                stage: spec.id.clone(),
                status: "MISS".to_string(),
                classification: "expected one-time migration".to_string(),
                reason: format!("no valid stage manifest: {error:#}"),
                stored_input_digest: None,
                current_input_digest: evaluation.inputs.full_digest,
                stored_output_digest: None,
                changes: Vec::new(),
                migration_eligible: false,
            });
        }
    };
    let reason = cache_miss_reason(repo_root, spec, &evaluation.inputs, &manifest)?;
    let changes = stage_input_changes(&manifest, &evaluation.details);
    if reason.is_empty() {
        return Ok(StageImpact {
            stage: spec.id.clone(),
            status: "HIT".to_string(),
            classification: "reusable".to_string(),
            reason: "input digest, output inventory, and output digest match".to_string(),
            stored_input_digest: Some(manifest.inputs.full_digest),
            current_input_digest: evaluation.inputs.full_digest,
            stored_output_digest: Some(manifest.output_content_digest),
            changes,
            migration_eligible: false,
        });
    }
    let migration_candidate = can_migrate_narrowed_manifest(repo_root, &evaluation, &manifest)?;
    let output_reusable = cached_output_miss_reason(repo_root, spec, &manifest)?.is_empty();
    let migration_eligible = migration_candidate && output_reusable;
    if migration_eligible {
        return Ok(StageImpact {
            stage: spec.id.clone(),
            status: "MIGRATE".to_string(),
            classification: "expected one-time migration".to_string(),
            reason: format!("{reason}; cached output is reusable after atomic manifest rekey"),
            stored_input_digest: Some(manifest.inputs.full_digest),
            current_input_digest: evaluation.inputs.full_digest,
            stored_output_digest: Some(manifest.output_content_digest),
            changes,
            migration_eligible: true,
        });
    }
    let classification = if migration_candidate && !output_reusable {
        "published output change"
    } else if manifest.schema_version != STAGE_MANIFEST_SCHEMA_VERSION
        || manifest.input_details.recipe != evaluation.details.recipe
    {
        "recipe/schema change"
    } else if changes.iter().any(|change| change.category == "source") {
        "source change"
    } else if changes
        .iter()
        .any(|change| change.category == "ownership-contract")
    {
        "ownership-contract change"
    } else if changes
        .iter()
        .any(|change| change.category == "dependency-output")
    {
        "dependency-output change"
    } else if changes
        .iter()
        .any(|change| change.category == "tool identity")
    {
        "tool identity"
    } else if changes
        .iter()
        .any(|change| change.category == "configuration")
    {
        "configuration change"
    } else {
        "unexplained/unrelated invalidation"
    };
    Ok(StageImpact {
        stage: spec.id.clone(),
        status: "MISS".to_string(),
        classification: classification.to_string(),
        reason,
        stored_input_digest: Some(manifest.inputs.full_digest),
        current_input_digest: evaluation.inputs.full_digest,
        stored_output_digest: Some(manifest.output_content_digest),
        changes,
        migration_eligible,
    })
}

fn stage_input_changes(
    manifest: &StageManifest,
    current: &StageInputDetails,
) -> Vec<StageInputChange> {
    let mut changes = Vec::new();
    if manifest.input_details.recipe != current.recipe {
        changes.push(StageInputChange {
            category: "recipe/schema".to_string(),
            key: "recipe".to_string(),
            stored: serde_json::json!(manifest.input_details.recipe),
            current: serde_json::json!(current.recipe),
        });
    }
    collect_detail_changes(
        "source",
        &manifest.input_details.source,
        &current.source,
        &mut changes,
    );
    for (key, stored) in &manifest.input_details.configuration {
        if current.configuration.get(key) != Some(stored) {
            let category = if key.contains("out/source-ownership/cargo/contracts/") {
                "ownership-contract"
            } else {
                "configuration"
            };
            changes.push(StageInputChange {
                category: category.to_string(),
                key: key.clone(),
                stored: serde_json::json!(stored),
                current: current
                    .configuration
                    .get(key)
                    .map_or(serde_json::Value::Null, |value| serde_json::json!(value)),
            });
        }
    }
    for (key, value) in &current.configuration {
        if !manifest.input_details.configuration.contains_key(key) {
            let category = if key.contains("out/source-ownership/cargo/contracts/") {
                "ownership-contract"
            } else {
                "configuration"
            };
            changes.push(StageInputChange {
                category: category.to_string(),
                key: key.clone(),
                stored: serde_json::Value::Null,
                current: serde_json::json!(value),
            });
        }
    }
    collect_detail_changes(
        "configuration",
        &manifest.input_details.environment,
        &current.environment,
        &mut changes,
    );
    collect_detail_changes(
        "tool identity",
        &manifest.input_details.tools,
        &current.tools,
        &mut changes,
    );
    collect_detail_changes(
        "dependency-output",
        &manifest.input_details.dependencies,
        &current.dependencies,
        &mut changes,
    );
    changes
}

fn collect_detail_changes<T: Serialize + PartialEq>(
    category: &str,
    stored: &BTreeMap<String, T>,
    current: &BTreeMap<String, T>,
    changes: &mut Vec<StageInputChange>,
) {
    let keys = stored.keys().chain(current.keys()).collect::<BTreeSet<_>>();
    for key in keys {
        if stored.get(key) != current.get(key) {
            changes.push(StageInputChange {
                category: category.to_string(),
                key: key.clone(),
                stored: stored.get(key).map_or(serde_json::Value::Null, |value| {
                    serde_json::to_value(value).unwrap()
                }),
                current: current.get(key).map_or(serde_json::Value::Null, |value| {
                    serde_json::to_value(value).unwrap()
                }),
            });
        }
    }
}

pub(crate) fn compute_stage_inputs(repo_root: &Path, spec: &StageSpec) -> Result<StageInputs> {
    Ok(compute_stage_evaluation(repo_root, spec)?.inputs)
}

pub(crate) fn compute_stage_evaluation(
    repo_root: &Path,
    spec: &StageSpec,
) -> Result<StageEvaluation> {
    let source_timer = Instant::now();
    let mut source = BTreeMap::new();
    let mut projected = false;
    for path in &spec.source_inputs {
        let digest = match crate::recipe_projection::projected_recipe_digest(repo_root, &spec.id, path)? {
            Some(digest) => {
                projected = true;
                digest
            }
            None => digest_source_inputs(repo_root, std::slice::from_ref(path))?,
        };
        source.insert(diagnostic_path(repo_root, path), digest);
    }
    // Recipe files the stage calls into without listing them contribute their
    // projection too, so shared recipe helpers are always part of the key.
    let implicit = crate::recipe_projection::implicit_recipe_inputs(
        repo_root,
        &spec.id,
        &spec.source_inputs,
    )?;
    for path in &implicit {
        // A file whose every item the stage reaches has no projection (the
        // projection would be the whole file); it is then hashed whole, like
        // a listed input.  Skipping it left fully reachable helpers such as
        // `helpers/meson.rs` out of every key.
        let digest = match crate::recipe_projection::projected_recipe_digest(repo_root, &spec.id, path)? {
            Some(digest) => digest,
            None => digest_source_inputs(repo_root, std::slice::from_ref(path))?,
        };
        source.insert(diagnostic_path(repo_root, path), digest);
        projected = true;
    }
    if !implicit.is_empty() {
        source.insert(
            crate::recipe_projection::IMPLICIT_RECIPE_COVERAGE_KEY.to_string(),
            crate::recipe_projection::IMPLICIT_RECIPE_COVERAGE_VERSION.to_string(),
        );
    }
    // A shared recipe file contributes only this stage's projection of it.
    let source_digest = if projected {
        digest_serializable(&source)?
    } else {
        digest_source_inputs(repo_root, &spec.source_inputs)?
    };
    record_category("input_phase:source", source_timer.elapsed());
    let configuration_timer = Instant::now();
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
    record_category("input_phase:configuration", configuration_timer.elapsed());
    let tool_timer = Instant::now();
    let tools = tool_identities(&spec.tools)?;
    let build_provenance_digest = digest_serializable(&tools)?;
    let validity_tools = validity_tool_identities(spec, &tools, strict_tool_identity_mode());
    let tool_digest = digest_serializable(&validity_tools)?;
    record_category("input_phase:tools", tool_timer.elapsed());
    let dependency_timer = Instant::now();
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
    if let Some(identity) = crate::target_toolchain_cache_identity(repo_root, &spec.id)? {
        let key = crate::TARGET_TOOLCHAIN_CACHE_KEY.to_string();
        dependency_digests.insert(key.clone(), identity.clone());
        dependencies.insert(
            key,
            DependencyIdentity {
                input_digest: identity.clone(),
                output_digest: identity,
            },
        );
    }
    record_category("input_phase:dependencies", dependency_timer.elapsed());
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
            build_provenance_digest,
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

/// Stages whose outputs are compiled by host tools and consumed by every
/// target stage: a host compiler change alters their bytes on the next
/// rebuild and so recompiles the whole system.
const HOST_BUILT_TOOLCHAIN_STAGES: &[&str] = &["cross-toolchain", "glibc", "gcc-runtime"];

/// A host tool whose executable differs from the one a stage recorded.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ToolDrift {
    pub(crate) tool: String,
    pub(crate) recorded: String,
    pub(crate) current: String,
    pub(crate) stage: String,
}

static TOOL_DRIFT: std::sync::Mutex<Vec<ToolDrift>> = std::sync::Mutex::new(Vec::new());

/// Host tools present in both identities whose executables differ.  Tool
/// identity is provenance, not a cache key (outside strict mode), so a host
/// upgrade silently changes the next rebuild of affected stages.
pub(crate) fn tool_drift(
    stage: &str,
    recorded: &BTreeMap<String, crate::cache_manifest::ToolIdentity>,
    current: &BTreeMap<String, crate::cache_manifest::ToolIdentity>,
) -> Vec<ToolDrift> {
    recorded
        .iter()
        .filter_map(|(tool, old)| {
            let now = current.get(tool)?;
            (!old.executable_sha256.is_empty()
                && !now.executable_sha256.is_empty()
                && old.executable_sha256 != now.executable_sha256)
                .then(|| ToolDrift {
                    tool: tool.clone(),
                    recorded: old.version.clone(),
                    current: now.version.clone(),
                    stage: stage.to_string(),
                })
        })
        .collect()
}

fn record_tool_drift(
    stage: &str,
    recorded: &BTreeMap<String, crate::cache_manifest::ToolIdentity>,
    current: &BTreeMap<String, crate::cache_manifest::ToolIdentity>,
) {
    let drift = tool_drift(stage, recorded, current);
    if !drift.is_empty() {
        TOOL_DRIFT.lock().unwrap().extend(drift);
    }
}

fn warn_host_built_toolchain_drift(
    stage: &str,
    recorded: &BTreeMap<String, crate::cache_manifest::ToolIdentity>,
    current: &BTreeMap<String, crate::cache_manifest::ToolIdentity>,
) {
    if !HOST_BUILT_TOOLCHAIN_STAGES.contains(&stage) {
        return;
    }
    for drift in tool_drift(stage, recorded, current) {
        eprintln!(
            "warning: {stage} is rebuilding with a different host {} than its cached output \
             (was {:?}, now {:?}); its output will change and every target stage will rebuild",
            drift.tool, drift.recorded, drift.current
        );
    }
}

/// Drains the drift recorded by reused stages and renders it.  Drift in the
/// host-built toolchain stages is named per tool, because rebuilding them
/// recompiles everything; other stages are compiled by the MattOS toolchain
/// (host tools only build their build-machine helpers) and are summarized in
/// one line, so a host upgrade does not produce a wall of warnings.
pub(crate) fn take_tool_drift_report() -> Option<String> {
    let mut drift = std::mem::take(&mut *TOOL_DRIFT.lock().unwrap());
    if drift.is_empty() {
        return None;
    }
    drift.sort();
    drift.dedup();
    let (toolchain, others): (Vec<_>, Vec<_>) = drift
        .iter()
        .partition(|entry| HOST_BUILT_TOOLCHAIN_STAGES.contains(&entry.stage.as_str()));
    let other_stages = others.iter().map(|entry| entry.stage.as_str()).collect::<BTreeSet<_>>();
    let mut report = String::new();
    if !toolchain.is_empty() {
        report.push_str("warning: host tools changed since the cached toolchain stages were built:\n");
        let mut groups: BTreeMap<(&str, &str, &str), Vec<&str>> = BTreeMap::new();
        for entry in &toolchain {
            groups
                .entry((&entry.tool, &entry.recorded, &entry.current))
                .or_default()
                .push(&entry.stage);
        }
        for ((tool, recorded, current), stages) in groups {
            report.push_str(&format!("  {tool}: {recorded:?} -> {current:?} ({})\n", stages.join(", ")));
        }
        report.push_str(
            "  Their cached outputs stay valid, but the next rebuild of any of them will produce \
             different compilers and recompile every target stage. To take that rebuild \
             deliberately now: mattos-build cache invalidate cross-toolchain && mattos-build build. \
             MATTOS_STRICT_REPRODUCIBLE=1 makes every tool change a cache miss.\n",
        );
    }
    if !other_stages.is_empty() {
        report.push_str(&format!(
            "note: {} other cached stage(s) recorded different host tools; they are compiled by the \
             MattOS toolchain, so only their build-machine helpers would differ on a rebuild.\n",
            other_stages.len()
        ));
    }
    Some(report)
}

/// GNU install-info rewrites the aggregate Info index, `share/info/dir`, for
/// each manual a `make install` installs. Parallel installs race on that
/// read-modify-write, so the index can lose entries between otherwise
/// identical builds (GCC was seen dropping `gccinstall`). A toolchain stage's
/// output digest keys every stage it compiles, so one lost line rebuilt the
/// whole system. MattOS never ships the index (install-info maintains it on
/// the installed system), so it is removed before outputs are inventoried.
/// Normalizes a successful stage's declared outputs before they are
/// inventoried: removes generated `share/info/dir` indexes and strips group
/// and other write permission (see `normalized_output_mode`).
fn finalize_stage_outputs(repo_root: &Path, outputs: &[PathBuf]) -> Result<()> {
    for output in outputs {
        let root = if output.is_absolute() {
            output.clone()
        } else {
            repo_root.join(output)
        };
        finalize_outputs_under(&root)?;
    }
    Ok(())
}

fn finalize_outputs_under(path: &Path) -> Result<()> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(error).with_context(|| format!("failed to inspect {}", path.display()));
        }
    };
    if metadata.file_type().is_symlink() {
        return Ok(());
    }
    if !metadata.is_dir() && path.ends_with("share/info/dir") {
        fs::remove_file(path).with_context(|| format!("failed to remove {}", path.display()))?;
        return Ok(());
    }
    let mode = metadata.permissions().mode() & 0o7777;
    let normalized = normalized_output_mode(mode);
    if normalized != mode {
        fs::set_permissions(path, fs::Permissions::from_mode(normalized))
            .with_context(|| format!("failed to normalize the mode of {}", path.display()))?;
    }
    if metadata.is_dir() {
        for entry in fs::read_dir(path).with_context(|| format!("failed to read {}", path.display()))? {
            finalize_outputs_under(&entry?.path())?;
        }
    }
    Ok(())
}

/// Git records only a file's executable bit, so group write on output copied
/// from a vendored tree comes from the checkout's umask, not from the source.
/// Stripping group and other write makes such outputs, and their digests,
/// independent of the host that checked the repository out. Sticky and
/// setgid modes (`/tmp`, shared group directories) are deliberate and kept.
fn normalized_output_mode(mode: u32) -> u32 {
    if mode & 0o3000 != 0 { mode } else { mode & !0o022 }
}

/// Development cache validity is based on the compatibility of the published
/// stage artifact, not on the host tool that happened to produce it. Cargo's
/// own fingerprints remain authoritative when Cargo is invoked. The higher
/// level stage only retains tool identity here for outputs which expose
/// compiler-private artifacts directly, or in strict reproducibility mode.
fn validity_tool_identities(
    spec: &StageSpec,
    tools: &BTreeMap<String, crate::cache_manifest::ToolIdentity>,
    strict: bool,
) -> BTreeMap<String, crate::cache_manifest::ToolIdentity> {
    if strict || exposes_compiler_private_output(spec) {
        return tools.clone();
    }
    BTreeMap::new()
}

fn exposes_compiler_private_output(spec: &StageSpec) -> bool {
    let _ = spec;
    false
}

pub(crate) fn strict_tool_identity_mode() -> bool {
    matches!(
        std::env::var("MATTOS_STRICT_REPRODUCIBLE").as_deref(),
        Ok("1") | Ok("true") | Ok("yes")
    )
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
            "build_provenance.digest",
            &manifest.inputs.build_provenance_digest,
            &evaluation.inputs.build_provenance_digest,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache_manifest::ToolIdentity;
    use std::cell::{Cell, RefCell};

    #[test]
    fn finalized_outputs_do_not_depend_on_the_checkout_umask() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let out = temp.path().join("out");
        let mode = |path: &Path| -> Result<u32> {
            Ok(fs::symlink_metadata(path)?.permissions().mode() & 0o7777)
        };
        for (path, mode) in [
            ("usr", 0o775),
            ("usr/bin", 0o775),
            ("tmp", 0o1777),
            ("var/mail", 0o2775),
        ] {
            fs::create_dir_all(out.join(path))?;
            fs::set_permissions(out.join(path), fs::Permissions::from_mode(mode))?;
        }
        for (path, mode) in [
            ("usr/bin/tool", 0o775),
            ("usr/data", 0o664),
            ("usr/world", 0o666),
            ("usr/share/info/dir", 0o644),
        ] {
            fs::create_dir_all(out.join(path).parent().unwrap())?;
            fs::write(out.join(path), path)?;
            fs::set_permissions(out.join(path), fs::Permissions::from_mode(mode))?;
        }
        std::os::unix::fs::symlink("tool", out.join("usr/bin/link"))?;

        finalize_stage_outputs(temp.path(), &[PathBuf::from("out")])?;

        for (path, expected) in [
            ("usr", 0o755),
            ("usr/bin", 0o755),
            ("usr/bin/tool", 0o755),
            ("usr/data", 0o644),
            ("usr/world", 0o644),
            ("tmp", 0o1777),
            ("var/mail", 0o2775),
        ] {
            assert_eq!(mode(&out.join(path))?, expected, "{path}");
        }
        assert!(!out.join("usr/share/info/dir").exists());
        assert_eq!(fs::read_link(out.join("usr/bin/link"))?, PathBuf::from("tool"));
        Ok(())
    }

    fn fixture_spec(output: &str) -> StageSpec {
        StageSpec {
            id: "tool-policy".to_string(),
            source_inputs: Vec::new(),
            configuration_inputs: Vec::new(),
            tools: vec!["gcc".to_string(), "meson".to_string(), "rustc".to_string()],
            dependencies: Vec::new(),
            outputs: vec![output.into()],
            recipe: "tool-policy".to_string(),
        }
    }

    #[test]
    fn finished_outputs_separate_validity_from_tool_provenance() {
        let tools = BTreeMap::from([(
            "gcc".to_string(),
            ToolIdentity {
                resolved_path: "/usr/bin/gcc".to_string(),
                executable_sha256: "gcc-one".to_string(),
                version: "gcc 15".to_string(),
                target: String::new(),
            },
        )]);
        let mut changed = tools.clone();
        changed.get_mut("gcc").unwrap().executable_sha256 = "gcc-two".to_string();
        let spec = fixture_spec("out/build/example/install/usr/bin/example");
        assert_eq!(
            digest_serializable(&validity_tool_identities(&spec, &tools, false)).unwrap(),
            digest_serializable(&validity_tool_identities(&spec, &changed, false)).unwrap()
        );
        assert_ne!(
            digest_serializable(&tools).unwrap(),
            digest_serializable(&changed).unwrap()
        );
    }

    #[test]
    fn compiler_private_outputs_keep_tool_compatibility_boundary() {
        let tools = BTreeMap::from([(
            "rustc".to_string(),
            ToolIdentity {
                resolved_path: "/usr/bin/rustc".to_string(),
                executable_sha256: "rust-one".to_string(),
                version: "rustc 1".to_string(),
                target: "x86_64-unknown-linux-gnu".to_string(),
            },
        )]);
        let mut changed = tools.clone();
        changed.get_mut("rustc").unwrap().executable_sha256 = "rust-two".to_string();
        let mut spec =
            fixture_spec("out/build/example/install/usr/lib/rustlib/example/lib/example.rlib");
        spec.id = "rust".to_string();
        assert_eq!(
            digest_serializable(&validity_tool_identities(&spec, &tools, false)).unwrap(),
            digest_serializable(&validity_tool_identities(&spec, &changed, false)).unwrap()
        );
        assert_ne!(
            digest_serializable(&validity_tool_identities(&spec, &tools, true,)).unwrap(),
            digest_serializable(&validity_tool_identities(&spec, &changed, true,)).unwrap()
        );
    }

    #[test]
    fn private_build_tree_intermediates_do_not_make_finished_stage_tool_sensitive() {
        let tools = BTreeMap::from([(
            "gcc".to_string(),
            ToolIdentity {
                resolved_path: "/usr/bin/gcc".to_string(),
                executable_sha256: "one".to_string(),
                version: "gcc 15".to_string(),
                target: String::new(),
            },
        )]);
        let private = fixture_spec("out/build/example/build/objects/startup.o");
        assert!(validity_tool_identities(&private, &tools, false).is_empty());
    }

    #[test]
    fn abi_stable_glibc_and_gcc_runtime_objects_are_not_host_tool_sensitive() {
        let tools = BTreeMap::from([(
            "gcc".to_string(),
            ToolIdentity {
                resolved_path: "/usr/bin/gcc".to_string(),
                executable_sha256: "one".to_string(),
                version: "gcc 15".to_string(),
                target: String::new(),
            },
        )]);
        for id in ["glibc", "gcc-runtime"] {
            let mut spec = fixture_spec("out/build/component/install/usr/lib/crt1.o");
            spec.id = id.to_string();
            assert!(validity_tool_identities(&spec, &tools, false).is_empty());
        }
    }

    #[test]
    fn cargo_dispatcher_identity_is_migratable_to_same_compiler() {
        let old = BTreeMap::from([(
            "cargo".to_string(),
            ToolIdentity {
                resolved_path: "/workspace/out/source-ownership/bin/cargo".to_string(),
                executable_sha256: "dispatcher-bytes".to_string(),
                version: "cargo 1.94.0 (85eff7c80 2026-01-15)".to_string(),
                target: String::new(),
            },
        )]);
        let current = BTreeMap::from([(
            "cargo".to_string(),
            ToolIdentity {
                resolved_path: "/home/matt/.rustup/toolchains/stable/bin/cargo".to_string(),
                executable_sha256: "compiler-bytes".to_string(),
                version: "cargo 1.94.0 (85eff7c80 2026-01-15)".to_string(),
                target: String::new(),
            },
        )]);
        assert!(tool_details_compatible(&old, &current));
    }

    #[test]
    fn changed_dependency_output_bytes_rebuild_the_real_manifest_consumer() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("upstream.source"), "one").unwrap();
        fs::write(root.path().join("consumer.source"), "one").unwrap();
        let spec = |id: &str, source: &str, dependencies: &[&str]| StageSpec {
            id: id.to_string(),
            source_inputs: vec![source.into()],
            configuration_inputs: Vec::new(),
            tools: Vec::new(),
            dependencies: dependencies.iter().map(|value| value.to_string()).collect(),
            outputs: vec![format!("out/{id}").into()],
            recipe: id.to_string(),
        };
        let upstream = spec("upstream", "upstream.source", &[]);
        let consumer = spec("consumer", "consumer.source", &["upstream"]);
        let consumer_runs = Cell::new(0);
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
        let run_consumer = || {
            execute_cached_stage(
                root.path(),
                &consumer,
                || Ok(()),
                || {
                    consumer_runs.set(consumer_runs.get() + 1);
                    let output = root.path().join(&consumer.outputs[0]);
                    fs::create_dir_all(output.parent().unwrap())?;
                    fs::write(output, format!("consumer run {}", consumer_runs.get()))?;
                    Ok(())
                },
            )
        };

        publish(&upstream, "first bytes").unwrap();
        run_consumer().unwrap();
        fs::write(root.path().join("upstream.source"), "two").unwrap();
        publish(&upstream, "changed bytes").unwrap();
        run_consumer().unwrap();
        assert_eq!(consumer_runs.get(), 2);
    }

    /// Serializes the tests that record into and drain the global drift list.
    static DRIFT_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn identity(sha: &str, version: &str) -> ToolIdentity {
        ToolIdentity {
            resolved_path: "/usr/bin/gcc".to_string(),
            executable_sha256: sha.to_string(),
            version: version.to_string(),
            target: String::new(),
        }
    }

    #[test]
    fn tool_drift_reports_changed_executables_and_flags_toolchain_stages() {
        let recorded = BTreeMap::from([
            ("gcc".to_string(), identity("old", "gcc 15.2.0-4")),
            ("make".to_string(), identity("same", "make 4.4.1")),
        ]);
        let current = BTreeMap::from([
            ("gcc".to_string(), identity("new", "gcc 15.2.0-16")),
            ("make".to_string(), identity("same", "make 4.4.1")),
            ("meson".to_string(), identity("added", "meson 1.9")),
        ]);
        let drift = tool_drift("gcc-runtime", &recorded, &current);
        assert_eq!(drift.len(), 1);
        assert_eq!((drift[0].tool.as_str(), drift[0].current.as_str()), ("gcc", "gcc 15.2.0-16"));
        assert!(tool_drift("zlib", &recorded, &recorded).is_empty());
    }

    #[test]
    fn a_reused_stage_built_by_an_older_host_compiler_reports_drift_without_rebuilding() {
        let _serial = DRIFT_TEST_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let root = tempfile::tempdir().unwrap();
        let spec = fixture_spec("out/build/gcc-runtime/output");
        let spec = StageSpec { id: "gcc-runtime".to_string(), tools: vec!["make".to_string()], ..spec };
        let runs = std::cell::Cell::new(0);
        let run = || {
            execute_cached_stage(root.path(), &spec, || Ok(()), || {
                runs.set(runs.get() + 1);
                let output = root.path().join(&spec.outputs[0]);
                fs::create_dir_all(output.parent().unwrap())?;
                fs::write(output, "compiler")?;
                Ok(())
            })
        };
        run().unwrap();
        let mut manifest = read_stage_manifest(root.path(), &spec.id).unwrap();
        manifest.input_details.tools.get_mut("make").unwrap().executable_sha256 = "older-host-make".to_string();
        write_stage_manifest(root.path(), &manifest).unwrap();
        run().unwrap();
        assert_eq!(runs.get(), 1, "tool identity is provenance, not a cache key");
        let report = take_tool_drift_report().expect("drift is reported");
        assert!(report.contains("make:") && report.contains("gcc-runtime"));
        assert!(report.contains("recompile every target stage"));
    }

    #[test]
    fn drift_outside_the_toolchain_stages_is_one_summary_line() {
        let _serial = DRIFT_TEST_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        TOOL_DRIFT.lock().unwrap().extend((0..200).map(|stage| ToolDrift {
            tool: "gcc".to_string(),
            recorded: "old".to_string(),
            current: "new".to_string(),
            stage: format!("target-stage-{stage}"),
        }));
        let report = take_tool_drift_report().unwrap();
        assert_eq!(report.lines().count(), 1, "{report}");
        assert!(report.contains("200 other cached stage(s)"));
    }

    #[test]
    fn racy_info_indexes_never_reach_the_output_digest() {
        // Two builds whose parallel installs raced differently on the Info
        // index must publish the same output digest.
        let digests = ["* GCC: (gcc).\n* gccinstall: (gccinstall).\n", "* GCC: (gcc).\n"].map(|index| {
            let root = tempfile::tempdir().unwrap();
            let spec = fixture_spec("out/build/toolchain");
            execute_cached_stage(root.path(), &spec, || Ok(()), || {
                let usr = root.path().join("out/build/toolchain/install/usr");
                fs::create_dir_all(usr.join("share/info"))?;
                fs::create_dir_all(usr.join("bin"))?;
                fs::write(usr.join("share/info/dir"), index)?;
                fs::write(usr.join("share/info/gcc.info"), "manual")?;
                fs::write(usr.join("bin/gcc"), "compiler")?;
                Ok(())
            })
            .unwrap();
            let usr = root.path().join("out/build/toolchain/install/usr");
            assert!(!usr.join("share/info/dir").exists());
            assert!(usr.join("share/info/gcc.info").is_file());
            crate::performance::read_stage_manifest(root.path(), &spec.id)
                .unwrap()
                .output_content_digest
        });
        assert_eq!(digests[0], digests[1]);
    }

    #[test]
    fn changed_published_output_is_a_miss_for_autotools_cmake_and_meson_consumers() {
        // These names model the three build-helper families.  The stage cache
        // must make the same decision before any helper can inspect its
        // private build stamp: a changed published install artifact is a real
        // miss, never a metadata-only migration.
        for helper in ["autotools", "cmake", "meson"] {
            let root = tempfile::tempdir().unwrap();
            let spec = StageSpec {
                id: helper.to_string(),
                source_inputs: Vec::new(),
                configuration_inputs: Vec::new(),
                tools: Vec::new(),
                dependencies: Vec::new(),
                outputs: vec![format!("out/build/{helper}/install/usr/bin/example").into()],
                recipe: format!("{helper}-recipe"),
            };
            let runs = Cell::new(0);
            let run = || {
                execute_cached_stage(
                    root.path(),
                    &spec,
                    || Ok(()),
                    || {
                        runs.set(runs.get() + 1);
                        let output = root.path().join(&spec.outputs[0]);
                        fs::create_dir_all(output.parent().unwrap())?;
                        fs::write(output, format!("published run {}", runs.get()))?;
                        Ok(())
                    },
                )
            };

            run().unwrap();
            fs::write(
                root.path().join(&spec.outputs[0]),
                "modified outside manifest",
            )
            .unwrap();

            let impact = explain_stage_impact(root.path(), &spec).unwrap();
            assert_eq!(impact.status, "MISS", "{helper}");
            assert_eq!(impact.classification, "published output change", "{helper}");
            assert!(!impact.migration_eligible, "{helper}");

            run().unwrap();
            assert_eq!(runs.get(), 2, "{helper}");
        }
    }

    #[test]
    fn narrowed_input_migration_with_unchanged_output_never_enters_action() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join("source/retained")).unwrap();
        fs::write(root.path().join("source/retained/input"), "same bytes").unwrap();
        fs::write(root.path().join("source/removed-input"), "also unchanged").unwrap();
        let old = StageSpec {
            id: "autotools".to_string(),
            source_inputs: vec!["source".into()],
            configuration_inputs: Vec::new(),
            tools: Vec::new(),
            dependencies: Vec::new(),
            outputs: vec!["out/build/autotools/install/usr/bin/example".into()],
            recipe: "autotools-recipe".to_string(),
        };
        let current = StageSpec {
            source_inputs: vec!["source/retained".into()],
            ..old.clone()
        };
        let runs = Cell::new(0);
        let run = |spec: &StageSpec| {
            execute_cached_stage(
                root.path(),
                spec,
                || Ok(()),
                || {
                    runs.set(runs.get() + 1);
                    let output = root.path().join(&spec.outputs[0]);
                    fs::create_dir_all(output.parent().unwrap())?;
                    fs::write(output, "published bytes")?;
                    Ok(())
                },
            )
        };

        run(&old).unwrap();
        let impact = explain_stage_impact(root.path(), &current).unwrap();
        assert_eq!(impact.status, "MIGRATE");
        assert!(impact.migration_eligible);
        run(&current).unwrap();
        assert_eq!(runs.get(), 1);
        assert_eq!(
            read_stage_manifest(root.path(), "autotools")
                .unwrap()
                .inputs
                .full_digest,
            compute_stage_evaluation(root.path(), &current)
                .unwrap()
                .inputs
                .full_digest
        );
    }

    #[test]
    fn cached_stage_action_inherits_and_releases_its_active_log_context() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("input"), "input").unwrap();
        let spec = StageSpec {
            id: "log-context".to_string(),
            source_inputs: vec!["input".into()],
            configuration_inputs: Vec::new(),
            tools: Vec::new(),
            dependencies: Vec::new(),
            outputs: vec!["out/result".into()],
            recipe: "log-context".to_string(),
        };
        let observed = RefCell::new(None);
        execute_cached_stage_with_resources(
            root.path(),
            &spec,
            || Ok(()),
            || Ok(()),
            || {
                *observed.borrow_mut() = crate::performance::active_stage_log_path_for_test();
                crate::performance::append_active_stage_log("cached-stage-action").unwrap();
                fs::create_dir_all(root.path().join("out")).unwrap();
                fs::write(root.path().join("out/result"), "result").unwrap();
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(
            observed.borrow().as_deref(),
            Some(root.path().join("out/logs/log-context.log").as_path())
        );
        assert!(crate::performance::active_stage_log_path_for_test().is_none());
        assert!(
            fs::read_to_string(root.path().join("out/logs/log-context.log"))
                .unwrap()
                .contains("cached-stage-action")
        );
    }
}
