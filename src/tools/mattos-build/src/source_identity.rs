use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// A Git option that reads no `.gitattributes` at all (the empty tree is the
/// attribute source), so no eol or filter attribute can make Git report a
/// file unchanged whose bytes differ from its blob.
pub(crate) const NO_ATTRIBUTES: &str = "--attr-source=4b825dc642cb6eb9a060e54bf8d69288fbee4904";

/// `git ls-files` exclude options for untracked source inputs: MattOS's own
/// ignore rules (the root `.gitignore` and `info/exclude`) only.  Vendored
/// trees keep upstream's nested `.gitignore` files, which ignore some of
/// upstream's own tracked sources; honoring them would leave such a file out
/// of source digests and build mirrors until it is force-added, so merely
/// committing it would change what a build reads.
pub(crate) fn mattos_exclude_arguments(repo_root: &Path) -> Result<Vec<String>> {
    let output = Command::new("git")
        .args(["rev-parse", "--git-path", "info/exclude"])
        .current_dir(repo_root)
        .output()?;
    if !output.status.success() {
        bail!("git could not locate info/exclude in {}", repo_root.display());
    }
    let info_exclude = repo_root.join(String::from_utf8_lossy(&output.stdout).trim());
    Ok([repo_root.join(".gitignore"), info_exclude]
        .into_iter()
        .filter(|exclude| exclude.is_file())
        .map(|exclude| format!("--exclude-from={}", exclude.display()))
        .collect())
}

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

#[derive(Debug)]
pub(crate) struct GitSourceSnapshot {
    index: BTreeMap<String, String>,
    modified: BTreeSet<String>,
    untracked: BTreeSet<String>,
    root_entries: Mutex<BTreeMap<SourceRootKey, Vec<SourceEntry>>>,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct SourceRootKey {
    root: PathBuf,
    exclude_documentation: bool,
}

#[derive(Clone, Debug)]
struct SourceEntry {
    path: String,
    prefix: String,
    value: String,
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
                arguments
                    .iter()
                    .copied()
                    .find(|argument| !argument.starts_with("--"))
                    .unwrap_or("unknown"),
                timer.elapsed(),
            );
            if !output.status.success() {
                bail!("git input inventory failed with {}", output.status)
            }
            Ok(output.stdout)
        };
        let index = git_output(&["ls-files", "--stage", "-z"])?;
        // Modified files are digested from their raw bytes, so they are
        // detected with no attributes at all (the empty tree as attribute
        // source): an eol or filter attribute cannot make Git call a file
        // unchanged whose bytes differ from its blob.
        let modified = git_output(&[
            NO_ATTRIBUTES,
            "diff",
            "--name-only",
            "-z",
        ])?;
        let excludes = mattos_exclude_arguments(repo_root)?;
        let mut untracked_arguments = vec!["ls-files", "--others", "-z"];
        untracked_arguments.extend(excludes.iter().map(String::as_str));
        let untracked = git_output(&untracked_arguments)?;
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
            root_entries: Mutex::new(BTreeMap::new()),
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
            let key = SourceRootKey {
                root: root.clone(),
                exclude_documentation: query.exclude_documentation,
            };
            let cached = self
                .root_entries
                .lock()
                .map_err(|_| anyhow::anyhow!("source root cache lock poisoned"))?
                .get(&key)
                .cloned();
            let entries = if let Some(entries) = cached {
                record_phase("root_cache_hit", Duration::ZERO);
                entries
            } else {
                let entries = self.compute_root_entries(
                    repo_root,
                    root,
                    query.exclude_documentation,
                    working_digest,
                    record_phase,
                )?;
                self.root_entries
                    .lock()
                    .map_err(|_| anyhow::anyhow!("source root cache lock poisoned"))?
                    .insert(key, entries.clone());
                entries
            };
            for entry in entries {
                write_entry(writer, &mut first, &entry.path, &entry.prefix, &entry.value)?;
            }
        }
        writer.write_all(b"}]")?;
        Ok(())
    }

    fn compute_root_entries(
        &self,
        repo_root: &Path,
        root: &Path,
        exclude_documentation: bool,
        working_digest: &mut impl FnMut(&Path) -> Result<Option<String>>,
        record_phase: &mut impl FnMut(&str, Duration),
    ) -> Result<Vec<SourceEntry>> {
        let root_text = root.to_string_lossy();
        let root_text = root_text.trim_end_matches('/');
        let lookup_timer = Instant::now();
        let (first, last) = root_path_bounds(root_text);
        let mut index = self
            .index
            .get_key_value(root_text)
            .into_iter()
            .chain(self.index.range(first.clone()..last.clone()))
            .peekable();
        let mut untracked = self
            .untracked
            .get(root_text)
            .into_iter()
            .chain(self.untracked.range(first..last))
            .peekable();
        record_phase("prefix_lookup", lookup_timer.elapsed());
        let selection_timer = Instant::now();
        let mut entries = Vec::new();
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
                if exclude_documentation && is_irrelevant_documentation(Path::new(path)) {
                    continue;
                }
                let digest = working_digest(&repo_root.join(path))?
                    .with_context(|| format!("untracked source disappeared: {path}"))?;
                // Recorded exactly as `git add` would index it, so tracking
                // or committing an unchanged file does not change its digest.
                entries.push(SourceEntry {
                    path: path.clone(),
                    prefix: "index:".to_string(),
                    value: digest,
                });
            } else {
                let (path, header) = index.next().expect("peeked index entry");
                if exclude_documentation && is_irrelevant_documentation(Path::new(path)) {
                    continue;
                }
                if self.modified.contains(path) {
                    // A deleted file is simply absent, exactly as it is once
                    // the deletion is committed; a placeholder would make
                    // committing the deletion change the digest.
                    if let Some(value) = working_digest(&repo_root.join(path))? {
                        entries.push(SourceEntry {
                            path: path.clone(),
                            prefix: "index:".to_string(),
                            value,
                        });
                    }
                } else {
                    entries.push(SourceEntry {
                        path: path.clone(),
                        prefix: "index:".to_string(),
                        value: header.clone(),
                    });
                }
            }
        }
        record_phase("entry_selection", selection_timer.elapsed());
        Ok(entries)
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

/// The `git ls-files --stage` value (`<mode> <blob id> 0`) the working-tree
/// file at `path` would have once added, so a modified or untracked file is
/// digested exactly like the committed file with the same content and mode.
/// `None` when the path is gone; directories (nested repositories) are not
/// blobs and return `Ok(None)` from here for the caller to digest.
pub(crate) fn git_index_value(path: &Path) -> Result<Option<String>> {
    let Ok(metadata) = path.symlink_metadata() else {
        return Ok(None);
    };
    let (mode, content) = if metadata.file_type().is_symlink() {
        ("120000", std::fs::read_link(path)?.into_os_string().into_encoded_bytes())
    } else if metadata.is_file() {
        use std::os::unix::fs::PermissionsExt;
        let executable = metadata.permissions().mode() & 0o100 != 0;
        (if executable { "100755" } else { "100644" }, std::fs::read(path)?)
    } else {
        return Ok(None);
    };
    let mut blob = sha1::Sha1::new();
    sha1::Digest::update(&mut blob, format!("blob {}\0", content.len()).as_bytes());
    sha1::Digest::update(&mut blob, &content);
    Ok(Some(format!("{mode} {:x} 0", sha1::Digest::finalize(blob))))
}

/// Bounds of the paths strictly inside directory `root`: `root/` up to (but
/// excluding) `root0`, `'0'` being the byte after `'/'`.  Siblings such as
/// `root-extra` or `root.d` sort between `root` and `root/`, so a scan
/// starting at `root` itself must not stop at the first unselected path.
fn root_path_bounds(root: &str) -> (String, String) {
    (format!("{root}/"), format!("{root}0"))
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
        let (first, last) = root_path_bounds(root);
        let range = values.get_key_value(root).into_iter().chain(values.range(first..last));
        profile.prefix_lookup += timer.elapsed();
        let timer = Instant::now();
        for (path, value) in range {
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
        let (first, last) = root_path_bounds(root);
        for path in values.get(root).into_iter().chain(values.range(first..last)) {
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
    fn siblings_sorting_before_the_directory_separator_do_not_hide_a_root() {
        // `openssh-portable` and `openssh.d` sort between `openssh` and
        // `openssh/` ('-' and '.' precede '/'); the root must still be read.
        let snapshot = GitSourceSnapshot::from_git_output(
            b"100644 a 0\topenssh-portable/configure\0100644 b 0\topenssh.d/x\0100644 c 0\topenssh/ssh-pam\0100644 d 0\topenssh/sshd_config\0",
            b"openssh/ssh-pam\0openssh-portable/configure\0",
            b"openssh-portable/new\0openssh/new\0",
        )
        .unwrap();
        let roots = [PathBuf::from("openssh")];
        assert_eq!(
            snapshot.index_entries(&roots).0.keys().copied().collect::<Vec<_>>(),
            ["openssh/ssh-pam", "openssh/sshd_config"]
        );
        assert_eq!(snapshot.untracked_paths(&roots), ["openssh/new"].into_iter().collect());

        let query = SourceQuery::new(&roots, false);
        let digest = |content: &'static str| {
            snapshot
                .canonical_query_bytes(Path::new("/repo"), &query, |path| {
                    Ok(Some(format!("{}={content}", path.display())))
                })
                .unwrap()
        };
        let body = String::from_utf8(digest("one")).unwrap();
        assert!(body.contains("openssh/ssh-pam") && body.contains("openssh/new"), "{body}");
        assert!(!body.contains("openssh-portable"), "{body}");
        // The modified working file is read from the working tree.
        assert!(body.contains("\"openssh/ssh-pam\":\"index:/repo/openssh/ssh-pam=one\""), "{body}");
    }

    #[test]
    fn a_file_digests_the_same_whether_untracked_staged_committed_or_modified_back() {
        let temporary = tempfile::tempdir().unwrap();
        let repo = temporary.path();
        let git = |args: &[&str]| {
            let status = std::process::Command::new("git")
                .args(["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false"])
                .args(args)
                .current_dir(repo)
                .output()
                .unwrap();
            assert!(status.status.success(), "{args:?}: {}", String::from_utf8_lossy(&status.stderr));
        };
        git(&["init", "-q"]);
        std::fs::create_dir_all(repo.join("component")).unwrap();
        std::fs::write(repo.join("component/recipe.rs"), "fn build() {}\n").unwrap();
        let script = repo.join("component/run.sh");
        std::fs::write(&script, "#!/bin/sh\n").unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::os::unix::fs::symlink("recipe.rs", repo.join("component/alias.rs")).unwrap();
        let digest = || {
            let snapshot = GitSourceSnapshot::capture(repo, |_, _| {}).unwrap();
            let query = SourceQuery::new(&[PathBuf::from("component")], false);
            snapshot
                .digest_query(repo, &query, |path| git_index_value(path), |_, _| {})
                .unwrap()
        };
        // Upstream ignore rules inside a vendored tree do not hide its files.
        std::fs::write(repo.join("component/.gitignore"), "generated.c\n").unwrap();
        std::fs::write(repo.join("component/generated.c"), "int x;\n").unwrap();
        let untracked = digest();
        git(&["add", "-A"]);
        git(&["add", "-f", "component/generated.c"]);
        assert_eq!(digest(), untracked, "staging changed the digest");
        git(&["commit", "-q", "-m", "c"]);
        assert_eq!(digest(), untracked, "committing changed the digest");
        std::fs::write(repo.join("component/recipe.rs"), "fn build() { edit(); }\n").unwrap();
        let modified = digest();
        assert_ne!(modified, untracked, "a content edit must change the digest");
        std::fs::write(repo.join("component/recipe.rs"), "fn build() {}\n").unwrap();
        assert_eq!(digest(), untracked, "restoring the content must restore the digest");
        // A deletion changes the digest once, not again when committed.
        std::fs::remove_file(repo.join("component/run.sh")).unwrap();
        let deleted = digest();
        assert_ne!(deleted, untracked, "a deletion must change the digest");
        git(&["add", "-A"]);
        git(&["commit", "-q", "-m", "delete"]);
        assert_eq!(digest(), deleted, "committing a deletion changed the digest");
    }

    #[test]
    fn an_eol_attribute_cannot_hide_a_line_ending_change() {
        let temporary = tempfile::tempdir().unwrap();
        let repo = temporary.path();
        let git = |args: &[&str]| {
            let status = std::process::Command::new("git")
                .args(["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false"])
                .args(args)
                .current_dir(repo)
                .output()
                .unwrap();
            assert!(status.status.success(), "{args:?}: {}", String::from_utf8_lossy(&status.stderr));
        };
        git(&["init", "-q"]);
        std::fs::create_dir_all(repo.join("component")).unwrap();
        std::fs::write(repo.join(".gitattributes"), "*.txt text\n").unwrap();
        std::fs::write(repo.join("component/dos.txt"), "a\nb\n").unwrap();
        git(&["add", "-A"]);
        git(&["commit", "-q", "-m", "c"]);
        let digest = || {
            let snapshot = GitSourceSnapshot::capture(repo, |_, _| {}).unwrap();
            let query = SourceQuery::new(&[PathBuf::from("component")], false);
            snapshot
                .digest_query(repo, &query, |path| git_index_value(path), |_, _| {})
                .unwrap()
        };
        let lf = digest();
        // Git's `text` normalization calls this file unchanged; its bytes
        // (what the build reads) are not.
        std::fs::write(repo.join("component/dos.txt"), "a\r\nb\r\n").unwrap();
        assert_ne!(digest(), lf, "a line-ending change must change the digest");
        std::fs::write(repo.join("component/dos.txt"), "a\nb\n").unwrap();
        assert_eq!(digest(), lf);
    }

    #[test]
    fn repeated_source_root_selection_reuses_the_snapshot_cache() {
        let snapshot = GitSourceSnapshot::from_git_output(
            b"100644 a 0\troot/file\0100644 b 0\tother/file\0",
            b"root/file\0",
            b"",
        )
        .unwrap();
        let query_a = SourceQuery::new(&[PathBuf::from("root")], false);
        let query_b = SourceQuery::new(&[PathBuf::from("root"), PathBuf::from("other")], false);
        let mut working_calls = 0;
        let mut digest = |_: &Path| {
            working_calls += 1;
            Ok(Some("working-digest".to_string()))
        };
        snapshot
            .digest_query(Path::new("/repo"), &query_a, &mut digest, |_, _| {})
            .unwrap();
        snapshot
            .digest_query(Path::new("/repo"), &query_b, &mut digest, |_, _| {})
            .unwrap();
        assert_eq!(working_calls, 1);
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
