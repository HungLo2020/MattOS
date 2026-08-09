use anyhow::{Context, Result, bail};
use chrono::Utc;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
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
        if can_migrate_narrowed_manifest(repo_root, &evaluation, &manifest)? {
            manifest.inputs = inputs.clone();
            manifest.input_details = evaluation.details.clone();
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
    acquire_resources()?;
    invalidate_integrity_paths(repo_root, &spec.outputs);
    let result = measured("stage_actions", || {
        with_stage_log(repo_root, &spec.id, action)
    });
    let mut output_digest = None;
    if result.is_ok() {
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
        || manifest.inputs.tool_digest != current.inputs.tool_digest
        || manifest.inputs.environment_digest != current.inputs.environment_digest
        || !current
            .inputs
            .dependency_digests
            .iter()
            .all(|(stage, digest)| manifest.inputs.dependency_digests.get(stage) == Some(digest))
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
    if !shared_values_match(&manifest.input_details.source, &current.details.source)
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
    for added in current
        .details
        .source
        .keys()
        .filter(|path| !manifest.input_details.source.contains_key(*path))
    {
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

pub(crate) fn compute_stage_inputs(repo_root: &Path, spec: &StageSpec) -> Result<StageInputs> {
    Ok(compute_stage_evaluation(repo_root, spec)?.inputs)
}

pub(crate) fn compute_stage_evaluation(
    repo_root: &Path,
    spec: &StageSpec,
) -> Result<StageEvaluation> {
    let source_timer = Instant::now();
    let source_digest = digest_source_inputs(repo_root, &spec.source_inputs)?;
    let mut source = BTreeMap::new();
    for path in &spec.source_inputs {
        source.insert(
            diagnostic_path(repo_root, path),
            digest_source_inputs(repo_root, std::slice::from_ref(path))?,
        );
    }
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
    let tool_digest = digest_serializable(&tools)?;
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
    use std::cell::Cell;

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
}
