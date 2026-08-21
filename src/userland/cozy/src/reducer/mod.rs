pub mod browse;
pub mod buffer;
pub mod clipboard;
pub mod command;
pub mod cursor;
pub mod delete;
pub mod editor;
pub mod file;
pub mod helper;
pub mod insert;
pub mod operator;
pub mod replace;
pub mod search;
pub mod status;

#[cfg(test)]
mod editor_test;

use crate::action::Action;
use crate::state::EditorState;

pub enum EventResult {
    Continue,
    Exit,
}

/// Read-only scrolling views (Markdown preview and Help) share one navigation
/// model: a cursor line, a scroll offset, a total line count, and a viewport
/// height. `ReadView` selects which set of `EditorState` fields to drive so the
/// same motion logic serves both.
#[derive(Clone, Copy)]
enum ReadView {
    Markdown,
    Help,
}

fn read_view(mode: &crate::state::EditorMode) -> Option<ReadView> {
    match mode {
        crate::state::EditorMode::Markdown => Some(ReadView::Markdown),
        crate::state::EditorMode::Help => Some(ReadView::Help),
        _ => None,
    }
}

/// Move the highlighted hunk in session diff review. The render layer keeps it
/// on screen by adjusting its own scroll offset.
fn diff_move_hunk(editor: &mut EditorState, delta: isize) -> EventResult {
    if let Some(dr) = editor.diff_review.as_mut() {
        if !dr.hunks.is_empty() {
            let max = dr.hunks.len() as isize - 1;
            dr.current = (dr.current as isize + delta).clamp(0, max) as usize;
        }
    }
    EventResult::Continue
}

fn rv_line_count(editor: &EditorState, view: ReadView) -> usize {
    match view {
        ReadView::Markdown => editor
            .markdown_rendered_line_count
            .max(editor.buffer.lines.len())
            .max(1),
        ReadView::Help => editor.help_rendered_line_count.max(1),
    }
}

fn rv_page_step(editor: &EditorState, view: ReadView) -> usize {
    let height = match view {
        ReadView::Markdown => editor.markdown_view_height,
        ReadView::Help => editor.help_view_height,
    };
    if height == 0 {
        editor.page_size
    } else {
        height
    }
    .max(1)
}

fn rv_cursor(editor: &EditorState, view: ReadView) -> usize {
    match view {
        ReadView::Markdown => editor.markdown_cursor_line,
        ReadView::Help => editor.help_cursor_line,
    }
}

fn rv_scroll(editor: &EditorState, view: ReadView) -> usize {
    match view {
        ReadView::Markdown => editor.markdown_scroll_offset,
        ReadView::Help => editor.help_scroll_offset,
    }
}

fn rv_store(editor: &mut EditorState, view: ReadView, cursor: usize, scroll: usize) {
    match view {
        ReadView::Markdown => {
            editor.markdown_cursor_line = cursor;
            editor.markdown_scroll_offset = scroll;
        }
        ReadView::Help => {
            editor.help_cursor_line = cursor;
            editor.help_scroll_offset = scroll;
        }
    }
}

fn set_read_cursor(editor: &mut EditorState, view: ReadView, line: usize) {
    let last = rv_line_count(editor, view).saturating_sub(1);
    let y = line.min(last);
    let mut scroll = rv_scroll(editor, view);
    let page = rv_page_step(editor, view);
    if y < scroll {
        scroll = y;
    } else if y >= scroll.saturating_add(page) {
        scroll = y.saturating_sub(page - 1);
    }
    rv_store(editor, view, y, scroll);
}

fn move_read_cursor(editor: &mut EditorState, view: ReadView, delta: isize) {
    let current = rv_cursor(editor, view);
    let next = if delta.is_negative() {
        current.saturating_sub(delta.unsigned_abs())
    } else {
        current.saturating_add(delta as usize)
    };
    set_read_cursor(editor, view, next);
}

fn take_read_count_opt(editor: &mut EditorState) -> Option<usize> {
    let n = if editor.glide_count.is_empty() {
        None
    } else {
        editor.glide_count.parse::<usize>().ok().filter(|&n| n >= 1)
    };
    editor.glide_count.clear();
    n
}

fn read_screen_motion(
    editor: &mut EditorState,
    view: ReadView,
    motion: crate::glide::Motion,
) -> EventResult {
    editor.glide_prefix = None;
    let top = rv_scroll(editor, view);
    let page = rv_page_step(editor, view);
    let count = take_read_count_opt(editor);
    let last = rv_line_count(editor, view).saturating_sub(1);
    let line = match motion {
        crate::glide::Motion::FileTop => match count {
            Some(line) => line.saturating_sub(1).min(last),
            None => 0,
        },
        crate::glide::Motion::FileBottom => match count {
            Some(line) => line.saturating_sub(1).min(last),
            None => last,
        },
        crate::glide::Motion::ScreenTop => top,
        crate::glide::Motion::ScreenMiddle => top.saturating_add(page / 2),
        crate::glide::Motion::ScreenBottom => top.saturating_add(page.saturating_sub(1)),
        _ => return editor::apply_editor_event(editor, &Action::GlideMove(motion)),
    };
    set_read_cursor(editor, view, line);
    EventResult::Continue
}

/// The recovery offer is two keys wide: Enter takes the unsaved edits back, Esc
/// throws them away. Anything else is ignored rather than guessed at — the one
/// thing worse than losing the edits is quietly overwriting them.
///
/// Restoring does **not** touch the file. It loads the buffer and marks it
/// modified, which is the truth: these edits were never saved. The user saves,
/// or quits and discards, exactly as if they had just typed them.
fn answer_recovery(editor: &mut EditorState, action: &Action) -> EventResult {
    match action {
        Action::Enter => {
            let recovery = editor.recovery.take().expect("checked by the caller");
            editor.buffer = crate::state::TextBuffer::from_lines(recovery.lines);
            editor.cursor = crate::state::Cursor::default();
            editor.modified = true;
            editor.highlighter.mark_dirty();
            // Persistent, not a 3-second flash: the buffer now holds edits that
            // exist nowhere but here, and the user needs to be told that for as
            // long as it is true — not for as long as they happened to be looking.
            editor.set_status_message(
                "Unsaved changes restored — save to keep them".to_string(),
                crate::state::StatusKind::Success,
                true,
            );
            EventResult::Continue
        }
        Action::Cancel => {
            editor.recovery = None;
            crate::swap::remove(editor);
            editor.set_status_message(
                "Unsaved changes discarded".to_string(),
                crate::state::StatusKind::Info,
                false,
            );
            EventResult::Continue
        }
        _ => EventResult::Continue,
    }
}

/// The create-directory offer is the same two keys as the recovery one: Enter
/// creates the missing parent and finishes the save, Esc leaves the filesystem
/// alone. Anything else is ignored rather than guessed at — creating a directory
/// is the kind of side effect that must not happen because someone kept typing.
///
/// The retry goes back through `Action::Save`/`SaveAndExit` with the **same name
/// the save was called with**, so the target is resolved by `save`/`save_as`
/// exactly once. Rebuilding the path here would be a second implementation of
/// that rule, and the two would drift.
///
/// ⚠️ Cancelling does not leave the Save prompt — the typed filename is still
/// there to be corrected. `~/.shh/config` is a typo, not a request for a new
/// directory, and this is the moment the user finds out.
fn answer_create_dir(editor: &mut EditorState, action: &Action) -> EventResult {
    match action {
        Action::Enter => {
            let offer = editor.create_dir.take().expect("checked by the caller");
            if let Err(e) = std::fs::create_dir_all(&offer.dir) {
                crate::reducer::status::set_error(
                    editor,
                    &format!("Failed to create '{}': {}", offer.dir.display(), e),
                );
                return EventResult::Continue;
            }
            let retry = if offer.and_exit {
                Action::SaveAndExit(offer.fname)
            } else {
                Action::Save(offer.fname)
            };
            reduce(editor, retry)
        }
        Action::Cancel => {
            editor.create_dir = None;
            editor.set_status_message(
                "Not saved — the directory was not created".to_string(),
                crate::state::StatusKind::Info,
                false,
            );
            EventResult::Continue
        }
        // ⚠️ Ctrl+Q must keep working. A question that swallows the quit key
        // traps the user inside the editor, and pressing it harder does nothing
        // visible — the worst shape a prompt can have. Quitting here is safe:
        // it is already the explicit discard path, and the swap still holds the
        // buffer for the next launch.
        Action::Quit => EventResult::Exit,
        _ => EventResult::Continue,
    }
}

pub fn reduce(editor: &mut EditorState, action: Action) -> EventResult {
    // A recovered swap is waiting for a yes or no. Answer it first: typing into a
    // buffer that is about to be replaced would silently throw the typing away.
    if editor.recovery.is_some() {
        return answer_recovery(editor, &action);
    }

    // A save is holding, waiting to be told whether to create the directory.
    // Same reason to answer first: the next keystroke would otherwise edit the
    // buffer while the user believes they are answering a question.
    if editor.create_dir.is_some() {
        return answer_create_dir(editor, &action);
    }

    // The yank flash lasts exactly until the next keypress: clear it here, before
    // dispatch, so a fresh yank in this same call can re-arm it for one frame.
    editor.yank_highlight = None;

    // Dispatch to buffer reducer for editing actions
    match action {
        Action::InsertChar(c) => match editor.mode {
            crate::state::EditorMode::Edit => insert::handle_insert_char(editor, c),
            crate::state::EditorMode::Search => {
                search::update_search_buffer(editor, c);
                EventResult::Continue
            }
            crate::state::EditorMode::Replace => {
                replace::update_replace_buffer(editor, c);
                EventResult::Continue
            }
            crate::state::EditorMode::Save
            | crate::state::EditorMode::Open
            | crate::state::EditorMode::Quit => file::update_filename_buffer(editor, c),
            crate::state::EditorMode::Goto => {
                if c.is_ascii_digit() {
                    editor.goto_line_buffer.push(c);
                }
                EventResult::Continue
            }
            crate::state::EditorMode::DiffCommitMsg => {
                editor.commit_msg_buffer.push(c);
                EventResult::Continue
            }
            _ => EventResult::Continue,
        },
        Action::InsertString(s) => clipboard::paste_string(editor, &s),
        Action::Enter => {
            // Enter is special, handled in both but depends on mode
            // insert reducer handles Insert/Edit mode Enter
            // editor reducer handles others
            match editor.mode {
                crate::state::EditorMode::Edit => insert::handle_enter(editor),
                crate::state::EditorMode::Search => search::apply_search_next(editor),
                crate::state::EditorMode::Replace => replace::apply_replace_current(editor),
                _ => editor::apply_editor_event(editor, &action),
            }
        }
        Action::Backspace => {
            match editor.mode {
                // Glide `X` deletes the char before the cursor (mirror of `x`).
                crate::state::EditorMode::Edit | crate::state::EditorMode::Glide => {
                    delete::handle_backspace(editor)
                }
                crate::state::EditorMode::Search => {
                    search::delete_from_search_buffer(editor);
                    EventResult::Continue
                }
                crate::state::EditorMode::Replace => {
                    replace::delete_from_replace_buffer(editor);
                    EventResult::Continue
                }
                crate::state::EditorMode::Save
                | crate::state::EditorMode::Open
                | crate::state::EditorMode::Quit => file::delete_from_filename_buffer(editor),
                crate::state::EditorMode::Goto => {
                    editor.goto_line_buffer.pop();
                    EventResult::Continue
                }
                crate::state::EditorMode::DiffCommitMsg => {
                    editor.commit_msg_buffer.pop();
                    EventResult::Continue
                }
                _ => EventResult::Continue,
            }
        }
        Action::Delete => match editor.mode {
            crate::state::EditorMode::Save
            | crate::state::EditorMode::Open
            | crate::state::EditorMode::Quit => file::delete_char_at_cursor(editor),
            crate::state::EditorMode::Search => {
                search::delete_search_char_at_cursor(editor);
                EventResult::Continue
            }
            crate::state::EditorMode::Replace => {
                replace::delete_replace_char_at_cursor(editor);
                EventResult::Continue
            }
            _ => delete::handle_delete(editor),
        },
        Action::PasteFromClipboard => clipboard::paste_from_clipboard(editor),
        Action::ReplaceCurrent => replace::apply_replace_current(editor),
        Action::ReplaceAll => replace::apply_replace_all(editor),
        Action::SearchNext => search::apply_search_next(editor),
        Action::SearchPrevious => search::apply_search_previous(editor),
        Action::ToggleSearchMode => search::apply_toggle_search_mode(editor),
        Action::SwitchFocus => replace::apply_switch_focus(editor),
        Action::MoveLeft => {
            match editor.mode {
                crate::state::EditorMode::Save
                | crate::state::EditorMode::Open
                | crate::state::EditorMode::Quit => file::move_filename_cursor_left(editor),
                crate::state::EditorMode::Search => {
                    search::move_search_cursor_left(editor);
                    EventResult::Continue
                }
                crate::state::EditorMode::Replace => {
                    replace::move_replace_cursor_left(editor);
                    EventResult::Continue
                }
                // ← collapses a dir / moves to the parent in the tree.
                crate::state::EditorMode::Browse => browse::collapse_or_parent(editor),
                _ => editor::apply_editor_event(editor, &action),
            }
        }
        Action::MoveRight => {
            match editor.mode {
                crate::state::EditorMode::Save
                | crate::state::EditorMode::Open
                | crate::state::EditorMode::Quit => file::move_filename_cursor_right(editor),
                crate::state::EditorMode::Search => {
                    search::move_search_cursor_right(editor);
                    EventResult::Continue
                }
                crate::state::EditorMode::Replace => {
                    replace::move_replace_cursor_right(editor);
                    EventResult::Continue
                }
                // → expands a dir / opens a file in the tree.
                crate::state::EditorMode::Browse => browse::expand_or_open(editor),
                _ => editor::apply_editor_event(editor, &action),
            }
        }
        Action::Home => match editor.mode {
            crate::state::EditorMode::Save
            | crate::state::EditorMode::Open
            | crate::state::EditorMode::Quit => file::move_filename_cursor_home(editor),
            crate::state::EditorMode::Search => {
                search::move_search_cursor_home(editor);
                EventResult::Continue
            }
            crate::state::EditorMode::Replace => {
                replace::move_replace_cursor_home(editor);
                EventResult::Continue
            }
            _ => {
                if let Some(view) = read_view(&editor.mode) {
                    set_read_cursor(editor, view, 0);
                    EventResult::Continue
                } else {
                    editor::apply_editor_event(editor, &action)
                }
            }
        },
        Action::End => match editor.mode {
            crate::state::EditorMode::Save
            | crate::state::EditorMode::Open
            | crate::state::EditorMode::Quit => file::move_filename_cursor_end(editor),
            crate::state::EditorMode::Search => {
                search::move_search_cursor_end(editor);
                EventResult::Continue
            }
            crate::state::EditorMode::Replace => {
                replace::move_replace_cursor_end(editor);
                EventResult::Continue
            }
            _ => {
                if let Some(view) = read_view(&editor.mode) {
                    let last = rv_line_count(editor, view).saturating_sub(1);
                    set_read_cursor(editor, view, last);
                    EventResult::Continue
                } else {
                    editor::apply_editor_event(editor, &action)
                }
            }
        },
        // Browse mode reuses MoveUp/MoveDown for cursor motion and PageTop/PageBottom
        // for gg/G; dispatch those to the tree, leaving every other mode untouched.
        Action::MoveUp => match editor.mode {
            crate::state::EditorMode::Browse => browse::move_up(editor),
            crate::state::EditorMode::DiffReview => diff_move_hunk(editor, -1),
            _ => {
                if let Some(view) = read_view(&editor.mode) {
                    let n = take_read_count_opt(editor).unwrap_or(1);
                    move_read_cursor(editor, view, -(n as isize));
                    EventResult::Continue
                } else {
                    editor::apply_editor_event(editor, &action)
                }
            }
        },
        Action::MoveDown => match editor.mode {
            crate::state::EditorMode::Browse => browse::move_down(editor),
            crate::state::EditorMode::DiffReview => diff_move_hunk(editor, 1),
            _ => {
                if let Some(view) = read_view(&editor.mode) {
                    let n = take_read_count_opt(editor).unwrap_or(1);
                    move_read_cursor(editor, view, n as isize);
                    EventResult::Continue
                } else {
                    editor::apply_editor_event(editor, &action)
                }
            }
        },
        Action::PageUp => {
            if let Some(view) = read_view(&editor.mode) {
                let n = take_read_count_opt(editor).unwrap_or(1);
                move_read_cursor(editor, view, -((rv_page_step(editor, view) * n) as isize));
                EventResult::Continue
            } else {
                editor::apply_editor_event(editor, &action)
            }
        }
        Action::PageDown => {
            if let Some(view) = read_view(&editor.mode) {
                let n = take_read_count_opt(editor).unwrap_or(1);
                move_read_cursor(editor, view, (rv_page_step(editor, view) * n) as isize);
                EventResult::Continue
            } else {
                editor::apply_editor_event(editor, &action)
            }
        }
        Action::PageTop => match editor.mode {
            crate::state::EditorMode::Browse => browse::goto_top(editor),
            _ => {
                if let Some(view) = read_view(&editor.mode) {
                    set_read_cursor(editor, view, 0);
                    EventResult::Continue
                } else {
                    editor::apply_editor_event(editor, &action)
                }
            }
        },
        Action::PageBottom => match editor.mode {
            crate::state::EditorMode::Browse => browse::goto_bottom(editor),
            _ => {
                if let Some(view) = read_view(&editor.mode) {
                    let last = rv_line_count(editor, view).saturating_sub(1);
                    set_read_cursor(editor, view, last);
                    EventResult::Continue
                } else {
                    editor::apply_editor_event(editor, &action)
                }
            }
        },
        Action::GlideMove(motion) => {
            if let Some(view) = read_view(&editor.mode) {
                read_screen_motion(editor, view, motion)
            } else {
                editor::apply_editor_event(editor, &action)
            }
        }
        Action::Cancel => match editor.mode {
            crate::state::EditorMode::Browse => browse::cancel(editor),
            crate::state::EditorMode::DiffReview => {
                editor.diff_review = None;
                editor.enter_mode(editor.home_mode());
                EventResult::Continue
            }
            // Esc out of the commit prompt returns to the review, not home — the
            // approved hunks are still pending; only the message entry is aborted.
            crate::state::EditorMode::DiffCommitMsg => {
                editor.enter_mode(crate::state::EditorMode::DiffReview);
                EventResult::Continue
            }
            _ => editor::apply_editor_event(editor, &action),
        },
        Action::BrowseExpandOrOpen => browse::expand_or_open(editor),
        Action::BrowseCollapseOrParent => browse::collapse_or_parent(editor),
        Action::BrowseStartFilter => browse::start_filter(editor),
        Action::BrowseFilterChar(c) => browse::filter_char(editor, c),
        Action::BrowseFilterBackspace => browse::filter_backspace(editor),
        Action::CommandInput(c) => command::input_char(editor, c),
        Action::CommandBackspace => command::backspace(editor),
        Action::CommandMoveUp => command::move_up(editor),
        Action::CommandMoveDown => command::move_down(editor),
        Action::CommandComplete => command::complete(editor),
        Action::CommandExecute => command::execute(editor),

        _ => editor::apply_editor_event(editor, &action),
    }
}
