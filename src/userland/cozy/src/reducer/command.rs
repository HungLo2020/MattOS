use crate::action::Action;
use crate::commands::{self, CommandAction};
use crate::reducer::EventResult;
use crate::state::EditorState;

fn clamp_selection(editor: &mut EditorState) {
    let len = commands::filtered_commands(&editor.command_query).len();
    if len == 0 {
        editor.command_selected = 0;
    } else if editor.command_selected >= len {
        editor.command_selected = len - 1;
    }
}

pub fn input_char(editor: &mut EditorState, c: char) -> EventResult {
    editor.command_query.push(c);
    clamp_selection(editor);
    EventResult::Continue
}

pub fn backspace(editor: &mut EditorState) -> EventResult {
    editor.command_query.pop();
    clamp_selection(editor);
    EventResult::Continue
}

pub fn move_up(editor: &mut EditorState) -> EventResult {
    let len = commands::filtered_commands(&editor.command_query).len();
    if len > 0 {
        editor.command_selected = editor.command_selected.saturating_sub(1);
    }
    EventResult::Continue
}

pub fn move_down(editor: &mut EditorState) -> EventResult {
    let len = commands::filtered_commands(&editor.command_query).len();
    if len > 0 {
        editor.command_selected = (editor.command_selected + 1).min(len - 1);
    }
    EventResult::Continue
}

pub fn complete(editor: &mut EditorState) -> EventResult {
    if let Some(completion) = commands::label_completion(&editor.command_query) {
        if completion.len() > editor.command_query.len() {
            editor.command_query = completion;
            clamp_selection(editor);
        }
    }
    EventResult::Continue
}

pub fn execute(editor: &mut EditorState) -> EventResult {
    let matches = commands::filtered_commands(&editor.command_query);
    let Some(command) = matches.get(editor.command_selected) else {
        crate::reducer::status::set_error(editor, "No command found");
        return EventResult::Continue;
    };

    let action = match command.action.clone() {
        CommandAction::Dispatch(action) => action,
        CommandAction::EnterMode(mode) => Action::EnterMode(mode),
        CommandAction::OpenConfig => match crate::state::Config::ensure_default_config_file(None) {
            Ok(path) => Action::Open(path.to_string_lossy().to_string()),
            Err(e) => {
                crate::reducer::status::set_error(editor, &e.to_string());
                return EventResult::Continue;
            }
        },
    };

    crate::reducer::editor::apply_editor_event(editor, &action)
}
