use crate::state::{EditorState, StatusKind};

pub fn set_success(editor: &mut EditorState, action: &str, target: &str) {
    let msg = if target.is_empty() {
        action.to_string()
    } else {
        format!("{}: {}", action, target)
    };
    editor.set_status_message(msg, StatusKind::Success, true);
}

pub fn set_error(editor: &mut EditorState, msg: &str) {
    editor.set_status_message(msg.to_string(), StatusKind::Error, true);
}

pub fn set_info(editor: &mut EditorState, msg: &str) {
    editor.set_status_message(msg.to_string(), StatusKind::Info, true);
}
