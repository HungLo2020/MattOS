use crate::reducer::EventResult;
use crate::reducer::helper::mark_modified;
use crate::state::{EditorMode, EditorState};

// Helper function for Backspace action
pub fn handle_backspace(editor: &mut EditorState) -> EventResult {
    match editor.mode {
        EditorMode::Edit | EditorMode::Glide => {
            if editor.mode == EditorMode::Glide {
                let (y, x) = (editor.cursor.y, editor.cursor.x);
                let ch = editor
                    .buffer
                    .lines
                    .get(y)
                    .and_then(|l| l[..x.min(l.len())].chars().next_back());
                if let Some(ch) = ch {
                    crate::reducer::clipboard::set_register_charwise(editor, ch.to_string());
                }
            }
            mark_modified(editor);
            editor.buffer.backspace(&mut editor.cursor);
        }
        EditorMode::Search => {
            crate::reducer::search::delete_from_search_buffer(editor);
        }
        EditorMode::Replace => {
            crate::reducer::replace::delete_from_replace_buffer(editor);
        }
        EditorMode::Save | EditorMode::Open | EditorMode::Quit => {
            crate::reducer::file::delete_from_filename_buffer(editor);
        }
        _ => {}
    }
    EventResult::Continue
}

// Helper function for Delete action
pub fn handle_delete(editor: &mut EditorState) -> EventResult {
    if editor.mode == EditorMode::Edit || editor.mode == EditorMode::Glide {
        if editor.mode == EditorMode::Glide {
            let (y, x) = (editor.cursor.y, editor.cursor.x);
            let ch = editor
                .buffer
                .lines
                .get(y)
                .and_then(|l| l.get(x.min(l.len())..).and_then(|s| s.chars().next()));
            if let Some(ch) = ch {
                crate::reducer::clipboard::set_register_charwise(editor, ch.to_string());
            }
        }
        mark_modified(editor);
        editor.buffer.delete(&mut editor.cursor);
    }
    EventResult::Continue
}
