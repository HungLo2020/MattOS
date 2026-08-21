pub mod body;
pub mod browse;
pub mod diff;
pub mod footer;
pub mod markdown;
pub mod screen;

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
};
use std::rc::Rc;

use crate::state::{EditorMode, EditorState};

// Re-export public functions
pub use body::render_text_buffer;
pub use screen::{render_help, render_welcome};

pub struct Renderer;

impl Renderer {
    pub fn editor_layout(area: Rect, editor: &EditorState) -> Rc<[Rect]> {
        let is_narrow = area.width < 50;
        let is_low_height = area.height < 18;
        let status_bar_h: u16 = if matches!(editor.mode, EditorMode::Welcome) || area.height < 2 {
            0
        } else {
            1
        };
        let requested_shortcut_h = if !editor.footer_visible_runtime {
            hidden_shortcut_rows(editor.mode)
        } else if is_low_height {
            compact_shortcut_rows(is_narrow, editor.mode)
        } else {
            normal_shortcut_rows(is_narrow, editor.mode)
        };
        let available = area.height.saturating_sub(1 + status_bar_h);
        let actual_shortcut_h = requested_shortcut_h.min(available);
        Layout::default()
            .direction(Direction::Vertical)
            .constraints(
                [
                    Constraint::Min(1),
                    Constraint::Length(actual_shortcut_h),
                    Constraint::Length(status_bar_h),
                ]
                .as_ref(),
            )
            .split(area)
    }

    pub fn render_body(editor: &mut EditorState, f: &mut Frame, area: Rect) {
        editor.page_size = area.height as usize;
        if editor.mode == EditorMode::Welcome {
            render_welcome(f, area);
            return;
        }
        if editor.mode == EditorMode::Help {
            render_help(editor, f, area);
            return;
        }
        if editor.mode == EditorMode::Browse {
            browse::render_browse(editor, f, area);
            return;
        }
        if editor.mode == EditorMode::Markdown {
            markdown::render_markdown(editor, f, area);
            return;
        }
        if editor.mode == EditorMode::DiffReview || editor.mode == EditorMode::DiffCommitMsg {
            // Keep the diff visible while the commit message is typed.
            diff::render_diff_review(editor, f, area);
            return;
        }
        render_text_buffer(editor, f, area);
    }

    pub fn render_shortcuts(editor: &EditorState, f: &mut Frame, area: Rect) {
        footer::render_shortcuts(editor, f, area);
    }

    pub fn render_status_bar(editor: &EditorState, f: &mut Frame, area: Rect) {
        footer::render_status_bar(editor, f, area);
    }
}

fn normal_shortcut_rows(is_narrow: bool, mode: EditorMode) -> u16 {
    match (is_narrow, mode) {
        (_, EditorMode::Welcome) => 0,
        (_, EditorMode::Command) => 10,
        (true, _) => 4,
        (false, _) => 2,
    }
}

fn compact_shortcut_rows(is_narrow: bool, mode: EditorMode) -> u16 {
    match mode {
        EditorMode::Welcome => 0,
        EditorMode::Command => {
            if is_narrow {
                3
            } else {
                4
            }
        }
        EditorMode::Save | EditorMode::Open | EditorMode::DiffCommitMsg => 2,
        EditorMode::Quit | EditorMode::Replace => 3,
        EditorMode::Search | EditorMode::Goto => 2,
        EditorMode::Edit
        | EditorMode::Glide
        | EditorMode::Browse
        | EditorMode::Markdown
        | EditorMode::DiffReview
        | EditorMode::Help => 1,
    }
}

fn hidden_shortcut_rows(mode: EditorMode) -> u16 {
    match mode {
        EditorMode::Welcome => 0,
        EditorMode::Save | EditorMode::Open | EditorMode::Replace | EditorMode::Command => 2,
        EditorMode::Quit | EditorMode::DiffCommitMsg => 2,
        EditorMode::Search | EditorMode::Goto => 1,
        EditorMode::Edit
        | EditorMode::Glide
        | EditorMode::Browse
        | EditorMode::Markdown
        | EditorMode::DiffReview
        | EditorMode::Help => 0,
    }
}
