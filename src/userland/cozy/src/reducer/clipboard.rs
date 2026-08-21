use crate::reducer::EventResult;
use crate::reducer::helper::mark_modified;
use crate::state::{EditorMode, EditorState, Register};

/// Store linewise text in the unnamed register and mirror it to the system
/// clipboard (deliberate, line-level cut/yank should be reachable from other apps).
pub fn set_register_linewise(editor: &mut EditorState, text: String) {
    crate::clipboard_io::set_text(&text);
    editor.register = Register {
        text,
        linewise: true,
    };
}

/// Store charwise text in the unnamed register only. Character-level edits stay
/// internal so a stray `x`/`D` never clobbers the system clipboard.
pub fn set_register_charwise(editor: &mut EditorState, text: String) {
    editor.register = Register {
        text,
        linewise: false,
    };
}

/// Paste the unnamed register. `after` = `p` (below line / after cursor),
/// `!after` = `P` (above line / before cursor).
pub fn paste_register(editor: &mut EditorState, after: bool) -> EventResult {
    if editor.register.text.is_empty() {
        return EventResult::Continue;
    }
    mark_modified(editor);

    if editor.register.linewise {
        let new_lines: Vec<String> = editor
            .register
            .text
            .split('\n')
            .map(|s| s.to_string())
            .collect();
        let at = (if after {
            editor.cursor.y + 1
        } else {
            editor.cursor.y
        })
        .min(editor.buffer.lines.len());
        for (i, line) in new_lines.iter().enumerate() {
            editor.buffer.lines.insert(at + i, line.clone());
        }
        editor.cursor.y = at;
        editor.cursor.x = crate::state::cursor::first_non_whitespace_byte(&editor.buffer.lines[at]);
        editor.set_status_message(
            format!("Pasted {} line(s)", new_lines.len()),
            crate::state::StatusKind::Success,
            false,
        );
    } else {
        let text = editor.register.text.clone();
        // `p` inserts after the current character (vim semantics); `P` at cursor.
        if after && editor.cursor.x < editor.buffer.lines[editor.cursor.y].len() {
            editor.cursor.move_right(&editor.buffer.lines);
        }
        for c in text.chars() {
            if c == '\n' {
                editor.buffer.enter(&mut editor.cursor);
            } else if c != '\r' {
                editor.buffer.insert_char(c, &mut editor.cursor);
            }
        }
        editor.set_status_message(
            "Pasted".to_string(),
            crate::state::StatusKind::Success,
            false,
        );
    }
    EventResult::Continue
}

pub fn paste_from_clipboard(editor: &mut EditorState) -> EventResult {
    #[cfg(not(feature = "clipboard"))]
    let _ = editor;

    match crate::clipboard_io::get_text() {
        #[cfg(feature = "clipboard")]
        crate::clipboard_io::ClipboardRead::Text(text) => paste_string(editor, &text),
        #[cfg(feature = "clipboard")]
        crate::clipboard_io::ClipboardRead::Failed => {
            editor.set_status_message(
                "Failed to read from clipboard".to_string(),
                crate::state::StatusKind::Error,
                false,
            );
            EventResult::Continue
        }
        #[cfg(not(feature = "clipboard"))]
        crate::clipboard_io::ClipboardRead::Unavailable => EventResult::Continue,
    }
}

pub fn paste_string(editor: &mut EditorState, s: &str) -> EventResult {
    if editor.mode == EditorMode::Edit {
        mark_modified(editor); // Single snapshot for entire paste
        for c in s.chars() {
            if c == '\n' {
                editor.buffer.enter(&mut editor.cursor);
            } else if c != '\r' {
                editor.buffer.insert_char(c, &mut editor.cursor);
            }
        }
    }
    EventResult::Continue
}

/// Cut current line to clipboard and delete it
pub fn cut_line(editor: &mut EditorState) -> EventResult {
    if editor.buffer.lines.is_empty() {
        return EventResult::Continue;
    }
    let y = editor.cursor.y;
    let text = editor.buffer.lines[y].clone();

    // Linewise cut: record to the register (and mirror to the system clipboard).
    set_register_linewise(editor, text.clone());

    editor.save_snapshot();
    editor.buffer.lines.remove(y);
    if editor.buffer.lines.is_empty() {
        editor.buffer.lines.push(String::new());
    }
    if editor.cursor.y >= editor.buffer.lines.len() {
        editor.cursor.y = editor.buffer.lines.len() - 1;
    }
    editor.cursor.x = 0;
    editor.set_status_message(
        format!("Cut line ({} chars)", text.chars().count()),
        crate::state::StatusKind::Success,
        false,
    );
    EventResult::Continue
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::TextBuffer;

    fn editor_with(lines: &[&str]) -> EditorState {
        let mut e = EditorState::new(None);
        e.buffer = TextBuffer::from_lines(lines.iter().map(|s| s.to_string()).collect());
        e
    }

    #[test]
    fn linewise_cut_then_paste_moves_line() {
        let mut e = editor_with(&["one", "two", "three"]);
        e.cursor.y = 0;
        cut_line(&mut e); // register = "one" (linewise); buffer = ["two","three"]
        assert_eq!(e.buffer.lines, vec!["two", "three"]);
        assert!(e.register.linewise);
        e.cursor.y = 1; // on "three"
        paste_register(&mut e, true); // p -> below "three"
        assert_eq!(e.buffer.lines, vec!["two", "three", "one"]);
        assert_eq!(e.cursor.y, 2);
    }

    #[test]
    fn paste_before_inserts_above() {
        let mut e = editor_with(&["a", "b"]);
        e.cursor.y = 0;
        cut_line(&mut e); // register "a"; buffer ["b"]
        e.cursor.y = 0; // on "b"
        paste_register(&mut e, false); // P -> above "b"
        assert_eq!(e.buffer.lines, vec!["a", "b"]);
    }

    #[test]
    fn charwise_register_paste_inline() {
        let mut e = editor_with(&["hello"]);
        set_register_charwise(&mut e, "XY".to_string());
        assert!(!e.register.linewise);
        e.cursor.x = 0;
        paste_register(&mut e, false); // P -> insert at cursor
        assert_eq!(e.buffer.lines[0], "XYhello");
    }

    #[test]
    fn paste_empty_register_is_noop() {
        let mut e = editor_with(&["x"]);
        let before = e.buffer.lines.clone();
        paste_register(&mut e, true);
        assert_eq!(e.buffer.lines, before);
    }
}
