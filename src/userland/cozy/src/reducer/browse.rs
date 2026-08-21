//! Browse モード（フォルダツリー）の reducer。
//!
//! カーソル移動・展開/畳み・ファイルを開く・絞り込み入力を処理する。ツリーの状態は
//! `editor.browse_tree`（`crate::browse::BrowseTree`）に持ち、ここはそれを駆動するだけ。

use crate::reducer::EventResult;
use crate::state::EditorState;

pub fn move_up(editor: &mut EditorState) -> EventResult {
    if let Some(tree) = &mut editor.browse_tree {
        tree.move_up();
    }
    EventResult::Continue
}

pub fn move_down(editor: &mut EditorState) -> EventResult {
    if let Some(tree) = &mut editor.browse_tree {
        tree.move_down();
    }
    EventResult::Continue
}

pub fn goto_top(editor: &mut EditorState) -> EventResult {
    if let Some(tree) = &mut editor.browse_tree {
        tree.goto_top();
    }
    EventResult::Continue
}

pub fn goto_bottom(editor: &mut EditorState) -> EventResult {
    if let Some(tree) = &mut editor.browse_tree {
        tree.goto_bottom();
    }
    EventResult::Continue
}

/// `l`/Enter/→: ディレクトリは展開トグル、ファイルは開いて Edit へ。絞り込み入力中の
/// Enter は「確定」（入力受付を終えて結果を保持したまま移動操作へ戻る）。
pub fn expand_or_open(editor: &mut EditorState) -> EventResult {
    let to_open = {
        let Some(tree) = &mut editor.browse_tree else {
            return EventResult::Continue;
        };
        if tree.filtering {
            tree.filtering = false;
            None
        } else {
            tree.expand_or_open()
        }
    };

    if let Some(path) = to_open {
        let path_str = path.to_string_lossy().to_string();
        let result = editor.open_file(&path_str);
        // open_file は失敗時 filename を変えないので、先に resting mode へ遷移してから結果を反映。
        editor.enter_mode(editor.home_mode());
        match result {
            Ok(_) => crate::reducer::status::set_success(editor, "Opened", &path_str),
            Err(e) => crate::reducer::status::set_error(editor, &e.to_string()),
        }
    }
    EventResult::Continue
}

/// `h`/←: 展開中ディレクトリは畳む、それ以外は親へ。
pub fn collapse_or_parent(editor: &mut EditorState) -> EventResult {
    if let Some(tree) = &mut editor.browse_tree {
        tree.collapse_or_parent();
    }
    EventResult::Continue
}

pub fn start_filter(editor: &mut EditorState) -> EventResult {
    if let Some(tree) = &mut editor.browse_tree {
        tree.filtering = true;
        tree.filter.clear();
    }
    EventResult::Continue
}

pub fn filter_char(editor: &mut EditorState, c: char) -> EventResult {
    if let Some(tree) = &mut editor.browse_tree {
        tree.push_filter_char(c);
    }
    EventResult::Continue
}

pub fn filter_backspace(editor: &mut EditorState) -> EventResult {
    if let Some(tree) = &mut editor.browse_tree {
        tree.pop_filter_char();
    }
    EventResult::Continue
}

/// Esc: 絞り込み中なら絞り込みを解除して Browse に留まる、そうでなければ Browse を抜けて resting mode へ。
pub fn cancel(editor: &mut EditorState) -> EventResult {
    let filtering = editor
        .browse_tree
        .as_ref()
        .map(|t| t.filtering)
        .unwrap_or(false);
    if filtering {
        if let Some(tree) = &mut editor.browse_tree {
            tree.filtering = false;
            tree.filter.clear();
        }
    } else {
        editor.mode = editor.home_mode();
        editor.status_message = None;
    }
    EventResult::Continue
}
