use crate::state::EditorState;
// Helper function to mark editor as modified and save snapshot for undo
pub fn mark_modified(editor: &mut EditorState) {
    editor.save_snapshot();
    editor.modified = true;
}
