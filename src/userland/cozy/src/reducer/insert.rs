use crate::reducer::EventResult;
use crate::reducer::helper::mark_modified;
use crate::state::{EditorMode, EditorState};

// Helper function for InsertChar action
pub fn handle_insert_char(editor: &mut EditorState, c: char) -> EventResult {
    if editor.mode == EditorMode::Edit {
        mark_modified(editor);
        editor.buffer.insert_char(c, &mut editor.cursor);
    }
    EventResult::Continue
}

// Helper function for Enter action
pub fn handle_enter(editor: &mut EditorState) -> EventResult {
    match editor.mode {
        EditorMode::Edit => {
            mark_modified(editor);
            editor.buffer.enter(&mut editor.cursor);
            EventResult::Continue
        }
        _ => EventResult::Continue, // Handled by editor reducer for other modes
    }
}
