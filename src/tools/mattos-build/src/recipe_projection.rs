//! Per-stage projection of shared recipe implementation files.
//!
//! Many stages share one `stages/*.rs` file.  Hashing the whole file made an
//! edit to one recipe rebuild every stage defined beside it.  A stage instead
//! hashes its file with the *other* stages' recipe functions removed: its own
//! recipe, and every shared helper, constant and import, remain, so edits to
//! shared code still invalidate every stage that could observe them.

use crate::stage_graph::{BuildStage, all_build_stages, stage_id};
use anyhow::Result;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use std::sync::OnceLock;

/// Marks a projected digest, distinguishing it from a whole-file digest.
pub(crate) const PROJECTION_PREFIX: &str = "recipe-projection-v1:";
const RECIPE_DIRECTORY: &str = "src/tools/mattos-build/src/stages/";
const REGISTRY_SOURCE: &str = include_str!("stages/registry.rs");

/// Recipe functions each stage dispatches to, from `build_stage_recipe`.
fn recipe_functions() -> &'static BTreeMap<String, BTreeSet<String>> {
    static FUNCTIONS: OnceLock<BTreeMap<String, BTreeSet<String>>> = OnceLock::new();
    FUNCTIONS.get_or_init(|| parse_recipe_dispatch(REGISTRY_SOURCE))
}

fn parse_recipe_dispatch(registry: &str) -> BTreeMap<String, BTreeSet<String>> {
    let mut functions: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let Some(start) = registry.find("\nfn build_stage_recipe(") else {
        return functions;
    };
    let body = &registry[start..];
    let body = &body[..body.find("\n}\n").unwrap_or(body.len())];
    let mut arms = body.split("BuildStage::").skip(1).peekable();
    while let Some(arm) = arms.next() {
        let Some((variants, action)) = arm.split_once("=>") else {
            continue;
        };
        let variant = variants.split(|c: char| !c.is_alphanumeric()).next().unwrap_or_default();
        let called = action
            .split(|c: char| !(c.is_alphanumeric() || c == '_'))
            .filter(|word| word.starts_with("build_"))
            .map(str::to_string);
        functions.entry(variant.to_string()).or_default().extend(called);
    }
    functions
}

fn stage_variant(stage: &str) -> Option<String> {
    all_build_stages()
        .iter()
        .find(|candidate| stage_id(**candidate) == stage)
        .map(|candidate: &BuildStage| format!("{candidate:?}"))
}

/// Digest of `path` as seen by `stage`, or `None` when the whole file is the
/// right input (it is not a recipe file, or defines no other stage's recipe).
pub(crate) fn projected_recipe_digest(repo_root: &Path, stage: &str, path: &Path) -> Result<Option<String>> {
    let relative = path.strip_prefix(repo_root).unwrap_or(path);
    if !relative.to_string_lossy().starts_with(RECIPE_DIRECTORY)
        || relative.extension().is_none_or(|extension| extension != "rs")
    {
        return Ok(None);
    }
    let Some(variant) = stage_variant(stage) else {
        return Ok(None);
    };
    let file = repo_root.join(relative);
    if !file.is_file() {
        return Ok(None);
    }
    let source = fs::read_to_string(file)?;
    Ok(project(&source, &variant, recipe_functions())
        .map(|projection| format!("{PROJECTION_PREFIX}{:x}", Sha256::digest(projection.as_bytes()))))
}

/// `source` without the recipe functions that belong only to other stages
/// and that the retained code does not call.
fn project(
    source: &str,
    variant: &str,
    functions: &BTreeMap<String, BTreeSet<String>>,
) -> Option<String> {
    let own = functions.get(variant).cloned().unwrap_or_default();
    let mut removable = functions
        .iter()
        .filter(|(other, _)| other.as_str() != variant)
        .flat_map(|(_, names)| names.iter().cloned())
        .filter(|name| !own.contains(name) && function_extent(source, name).is_some())
        .collect::<BTreeSet<_>>();
    loop {
        let projection = remove_functions(source, &removable);
        let called = removable
            .iter()
            .filter(|name| projection.contains(&format!("{name}(")))
            .cloned()
            .collect::<Vec<_>>();
        if called.is_empty() {
            return (!removable.is_empty()).then_some(projection);
        }
        for name in called {
            removable.remove(&name);
        }
    }
}

fn remove_functions(source: &str, names: &BTreeSet<String>) -> String {
    let mut extents = names
        .iter()
        .filter_map(|name| function_extent(source, name))
        .collect::<Vec<_>>();
    extents.sort_unstable();
    let mut projection = String::with_capacity(source.len());
    let mut cursor = 0;
    for (start, end) in extents {
        if start >= cursor {
            projection.push_str(&source[cursor..start]);
            cursor = end;
        }
    }
    projection.push_str(&source[cursor..]);
    projection
}

/// Byte range of the top-level `fn name(...)` item, including its leading doc
/// comments and attributes, through its closing `}` line.  Rustfmt places a
/// top-level item's closing brace alone at column 0.
fn function_extent(source: &str, name: &str) -> Option<(usize, usize)> {
    let signature = [
        format!("\nfn {name}("),
        format!("\npub(crate) fn {name}("),
        format!("\npub fn {name}("),
    ]
    .into_iter()
    .filter_map(|needle| source.find(&needle).map(|at| at + 1))
    .min()?;
    let close = source[signature..].find("\n}\n")? + signature + 3;
    let mut start = signature;
    while let Some(previous_end) = start.checked_sub(1) {
        let previous_start = source[..previous_end].rfind('\n').map_or(0, |at| at + 1);
        let line = source[previous_start..previous_end].trim_start();
        if line.starts_with("//") || line.starts_with("#[") {
            start = previous_start;
        } else {
            break;
        }
    }
    Some((start, close))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE: &str = "use x;\n\nconst SHARED: u8 = 1;\n\n/// Builds a.\nfn build_a(r: &Path) -> Result<()> {\n    helper(r)\n}\n\n// Builds b.\n#[allow(dead_code)]\nfn build_b(r: &Path) -> Result<()> {\n    helper(r)?;\n    build_c(r)\n}\n\nfn build_c(r: &Path) -> Result<()> {\n    Ok(())\n}\n\nfn helper(r: &Path) -> Result<()> {\n    Ok(())\n}\n";

    fn dispatch() -> BTreeMap<String, BTreeSet<String>> {
        parse_recipe_dispatch(
            "\nfn build_stage_recipe(repo_root: &Path, stage: BuildStage) -> Result<()> {\n    match stage {\n        BuildStage::A => build_a(repo_root),\n        BuildStage::B => {\n            trace();\n            build_b(repo_root)\n        }\n        BuildStage::C => build_c(repo_root),\n        BuildStage::D => packaging::build_d(repo_root),\n    }\n}\n",
        )
    }

    #[test]
    fn dispatch_maps_each_stage_to_its_recipe_functions() {
        let functions = dispatch();
        assert_eq!(functions["A"], BTreeSet::from(["build_a".to_string()]));
        assert_eq!(functions["B"], BTreeSet::from(["build_b".to_string()]));
        assert_eq!(functions["D"], BTreeSet::from(["build_d".to_string()]));
    }

    #[test]
    fn projection_drops_only_other_stages_recipes_with_their_comments() {
        let a = project(SOURCE, "A", &dispatch()).unwrap();
        assert!(a.contains("fn build_a(") && a.contains("fn helper(") && a.contains("SHARED"));
        assert!(!a.contains("build_b") && !a.contains("Builds b") && !a.contains("fn build_c("));
        // B calls C's recipe, so it stays part of B's input.
        let b = project(SOURCE, "B", &dispatch()).unwrap();
        assert!(b.contains("fn build_c(") && !b.contains("fn build_a("));
    }

    #[test]
    fn editing_another_recipe_keeps_the_projection_but_shared_code_changes_it() {
        let functions = dispatch();
        let a = project(SOURCE, "A", &functions).unwrap();
        let b_edited = SOURCE.replace("    helper(r)?;\n", "    helper(r)?;\n    helper(r)?;\n");
        assert_eq!(project(&b_edited, "A", &functions).unwrap(), a);
        let helper_edited = SOURCE.replace("fn helper(r: &Path) -> Result<()> {\n    Ok(())", "fn helper(r: &Path) -> Result<()> {\n    Err(x)");
        assert_ne!(project(&helper_edited, "A", &functions).unwrap(), a);
        let a_edited = SOURCE.replace("    helper(r)\n}", "    helper(r)?;\n    helper(r)\n}");
        assert_ne!(project(&a_edited, "A", &functions).unwrap(), a);
    }

    #[test]
    fn files_without_other_recipes_are_hashed_whole() {
        let only_a = "fn build_a(r: &Path) -> Result<()> {\n    Ok(())\n}\n";
        assert_eq!(project(only_a, "A", &dispatch()), None);
    }

    #[test]
    fn real_registry_dispatch_covers_every_build_stage() {
        let functions = recipe_functions();
        for stage in all_build_stages() {
            if *stage == BuildStage::All {
                continue;
            }
            let variant = format!("{stage:?}");
            assert!(
                functions.get(&variant).is_some_and(|names| !names.is_empty()),
                "no recipe function parsed for {variant}"
            );
        }
    }
}
