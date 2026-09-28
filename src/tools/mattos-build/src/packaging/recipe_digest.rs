//! Per-package digest of the staging code that assembles its payload.
//!
//! A package's cache key must change whenever the code that stages it
//! changes, without depending on which file that code lives in and without
//! restaging unrelated packages.  The digest covers:
//!
//! * `stage_package` with every other package's dispatch arm removed: the
//!   shared prologue and epilogue plus this package's own arm (or the
//!   fallback arm when it has none);
//! * every function, constant and static of the packaging module reachable
//!   from that view (see `rust_items`), each hashed by name, not by file;
//! * the packaging module's types, impls and macros (anything but `use` and
//!   `mod` declarations), which any staging code may rely on;
//! * crate-level constants and statics that reachable packaging code names
//!   (data only: crate helper functions are shared infrastructure outside the
//!   packaging module, covered like stage helpers by explicit revisions).

use super::*;
use crate::rust_items::{self, Chunk, ChunkKind, ItemIndex, Tokens};
use std::sync::{Arc, Mutex, OnceLock};

const PACKAGING_DIRECTORY: &str = "src/tools/mattos-build/src/packaging";
const PACKAGING_MODULE: &str = "src/tools/mattos-build/src/packaging.rs";
const CRATE_DIRECTORY: &str = "src/tools/mattos-build/src";
const DISPATCHER: &str = "stage_package";

/// Marks the digest format so a future definition change is distinguishable.
const DIGEST_FORMAT: &str = "package-staging-recipe-v3";

/// The digest of the staging code `package` executes.
pub(crate) fn package_staging_digest(repo_root: &Path, package: &str) -> Result<String> {
    let index = rust_items::indexed_files(
        &repo_root.join(PACKAGING_DIRECTORY),
        &[repo_root.join(PACKAGING_MODULE)],
        Tokens::Code,
    )?;
    let crate_index =
        rust_items::indexed_files(&repo_root.join(CRATE_DIRECTORY), &[], Tokens::Code)?;
    Ok(digest_with(&shared_context(&index, &crate_index), &index, package))
}

/// Package-independent material: the dispatcher, every named item with its
/// identifiers, the module's types, impls and macros with their subjects,
/// and crate-level data.  Built once per pair of index generations rather
/// than once per package.
struct Context {
    /// `stage_package`, split into its arms once.
    dispatcher: Option<Dispatcher>,
    named: BTreeMap<String, Item>,
    /// Types, impls and macros: (subject, item), sorted by text digest.
    other: Vec<(Option<String>, Item)>,
    /// Indices into `other` by subject; items without one are always kept.
    other_by_subject: BTreeMap<String, Vec<usize>>,
    crate_data: BTreeMap<String, Item>,
}

/// One item (every definition of one name, or one unnamed item): the digest
/// of its text and the identifiers it mentions.  Hashing each item once lets
/// every package's key hash a short list of digests instead of the texts.
struct Item {
    digest: [u8; 32],
    identifiers: Vec<String>,
}

impl Item {
    fn new(mut texts: Vec<&str>) -> Self {
        texts.sort_unstable();
        let text = texts.join("\n");
        let mut identifiers = rust_items::identifiers(&text, Tokens::Code);
        identifiers.sort_unstable();
        identifiers.dedup();
        Self {
            digest: Sha256Hasher::digest(text.as_bytes()).into(),
            identifiers,
        }
    }
}

impl Context {
    fn new(index: &ItemIndex, crate_index: &ItemIndex) -> Self {
        let mut named = BTreeMap::<String, Vec<&str>>::new();
        let mut other = Vec::new();
        for chunk in index.files.values().flatten().filter(|chunk| !chunk.test_only) {
            match &chunk.kind {
                ChunkKind::Named(name) if name != DISPATCHER => {
                    named.entry(name.clone()).or_default().push(chunk.content());
                }
                ChunkKind::Named(_) => {}
                ChunkKind::Other if !chunk.is_declaration() => {
                    other.push((chunk.subject(), Item::new(vec![chunk.content()])));
                }
                ChunkKind::Other => {}
            }
        }
        other.sort_unstable_by(|left, right| left.1.digest.cmp(&right.1.digest));
        let mut crate_data = BTreeMap::<String, Vec<&str>>::new();
        for chunk in crate_index.files.values().flatten().filter(|chunk| !chunk.test_only) {
            if let ChunkKind::Named(name) = &chunk.kind
                && is_data_name(name)
                && !index.defines(name)
            {
                crate_data.entry(name.clone()).or_default().push(chunk.content());
            }
        }
        let mut other_by_subject = BTreeMap::<String, Vec<usize>>::new();
        for (at, (subject, _)) in other.iter().enumerate() {
            if let Some(subject) = subject {
                other_by_subject.entry(subject.clone()).or_default().push(at);
            }
        }
        Self {
            dispatcher: dispatcher_text(index).map(|text| Dispatcher::new(&text)),
            named: named.into_iter().map(|(name, texts)| (name, Item::new(texts))).collect(),
            other,
            other_by_subject,
            crate_data: crate_data.into_iter().map(|(name, texts)| (name, Item::new(texts))).collect(),
        }
    }
}

fn shared_context(index: &Arc<ItemIndex>, crate_index: &Arc<ItemIndex>) -> Arc<Context> {
    type Key = (usize, usize);
    static CACHE: OnceLock<Mutex<Option<(Key, Arc<Context>)>>> = OnceLock::new();
    let key = (Arc::as_ptr(index) as usize, Arc::as_ptr(crate_index) as usize);
    let cache = CACHE.get_or_init(|| Mutex::new(None));
    let mut slot = cache.lock().unwrap();
    if let Some((cached, context)) = slot.as_ref()
        && *cached == key
    {
        return context.clone();
    }
    let context = Arc::new(Context::new(index, crate_index));
    *slot = Some((key, context.clone()));
    context
}

#[cfg(test)]
fn staging_digest(index: &ItemIndex, crate_index: &ItemIndex, package: &str) -> String {
    digest_with(&Context::new(index, crate_index), index, package)
}

fn digest_with(context: &Context, index: &ItemIndex, package: &str) -> String {
    let view = context
        .dispatcher
        .as_ref()
        .map_or_else(String::new, |dispatcher| dispatcher.view(package));
    let view_identifiers = rust_items::identifiers(&view, Tokens::Code);
    let reachable = index.reachable(&format!("package:{package}"), view_identifiers.clone(), &[DISPATCHER]);
    let named = reachable
        .iter()
        .filter_map(|name| context.named.get(name).map(|item| (name, item)))
        .collect::<BTreeMap<_, _>>();
    // The module's types, impls and macros whose subject this code names,
    // and crate-level data (constants, statics) it names, followed through
    // what those name in turn.  An item without a single subject is kept.
    let mut included = context
        .other
        .iter()
        .map(|(subject, _)| subject.is_none())
        .collect::<Vec<_>>();
    let mut data = BTreeMap::<&String, &Item>::new();
    let mut seen = BTreeSet::<&str>::new();
    let mut pending = named
        .values()
        .flat_map(|item| item.identifiers.iter())
        .chain(view_identifiers.iter())
        .chain(
            context
                .other
                .iter()
                .filter(|(subject, _)| subject.is_none())
                .flat_map(|(_, item)| item.identifiers.iter()),
        )
        .map(String::as_str)
        .collect::<Vec<_>>();
    while let Some(identifier) = pending.pop() {
        if !seen.insert(identifier) {
            continue;
        }
        for &at in context.other_by_subject.get(identifier).into_iter().flatten() {
            if !included[at] {
                included[at] = true;
                pending.extend(context.other[at].1.identifiers.iter().map(String::as_str));
            }
        }
        if let Some((name, item)) = context.crate_data.get_key_value(identifier)
            && data.insert(name, item).is_none()
        {
            pending.extend(item.identifiers.iter().map(String::as_str));
        }
    }
    let mut digest = Sha256Hasher::new();
    digest.update(DIGEST_FORMAT.as_bytes());
    digest.update(view.len().to_le_bytes());
    digest.update(view.as_bytes());
    for (name, item) in named.into_iter().chain(data) {
        digest.update(b"item:");
        digest.update(name.len().to_le_bytes());
        digest.update(name.as_bytes());
        digest.update(item.digest);
    }
    for ((_, item), _) in context.other.iter().zip(&included).filter(|(_, included)| **included) {
        digest.update(b"other:");
        digest.update(item.digest);
    }
    format!("{DIGEST_FORMAT}:{:x}", digest.finalize())
}

/// Constants and statics are SCREAMING_CASE; functions are not.
fn is_data_name(name: &str) -> bool {
    name.chars().any(|c| c.is_ascii_uppercase())
        && name
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

fn dispatcher_text(index: &ItemIndex) -> Option<String> {
    let texts = index
        .files
        .values()
        .flatten()
        .filter(|chunk| !chunk.test_only && chunk.kind == ChunkKind::Named(DISPATCHER.to_string()))
        .map(|chunk: &Chunk| chunk.content().to_string())
        .collect::<Vec<_>>();
    (texts.len() == 1).then(|| texts.into_iter().next().unwrap())
}

/// One arm of the dispatcher's `match spec.name`.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct DispatchArm {
    pub(crate) packages: Vec<String>,
    pub(crate) fallback: bool,
    pub(crate) text: String,
}

/// Splits `stage_package` into the text before its `match spec.name`, the
/// arms, and the text after the match.
pub(crate) fn dispatch_arms(dispatcher: &str) -> Option<(String, Vec<DispatchArm>, String)> {
    let start = dispatcher.find("    match spec.name {\n")? + "    match spec.name {\n".len();
    let end = start + dispatcher[start..].find("\n    }\n")? + 1;
    let mut arms: Vec<DispatchArm> = Vec::new();
    // Rustfmt indents arm patterns by eight spaces; arm bodies, `|`
    // continuations and closing delimiters are indented further or start
    // with punctuation.
    for line in dispatcher[start..end].split_inclusive('\n') {
        let arm_start = line.starts_with("        ")
            && line[8..].starts_with(|c: char| c == '"' || c == '_' || c.is_ascii_lowercase());
        if arm_start || arms.is_empty() {
            arms.push(DispatchArm {
                packages: Vec::new(),
                fallback: false,
                text: String::new(),
            });
        }
        arms.last_mut()?.text.push_str(line);
    }
    for arm in &mut arms {
        let pattern = &arm.text[..arm.text.find("=>").unwrap_or(arm.text.len())];
        arm.packages = pattern
            .split('"')
            .skip(1)
            .step_by(2)
            .map(str::to_string)
            .collect();
        // `_` or a bare binding such as `name`: every package without an arm.
        arm.fallback = arm.packages.is_empty();
    }
    Some((
        dispatcher[..start].to_string(),
        arms,
        dispatcher[end..].to_string(),
    ))
}

/// `stage_package` split into the text around its match and its arms.
enum Dispatcher {
    Split(String, Vec<DispatchArm>, String),
    /// A dispatcher whose match could not be parsed is hashed whole.
    Whole(String),
}

impl Dispatcher {
    fn new(text: &str) -> Self {
        match dispatch_arms(text) {
            Some((prologue, arms, epilogue)) => Self::Split(prologue, arms, epilogue),
            None => Self::Whole(text.to_string()),
        }
    }

    /// `stage_package` as `package` executes it: every other package's arm
    /// removed (the fallback arm stands in for a package without one).
    fn view(&self, package: &str) -> String {
        let (prologue, arms, epilogue) = match self {
            Self::Split(prologue, arms, epilogue) => (prologue, arms, epilogue),
            Self::Whole(text) => return text.clone(),
        };
        let own = arms
            .iter()
            .find(|arm| arm.packages.iter().any(|name| name == package))
            .or_else(|| arms.iter().find(|arm| arm.fallback));
        format!("{prologue}{}{epilogue}", own.map_or("", |arm| arm.text.as_str()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STAGING: &str = "use super::*;\n\npub(crate) fn stage_package(repo_root: &Path, spec: &PackageSpec) -> Result<()> {\n    let staging = root(spec);\n    match spec.name {\n        \"alpha\" => stage_alpha(repo_root, &staging)?,\n        \"beta\" | \"gamma\" => {\n            stage_shared(repo_root, \"beta\")?;\n        }\n        name => bail!(\"no staging implementation for {name}\"),\n    }\n    finish(&staging)?;\n    Ok(())\n}\n\nfn stage_alpha(repo_root: &Path, staging: &Path) -> Result<()> {\n    copy(ALPHA_FILES)\n}\n\nconst ALPHA_FILES: &[&str] = &[\"a\"];\n\nfn stage_shared(repo_root: &Path, name: &str) -> Result<()> {\n    Ok(())\n}\n\nfn finish(staging: &Path) -> Result<()> {\n    write(Provenance::default())\n}\n\nstruct Provenance {\n    name: String,\n}\n\nstruct CacheManifest {\n    key: String,\n}\n";

    fn digest(files: &[(&str, &str)], crate_files: &[(&str, &str)], package: &str) -> String {
        let index = |files: &[(&str, &str)]| {
            ItemIndex::build(
                files
                    .iter()
                    .map(|(path, source)| (PathBuf::from(path), rust_items::chunks(source)))
                    .collect(),
                Tokens::Code,
            )
        };
        staging_digest(&index(files), &index(crate_files), package)
    }

    fn crate_files_owned(main: String) -> [(&'static str, &'static str); 1] {
        [("main.rs", Box::leak(main.into_boxed_str()))]
    }

    fn staging(source: &str, package: &str) -> String {
        digest(&[("packaging/staging.rs", source)], &[], package)
    }

    #[test]
    fn dispatcher_arms_name_their_packages() {
        let (prologue, arms, epilogue) = dispatch_arms(STAGING).unwrap();
        assert!(prologue.ends_with("    match spec.name {\n"));
        assert!(epilogue.starts_with("    }\n    finish("));
        assert_eq!(arms.len(), 3);
        assert_eq!(arms[0].packages, ["alpha"]);
        assert_eq!(arms[1].packages, ["beta", "gamma"]);
        assert!(arms[2].fallback);
    }

    #[test]
    fn a_package_restages_when_its_own_code_changes() {
        let alpha = staging(STAGING, "alpha");
        for edited in [
            STAGING.replace(
                "stage_alpha(repo_root, &staging)?",
                "stage_alpha(repo_root, &staging).ok();",
            ),
            STAGING.replace("&[\"a\"]", "&[\"a\", \"b\"]"),
            STAGING.replace(
                "write(Provenance::default())",
                "write(Provenance::new())",
            ),
            STAGING.replace(
                "    let staging = root(spec);",
                "    let staging = other_root(spec);",
            ),
            STAGING.replace(
                "    name: String,",
                "    name: String,\n    version: String,",
            ),
        ] {
            assert_ne!(staging(&edited, "alpha"), alpha);
        }
    }

    #[test]
    fn other_packages_code_and_file_layout_never_restage_a_package() {
        let alpha = staging(STAGING, "alpha");
        for edited in [
            STAGING.replace(
                "stage_shared(repo_root, \"beta\")?;",
                "stage_shared(repo_root, \"beta\")?;\n            other()?;",
            ),
            STAGING.replace(
                "fn stage_shared(repo_root: &Path, name: &str) -> Result<()> {\n    Ok(())",
                "fn stage_shared(repo_root: &Path, name: &str) -> Result<()> {\n    Err(x)",
            ),
            format!("{STAGING}\n#[cfg(test)]\nmod tests {{\n    fn t() {{}}\n}}\n"),
            // A type no staging code mentions (package-cache bookkeeping).
            STAGING.replace("    key: String,", "    key: String,\n    schema: u32,"),
            format!("{STAGING}\nimpl CacheManifest {{\n    fn bump(&mut self) {{}}\n}}\n"),
            STAGING.replace("use super::*;", "use super::*;\nuse std::fs;"),
        ] {
            assert_eq!(staging(&edited, "alpha"), alpha, "edit {edited:?}");
        }
        // Moving stage_alpha into another file changes no key.
        let (start, end) = (
            STAGING.find("fn stage_alpha(").unwrap(),
            STAGING.find("const ALPHA_FILES").unwrap(),
        );
        let moved = format!("{}{}", &STAGING[..start], &STAGING[end..]);
        let split = digest(
            &[
                ("packaging/staging.rs", &moved),
                ("packaging/staging/alpha.rs", &STAGING[start..end]),
            ],
            &[],
            "alpha",
        );
        assert_eq!(split, alpha);
    }

    #[test]
    fn packages_sharing_an_arm_share_its_changes_and_unknown_ones_use_the_fallback() {
        let edited = STAGING.replace(
            "stage_shared(repo_root, \"beta\")?;",
            "stage_shared(repo_root, \"gamma\")?;",
        );
        assert_ne!(staging(&edited, "beta"), staging(STAGING, "beta"));
        assert_ne!(staging(&edited, "gamma"), staging(STAGING, "gamma"));
        let fallback = STAGING.replace(
            "no staging implementation",
            "missing staging implementation",
        );
        assert_ne!(staging(&fallback, "delta"), staging(STAGING, "delta"));
        assert_eq!(staging(&fallback, "alpha"), staging(STAGING, "alpha"));
    }

    #[test]
    fn crate_constants_named_by_staging_code_are_part_of_the_key() {
        let source = STAGING.replace("copy(ALPHA_FILES)", "copy(TERMINFO_ENTRIES)");
        let crate_files = |entries: &'static str| [("main.rs", entries)];
        let one = digest(
            &[("packaging/staging.rs", &source)],
            &crate_files("const TERMINFO_ENTRIES: &[&str] = &[\"xterm\"];\n\nfn helper() {}\n"),
            "alpha",
        );
        let two = digest(
            &[("packaging/staging.rs", &source)],
            &crate_files("const TERMINFO_ENTRIES: &[&str] = &[\"vt100\"];\n\nfn helper() {}\n"),
            "alpha",
        );
        let three = digest(
            &[("packaging/staging.rs", &source)],
            &crate_files(
                "const TERMINFO_ENTRIES: &[&str] = &[\"xterm\"];\n\nfn helper() { changed() }\n",
            ),
            "alpha",
        );
        assert_ne!(one, two, "crate data the package names is hashed");
        // Data named only by other data, or only by the dispatcher itself.
        let chained = |value: &str| {
            digest(
                &[("packaging/staging.rs", &source)],
                &crate_files_owned(format!("const TERMINFO_ENTRIES: &[&str] = BASE;\n\nconst BASE: &[&str] = &[\"{value}\"];\n")),
                "alpha",
            )
        };
        assert_ne!(chained("xterm"), chained("vt100"));
        let dispatched = STAGING.replace("    finish(&staging)?;", "    finish(&staging, EPILOGUE_DATA)?;");
        let by_view = |value: &str| {
            digest(
                &[("packaging/staging.rs", &dispatched)],
                &crate_files_owned(format!("const EPILOGUE_DATA: u8 = {value};\n")),
                "alpha",
            )
        };
        assert_ne!(by_view("1"), by_view("2"));
        assert_eq!(
            one, three,
            "crate helper functions are outside the packaging key"
        );
    }

    #[test]
    fn every_registered_package_has_one_dispatch_arm_or_one_table_row() {
        let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let index = rust_items::indexed_files(
            &repo_root.join(PACKAGING_DIRECTORY),
            &[repo_root.join(PACKAGING_MODULE)],
            Tokens::Code,
        )
        .unwrap();
        let dispatcher = dispatcher_text(&index).expect("exactly one stage_package");
        let (_, arms, _) = dispatch_arms(&dispatcher).unwrap();
        let mut owners = BTreeMap::<&str, usize>::new();
        for arm in &arms {
            for package in &arm.packages {
                *owners.entry(package.as_str()).or_default() += 1;
            }
        }
        let table_rows = super::super::staging::table_packages();
        for package in PACKAGE_NAMES {
            let rows = table_rows.iter().filter(|row| row == &package).count();
            let arms = owners.get(package).copied().unwrap_or(0);
            assert_eq!(
                arms + rows,
                1,
                "{package} needs exactly one dispatch arm or table row"
            );
        }
        assert_eq!(arms.iter().filter(|arm| arm.fallback).count(), 1);
    }

    #[test]
    fn the_real_packages_have_distinct_keys_for_distinct_staging_code() {
        let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let flatpak = package_staging_digest(&repo_root, "flatpak").unwrap();
        let libgcc = package_staging_digest(&repo_root, "libgcc-s1").unwrap();
        let libgomp = package_staging_digest(&repo_root, "libgomp1").unwrap();
        assert_ne!(flatpak, libgcc);
        assert_ne!(
            libgcc, libgomp,
            "each arm's own arguments are part of its key"
        );
        assert_eq!(
            flatpak,
            package_staging_digest(&repo_root, "flatpak").unwrap()
        );
    }
}
