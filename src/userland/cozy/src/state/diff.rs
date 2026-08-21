//! Session diff review model.
//!
//! A read-only review surface for *your own changes before you commit them*:
//! `git diff` is parsed into hunks, the user reads the `+`/`-` lines hunk by
//! hunk, approves the ones to keep, and stages them — comfortable `git add -p`.
//! **Standalone, that is the whole feature** (self-review + partial staging, no
//! AI required).
//!
//! The exact same mechanism is the human approval gate of argo-tty's agentic
//! loop: because the diff is **author-blind**, "review changes before commit"
//! works whether you wrote them or an AI did — standalone self-review and the AI
//! "read == approve" gate are one tool with two narratives. Data source is plain
//! `git diff` (uncommitted working-tree changes); approving then staging records
//! the selected hunks into git's index, so a partial selection stages exactly
//! those hunks. The structured approval + index handoff is the seam the AI loop
//! later consumes (a Flux signal / reading the index) without coupling here now.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffLineKind {
    Context,
    Add,
    Del,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffLine {
    pub kind: DiffLineKind,
    pub text: String,
}

/// One `@@ ... @@` hunk, tagged with the file it belongs to. Approval is the
/// unit the user reads at: the `+`/`-` lines inside are the meat, the file
/// header (`+++`/`---`) is just the divider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffHunk {
    pub file: String,
    pub header: String,
    pub lines: Vec<DiffLine>,
    pub approved: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiffReviewState {
    pub hunks: Vec<DiffHunk>,
    pub current: usize,
    pub scroll: usize,
}

impl DiffReviewState {
    pub fn approved_count(&self) -> usize {
        self.hunks.iter().filter(|h| h.approved).count()
    }
}

/// Run `git diff` in `dir` and parse the unified output into hunks.
pub fn load_git_diff(dir: &Path) -> Result<Vec<DiffHunk>, String> {
    let output = Command::new("git")
        .arg("diff")
        .current_dir(dir)
        .output()
        .map_err(|e| format!("could not run git: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(stderr.trim().to_string());
    }
    let text = String::from_utf8_lossy(&output.stdout);
    Ok(parse_unified_diff(&text))
}

/// Parse a unified diff into hunks. File headers (`+++ b/path` / `--- a/path`)
/// set the current file; `@@` opens a hunk; `+`/`-`/space lines are content.
pub fn parse_unified_diff(diff: &str) -> Vec<DiffHunk> {
    let mut hunks: Vec<DiffHunk> = Vec::new();
    let mut current_file = String::new();
    let mut cur: Option<DiffHunk> = None;

    for line in diff.lines() {
        // File header (triple +++ / ---). Check before the single +/- content
        // test so an added line `+foo` is never mistaken for a header.
        if let Some(rest) = line.strip_prefix("+++ ") {
            current_file = rest
                .strip_prefix("b/")
                .unwrap_or(rest)
                .trim_end()
                .to_string();
            continue;
        }
        if line.starts_with("--- ") {
            continue;
        }
        // git metadata lines that are not part of any hunk body.
        if line.starts_with("diff --git")
            || line.starts_with("index ")
            || line.starts_with("new file")
            || line.starts_with("deleted file")
            || line.starts_with("rename ")
            || line.starts_with("copy ")
            || line.starts_with("similarity ")
            || line.starts_with("dissimilarity ")
            || line.starts_with("old mode")
            || line.starts_with("new mode")
            || line.starts_with("Binary files")
            || line.starts_with("\\ No newline")
        {
            continue;
        }
        if line.starts_with("@@") {
            if let Some(h) = cur.take() {
                hunks.push(h);
            }
            cur = Some(DiffHunk {
                file: current_file.clone(),
                header: line.to_string(),
                lines: Vec::new(),
                approved: false,
            });
            continue;
        }
        if let Some(h) = cur.as_mut() {
            let (kind, text) = if let Some(t) = line.strip_prefix('+') {
                (DiffLineKind::Add, t.to_string())
            } else if let Some(t) = line.strip_prefix('-') {
                (DiffLineKind::Del, t.to_string())
            } else {
                let t = line.strip_prefix(' ').unwrap_or(line);
                (DiffLineKind::Context, t.to_string())
            };
            h.lines.push(DiffLine { kind, text });
        }
    }
    if let Some(h) = cur.take() {
        hunks.push(h);
    }
    hunks
}

/// Rebuild unified-diff text from parsed hunks — the inverse of
/// [`parse_unified_diff`] over a chosen subset. One `diff --git` + `---`/`+++`
/// header per file, then each hunk's `@@` header and its lines with the leading
/// sign restored. Hunks of the same file must be consecutive (they always are,
/// since review keeps git's diff order).
fn reconstruct_patch(hunks: &[&DiffHunk]) -> String {
    let mut out = String::new();
    let mut last_file: Option<&str> = None;
    for h in hunks {
        if last_file != Some(h.file.as_str()) {
            out.push_str(&format!("diff --git a/{f} b/{f}\n", f = h.file));
            out.push_str(&format!("--- a/{}\n", h.file));
            out.push_str(&format!("+++ b/{}\n", h.file));
            last_file = Some(h.file.as_str());
        }
        out.push_str(&h.header);
        out.push('\n');
        for l in &h.lines {
            let sign = match l.kind {
                DiffLineKind::Context => ' ',
                DiffLineKind::Add => '+',
                DiffLineKind::Del => '-',
            };
            out.push(sign);
            out.push_str(&l.text);
            out.push('\n');
        }
    }
    out
}

/// Stage the approved hunks into git's index via `git apply --cached`. Approved
/// hunks are emitted as one patch (grouped per file, in original coordinates),
/// so a partial selection stages exactly those hunks — `git add -p` territory.
/// `--recount` lets git recompute the `@@` line counts from the body, which
/// keeps us robust even if a reconstructed header is slightly off. Returns the
/// number of hunks staged; `Ok(0)` (no-op) when nothing is approved.
pub fn stage_approved(dir: &Path, hunks: &[DiffHunk]) -> Result<usize, String> {
    let approved: Vec<&DiffHunk> = hunks.iter().filter(|h| h.approved).collect();
    if approved.is_empty() {
        return Ok(0);
    }
    let patch = reconstruct_patch(&approved);
    let mut child = Command::new("git")
        .args(["apply", "--cached", "--recount", "-"])
        .current_dir(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("could not run git: {e}"))?;
    // Drop stdin after writing so git sees EOF and proceeds.
    child
        .stdin
        .take()
        .ok_or_else(|| "git stdin unavailable".to_string())?
        .write_all(patch.as_bytes())
        .map_err(|e| format!("failed to send patch to git: {e}"))?;
    let output = child
        .wait_with_output()
        .map_err(|e| format!("git apply failed: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(stderr.trim().to_string());
    }
    Ok(approved.len())
}

/// `git commit -m <msg>` in `dir`. Commits whatever is in the index (the caller
/// stages the approved hunks first). Returns the new short hash on success, or
/// git's stderr (e.g. "nothing to commit") on failure.
pub fn commit_index(dir: &Path, message: &str) -> Result<String, String> {
    let status = Command::new("git")
        .args(["commit", "-m", message])
        .current_dir(dir)
        .output()
        .map_err(|e| format!("could not run git: {e}"))?;
    if !status.status.success() {
        let stderr = String::from_utf8_lossy(&status.stderr);
        let stdout = String::from_utf8_lossy(&status.stdout);
        // git puts "nothing to commit" on stdout; surface whichever is non-empty.
        let msg = if stderr.trim().is_empty() {
            stdout.trim()
        } else {
            stderr.trim()
        };
        return Err(msg.to_string());
    }
    let head = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .current_dir(dir)
        .output()
        .map_err(|e| format!("could not run git: {e}"))?;
    Ok(String::from_utf8_lossy(&head.stdout).trim().to_string())
}

/// `git push` in `dir` — pushes the current branch to its upstream. Explicit and
/// separate from commit (an outward-facing action). Returns git's summary line
/// on success, or stderr (no upstream / rejected / auth / offline) on failure.
pub fn push_current(dir: &Path) -> Result<String, String> {
    let output = Command::new("git")
        .arg("push")
        .current_dir(dir)
        .output()
        .map_err(|e| format!("could not run git: {e}"))?;
    // git push writes its progress/summary to stderr even on success.
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() {
        return Err(stderr.trim().to_string());
    }
    let summary = stderr
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("pushed")
        .trim()
        .to_string();
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "diff --git a/src/main.rs b/src/main.rs\n\
index 1234567..89abcde 100644\n\
--- a/src/main.rs\n\
+++ b/src/main.rs\n\
@@ -1,3 +1,3 @@\n\
 fn main() {\n\
-    old();\n\
+    new();\n\
 }\n\
diff --git a/README.md b/README.md\n\
--- a/README.md\n\
+++ b/README.md\n\
@@ -10,2 +10,3 @@\n\
 line\n\
+added line\n\
 tail\n";

    #[test]
    fn parses_two_hunks_across_two_files() {
        let hunks = parse_unified_diff(SAMPLE);
        assert_eq!(hunks.len(), 2);
        assert_eq!(hunks[0].file, "src/main.rs");
        assert_eq!(hunks[1].file, "README.md");
    }

    #[test]
    fn classifies_add_del_context_lines() {
        let hunks = parse_unified_diff(SAMPLE);
        let kinds: Vec<_> = hunks[0].lines.iter().map(|l| l.kind).collect();
        assert_eq!(
            kinds,
            vec![
                DiffLineKind::Context,
                DiffLineKind::Del,
                DiffLineKind::Add,
                DiffLineKind::Context,
            ]
        );
        // The leading sign is stripped from the stored text.
        assert_eq!(hunks[0].lines[2].text, "    new();");
    }

    #[test]
    fn file_header_is_not_mistaken_for_added_line() {
        let hunks = parse_unified_diff(SAMPLE);
        // No hunk line should carry the "++ b/..." header text.
        assert!(
            !hunks
                .iter()
                .flat_map(|h| &h.lines)
                .any(|l| l.text.starts_with("++ ") || l.text.contains("b/src/main.rs"))
        );
    }

    #[test]
    fn empty_diff_yields_no_hunks() {
        assert!(parse_unified_diff("").is_empty());
    }

    #[test]
    fn reconstruct_round_trips_through_parser() {
        // hunks -> patch text -> hunks must preserve file + line content, so the
        // patch we feed `git apply` faithfully encodes what was approved.
        let hunks = parse_unified_diff(SAMPLE);
        let refs: Vec<&DiffHunk> = hunks.iter().collect();
        let reparsed = parse_unified_diff(&reconstruct_patch(&refs));
        assert_eq!(reparsed.len(), hunks.len());
        for (got, want) in reparsed.iter().zip(hunks.iter()) {
            assert_eq!(got.file, want.file);
            assert_eq!(got.header, want.header);
            assert_eq!(got.lines, want.lines);
        }
    }

    #[test]
    fn reconstruct_keeps_only_the_chosen_subset() {
        // Approving just the second hunk must emit a patch with only that file.
        let hunks = parse_unified_diff(SAMPLE);
        let only_second: Vec<&DiffHunk> = hunks.iter().skip(1).collect();
        let patch = reconstruct_patch(&only_second);
        assert!(patch.contains("README.md"));
        assert!(!patch.contains("src/main.rs"));
        let reparsed = parse_unified_diff(&patch);
        assert_eq!(reparsed.len(), 1);
        assert_eq!(reparsed[0].file, "README.md");
    }
}
