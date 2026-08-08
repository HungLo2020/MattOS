use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct SourceQuery {
    pub(crate) roots: Vec<PathBuf>,
    pub(crate) exclude_documentation: bool,
}

impl SourceQuery {
    pub(crate) fn new(roots: &[PathBuf], exclude_documentation: bool) -> Self {
        let mut roots = roots
            .iter()
            .map(|root| PathBuf::from(root.to_string_lossy().trim_end_matches('/')))
            .collect::<Vec<_>>();
        roots.sort();
        roots.dedup();
        let mut canonical = Vec::<PathBuf>::new();
        for root in roots {
            if !canonical.iter().any(|parent| root.starts_with(parent)) {
                canonical.push(root);
            }
        }
        Self {
            roots: canonical,
            exclude_documentation,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct GitSourceSnapshot {
    index: BTreeMap<String, String>,
    modified: BTreeSet<String>,
    untracked: BTreeSet<String>,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct SelectionProfile {
    pub(crate) prefix_lookup: Duration,
    pub(crate) entry_selection_sorting: Duration,
}

impl GitSourceSnapshot {
    pub(crate) fn capture(
        repo_root: &Path,
        mut record_command: impl FnMut(&str, Duration),
    ) -> Result<Self> {
        let mut git_output = |arguments: &[&str]| -> Result<Vec<u8>> {
            let timer = Instant::now();
            let output = Command::new("git")
                .args(arguments)
                .current_dir(repo_root)
                .output()?;
            record_command(
                arguments.first().copied().unwrap_or("unknown"),
                timer.elapsed(),
            );
            if !output.status.success() {
                bail!("git input inventory failed with {}", output.status)
            }
            Ok(output.stdout)
        };
        let index = git_output(&["ls-files", "--stage", "-z"])?;
        let modified = git_output(&["diff", "--name-only", "-z"])?;
        let untracked = git_output(&["ls-files", "--others", "--exclude-standard", "-z"])?;
        let timer = Instant::now();
        let snapshot = Self::from_git_output(&index, &modified, &untracked)?;
        record_command("snapshot-map-construction", timer.elapsed());
        Ok(snapshot)
    }

    fn from_git_output(index: &[u8], modified: &[u8], untracked: &[u8]) -> Result<Self> {
        let mut index_entries = BTreeMap::new();
        for entry in index
            .split(|byte| *byte == 0)
            .filter(|entry| !entry.is_empty())
        {
            let tab = entry
                .iter()
                .position(|byte| *byte == b'\t')
                .ok_or_else(|| anyhow::anyhow!("Git index entry lacks a path separator"))?;
            let header = String::from_utf8_lossy(&entry[..tab]).into_owned();
            if header.split_whitespace().nth(2) != Some("0") {
                bail!("Git index contains an unresolved non-stage-0 entry")
            }
            let path = String::from_utf8_lossy(&entry[tab + 1..]).into_owned();
            if index_entries.insert(path, header).is_some() {
                bail!("Git index contains duplicate path entries")
            }
        }
        Ok(Self {
            index: index_entries,
            modified: nul_paths(modified),
            untracked: nul_paths(untracked),
        })
    }

    #[cfg(test)]
    pub(crate) fn index_entries(
        &self,
        roots: &[PathBuf],
    ) -> (BTreeMap<&str, &str>, SelectionProfile) {
        selected_values(&self.index, roots)
    }

    #[cfg(test)]
    pub(crate) fn index_entries_full_scan(&self, roots: &[PathBuf]) -> BTreeMap<&str, &str> {
        self.index
            .iter()
            .filter(|(path, _)| roots.iter().any(|root| path_is_selected(path, root)))
            .map(|(path, header)| (path.as_str(), header.as_str()))
            .collect()
    }

    #[cfg(test)]
    pub(crate) fn is_modified(&self, path: &str) -> bool {
        self.modified.contains(path)
    }

    #[cfg(test)]
    pub(crate) fn untracked_paths(&self, roots: &[PathBuf]) -> BTreeSet<&str> {
        selected_paths(&self.untracked, roots)
    }

    pub(crate) fn digest_query(
        &self,
        repo_root: &Path,
        query: &SourceQuery,
        mut working_digest: impl FnMut(&Path) -> Result<Option<String>>,
        mut record_phase: impl FnMut(&str, Duration),
    ) -> Result<String> {
        let mut hasher = HashWriter(Sha256::new());
        self.write_query(
            repo_root,
            query,
            &mut working_digest,
            &mut hasher,
            &mut record_phase,
        )?;
        Ok(format!("{:x}", hasher.0.finalize()))
    }

    fn write_query(
        &self,
        repo_root: &Path,
        query: &SourceQuery,
        working_digest: &mut impl FnMut(&Path) -> Result<Option<String>>,
        writer: &mut impl Write,
        record_phase: &mut impl FnMut(&str, Duration),
    ) -> Result<()> {
        writer.write_all(b"[\"git-index-and-working-tree\",{")?;
        let mut first = true;
        for root in &query.roots {
            let root_text = root.to_string_lossy();
            let root_text = root_text.trim_end_matches('/');
            let lookup_timer = Instant::now();
            let mut index = self.index.range(root_text.to_string()..).peekable();
            let mut untracked = self.untracked.range(root_text.to_string()..).peekable();
            record_phase("prefix_lookup", lookup_timer.elapsed());
            let selection_timer = Instant::now();
            loop {
                let next_index = index
                    .peek()
                    .and_then(|(path, _)| path_is_selected(path, root).then_some(path.as_str()));
                let next_untracked = untracked
                    .peek()
                    .and_then(|path| path_is_selected(path, root).then_some(path.as_str()));
                let take_untracked = match (next_index, next_untracked) {
                    (None, None) => break,
                    (None, Some(_)) => true,
                    (Some(_), None) => false,
                    (Some(index_path), Some(untracked_path)) => untracked_path < index_path,
                };
                if take_untracked {
                    let path = untracked.next().expect("peeked untracked path");
                    if query.exclude_documentation && is_irrelevant_documentation(Path::new(path)) {
                        continue;
                    }
                    let digest = working_digest(&repo_root.join(path))?
                        .with_context(|| format!("untracked source disappeared: {path}"))?;
                    write_entry(writer, &mut first, path, "untracked:", &digest)?;
                } else {
                    let (path, header) = index.next().expect("peeked index entry");
                    if query.exclude_documentation && is_irrelevant_documentation(Path::new(path)) {
                        continue;
                    }
                    if self.modified.contains(path) {
                        match working_digest(&repo_root.join(path))? {
                            Some(digest) => {
                                write_entry(writer, &mut first, path, "working:", &digest)?
                            }
                            None => write_entry(writer, &mut first, path, "working:", "<deleted>")?,
                        }
                    } else {
                        write_entry(writer, &mut first, path, "index:", header)?;
                    }
                }
            }
            record_phase("entry_selection", selection_timer.elapsed());
        }
        writer.write_all(b"}]")?;
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn canonical_query_bytes(
        &self,
        repo_root: &Path,
        query: &SourceQuery,
        mut working_digest: impl FnMut(&Path) -> Result<Option<String>>,
    ) -> Result<Vec<u8>> {
        let mut body = Vec::new();
        self.write_query(
            repo_root,
            query,
            &mut working_digest,
            &mut body,
            &mut |_, _| {},
        )?;
        Ok(body)
    }
}

struct HashWriter(Sha256);

impl Write for HashWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.0.update(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn write_entry(
    writer: &mut impl Write,
    first: &mut bool,
    path: &str,
    prefix: &str,
    value: &str,
) -> Result<()> {
    if !*first {
        writer.write_all(b",")?;
    }
    *first = false;
    serde_json::to_writer(&mut *writer, path)?;
    writer.write_all(b":\"")?;
    writer.write_all(prefix.as_bytes())?;
    writer.write_all(value.as_bytes())?;
    writer.write_all(b"\"")?;
    Ok(())
}

fn nul_paths(output: &[u8]) -> BTreeSet<String> {
    output
        .split(|byte| *byte == 0)
        .filter(|bytes| !bytes.is_empty())
        .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
        .collect()
}

fn path_is_selected(path: &str, root: &Path) -> bool {
    let root = root.to_string_lossy();
    let root = root.trim_end_matches('/');
    path == root
        || path
            .strip_prefix(root)
            .is_some_and(|rest| rest.starts_with('/'))
}

fn is_irrelevant_documentation(path: &Path) -> bool {
    for component in path.components() {
        let value = component.as_os_str().to_string_lossy().to_ascii_lowercase();
        if matches!(value.as_str(), "doc" | "docs" | "documentation") {
            return true;
        }
    }
    let name = path
        .file_name()
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or("")
        .to_ascii_lowercase();
    name.starts_with("readme")
        || name.starts_with("changelog")
        || name == "news"
        || name.starts_with("copying")
}

#[cfg(test)]
fn selected_values<'a>(
    values: &'a BTreeMap<String, String>,
    roots: &[PathBuf],
) -> (BTreeMap<&'a str, &'a str>, SelectionProfile) {
    let mut selected = BTreeMap::new();
    let mut profile = SelectionProfile::default();
    for root in roots {
        let root = root.to_string_lossy();
        let root = root.trim_end_matches('/');
        let timer = Instant::now();
        let range = values.range(root.to_string()..);
        profile.prefix_lookup += timer.elapsed();
        let timer = Instant::now();
        for (path, value) in range {
            if path != root
                && !path
                    .strip_prefix(root)
                    .is_some_and(|rest| rest.starts_with('/'))
            {
                break;
            }
            selected.insert(path.as_str(), value.as_str());
        }
        profile.entry_selection_sorting += timer.elapsed();
    }
    (selected, profile)
}

#[cfg(test)]
fn selected_paths<'a>(values: &'a BTreeSet<String>, roots: &[PathBuf]) -> BTreeSet<&'a str> {
    let mut selected = BTreeSet::new();
    for root in roots {
        let root = root.to_string_lossy();
        let root = root.trim_end_matches('/');
        for path in values.range(root.to_string()..) {
            if path != root
                && !path
                    .strip_prefix(root)
                    .is_some_and(|rest| rest.starts_with('/'))
            {
                break;
            }
            selected.insert(path.as_str());
        }
    }
    selected
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefix_selection_respects_path_boundaries_and_deduplicates_roots() {
        let snapshot = GitSourceSnapshot::from_git_output(
            b"100644 a 0\troot\0100644 b 0\troot/file\0100644 c 0\troot/sub/file\0100644 d 0\trooted/file\0",
            b"root/file\0rooted/file\0",
            b"root/new\0rooted/new\0",
        )
        .unwrap();
        let roots = [PathBuf::from("root"), PathBuf::from("root/sub")];
        assert_eq!(
            snapshot
                .index_entries(&roots)
                .0
                .keys()
                .copied()
                .collect::<Vec<_>>(),
            ["root", "root/file", "root/sub/file"]
        );
        assert!(snapshot.is_modified("root/file"));
        assert_eq!(
            snapshot.untracked_paths(&roots),
            ["root/new"].into_iter().collect()
        );
    }

    #[test]
    fn unresolved_or_duplicate_index_entries_fail_closed() {
        assert!(GitSourceSnapshot::from_git_output(b"100644 a 2\troot/file\0", b"", b"").is_err());
        assert!(
            GitSourceSnapshot::from_git_output(
                b"100644 a 0\troot/file\0100644 b 0\troot/file\0",
                b"",
                b""
            )
            .is_err()
        );
    }

    #[test]
    fn source_query_canonicalizes_order_duplicates_and_nested_roots() {
        let first = SourceQuery::new(
            &[
                PathBuf::from("root/sub"),
                PathBuf::from("root"),
                PathBuf::from("root"),
            ],
            true,
        );
        let second = SourceQuery::new(&[PathBuf::from("root")], true);
        assert_eq!(first, second);
        assert_ne!(first, SourceQuery::new(&[PathBuf::from("root")], false));
    }
}
