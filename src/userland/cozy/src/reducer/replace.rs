use crate::reducer::EventResult;
use crate::reducer::helper::mark_modified;
use crate::state::{EditorMode, EditorState, ReplaceFocus, SearchMode};
use regex::Regex;

/// Expand the replacement text for a single match. In Regex mode the `$1` /
/// `${name}` capture references are substituted from `matched`; other modes
/// treat the replacement literally (so `$` stays a plain character there).
fn expand_replacement(matched: &str, query: &str, replacement: &str, mode: &SearchMode) -> String {
    if let SearchMode::Regex = mode {
        if let Ok(re) = Regex::new(query) {
            return re.replace(matched, replacement).into_owned();
        }
    }
    replacement.to_string()
}

pub fn update_replace_buffer(editor: &mut EditorState, c: char) {
    let pos = editor.search_cursor;
    if editor.replace_focus == ReplaceFocus::Query {
        let buf = &mut editor.search_buffer;
        if pos <= buf.len() && buf.is_char_boundary(pos) {
            buf.insert(pos, c);
            editor.search_cursor = pos + c.len_utf8();
        }
        crate::reducer::search::recompute_matches(editor);
        crate::reducer::search::focus_nearest_match(editor);
    } else {
        let buf = &mut editor.replace_buffer;
        if pos <= buf.len() && buf.is_char_boundary(pos) {
            buf.insert(pos, c);
            editor.search_cursor = pos + c.len_utf8();
        }
    }
}

pub fn delete_from_replace_buffer(editor: &mut EditorState) {
    let pos = editor.search_cursor;
    if pos == 0 {
        return;
    }
    match editor.replace_focus {
        ReplaceFocus::Query => {
            let buf = &mut editor.search_buffer;
            let prev = buf[..pos]
                .char_indices()
                .last()
                .map(|(i, _)| i)
                .unwrap_or(0);
            buf.remove(prev);
            editor.search_cursor = prev;
            crate::reducer::search::recompute_matches(editor);
            crate::reducer::search::focus_nearest_match(editor);
        }
        ReplaceFocus::Replace => {
            let buf = &mut editor.replace_buffer;
            let prev = buf[..pos]
                .char_indices()
                .last()
                .map(|(i, _)| i)
                .unwrap_or(0);
            buf.remove(prev);
            editor.search_cursor = prev;
        }
    }
}

pub fn delete_replace_char_at_cursor(editor: &mut EditorState) {
    let pos = editor.search_cursor;
    match editor.replace_focus {
        ReplaceFocus::Query => {
            let buf = &mut editor.search_buffer;
            if pos < buf.len() && buf.is_char_boundary(pos) {
                buf.remove(pos);
            }
            crate::reducer::search::recompute_matches(editor);
            crate::reducer::search::focus_nearest_match(editor);
        }
        ReplaceFocus::Replace => {
            let buf = &mut editor.replace_buffer;
            if pos < buf.len() && buf.is_char_boundary(pos) {
                buf.remove(pos);
            }
        }
    }
}

pub fn move_replace_cursor_left(editor: &mut EditorState) {
    let pos = editor.search_cursor;
    if pos == 0 {
        return;
    }
    let buf = match editor.replace_focus {
        ReplaceFocus::Query => &editor.search_buffer,
        ReplaceFocus::Replace => &editor.replace_buffer,
    };
    editor.search_cursor = buf[..pos]
        .char_indices()
        .last()
        .map(|(i, _)| i)
        .unwrap_or(0);
}

pub fn move_replace_cursor_right(editor: &mut EditorState) {
    let pos = editor.search_cursor;
    let buf = match editor.replace_focus {
        ReplaceFocus::Query => &editor.search_buffer,
        ReplaceFocus::Replace => &editor.replace_buffer,
    };
    if pos >= buf.len() {
        return;
    }
    editor.search_cursor = buf[pos..]
        .chars()
        .next()
        .map(|c| pos + c.len_utf8())
        .unwrap_or(pos);
}

pub fn move_replace_cursor_home(editor: &mut EditorState) {
    editor.search_cursor = 0;
}

pub fn move_replace_cursor_end(editor: &mut EditorState) {
    editor.search_cursor = match editor.replace_focus {
        ReplaceFocus::Query => editor.search_buffer.len(),
        ReplaceFocus::Replace => editor.replace_buffer.len(),
    };
}

pub fn apply_switch_focus(editor: &mut EditorState) -> EventResult {
    if editor.mode == EditorMode::Replace {
        editor.replace_focus = match editor.replace_focus {
            ReplaceFocus::Query => ReplaceFocus::Replace,
            ReplaceFocus::Replace => ReplaceFocus::Query,
        };
        // Reset cursor to end of newly focused buffer
        editor.search_cursor = match editor.replace_focus {
            ReplaceFocus::Query => editor.search_buffer.len(),
            ReplaceFocus::Replace => editor.replace_buffer.len(),
        };
    }
    EventResult::Continue
}

/// Replace the current match and move to the next one
pub fn apply_replace_current(editor: &mut EditorState) -> EventResult {
    let query = editor.search_buffer.clone();
    let replacement = editor.replace_buffer.clone();

    if query.is_empty() {
        crate::reducer::status::set_error(editor, "Find query is empty");
        return EventResult::Continue;
    }

    // Ensure matches are computed
    if editor.search_matches.is_empty() {
        crate::reducer::search::recompute_matches(editor);
    }
    if editor.search_matches.is_empty() {
        crate::reducer::status::set_error(editor, "No matches");
        return EventResult::Continue;
    }

    // Get the current match from the list
    let (y, s, e) = editor.search_matches[editor.search_current];
    let matched = editor.buffer.lines[y][s..e].to_string();
    let expanded = expand_replacement(&matched, &query, &replacement, &editor.search_mode);

    mark_modified(editor);
    editor.buffer.lines[y].replace_range(s..e, &expanded);

    // Land the cursor at the end of the just-replaced text. If matches remain,
    // focus_nearest_match advances to the next one anchored at this position;
    // if none remain, the cursor stays here instead of a stale spot.
    editor.cursor.y = y;
    editor.cursor.x = s + expanded.len();

    // Recompute after buffer change and move to next match
    crate::reducer::search::recompute_matches(editor);
    crate::reducer::search::focus_nearest_match(editor);

    let remaining = editor.search_matches.len();
    let status_msg = if remaining > 0 {
        format!("Replaced 1 ({} remaining)", remaining)
    } else {
        "Done".to_string()
    };
    crate::reducer::status::set_success(editor, "Replace", &status_msg);
    EventResult::Continue
}

// Helper function for ReplaceAll action
/// Replace all matches
pub fn apply_replace_all(editor: &mut EditorState) -> EventResult {
    let query = editor.search_buffer.clone();
    let replacement = editor.replace_buffer.clone();

    if query.is_empty() {
        return EventResult::Continue;
    }

    // Ensure matches are computed
    crate::reducer::search::recompute_matches(editor);
    if editor.search_matches.is_empty() {
        crate::reducer::status::set_info(editor, "No matches");
        return EventResult::Continue;
    }

    let total = editor.search_matches.len();
    mark_modified(editor);

    // Replace in reverse order so byte offsets don't shift for earlier matches
    let mut matches_rev = editor.search_matches.clone();
    matches_rev.reverse();

    for (y, s, e) in matches_rev {
        if y < editor.buffer.lines.len()
            && s <= editor.buffer.lines[y].len()
            && e <= editor.buffer.lines[y].len()
        {
            let matched = editor.buffer.lines[y][s..e].to_string();
            let expanded = expand_replacement(&matched, &query, &replacement, &editor.search_mode);
            editor.buffer.lines[y].replace_range(s..e, &expanded);
        }
    }

    // Clear matches after replacement
    editor.search_matches.clear();
    editor.search_current = 0;

    crate::reducer::status::set_success(editor, "Replace all", &format!("{} matches", total));
    EventResult::Continue
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{Cursor, EditorState, StatusKind, TextBuffer};

    fn create_editor_state(content: &str) -> EditorState {
        let lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
        let mut editor = EditorState::new(None);
        editor.buffer = TextBuffer::from_lines(lines);
        editor.cursor = Cursor::default();
        editor
    }

    #[test]
    fn test_apply_replace_current_match() {
        let mut editor = create_editor_state("hello world");
        editor.search_buffer = "hello".to_string();
        editor.replace_buffer = "hi".to_string();
        // Cursor at (0, 0) matches "hello"

        apply_replace_current(&mut editor);

        assert_eq!(editor.buffer.lines[0], "hi world");
        // Cursor should move after replacement: len("hi") = 2
        assert_eq!(editor.cursor.x, 2);
        // Status becomes Success after replacement
        assert_eq!(editor.status_kind, StatusKind::Success);
    }

    #[test]
    fn test_apply_replace_current_no_match() {
        let mut editor = create_editor_state("hello world");
        editor.search_buffer = "hello".to_string();
        editor.replace_buffer = "hi".to_string();
        editor.cursor.x = 1; // "e" - no match starting here

        apply_replace_current(&mut editor);

        // The current match (search_current) is replaced regardless of cursor column
        assert_eq!(editor.buffer.lines[0], "hi world");
        // Cursor lands at the end of the replacement: len("hi") = 2
        assert_eq!(editor.cursor.x, 2);
        // A successful replace always reports Success (no wrap-around distinction)
        assert_eq!(editor.status_kind, StatusKind::Success);
    }

    #[test]
    fn test_apply_replace_all_match() {
        let mut editor = create_editor_state("foo bar foo");
        editor.search_buffer = "foo".to_string();
        editor.replace_buffer = "baz".to_string();

        apply_replace_all(&mut editor);

        assert_eq!(editor.buffer.lines[0], "baz bar baz");
        assert_eq!(editor.status_kind, StatusKind::Success);
    }

    #[test]
    fn test_apply_replace_all_regex_captures() {
        // Regex mode expands $1/$2 capture references in the replacement.
        let mut editor = create_editor_state("a1 b2 c3");
        editor.search_buffer = r"([a-z])([0-9])".to_string();
        editor.replace_buffer = "$2$1".to_string();
        editor.search_mode = crate::state::SearchMode::Regex;
        apply_replace_all(&mut editor);
        assert_eq!(editor.buffer.lines[0], "1a 2b 3c");
    }

    #[test]
    fn test_apply_replace_current_regex_capture() {
        let mut editor = create_editor_state("a1 b2");
        editor.search_buffer = r"([a-z])([0-9])".to_string();
        editor.replace_buffer = "$2$1".to_string();
        editor.search_mode = crate::state::SearchMode::Regex;
        apply_replace_current(&mut editor); // first match "a1" -> "1a"
        assert_eq!(editor.buffer.lines[0], "1a b2");
    }

    #[test]
    fn test_literal_modes_keep_dollar_literal() {
        // In non-regex modes `$1` is inserted literally, not expanded.
        let mut editor = create_editor_state("foo");
        editor.search_buffer = "foo".to_string();
        editor.replace_buffer = "$1".to_string();
        editor.search_mode = crate::state::SearchMode::ByWord;
        apply_replace_all(&mut editor);
        assert_eq!(editor.buffer.lines[0], "$1");
    }

    #[test]
    fn test_apply_replace_all_no_match() {
        let mut editor = create_editor_state("foo bar");
        editor.search_buffer = "baz".to_string();
        editor.replace_buffer = "qux".to_string();

        apply_replace_all(&mut editor);

        assert_eq!(editor.buffer.lines[0], "foo bar");
        assert_eq!(editor.status_kind, StatusKind::Info);
    }
}
