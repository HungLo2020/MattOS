use crate::reducer::EventResult;
use crate::state::{EditorMode, EditorState};

fn active_buffer(editor: &EditorState) -> &String {
    match editor.mode {
        EditorMode::Open => &editor.open_filename_buffer,
        _ => &editor.save_filename_buffer,
    }
}

fn active_buffer_mut(editor: &mut EditorState) -> &mut String {
    match editor.mode {
        EditorMode::Open => &mut editor.open_filename_buffer,
        _ => &mut editor.save_filename_buffer,
    }
}

pub fn update_filename_buffer(editor: &mut EditorState, c: char) -> EventResult {
    let pos = editor.filename_cursor;
    let buf = active_buffer_mut(editor);
    if pos <= buf.len() && buf.is_char_boundary(pos) {
        buf.insert(pos, c);
        editor.filename_cursor = pos + c.len_utf8();
    }
    EventResult::Continue
}

pub fn delete_from_filename_buffer(editor: &mut EditorState) -> EventResult {
    let pos = editor.filename_cursor;
    if pos == 0 {
        return EventResult::Continue;
    }
    let buf = active_buffer_mut(editor);
    let prev = buf[..pos]
        .char_indices()
        .last()
        .map(|(i, _)| i)
        .unwrap_or(0);
    buf.remove(prev);
    editor.filename_cursor = prev;
    EventResult::Continue
}

pub fn delete_char_at_cursor(editor: &mut EditorState) -> EventResult {
    let pos = editor.filename_cursor;
    let buf = active_buffer_mut(editor);
    if pos < buf.len() && buf.is_char_boundary(pos) {
        buf.remove(pos);
    }
    EventResult::Continue
}

pub fn move_filename_cursor_left(editor: &mut EditorState) -> EventResult {
    let pos = editor.filename_cursor;
    if pos == 0 {
        return EventResult::Continue;
    }
    let prev = active_buffer(editor)[..pos]
        .char_indices()
        .last()
        .map(|(i, _)| i)
        .unwrap_or(0);
    editor.filename_cursor = prev;
    EventResult::Continue
}

pub fn move_filename_cursor_right(editor: &mut EditorState) -> EventResult {
    let pos = editor.filename_cursor;
    let len = active_buffer(editor).len();
    if pos >= len {
        return EventResult::Continue;
    }
    let next = active_buffer(editor)[pos..]
        .chars()
        .next()
        .map(|c| pos + c.len_utf8())
        .unwrap_or(pos);
    editor.filename_cursor = next;
    EventResult::Continue
}

pub fn move_filename_cursor_home(editor: &mut EditorState) -> EventResult {
    editor.filename_cursor = 0;
    EventResult::Continue
}

pub fn move_filename_cursor_end(editor: &mut EditorState) -> EventResult {
    editor.filename_cursor = active_buffer(editor).len();
    EventResult::Continue
}
