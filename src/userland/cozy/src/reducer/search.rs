use crate::reducer::EventResult;
use crate::state::{EditorState, SearchMode};
use regex::Regex;

// ── buffer editing ────────────────────────────────────────────────────────────

pub fn update_search_buffer(editor: &mut EditorState, c: char) {
    let pos = editor.search_cursor;
    let buf = &mut editor.search_buffer;
    if pos <= buf.len() && buf.is_char_boundary(pos) {
        buf.insert(pos, c);
        editor.search_cursor = pos + c.len_utf8();
    }
    recompute_matches(editor);
    focus_nearest_match(editor);
}

pub fn delete_from_search_buffer(editor: &mut EditorState) {
    let pos = editor.search_cursor;
    if pos == 0 {
        return;
    }
    let buf = &mut editor.search_buffer;
    let prev = buf[..pos]
        .char_indices()
        .last()
        .map(|(i, _)| i)
        .unwrap_or(0);
    buf.remove(prev);
    editor.search_cursor = prev;
    recompute_matches(editor);
    focus_nearest_match(editor);
}

pub fn delete_search_char_at_cursor(editor: &mut EditorState) {
    let pos = editor.search_cursor;
    let buf = &mut editor.search_buffer;
    if pos < buf.len() && buf.is_char_boundary(pos) {
        buf.remove(pos);
    }
    recompute_matches(editor);
    focus_nearest_match(editor);
}

pub fn move_search_cursor_left(editor: &mut EditorState) {
    let pos = editor.search_cursor;
    if pos == 0 {
        return;
    }
    editor.search_cursor = editor.search_buffer[..pos]
        .char_indices()
        .last()
        .map(|(i, _)| i)
        .unwrap_or(0);
}

pub fn move_search_cursor_right(editor: &mut EditorState) {
    let pos = editor.search_cursor;
    let len = editor.search_buffer.len();
    if pos >= len {
        return;
    }
    editor.search_cursor = editor.search_buffer[pos..]
        .chars()
        .next()
        .map(|c| pos + c.len_utf8())
        .unwrap_or(pos);
}

pub fn move_search_cursor_home(editor: &mut EditorState) {
    editor.search_cursor = 0;
}

pub fn move_search_cursor_end(editor: &mut EditorState) {
    editor.search_cursor = editor.search_buffer.len();
}

// ── match computation ─────────────────────────────────────────────────────────

/// Collect all byte-range matches of `query` in `line`.
fn find_in_line(line: &str, query: &str, mode: &SearchMode) -> Vec<(usize, usize)> {
    if query.is_empty() || line.is_empty() {
        return vec![];
    }
    match mode {
        SearchMode::MatchCase => line
            .match_indices(query)
            .map(|(i, s)| (i, i + s.len()))
            .collect(),
        SearchMode::ByWord => {
            let q = query.to_lowercase();
            let lower = line.to_lowercase();
            lower
                .match_indices(q.as_str())
                .map(|(i, s)| (i, i + s.len()))
                .collect()
        }
        SearchMode::Regex => Regex::new(query)
            .map(|re| re.find_iter(line).map(|m| (m.start(), m.end())).collect())
            .unwrap_or_default(),
    }
}

/// Rebuild editor.search_matches from the current search_buffer.
pub fn recompute_matches(editor: &mut EditorState) {
    editor.search_matches.clear();
    let query = editor.search_buffer.clone();
    if query.is_empty() {
        return;
    }
    for (y, line) in editor.buffer.lines.iter().enumerate() {
        for (s, e) in find_in_line(line, &query, &editor.search_mode) {
            editor.search_matches.push((y, s, e));
        }
    }
}

/// Set search_current to the first match at-or-after the current cursor.
pub fn focus_nearest_match(editor: &mut EditorState) {
    if editor.search_matches.is_empty() {
        editor.search_current = 0;
        return;
    }
    let cy = editor.cursor.y;
    let cx = editor.cursor.x;
    editor.search_current = editor
        .search_matches
        .iter()
        .position(|&(y, s, _)| y > cy || (y == cy && s >= cx))
        .unwrap_or(0);
    jump_to_current(editor);
    update_status(editor);
}

/// Move cursor to the currently focused match.
fn jump_to_current(editor: &mut EditorState) {
    if let Some(&(y, s, _)) = editor.search_matches.get(editor.search_current) {
        editor.cursor.y = y;
        editor.cursor.x = s;
    }
}

fn update_status(editor: &mut EditorState) {
    if editor.search_matches.is_empty() {
        crate::reducer::status::set_error(editor, &format!("'{}' not found", editor.search_buffer));
    } else {
        editor.status_message = None;
    }
}

// ── navigation ────────────────────────────────────────────────────────────────

pub fn apply_search_next(editor: &mut EditorState) -> EventResult {
    if editor.search_matches.is_empty() {
        recompute_matches(editor);
    }
    if editor.search_matches.is_empty() {
        crate::reducer::status::set_error(editor, &format!("'{}' not found", editor.search_buffer));
        return EventResult::Continue;
    }
    let total = editor.search_matches.len();
    editor.search_current = (editor.search_current + 1) % total;
    jump_to_current(editor);
    update_status(editor);
    EventResult::Continue
}

pub fn apply_search_previous(editor: &mut EditorState) -> EventResult {
    if editor.search_matches.is_empty() {
        recompute_matches(editor);
    }
    if editor.search_matches.is_empty() {
        crate::reducer::status::set_error(editor, &format!("'{}' not found", editor.search_buffer));
        return EventResult::Continue;
    }
    let total = editor.search_matches.len();
    editor.search_current = (editor.search_current + total - 1) % total;
    jump_to_current(editor);
    update_status(editor);
    EventResult::Continue
}

// ── mode helpers ──────────────────────────────────────────────────────────────

pub fn apply_toggle_search_mode(editor: &mut EditorState) -> EventResult {
    editor.search_mode = match editor.search_mode {
        SearchMode::MatchCase => SearchMode::Regex,
        SearchMode::Regex => SearchMode::ByWord,
        SearchMode::ByWord => SearchMode::MatchCase,
    };
    recompute_matches(editor);
    focus_nearest_match(editor);
    EventResult::Continue
}
