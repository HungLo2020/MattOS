//! Per-stage projection of shared recipe implementation files.
//!
//! Many stages share one `stages/*.rs` file.  Hashing the whole file made an
//! edit to one recipe rebuild every stage defined beside it, and an edit to a
//! test or to a helper only one stage calls rebuilt them all as well.
//!
//! A stage instead hashes each recipe file as the code it can execute: the
//! top-level functions, constants and statics reachable from its own recipe
//! functions, plus every other top-level item (imports, types, impls, macros),
//! which are kept unconditionally.  `#[cfg(test)]` items are never part of a
//! stage's input.  Reachability follows identifier references through every
//! Rust file of the crate, so a helper in an unhashed file that calls back
//! into a hashed file still keeps its callee in the projection.

use crate::rust_items::{self, Chunk, ChunkKind, ItemIndex, Tokens};
use crate::stage_graph::{BuildStage, all_build_stages, stage_id};
use anyhow::Result;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

/// Marks a reachability projection digest.
pub(crate) const PROJECTION_PREFIX: &str = "recipe-projection-v4:";
/// Earlier projections, kept so that manifests recorded under them migrate
/// without a rebuild.  v2 read identifiers as every word of an item,
/// including comments and strings (so the word "main" reached the CLI entry
/// point); v1 removed only other stages' recipe functions.  Each successive
/// projection hashes a subset of its predecessor's view (see
/// `legacy_projection_matches`).
/// v3 split items by lines alone, so a column-0 line inside a multi-line
/// string could cut a function in two.
pub(crate) const LEGACY_V3_PREFIX: &str = "recipe-projection-v3:";
pub(crate) const LEGACY_V2_PREFIX: &str = "recipe-projection-v2:";
pub(crate) const LEGACY_PROJECTION_PREFIX: &str = "recipe-projection-v1:";
const CRATE_DIRECTORY: &str = "src/tools/mattos-build/src/";
const RECIPE_DIRECTORY: &str = "src/tools/mattos-build/src/stages/";
const REGISTRY_SOURCE: &str = include_str!("stages/registry.rs");
/// The stage dispatcher calls every recipe and the CLI entry point reaches
/// every command.  A stage's own dispatch arm is already its root set, so
/// reachability must not pass through either.
const BARRIERS: &[&str] = &["build_stage_recipe", "build_stage", "main"];
/// v2's barriers, for recomputing v2 digests only.
const V2_BARRIERS: &[&str] = &["build_stage_recipe", "build_stage"];

/// Whether `digest` is any recipe projection rather than a whole-file digest.
pub(crate) fn is_projection_digest(digest: &str) -> bool {
    digest.starts_with("recipe-projection-")
}

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
        let variant = variants
            .split(|c: char| !c.is_alphanumeric())
            .next()
            .unwrap_or_default();
        let called = action
            .split(|c: char| !(c.is_alphanumeric() || c == '_'))
            .filter(|word| word.starts_with("build_"))
            .map(str::to_string);
        functions
            .entry(variant.to_string())
            .or_default()
            .extend(called);
    }
    functions
}

fn stage_variant(stage: &str) -> Option<String> {
    all_build_stages()
        .iter()
        .find(|candidate| stage_id(**candidate) == stage)
        .map(|candidate: &BuildStage| format!("{candidate:?}"))
}

fn recipe_file(repo_root: &Path, path: &Path) -> Option<PathBuf> {
    let relative = path.strip_prefix(repo_root).unwrap_or(path);
    if !relative.to_string_lossy().starts_with(RECIPE_DIRECTORY)
        || relative
            .extension()
            .is_none_or(|extension| extension != "rs")
    {
        return None;
    }
    let file = repo_root.join(relative);
    file.is_file().then_some(file)
}

/// Digest of `path` as seen by `stage`, or `None` when the whole file is the
/// right input (it is not a recipe file, or the stage can execute all of it).
pub(crate) fn projected_recipe_digest(
    repo_root: &Path,
    stage: &str,
    path: &Path,
) -> Result<Option<String>> {
    let Some(file) = recipe_file(repo_root, path) else {
        return Ok(None);
    };
    let Some(variant) = stage_variant(stage) else {
        return Ok(None);
    };
    let index = crate_index(repo_root, Tokens::Code)?;
    let reachable = reachable(&index, &variant, BARRIERS);
    let Some(chunks) = index.files.get(&file) else {
        return Ok(None);
    };
    Ok(project(chunks, &reachable).map(|projection| {
        format!(
            "{PROJECTION_PREFIX}{:x}",
            Sha256::digest(projection.as_bytes())
        )
    }))
}

/// Marks a stage whose key also covers the recipe files it reaches but does
/// not list (see `implicit_recipe_inputs`).  Manifests recorded before this
/// coverage lack the marker and adopt those files once (see
/// `stage_cache::can_migrate_narrowed_manifest`).
pub(crate) const IMPLICIT_RECIPE_COVERAGE_KEY: &str = "recipe-coverage:implicit";
pub(crate) const IMPLICIT_RECIPE_COVERAGE_VERSION: &str = "v1";

/// Recipe files (`stages/*.rs`, not the shared `stages/helpers/` or the
/// dispatcher) that define code `stage`'s recipe calls but that are not among
/// its listed `source_inputs`.  A stage's key covers each of these through
/// its projection, so a change to code the stage runs can never leave its
/// cached output in place merely because the file was not listed.
pub(crate) fn implicit_recipe_inputs(
    repo_root: &Path,
    stage: &str,
    source_inputs: &[PathBuf],
) -> Result<Vec<PathBuf>> {
    let Some(variant) = stage_variant(stage) else {
        return Ok(Vec::new());
    };
    let index = crate_index(repo_root, Tokens::Code)?;
    // The same reachability a projection uses: what the recipe calls plus
    // what always-kept items (impls, macros) mention.
    let reached = reachable(&index, &variant, BARRIERS);
    let listed = source_inputs
        .iter()
        .map(|path| repo_root.join(path.strip_prefix(repo_root).unwrap_or(path)))
        .collect::<BTreeSet<_>>();
    let recipe_directory = repo_root.join(RECIPE_DIRECTORY);
    let mut implicit = Vec::new();
    for (file, chunks) in &index.files {
        let Ok(relative) = file.strip_prefix(&recipe_directory) else {
            continue;
        };
        if relative.starts_with("helpers")
            || relative == Path::new("registry.rs")
            || listed.contains(file)
        {
            continue;
        }
        let calls_into = chunks.iter().any(|chunk| {
            !chunk.test_only
                && matches!(&chunk.kind, ChunkKind::Named(name) if reached.contains(name))
        });
        if calls_into {
            implicit.push(file.clone());
        }
    }
    Ok(implicit)
}

/// Whether `stored`, a digest recorded under an earlier projection, still
/// describes `path` for `stage`, proving the current (v3) view unchanged.
///
/// * v2 kept every item reachable when identifiers were read as all words,
///   with fewer barriers.  v3's identifiers are a subset and its barriers a
///   superset, so its view is a subset of v2's: an unchanged v2 view proves
///   an unchanged v3 view.
/// * v1 kept everything except other stages' recipe functions that the kept
///   text did not call.  v3 can still reach one of those (through a function
///   pointer, or a helper in another file), so an unchanged v1 view proves the
///   v3 view unchanged only when v3 reaches none of the functions v1 removed.
pub(crate) fn legacy_projection_matches(
    repo_root: &Path,
    stage: &str,
    path: &Path,
    stored: &str,
) -> Result<bool> {
    let (Some(file), Some(variant)) = (recipe_file(repo_root, path), stage_variant(stage)) else {
        return Ok(false);
    };
    if stored.starts_with(LEGACY_V3_PREFIX) {
        // v3's view of the current file must be unchanged, and every line v4
        // keeps must be one v3 kept: then the code this stage executes, which
        // lies within the v4 view, is unchanged since the output was built.
        let v3_index = crate_index(repo_root, Tokens::CodeLineSplit)?;
        let v3_reachable = reachable(&v3_index, &variant, BARRIERS);
        let index = crate_index(repo_root, Tokens::Code)?;
        let current_reachable = reachable(&index, &variant, BARRIERS);
        let (Some(v3_chunks), Some(chunks)) = (v3_index.files.get(&file), index.files.get(&file)) else {
            return Ok(false);
        };
        let Some(v3_projection) = project(v3_chunks, &v3_reachable) else {
            return Ok(false);
        };
        if format!("{LEGACY_V3_PREFIX}{:x}", Sha256::digest(v3_projection.as_bytes())) != stored {
            return Ok(false);
        }
        return Ok(kept_lines(chunks, &current_reachable).is_subset(&kept_lines(v3_chunks, &v3_reachable)));
    }
    if stored.starts_with(LEGACY_V2_PREFIX) {
        let index = crate_index(repo_root, Tokens::Words)?;
        let reachable = reachable(&index, &variant, V2_BARRIERS);
        let Some(chunks) = index.files.get(&file) else {
            return Ok(false);
        };
        return Ok(project(chunks, &reachable).is_some_and(|projection| {
            format!("{LEGACY_V2_PREFIX}{:x}", Sha256::digest(projection.as_bytes())) == stored
        }));
    }
    if !stored.starts_with(LEGACY_PROJECTION_PREFIX) {
        return Ok(false);
    }
    let source = fs::read_to_string(&file)?;
    let Some((projection, removed)) = project_v1(&source, &variant, recipe_functions()) else {
        return Ok(false);
    };
    if format!("{LEGACY_PROJECTION_PREFIX}{:x}", Sha256::digest(projection.as_bytes())) != stored {
        return Ok(false);
    }
    let index = crate_index(repo_root, Tokens::Code)?;
    let reachable = reachable(&index, &variant, BARRIERS);
    Ok(removed.is_disjoint(&reachable))
}

/// The index of every Rust file of the crate, shared by all stages.
fn crate_index(repo_root: &Path, tokens: Tokens) -> Result<Arc<ItemIndex>> {
    rust_items::indexed_files(&repo_root.join(CRATE_DIRECTORY), &[], tokens)
}

/// Named items `variant`'s recipe can reach without passing a barrier.
fn reachable(index: &ItemIndex, variant: &str, barriers: &[&str]) -> Arc<BTreeSet<String>> {
    let roots = recipe_functions().get(variant).cloned().unwrap_or_default();
    index.reachable(variant, roots, barriers)
}

/// `chunks` without the unreachable named items and test-only items (each
/// kept item without its trailing blank lines, so removing a neighbour does
/// not change it), or `None` when nothing is removed and the whole file is
/// the input.
fn project(chunks: &[Chunk], reachable: &BTreeSet<String>) -> Option<String> {
    let mut removed = false;
    let mut projection = String::new();
    for chunk in chunks {
        let keep = !chunk.test_only
            && match &chunk.kind {
                ChunkKind::Named(name) => reachable.contains(name),
                ChunkKind::Other => true,
            };
        if keep {
            projection.push_str(chunk.content());
            projection.push('\n');
        } else {
            removed = true;
        }
    }
    removed.then_some(projection)
}

/// Line numbers of the file (as split into `chunks`) that a projection with
/// `reachable` keeps: each kept item's non-blank content lines.  Blank lines
/// are layout, and the two splits attach them to items differently.
fn kept_lines(chunks: &[Chunk], reachable: &BTreeSet<String>) -> BTreeSet<usize> {
    let mut kept = BTreeSet::new();
    let mut line = 0;
    for chunk in chunks {
        let keep = !chunk.test_only
            && match &chunk.kind {
                ChunkKind::Named(name) => reachable.contains(name),
                ChunkKind::Other => true,
            };
        if keep {
            kept.extend(
                chunk
                    .content()
                    .lines()
                    .enumerate()
                    .filter(|(_, text)| !text.trim().is_empty())
                    .map(|(offset, _)| line + offset),
            );
        }
        line += chunk.text.lines().count();
    }
    kept
}

/// The v1 projection: `source` without the recipe functions that belong only
/// to other stages and that the retained code does not call, together with
/// the names it removed.
fn project_v1(
    source: &str,
    variant: &str,
    functions: &BTreeMap<String, BTreeSet<String>>,
) -> Option<(String, BTreeSet<String>)> {
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
            return (!removable.is_empty()).then_some((projection, removable));
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

    const RECIPES: &str = "use x;\n\nconst SHARED: u8 = 1;\nconst ONLY_B: u8 = 2;\n\n/// Builds a.\nfn build_a(r: &Path) -> Result<()> {\n    helper(r, SHARED)\n}\n\n// Builds b.\n#[allow(dead_code)]\nfn build_b(r: &Path) -> Result<()> {\n    helper(r, ONLY_B)?;\n    build_c(r)\n}\n\nfn build_c(r: &Path) -> Result<()> {\n    Ok(())\n}\n\nfn helper(r: &Path, x: u8) -> Result<()> {\n    Ok(())\n}\n\nfn helper_only_b(\n    r: &Path,\n) -> Result<()> {\n    Ok(())\n}\n\n#[cfg(test)]\nmod tests {\n    fn build_a_works() {}\n}\n";

    fn dispatch() -> BTreeMap<String, BTreeSet<String>> {
        parse_recipe_dispatch(
            "\nfn build_stage_recipe(repo_root: &Path, stage: BuildStage) -> Result<()> {\n    match stage {\n        BuildStage::A => build_a(repo_root),\n        BuildStage::B => {\n            trace();\n            build_b(repo_root)\n        }\n        BuildStage::C => build_c(repo_root),\n        BuildStage::D => packaging::build_d(repo_root),\n    }\n}\n",
        )
    }

    fn view(files: &[(&str, &str)], variant: &str, file: &str) -> Option<String> {
        let index = ItemIndex::build(
            files
                .iter()
                .map(|(path, source)| (PathBuf::from(path), rust_items::chunks(source)))
                .collect(),
            Tokens::Code,
        );
        let roots = dispatch().get(variant).cloned().unwrap_or_default();
        let reachable = index.reachable(variant, roots, BARRIERS);
        project(&index.files[Path::new(file)], &reachable)
    }

    fn recipes_view(source: &str, variant: &str) -> String {
        view(&[("recipes.rs", source)], variant, "recipes.rs").unwrap()
    }

    #[test]
    fn dispatch_maps_each_stage_to_its_recipe_functions() {
        let functions = dispatch();
        assert_eq!(functions["A"], BTreeSet::from(["build_a".to_string()]));
        assert_eq!(functions["B"], BTreeSet::from(["build_b".to_string()]));
        assert_eq!(functions["D"], BTreeSet::from(["build_d".to_string()]));
    }

    #[test]
    fn a_stage_hashes_only_what_its_recipe_reaches() {
        let a = recipes_view(RECIPES, "A");
        assert!(a.contains("fn build_a(") && a.contains("fn helper(") && a.contains("SHARED"));
        assert!(a.contains("use x;"), "imports are kept unconditionally");
        for absent in [
            "fn build_b(",
            "Builds b",
            "fn build_c(",
            "ONLY_B",
            "helper_only_b",
            "mod tests",
        ] {
            assert!(
                !a.contains(absent),
                "A's projection must not contain {absent}"
            );
        }
        // B calls C's recipe, so it stays part of B's input.
        let b = recipes_view(RECIPES, "B");
        assert!(b.contains("fn build_c(") && b.contains("ONLY_B") && !b.contains("fn build_a("));
    }

    #[test]
    fn tests_unreachable_helpers_and_other_recipes_never_change_a_stage() {
        let a = recipes_view(RECIPES, "A");
        for edited in [
            RECIPES.replace(
                "    helper(r, ONLY_B)?;\n",
                "    helper(r, ONLY_B)?;\n    helper(r, ONLY_B)?;\n",
            ),
            RECIPES.replace(
                "fn build_a_works() {}",
                "fn build_a_works() { assert!(true) }",
            ),
            RECIPES.replace(
                "fn helper_only_b(\n    r: &Path,\n) -> Result<()> {\n    Ok(())",
                "fn helper_only_b(\n    r: &Path,\n) -> Result<()> {\n    Err(x)",
            ),
            RECIPES.replace("const ONLY_B: u8 = 2;", "const ONLY_B: u8 = 3;"),
            format!(
                "{RECIPES}\nfn new_helper_nobody_calls() {{}}\n\n#[cfg(test)]\nmod more_tests {{}}\n"
            ),
            RECIPES.replace(
                "fn helper(r: &Path, x: u8)",
                "#[cfg(test)]\nmod early_tests {}\n\nfn helper(r: &Path, x: u8)",
            ),
            RECIPES.replace("const ONLY_B: u8 = 2;\n", "const ONLY_B: u8 = 2;\n\n\n"),
        ] {
            assert_eq!(recipes_view(&edited, "A"), a);
        }
        for edited in [
            RECIPES.replace(
                "fn helper(r: &Path, x: u8) -> Result<()> {\n    Ok(())",
                "fn helper(r: &Path, x: u8) -> Result<()> {\n    Err(x)",
            ),
            RECIPES.replace("const SHARED: u8 = 1;", "const SHARED: u8 = 9;"),
            RECIPES.replace(
                "    helper(r, SHARED)\n}",
                "    helper(r, SHARED)?;\n    helper(r, SHARED)\n}",
            ),
            RECIPES.replace("use x;", "use y;"),
        ] {
            assert_ne!(recipes_view(&edited, "A"), a);
        }
    }

    #[test]
    fn helpers_in_other_files_keep_their_callbacks_reachable() {
        // build_a calls run() in an unhashed helper file, which calls back
        // into callback() beside the recipe: callback() is A's input.
        let recipes = "fn build_a(r: &Path) -> Result<()> {\n    run(r)\n}\n\nfn callback() {}\n\nfn unused() {}\n";
        let helpers = "fn run(r: &Path) -> Result<()> {\n    callback();\n    Ok(())\n}\n";
        let a = view(
            &[("recipes.rs", recipes), ("helpers.rs", helpers)],
            "A",
            "recipes.rs",
        )
        .unwrap();
        assert!(a.contains("fn callback()") && !a.contains("fn unused()"));
    }

    #[test]
    fn a_file_the_stage_can_execute_entirely_is_hashed_whole() {
        let only_a = "fn build_a(r: &Path) -> Result<()> {\n    Ok(())\n}\n";
        assert_eq!(view(&[("recipes.rs", only_a)], "A", "recipes.rs"), None);
    }

    #[test]
    fn legacy_v1_projection_still_identifies_unchanged_recipes() {
        let (v1, _) = project_v1(RECIPES, "A", &dispatch()).unwrap();
        assert!(
            v1.contains("fn helper_only_b(") && v1.contains("mod tests"),
            "v1 kept shared code"
        );
        assert!(!v1.contains("fn build_b("));
    }

    #[test]
    fn manifests_recorded_under_v1_migrate_only_while_the_v1_view_is_unchanged() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join(RECIPE_DIRECTORY).join("toolchain.rs");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        let source = "fn build_cross_toolchain(r: &Path) -> Result<()> {\n    Ok(())\n}\n\nfn build_glibc(r: &Path) -> Result<()> {\n    Ok(())\n}\n";
        fs::write(&file, source).unwrap();
        let (v1, removed) = project_v1(source, "CrossToolchain", recipe_functions()).unwrap();
        assert!(removed.contains("build_glibc"));
        let stored = format!(
            "{LEGACY_PROJECTION_PREFIX}{:x}",
            Sha256::digest(v1.as_bytes())
        );
        let relative = Path::new(RECIPE_DIRECTORY).join("toolchain.rs");
        assert!(
            legacy_projection_matches(root.path(), "cross-toolchain", &relative, &stored).unwrap()
        );
        // Another stage's recipe is outside both views.
        fs::write(
            &file,
            source.replace(
                "fn build_glibc(r: &Path) -> Result<()> {\n    Ok(())",
                "fn build_glibc(r: &Path) -> Result<()> {\n    Err(x)",
            ),
        )
        .unwrap();
        assert!(
            legacy_projection_matches(root.path(), "cross-toolchain", &relative, &stored).unwrap()
        );
        fs::write(
            &file,
            source.replace(
                "    Ok(())\n}\n\nfn build_glibc",
                "    Err(x)\n}\n\nfn build_glibc",
            ),
        )
        .unwrap();
        assert!(
            !legacy_projection_matches(root.path(), "cross-toolchain", &relative, &stored).unwrap()
        );
        assert!(
            !legacy_projection_matches(
                root.path(),
                "cross-toolchain",
                &relative,
                "whole-file-digest"
            )
            .unwrap()
        );

        // Here cross-toolchain names build_glibc only as a function pointer:
        // v1 removed it, v2 reaches it, so an unchanged v1 view proves
        // nothing about the v2 view and migration is refused.
        let pointer = "fn build_cross_toolchain(r: &Path) -> Result<()> {\n    run(build_glibc)\n}\n\nfn build_glibc(r: &Path) -> Result<()> {\n    Ok(())\n}\n";
        fs::write(&file, pointer).unwrap();
        let (v1, removed) = project_v1(pointer, "CrossToolchain", recipe_functions()).unwrap();
        assert!(removed.contains("build_glibc"));
        let stored = format!("{LEGACY_PROJECTION_PREFIX}{:x}", Sha256::digest(v1.as_bytes()));
        assert!(!legacy_projection_matches(root.path(), "cross-toolchain", &relative, &stored).unwrap());
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
                functions
                    .get(&variant)
                    .is_some_and(|names| !names.is_empty()),
                "no recipe function parsed for {variant}"
            );
        }
    }

    #[test]
    fn the_real_toolchain_stages_do_not_share_tests_or_each_others_helpers() {
        // Adding a test or a gcc-compiler-only helper to stages/toolchain.rs
        // used to change cross-toolchain's key and rebuild the whole system.
        let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let index = crate_index(&repo_root, Tokens::Code).unwrap();
        let cross = reachable(&index, "CrossToolchain", BARRIERS);
        assert!(cross.contains("build_cross_toolchain"));
        for other in [
            "build_gcc_toolchain",
            "build_glibc",
            "build_gcc_runtime",
            "build_kde_cmake",
        ] {
            assert!(
                !cross.contains(other),
                "cross-toolchain must not reach {other}"
            );
        }
        let toolchain = repo_root.join("src/tools/mattos-build/src/stages/toolchain.rs");
        let projection = project(&index.files[&toolchain], &cross).unwrap();
        assert!(projection.contains("fn build_cross_toolchain("));
        assert!(!projection.contains("#[cfg(test)]"));
        assert!(!projection.contains("fn build_gcc_toolchain("));
    }
}


