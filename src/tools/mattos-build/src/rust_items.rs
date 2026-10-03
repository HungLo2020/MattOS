//! Top-level items of mattos-build's own Rust sources, and the identifier
//! graph between them.
//!
//! Cache keys that cover build-tool code (stage recipe projections, package
//! staging digests) hash the code a stage or package can actually execute:
//! the functions, constants and statics reachable from its entry points.
//! This module splits rustfmt-formatted files into top-level items and finds
//! that reachable set.  Reachability is textual (an item is reached when a
//! reached item mentions its name), which over-approximates calls: a
//! misjudgement can only widen a key, never let an executed change go
//! unhashed.

use anyhow::Result;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::SystemTime;

/// One top-level item and the comments, attributes and blank lines around it.
#[derive(Clone, Debug)]
pub(crate) struct Chunk {
    pub(crate) text: String,
    pub(crate) kind: ChunkKind,
    pub(crate) test_only: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ChunkKind {
    /// A function, constant or static: part of a key only when reachable.
    Named(String),
    /// Imports, types, impls, macros and anything unrecognized.
    Other,
}

impl Chunk {
    /// The item's text without the blank lines that separate it from the
    /// next item, which belong to layout rather than to the item.
    pub(crate) fn content(&self) -> &str {
        self.text.trim_end()
    }

    /// A `use` or `mod` declaration: namespace plumbing with no behavior.
    pub(crate) fn is_declaration(&self) -> bool {
        let header = header(&self.text);
        let header = header
            .strip_prefix("pub(crate) ")
            .or_else(|| header.strip_prefix("pub(super) "))
            .or_else(|| header.strip_prefix("pub "))
            .unwrap_or(header);
        header.starts_with("use ") || (header.starts_with("mod ") && header.ends_with(';'))
    }

    /// The type, trait or macro a non-function item defines or implements:
    /// `Foo` for `struct Foo`, `enum Foo<T>`, `impl<T> Bar for Foo<T>` or
    /// `impl Foo`, and `name` for `macro_rules! name`.  `None` when the item
    /// has no single subject.
    pub(crate) fn subject(&self) -> Option<String> {
        let header = header(&self.text);
        let mut rest = header;
        if let Some(after) = rest.strip_prefix("pub") {
            rest = after
                .strip_prefix(' ')
                .or_else(|| after.find(") ").map(|at| &after[at + 2..]))?;
        }
        let rest = rest.strip_prefix("unsafe ").unwrap_or(rest);
        let name_of = |text: &str| {
            let name = text
                .trim_start_matches(['&', '\'', ' '])
                .split(|c: char| !is_identifier_continue(c))
                .next()
                .unwrap_or_default();
            (!name.is_empty()).then(|| name.to_string())
        };
        for keyword in ["struct ", "enum ", "union ", "trait ", "type ", "macro_rules! "] {
            if let Some(after) = rest.strip_prefix(keyword) {
                return name_of(after);
            }
        }
        let after = rest.strip_prefix("impl")?;
        // Skip the impl's own generic parameters, then take the implemented
        // type (after `for` when a trait is implemented), without its path.
        let mut depth = 0;
        let mut body = after;
        for (at, c) in after.char_indices() {
            match c {
                '<' => depth += 1,
                '>' => depth -= 1,
                _ if depth == 0 && !c.is_whitespace() => {
                    body = &after[at..];
                    break;
                }
                _ => {}
            }
        }
        let body = body.split('{').next().unwrap_or_default();
        let body = body.split(" where ").next().unwrap_or_default();
        let target = body.rsplit(" for ").next().unwrap_or(body).trim();
        let target = target.split('<').next().unwrap_or(target);
        name_of(target.rsplit("::").next().unwrap_or(target))
    }
}

/// Every top-level item of a set of Rust files, keyed by file.
pub(crate) struct ItemIndex {
    pub(crate) files: BTreeMap<PathBuf, Vec<Chunk>>,
    /// Top-level function/constant/static names to the identifiers their
    /// bodies mention.  A name defined in several files merges their bodies.
    references: BTreeMap<String, BTreeSet<String>>,
    /// Identifiers mentioned by non-test items that are always kept.
    unconditional: BTreeSet<String>,
    reachable: Mutex<BTreeMap<String, Arc<BTreeSet<String>>>>,
    /// Closure of the always-kept items' identifiers, per barrier set: the
    /// part of every reachable set that does not depend on its roots.
    base: Mutex<BTreeMap<Vec<String>, Arc<BTreeSet<String>>>>,
}

impl ItemIndex {
    pub(crate) fn build(files: BTreeMap<PathBuf, Vec<Chunk>>, tokens: Tokens) -> Self {
        let mut references: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut unconditional = BTreeSet::new();
        for chunk in files.values().flatten().filter(|chunk| !chunk.test_only) {
            match &chunk.kind {
                ChunkKind::Named(name) => {
                    references
                        .entry(name.clone())
                        .or_default()
                        .extend(identifiers(&chunk.text, tokens));
                }
                ChunkKind::Other => unconditional.extend(identifiers(&chunk.text, tokens)),
            }
        }
        Self {
            files,
            references,
            unconditional,
            reachable: Mutex::new(BTreeMap::new()),
            base: Mutex::new(BTreeMap::new()),
        }
    }

    /// Whether `name` is a top-level function, constant or static here.
    pub(crate) fn defines(&self, name: &str) -> bool {
        self.references.contains_key(name)
    }

    /// Named items reachable from `roots`, from the identifiers of every
    /// always-kept item (impl methods and macros can call any function), and
    /// transitively from what those mention.  Reachability never passes
    /// through a `barrier`.  Results are memoized under `key`.
    pub(crate) fn reachable(
        &self,
        key: &str,
        roots: impl IntoIterator<Item = String>,
        barriers: &[&str],
    ) -> Arc<BTreeSet<String>> {
        if let Some(cached) = self.reachable.lock().unwrap().get(key) {
            return cached.clone();
        }
        let mut reached = (*self.base(barriers)).clone();
        self.extend_closure(&mut reached, roots, barriers);
        let reached = Arc::new(reached);
        self.reachable
            .lock()
            .unwrap()
            .insert(key.to_string(), reached.clone());
        reached
    }

    fn base(&self, barriers: &[&str]) -> Arc<BTreeSet<String>> {
        let key = barriers.iter().map(|barrier| (*barrier).to_string()).collect::<Vec<_>>();
        if let Some(cached) = self.base.lock().unwrap().get(&key) {
            return cached.clone();
        }
        let mut base = BTreeSet::new();
        self.extend_closure(&mut base, self.unconditional.iter().cloned(), barriers);
        let base = Arc::new(base);
        self.base.lock().unwrap().insert(key, base.clone());
        base
    }

    /// Adds to `reached` every defined item reachable from `roots` without
    /// passing a barrier or revisiting an item already reached.
    fn extend_closure(
        &self,
        reached: &mut BTreeSet<String>,
        roots: impl IntoIterator<Item = String>,
        barriers: &[&str],
    ) {
        let mut pending = roots
            .into_iter()
            .filter(|name| self.defines(name) && !reached.contains(name))
            .collect::<BTreeSet<_>>();
        while let Some(name) = pending.pop_first() {
            if barriers.contains(&name.as_str()) || !reached.insert(name.clone()) {
                continue;
            }
            if let Some(mentioned) = self.references.get(&name) {
                pending.extend(
                    mentioned
                        .iter()
                        .filter(|word| self.defines(word) && !reached.contains(*word))
                        .cloned(),
                );
            }
        }
    }
}

type Fingerprint = Vec<(PathBuf, u64, Option<SystemTime>)>;

/// The index of every non-test Rust file under `directory` (and of each
/// file in `extra_files` that exists), rebuilt whenever one of them changes.
pub(crate) fn indexed_files(
    directory: &Path,
    extra_files: &[PathBuf],
    tokens: Tokens,
) -> Result<Arc<ItemIndex>> {
    static CACHE: OnceLock<
        Mutex<BTreeMap<(PathBuf, Vec<PathBuf>, Tokens), (Fingerprint, Arc<ItemIndex>)>>,
    > = OnceLock::new();
    let mut paths = Vec::new();
    collect_rust_files(directory, &mut paths)?;
    paths.extend(extra_files.iter().filter(|path| path.is_file()).cloned());
    paths.sort();
    paths.dedup();
    let fingerprint = paths
        .iter()
        .map(|path| {
            let metadata = fs::metadata(path).ok();
            (
                path.clone(),
                metadata.as_ref().map_or(0, fs::Metadata::len),
                metadata.and_then(|metadata| metadata.modified().ok()),
            )
        })
        .collect::<Fingerprint>();
    let key = (directory.to_path_buf(), extra_files.to_vec(), tokens);
    let cache = CACHE.get_or_init(|| Mutex::new(BTreeMap::new()));
    if let Some((cached, index)) = cache.lock().unwrap().get(&key)
        && *cached == fingerprint
    {
        return Ok(index.clone());
    }
    let mut files = BTreeMap::new();
    for path in &paths {
        let source = fs::read_to_string(path)?;
        let split = match tokens {
            Tokens::Code => chunks(&source),
            Tokens::Words | Tokens::CodeLineSplit => line_split_chunks(&source),
        };
        files.insert(path.clone(), split);
    }
    let index = Arc::new(ItemIndex::build(files, tokens));
    cache
        .lock()
        .unwrap()
        .insert(key, (fingerprint, index.clone()));
    Ok(index)
}

fn collect_rust_files(directory: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    let Ok(entries) = fs::read_dir(directory) else {
        return Ok(());
    };
    for entry in entries {
        let path = entry?.path();
        if path.is_dir() {
            collect_rust_files(&path, files)?;
        } else if path.extension().is_some_and(|extension| extension == "rs")
            && !is_test_module(&path)
        {
            files.push(path);
        }
    }
    Ok(())
}

/// Files holding only a `#[cfg(test)] mod` body (`build_system_tests.rs`,
/// `*_tests.rs`, `tests.rs`) carry no build behavior.
pub(crate) fn is_test_module(path: &Path) -> bool {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .is_some_and(|stem| stem == "tests" || stem.ends_with("_tests"))
}

/// How identifiers are read from an item's text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Tokens {
    /// Identifiers in code: comments and string contents are skipped (a call
    /// cannot hide there), except identifiers a format string captures
    /// (`"{NAME}"` can name a constant).
    Code,
    /// Every alphanumeric word, including those in comments and strings.
    /// Only for recomputing digests recorded before `Code` existed.
    Words,
    /// `Code` identifiers over the line-based item split that preceded
    /// [`chunks`]' lexical split.  Only for recomputing v3 projections.
    CodeLineSplit,
}

/// The identifiers `text` mentions, read as `tokens` describes.
pub(crate) fn identifiers(text: &str, tokens: Tokens) -> Vec<String> {
    match tokens {
        Tokens::Code | Tokens::CodeLineSplit => code_identifiers(text),
        Tokens::Words => text
            .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .filter(|word| word.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_'))
            .map(str::to_string)
            .collect(),
    }
}

fn is_identifier_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

fn is_identifier_continue(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// A small Rust lexer: identifiers outside comments, string and character
/// literals, plus the identifiers format strings capture.
fn code_identifiers(text: &str) -> Vec<String> {
    let chars = text.chars().collect::<Vec<_>>();
    let mut identifiers = Vec::new();
    let mut at = 0;
    while at < chars.len() {
        let c = chars[at];
        let next = chars.get(at + 1).copied();
        if c == '/' && next == Some('/') {
            while at < chars.len() && chars[at] != '\n' {
                at += 1;
            }
        } else if c == '/' && next == Some('*') {
            let mut depth = 0;
            while at < chars.len() {
                if chars[at] == '/' && chars.get(at + 1) == Some(&'*') {
                    depth += 1;
                    at += 2;
                } else if chars[at] == '*' && chars.get(at + 1) == Some(&'/') {
                    depth -= 1;
                    at += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    at += 1;
                }
            }
        } else if c == '"' {
            at = skip_string(&chars, at + 1, 0, &mut identifiers);
        } else if c == '\'' {
            // A character literal ('x', '\n', '\u{..}') or a lifetime ('a).
            if next == Some('\\') {
                at += 2;
                while at < chars.len() && chars[at] != '\'' {
                    at += 1;
                }
                at += 1;
            } else if chars.get(at + 2) == Some(&'\'') {
                at += 3;
            } else {
                at += 1;
                while at < chars.len() && is_identifier_continue(chars[at]) {
                    at += 1;
                }
            }
        } else if is_identifier_start(c) {
            let start = at;
            while at < chars.len() && is_identifier_continue(chars[at]) {
                at += 1;
            }
            let word = chars[start..at].iter().collect::<String>();
            // String prefixes: r"..", r#".."#, b"..", br#".."#.
            if matches!(word.as_str(), "r" | "b" | "br") {
                let mut hashes = 0;
                while chars.get(at + hashes) == Some(&'#') {
                    hashes += 1;
                }
                if chars.get(at + hashes) == Some(&'"') && (hashes == 0 || word != "b") {
                    let raw = word != "b";
                    at = if raw {
                        skip_raw_string(&chars, at + hashes + 1, hashes, &mut identifiers)
                    } else {
                        skip_string(&chars, at + 1, 0, &mut identifiers)
                    };
                    continue;
                }
            }
            identifiers.push(word);
        } else {
            at += 1;
        }
    }
    identifiers
}

/// Skips an escaped string body starting at `at`; returns the index after
/// its closing quote.
fn skip_string(chars: &[char], mut at: usize, _hashes: usize, identifiers: &mut Vec<String>) -> usize {
    let start = at;
    while at < chars.len() && chars[at] != '"' {
        at += if chars[at] == '\\' { 2 } else { 1 };
    }
    format_captures(&chars[start..at.min(chars.len())], identifiers);
    at + 1
}

/// Skips a raw string body closed by `"` and `hashes` `#`s.
fn skip_raw_string(chars: &[char], mut at: usize, hashes: usize, identifiers: &mut Vec<String>) -> usize {
    let start = at;
    while at < chars.len() {
        if chars[at] == '"' && (0..hashes).all(|offset| chars.get(at + 1 + offset) == Some(&'#')) {
            format_captures(&chars[start..at], identifiers);
            return at + 1 + hashes;
        }
        at += 1;
    }
    format_captures(&chars[start..], identifiers);
    at
}

/// Identifiers captured by `{NAME}` or `{NAME:spec}` in a format string;
/// `{{` is an escaped brace.
fn format_captures(body: &[char], identifiers: &mut Vec<String>) {
    let mut at = 0;
    while at < body.len() {
        if body[at] == '{' && body.get(at + 1) == Some(&'{') {
            at += 2;
        } else if body[at] == '{' && body.get(at + 1).copied().is_some_and(is_identifier_start) {
            let start = at + 1;
            at = start;
            while at < body.len() && is_identifier_continue(body[at]) {
                at += 1;
            }
            if matches!(body.get(at), Some('}' | ':')) {
                identifiers.push(body[start..at].iter().collect());
            }
        } else {
            at += 1;
        }
    }
}

/// The first line of `text` that is not blank, a comment or an attribute.
fn header(text: &str) -> &str {
    text.lines()
        .map(str::trim)
        .find(|line| {
            !line.is_empty()
                && !line.starts_with("//")
                && !line.starts_with("#[")
                && !line.starts_with("#![")
        })
        .unwrap_or_default()
}

/// Splits rustfmt-formatted `source` into top-level items.  An item starts at
/// a column-0 line in code (not inside a string or comment) at brace depth 0
/// that follows a blank line or the end of the previous item; an item ends at
/// a line that returns to brace depth 0 in code and closes with `}` or `;`.
/// Leading comments and attributes stay with their item.  Tracking literals
/// and depth keeps a column-0 line inside a multi-line string (an embedded
/// C++ snippet, say) from splitting the function that contains it.
pub(crate) fn chunks(source: &str) -> Vec<Chunk> {
    let lines = source.split_inclusive('\n').collect::<Vec<_>>();
    let states = line_states(source);
    let mut texts: Vec<String> = Vec::new();
    let mut previous_ends_item = true;
    for (line, state) in lines.iter().zip(states) {
        let content = line.trim_end_matches(['\n', '\r']);
        let top_level_code = state.starts_in_code && state.depth_at_start == 0;
        if content.trim().is_empty() {
            if top_level_code {
                previous_ends_item = true;
            }
        } else {
            let at_column_zero = !content.starts_with(char::is_whitespace);
            let continuation = content.starts_with(['}', ')', ']', ';']);
            if top_level_code && at_column_zero && !continuation && previous_ends_item {
                texts.push(String::new());
            }
            let trimmed = content.trim_end();
            previous_ends_item = state.ends_in_code
                && state.depth_at_end == 0
                && (trimmed.ends_with('}') || trimmed.ends_with(';'));
        }
        if texts.is_empty() {
            texts.push(String::new());
        }
        texts.last_mut().unwrap().push_str(line);
    }
    texts.into_iter().map(chunk_from_text).collect()
}

fn chunk_from_text(text: String) -> Chunk {
    let test_only = text
        .lines()
        .map(str::trim)
        .take_while(|line| line.is_empty() || line.starts_with("//") || line.starts_with("#["))
        .any(|line| line == "#[cfg(test)]");
    Chunk {
        kind: item_name(header(&text)).map_or(ChunkKind::Other, ChunkKind::Named),
        text,
        test_only,
    }
}

/// Lexical state at the start and end of each line of `source` (as split by
/// `split_inclusive('\n')`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LineState {
    starts_in_code: bool,
    ends_in_code: bool,
    depth_at_start: i64,
    depth_at_end: i64,
}

/// Scans `source` once, tracking comments, string, raw-string and character
/// literals across lines, and the depth of `{}` braces in code.
fn line_states(source: &str) -> Vec<LineState> {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Mode {
        Code,
        Block(usize),
        Str,
        Raw(usize),
    }
    let chars = source.chars().collect::<Vec<_>>();
    let mut states = Vec::new();
    let mut mode = Mode::Code;
    let mut depth = 0i64;
    let mut line_start = (true, 0i64);
    let mut at = 0;
    while at < chars.len() {
        let c = chars[at];
        let next = chars.get(at + 1).copied();
        match mode {
            Mode::Block(level) => {
                if c == '/' && next == Some('*') {
                    mode = Mode::Block(level + 1);
                    at += 2;
                    continue;
                }
                if c == '*' && next == Some('/') {
                    mode = if level == 1 { Mode::Code } else { Mode::Block(level - 1) };
                    at += 2;
                    continue;
                }
            }
            Mode::Str => {
                if c == '\\' && next.is_some_and(|next| next != '\n') {
                    at += 2;
                    continue;
                }
                if c == '"' {
                    mode = Mode::Code;
                }
            }
            Mode::Raw(hashes) => {
                if c == '"' && (0..hashes).all(|offset| chars.get(at + 1 + offset) == Some(&'#')) {
                    mode = Mode::Code;
                    at += 1 + hashes;
                    continue;
                }
            }
            Mode::Code => {
                if c == '/' && next == Some('/') {
                    while at < chars.len() && chars[at] != '\n' {
                        at += 1;
                    }
                    continue;
                }
                if c == '/' && next == Some('*') {
                    mode = Mode::Block(1);
                    at += 2;
                    continue;
                }
                if c == '"' {
                    mode = Mode::Str;
                } else if c == '\'' {
                    // A character literal ('x', '\n', '\u{..}') or a lifetime ('a).
                    if next == Some('\\') {
                        at += 2;
                        while at < chars.len() && chars[at] != '\'' {
                            at += 1;
                        }
                    } else if chars.get(at + 2) == Some(&'\'') {
                        at += 2;
                    }
                } else if (c == 'r' || c == 'b')
                    && !at.checked_sub(1).is_some_and(|before| is_identifier_continue(chars[before]))
                {
                    let mut after = at + 1;
                    if c == 'b' && chars.get(after) == Some(&'r') {
                        after += 1;
                    }
                    let raw = c == 'r' || after == at + 2;
                    let mut hashes = 0;
                    while raw && chars.get(after + hashes) == Some(&'#') {
                        hashes += 1;
                    }
                    if chars.get(after + hashes) == Some(&'"') {
                        mode = if raw { Mode::Raw(hashes) } else { Mode::Str };
                        at = after + hashes + 1;
                        continue;
                    }
                } else if c == '{' {
                    depth += 1;
                } else if c == '}' {
                    depth -= 1;
                }
            }
        }
        if c == '\n' {
            states.push(LineState {
                starts_in_code: line_start.0,
                ends_in_code: mode == Mode::Code,
                depth_at_start: line_start.1,
                depth_at_end: depth,
            });
            line_start = (mode == Mode::Code, depth);
        }
        at += 1;
    }
    if !source.is_empty() && !source.ends_with('\n') {
        states.push(LineState {
            starts_in_code: line_start.0,
            ends_in_code: mode == Mode::Code,
            depth_at_start: line_start.1,
            depth_at_end: depth,
        });
    }
    states
}

/// The line-based item split used by projection v3 and earlier: an item
/// starts at a column-0 line that follows a blank line or the end of the
/// previous item (a column-0 line that closes with `}` or `;`).  It cannot
/// see string literals, so a column-0 line inside a multi-line string could
/// split a function.  Kept only to recompute digests recorded under it.
pub(crate) fn line_split_chunks(source: &str) -> Vec<Chunk> {
    let mut texts: Vec<String> = Vec::new();
    let mut previous_ends_item = true;
    for line in source.split_inclusive('\n') {
        let content = line.trim_end_matches(['\n', '\r']);
        if content.trim().is_empty() {
            previous_ends_item = true;
        } else {
            let at_column_zero = !content.starts_with(char::is_whitespace);
            let continuation = content.starts_with(['}', ')', ']', ';']);
            if at_column_zero && !continuation && previous_ends_item {
                texts.push(String::new());
            }
            // A column-0 line closing with `}` or `;` ends an item: the close
            // of a multi-line item, or a complete one-line item such as
            // `fn build_x(r: &Path) -> Result<()> { build_y(r) }`, which must
            // not absorb the one-line item after it.
            previous_ends_item = at_column_zero
                && (content.starts_with('}') || content.ends_with('}') || content.ends_with(';'));
        }
        if texts.is_empty() {
            texts.push(String::new());
        }
        texts.last_mut().unwrap().push_str(line);
    }
    texts
        .into_iter()
        .map(|text| {
            let test_only = text
                .lines()
                .map(str::trim)
                .take_while(|line| {
                    line.is_empty() || line.starts_with("//") || line.starts_with("#[")
                })
                .any(|line| line == "#[cfg(test)]");
            Chunk {
                kind: item_name(header(&text)).map_or(ChunkKind::Other, ChunkKind::Named),
                text,
                test_only,
            }
        })
        .collect()
}

/// The name of a top-level function, constant or static declared by `header`.
pub(crate) fn item_name(header: &str) -> Option<String> {
    let mut rest = header;
    if let Some(after) = rest.strip_prefix("pub") {
        rest = after
            .strip_prefix(' ')
            .or_else(|| after.find(") ").map(|at| &after[at + 2..]))?;
    }
    let mut words = rest.split_whitespace();
    let mut word = words.next()?;
    while matches!(word, "async" | "unsafe" | "extern") {
        word = words.next()?;
    }
    let name = match word {
        "fn" => words.next()?,
        "const" => match words.next()? {
            "fn" => words.next()?,
            name => name,
        },
        "static" => match words.next()? {
            "mut" => words.next()?,
            name => name,
        },
        _ => return None,
    };
    let name = name
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .next()
        .filter(|name| !name.is_empty())?;
    Some(name.to_string())
}

#[cfg(test)]
mod tests {

    #[test]
    fn column_zero_lines_inside_strings_and_comments_do_not_split_an_item() {
        // An embedded C++ snippet: column-0 `};`, a blank line and a column-0
        // line after it, all inside one raw string, plus a block comment.
        let source = "fn build(r: &Path) -> Result<()> {\n    let snippet = r#\"\nclass X\n{\n};\n\n#endif\"#;\n/*\n}\n*/\n    let quote = '\"';\n    let brace = '{';\n    later(r)\n}\n\nfn later(r: &Path) -> Result<()> { Ok(()) }\n";
        let named = chunks(source)
            .into_iter()
            .filter_map(|chunk| match chunk.kind {
                ChunkKind::Named(name) => Some((name, chunk.text)),
                ChunkKind::Other => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(named.iter().map(|(name, _)| name.as_str()).collect::<Vec<_>>(), ["build", "later"]);
        assert!(named[0].1.contains("later(r)\n}\n"), "the call after the snippet stays in build");
        let index = ItemIndex::build(
            BTreeMap::from([(PathBuf::from("a.rs"), chunks(source))]),
            Tokens::Code,
        );
        assert!(index.reachable("build", ["build".to_string()], &[]).contains("later"));
        // The line split used by v3 cut the function at the snippet's `};`.
        assert!(line_split_chunks(source).len() > chunks(source).len());
    }

    #[test]
    fn consecutive_one_line_items_are_separate_and_keep_their_calls() {
        let source = "fn build_a(r: &Path) -> Result<()> { shared(r, \"a\") }\n\
                      fn build_b(r: &Path) -> Result<()> { shared(r, \"b\") }\n\
                      fn build_c(r: &Path) -> Result<()> { other(r) }\n\
                      \n\
                      fn shared(r: &Path, x: &str) -> Result<()> {\n    Ok(())\n}\n";
        let names = chunks(source)
            .into_iter()
            .filter_map(|chunk| match chunk.kind {
                ChunkKind::Named(name) => Some(name),
                ChunkKind::Other => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(names, ["build_a", "build_b", "build_c", "shared"]);
    }

    use super::*;

    const SOURCE: &str = "use x;\n\nconst SHARED: u8 = 1;\n\n/// Builds b.\n#[allow(dead_code)]\nfn build_b(r: &Path) -> Result<()> {\n    helper(r)?;\n\n    Ok(())\n}\nfn helper(\n    r: &Path,\n) -> Result<()> {\n    Ok(())\n}\n\nimpl Thing {\n    fn method(&self) {\n        callback()\n    }\n}\n\nfn callback() {}\n\nfn unused() {}\n\n#[cfg(test)]\nmod tests {\n    fn build_b_works() {}\n}\n";

    #[test]
    fn chunks_are_contiguous_top_level_items_with_their_comments() {
        let chunks = chunks(SOURCE);
        assert_eq!(
            chunks
                .iter()
                .map(|chunk| chunk.text.as_str())
                .collect::<String>(),
            SOURCE
        );
        let named = |name: &str| {
            chunks
                .iter()
                .find(|chunk| chunk.kind == ChunkKind::Named(name.to_string()))
                .unwrap_or_else(|| panic!("no chunk for {name}"))
        };
        assert!(
            named("build_b")
                .text
                .starts_with("/// Builds b.\n#[allow(dead_code)]\nfn build_b(")
        );
        assert!(
            named("build_b").text.contains("\n\n    Ok(())\n}\n"),
            "a blank line inside a body does not split"
        );
        assert!(
            named("helper")
                .text
                .contains(") -> Result<()> {\n    Ok(())\n}\n")
        );
        assert!(
            chunks
                .iter()
                .any(|chunk| chunk.test_only && chunk.text.contains("mod tests"))
        );
        assert!(chunks[0].is_declaration() && !named("SHARED").is_declaration());
    }

    #[test]
    fn code_identifiers_skip_comments_and_strings_but_keep_format_captures() {
        let text = r####"fn a() {
    // calls main() in a comment
    /* nested /* main */ still comment */
    let s = "main and {{not_this}} but {CAPTURED} and {spec:>4}";
    let raw = r#"main "quoted" {RAW_CAPTURE}"#;
    let bytes = b"main";
    let c = 'm';
    let q = '''; let l: &'static str = x;
    real_call(ARG)
}"####;
        let found = identifiers(text, Tokens::Code);
        assert!(!found.iter().any(|word| word == "main" || word == "not_this" || word == "comment"));
        for expected in ["real_call", "ARG", "CAPTURED", "spec", "RAW_CAPTURE", "str", "x"] {
            assert!(found.iter().any(|word| word == expected), "missing {expected}: {found:?}");
        }
        assert!(!found.iter().any(|word| word == "static"), "lifetimes are not item references");
        assert!(identifiers(text, Tokens::Words).iter().any(|word| word == "main"));
    }

    #[test]
    fn non_function_items_name_their_subject() {
        let subject = |header: &str| chunks(&format!("{header} {{}}\n"))[0].subject();
        assert_eq!(subject("pub(crate) struct Provenance"), Some("Provenance".to_string()));
        assert_eq!(subject("enum Kind<T>"), Some("Kind".to_string()));
        assert_eq!(subject("impl<T: Clone> fmt::Display for Wrapper<T>"), Some("Wrapper".to_string()));
        assert_eq!(subject("impl Context"), Some("Context".to_string()));
        assert_eq!(subject("impl<'a> Iterator for Walk<'a> where Self: Sized"), Some("Walk".to_string()));
        assert_eq!(subject("macro_rules! stage"), Some("stage".to_string()));
        assert_eq!(subject("extern \"C\""), None);
    }

    #[test]
    fn item_names_cover_functions_constants_and_statics() {
        assert_eq!(
            item_name("pub(crate) const fn f() {"),
            Some("f".to_string())
        );
        assert_eq!(item_name("pub fn g<T>(x: T) {"), Some("g".to_string()));
        assert_eq!(
            item_name("static mut COUNT: u8 = 0;"),
            Some("COUNT".to_string())
        );
        assert_eq!(
            item_name("pub(super) const LIMIT: u8 = 1;"),
            Some("LIMIT".to_string())
        );
        assert_eq!(item_name("impl Foo {"), None);
        assert_eq!(item_name("struct Foo;"), None);
    }

    #[test]
    fn reachability_follows_calls_and_impl_bodies_but_not_barriers_or_tests() {
        let index = ItemIndex::build(
            BTreeMap::from([(PathBuf::from("a.rs"), chunks(SOURCE))]),
            Tokens::Code,
        );
        let reached = index.reachable("b", ["build_b".to_string()], &[]);
        assert!(reached.contains("build_b") && reached.contains("helper"));
        assert!(
            reached.contains("callback"),
            "impl methods may call any function"
        );
        assert!(!reached.contains("unused") && !reached.contains("build_b_works"));
        let barred = index.reachable("barred", ["build_b".to_string()], &["helper"]);
        assert!(!barred.contains("helper"));
    }
}
