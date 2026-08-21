//! Recovery journal (a swap file) for the buffer you have *not* saved yet.
//!
//! `file_io` makes the moment you press save durable. This makes the time
//! *between* saves survivable.
//!
//! cozy ships inside hsh-ios, and iOS kills a backgrounded app with SIGKILL:
//! no handler runs, no destructor runs, nothing is flushed. So the nano model
//! (write an emergency copy when a signal arrives) buys nothing here — the
//! signal never arrives. vim's model does: don't write *when you die*, write
//! *while you edit*. We snapshot the buffer to a swap file shortly after you
//! stop typing, and offer it back on the next launch.
//!
//! The swap lives in a state directory keyed by the file's path, not beside the
//! file: a `.swp` next to the target shows up in git status and cannot be
//! written at all when the directory is read-only. On iOS the host hands us its
//! sandboxed config dir, so this lands under Documents with everything else.
//!
//! The swap is removed when there is nothing left to recover: a successful save,
//! or a deliberate quit. It exists for the exits you did not choose.

use crate::state::EditorState;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// Format marker. Bump when the layout changes; an unreadable swap is ignored,
/// never guessed at.
const MAGIC: &str = "cozy-swap 1";

/// Write the swap after this many edits even while the user keeps typing, so a
/// long uninterrupted burst cannot outrun the journal. vim uses 200 keystrokes.
pub(crate) const EDITS_PER_WRITE: usize = 200;

/// What a swap file gave back.
pub(crate) struct Recovery {
    pub lines: Vec<String>,
    /// How long ago it was written (for the one-line offer).
    pub age: Duration,
}

/// Where swap files live. The host's config dir wins when there is one (iOS
/// sandbox), so we never write outside the sandbox.
pub(crate) fn swap_dir(config_dir: Option<&PathBuf>) -> Option<PathBuf> {
    if let Some(dir) = config_dir {
        return Some(dir.join("swap"));
    }
    dirs::state_dir()
        .map(|p| p.join("cozy/swap"))
        .or_else(|| dirs::home_dir().map(|p| p.join(".cozy/swap")))
}

/// The swap file for `target`, keyed by its absolute path.
///
/// The key is a hash rather than the path itself: paths contain separators and
/// can be longer than a filename may be, and two files with the same basename in
/// different directories must not share a swap.
pub(crate) fn swap_path(dir: &Path, target: &Path) -> PathBuf {
    let absolute = std::path::absolute(target).unwrap_or_else(|_| target.to_path_buf());
    dir.join(format!("{:016x}.swap", path_hash(&absolute)))
}

/// The swap file for whatever the editor currently has open (`None` for an
/// unnamed buffer — there is no key to file it under, and no file to recover to).
pub(crate) fn path_for(editor: &EditorState) -> Option<PathBuf> {
    let dir = editor.swap_dir.as_ref()?;
    let target = editor.filename.as_ref()?;
    Some(swap_path(dir, &editor.resolve_in_working_dir(target)))
}

/// FNV-1a over the path bytes. Not cryptographic — it only has to be stable
/// across runs and across cozy versions, which `DefaultHasher` does not promise.
fn path_hash(path: &Path) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in path.to_string_lossy().as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Snapshot the buffer. Whole-buffer, not a diff log: cozy's buffers are small,
/// and recovery then means reading a file instead of replaying an edit stream.
///
/// Written atomically (see `file_io`) — a swap torn in half is worse than none,
/// because it looks recoverable.
pub(crate) fn write(editor: &mut EditorState) {
    let (Some(path), Some(target)) = (editor.swap_path.clone(), editor.filename.clone()) else {
        editor.swap_dirty = false;
        return;
    };

    let absolute = std::path::absolute(&target).unwrap_or(target);
    let mut content = String::with_capacity(editor.buffer.lines.len() * 40);
    content.push_str(MAGIC);
    content.push('\n');
    content.push_str(&format!("path {}\n", absolute.display()));
    content.push_str("---\n");
    for line in &editor.buffer.lines {
        content.push_str(line);
        content.push('\n');
    }

    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    // A failed swap write must not interrupt editing — the user did not ask for
    // this file to exist. It is a safety net, not the product.
    let _ = crate::file_io::write_bytes_atomic(&path, content.as_bytes());

    editor.swap_dirty = false;
    editor.swap_edits = 0;
}

/// Drop the swap: there is nothing left to recover.
pub(crate) fn remove(editor: &mut EditorState) {
    if let Some(path) = &editor.swap_path {
        let _ = std::fs::remove_file(path);
    }
    editor.swap_dirty = false;
    editor.swap_edits = 0;
}

/// How long an unclaimed swap is kept. A swap is only reclaimed when its file is
/// opened again, so one for a file you never reopen would otherwise live forever.
const KEEP: Duration = Duration::from_secs(14 * 24 * 60 * 60);

/// Sweep swaps nobody came back for. Best-effort and quiet: this is housekeeping,
/// not a feature, and a failure here must not keep the editor from starting.
pub(crate) fn prune(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "swap") {
            continue;
        }
        let expired = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| SystemTime::now().duration_since(t).ok())
            .is_some_and(|age| age > KEEP);
        if expired {
            let _ = std::fs::remove_file(&path);
        }
    }
}

/// Read a swap for `target`, if one is worth offering.
///
/// Nothing is offered when the swap merely repeats what is already on disk —
/// that is the normal aftermath of a crash *right after* a save, and prompting
/// there would train people to dismiss the prompt without reading it.
pub(crate) fn load(path: &Path, target: &Path) -> Option<Recovery> {
    let raw = std::fs::read_to_string(path).ok()?;
    let body = raw.strip_prefix(MAGIC)?;
    let (_header, text) = body.split_once("\n---\n")?;

    let lines = split_lines(text);
    let on_disk = std::fs::read_to_string(target)
        .ok()
        .map(|c| split_lines(&c))
        .unwrap_or_default();
    if lines == on_disk {
        return None;
    }

    let age = std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| SystemTime::now().duration_since(t).ok())
        .unwrap_or_default();

    Some(Recovery { lines, age })
}

/// The buffer keeps a trailing empty line the file does not; normalize both
/// sides the same way so "identical" means identical.
fn split_lines(text: &str) -> Vec<String> {
    let mut lines: Vec<String> = text.lines().map(|s| s.to_string()).collect();
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

/// "2 minutes ago" — coarse on purpose. The user is deciding whether these are
/// *their* edits, not auditing a timestamp.
pub(crate) fn describe_age(age: Duration) -> String {
    let secs = age.as_secs();
    if secs < 90 {
        return "less than a minute ago".to_string();
    }
    let minutes = secs / 60;
    if minutes < 60 {
        return format!("{} minutes ago", minutes);
    }
    let hours = minutes / 60;
    if hours < 24 {
        return format!("{} hours ago", hours);
    }
    format!("{} days ago", hours / 24)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{EditorState, TextBuffer};
    use std::fs;

    fn scratch(name: &str) -> PathBuf {
        let base =
            std::env::temp_dir().join(format!("cozy_swap_test_{}_{}", std::process::id(), name));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        base
    }

    fn editor_at(dir: &Path, file: &str, lines: &[&str]) -> EditorState {
        let target = dir.join(file);
        let mut editor = EditorState::new_with_config_dir(
            Some(target.to_string_lossy().to_string()),
            Some(&dir.join("config")),
        );
        editor.buffer = TextBuffer::from_lines(lines.iter().map(|s| s.to_string()).collect());
        editor.modified = true;
        editor
    }

    /// The whole point: what was typed but never saved comes back.
    #[test]
    fn an_unsaved_buffer_survives_and_is_offered_back() {
        let dir = scratch("survives");
        fs::write(dir.join("notes.md"), "saved\n").unwrap();

        let mut editor = editor_at(&dir, "notes.md", &["saved", "typed but never saved"]);
        write(&mut editor);

        let swap = editor.swap_path.clone().unwrap();
        let recovered = load(&swap, &dir.join("notes.md")).expect("nothing offered");
        assert_eq!(recovered.lines, vec!["saved", "typed but never saved"]);
    }

    /// A kill arrives without warning; losing focus is the warning we do get.
    /// The swap has to be on disk by the time we leave the screen — not one idle
    /// tick later, because that tick may never come.
    #[test]
    fn losing_focus_writes_the_swap_at_once() {
        let dir = scratch("focus_lost");
        fs::write(dir.join("notes.md"), "saved\n").unwrap();

        // Typed, but the idle tick has not fired: this is the second of work
        // that a kill would take.
        let mut editor = editor_at(&dir, "notes.md", &["saved", "typed a moment ago"]);
        editor.swap_dirty = true;

        match crate::input::map_event(&editor, crossterm::event::Event::FocusLost) {
            crate::input::InputEvent::Flush => write(&mut editor),
            _ => panic!("losing focus must ask for a flush"),
        }

        let swap = editor.swap_path.clone().unwrap();
        let recovered = load(&swap, &dir.join("notes.md")).expect("nothing offered");
        assert_eq!(recovered.lines, vec!["saved", "typed a moment ago"]);
        assert!(!editor.swap_dirty, "the flush must clear the debt");
    }

    /// A swap that only repeats the file is not worth a prompt (the crash landed
    /// just after a save). Prompting here teaches people to dismiss prompts.
    #[test]
    fn a_swap_matching_the_file_is_not_offered() {
        let dir = scratch("identical");
        fs::write(dir.join("notes.md"), "same\n").unwrap();

        let mut editor = editor_at(&dir, "notes.md", &["same"]);
        write(&mut editor);

        let swap = editor.swap_path.clone().unwrap();
        assert!(load(&swap, &dir.join("notes.md")).is_none());
    }

    /// Saving means there is nothing to recover.
    #[test]
    fn saving_removes_the_swap() {
        let dir = scratch("saved");
        fs::write(dir.join("notes.md"), "old\n").unwrap();

        let mut editor = editor_at(&dir, "notes.md", &["new"]);
        write(&mut editor);
        let swap = editor.swap_path.clone().unwrap();
        assert!(swap.exists());

        crate::file_io::save(&mut editor).unwrap();

        assert!(!swap.exists(), "swap outlived the save");
        assert_eq!(fs::read_to_string(dir.join("notes.md")).unwrap(), "new\n");
    }

    /// The path a user actually walks: a session died with unsaved edits, cozy is
    /// launched on the same file, and Enter brings the edits back — still unsaved,
    /// so the file on disk is untouched until they choose to save.
    #[test]
    fn a_new_session_offers_the_swap_and_enter_restores_it() {
        let dir = scratch("startup_restore");
        let target = dir.join("notes.md");
        fs::write(&target, "saved\n").unwrap();
        let config = dir.join("config");

        // The session that died.
        let mut doomed = editor_at(&dir, "notes.md", &["saved", "never saved"]);
        write(&mut doomed);

        // The next launch.
        let mut editor = EditorState::new_with_config_dir(
            Some(target.to_string_lossy().to_string()),
            Some(&config),
        );
        assert_eq!(
            editor.buffer.lines,
            vec!["saved"],
            "file, not swap, is shown"
        );
        assert!(editor.recovery.is_some(), "no offer was made");

        crate::reducer::reduce(&mut editor, crate::action::Action::Enter);

        assert_eq!(editor.buffer.lines, vec!["saved", "never saved"]);
        assert!(
            editor.modified,
            "restored edits must still count as unsaved"
        );
        assert_eq!(
            fs::read_to_string(&target).unwrap(),
            "saved\n",
            "restoring must not write the file"
        );
    }

    /// Esc means no. The swap goes away, so the offer is not made again.
    #[test]
    fn esc_discards_the_swap_for_good() {
        let dir = scratch("startup_discard");
        let target = dir.join("notes.md");
        fs::write(&target, "saved\n").unwrap();
        let config = dir.join("config");

        let mut doomed = editor_at(&dir, "notes.md", &["saved", "never saved"]);
        write(&mut doomed);
        let swap = doomed.swap_path.clone().unwrap();

        let mut editor = EditorState::new_with_config_dir(
            Some(target.to_string_lossy().to_string()),
            Some(&config),
        );
        crate::reducer::reduce(&mut editor, crate::action::Action::Cancel);

        assert!(editor.recovery.is_none());
        assert!(!swap.exists(), "a discarded swap must not come back");
        assert_eq!(editor.buffer.lines, vec!["saved"]);
    }

    /// While the question is open, typing must not fall through into the buffer:
    /// those keystrokes would be thrown away the moment the swap is restored.
    #[test]
    fn typing_is_swallowed_until_the_offer_is_answered() {
        let dir = scratch("swallow");
        let target = dir.join("notes.md");
        fs::write(&target, "saved\n").unwrap();
        let config = dir.join("config");

        let mut doomed = editor_at(&dir, "notes.md", &["saved", "never saved"]);
        write(&mut doomed);

        let mut editor = EditorState::new_with_config_dir(
            Some(target.to_string_lossy().to_string()),
            Some(&config),
        );
        crate::reducer::reduce(&mut editor, crate::action::Action::InsertChar('x'));

        assert!(editor.recovery.is_some(), "the question is still open");
        assert_eq!(
            editor.buffer.lines,
            vec!["saved"],
            "the keystroke leaked into the buffer"
        );
    }

    /// Two files with the same basename in different directories get their own swap.
    #[test]
    fn the_key_is_the_whole_path() {
        let dir = scratch("key");
        let a = swap_path(&dir, &dir.join("one/notes.md"));
        let b = swap_path(&dir, &dir.join("two/notes.md"));
        assert_ne!(a, b);
    }

    /// Garbage in the swap file is ignored, not half-read.
    #[test]
    fn an_unreadable_swap_is_ignored() {
        let dir = scratch("garbage");
        let swap = dir.join("x.swap");
        fs::write(&swap, "not a cozy swap file").unwrap();
        assert!(load(&swap, &dir.join("notes.md")).is_none());
    }
}
