use crate::action::Action;
use crate::reducer::EventResult;
use crate::state::{EditorMode, EditorState, StatusKind};

/// Consume the accumulated Glide count (default 1) and clear the buffer.
fn take_count(editor: &mut EditorState) -> usize {
    let n = editor.glide_count.parse::<usize>().unwrap_or(1).max(1);
    editor.glide_count.clear();
    n
}

/// Consume the Glide count, preserving "was a count typed?" as `Some`/`None`.
fn take_count_opt(editor: &mut EditorState) -> Option<usize> {
    let n = if editor.glide_count.is_empty() {
        None
    } else {
        editor.glide_count.parse::<usize>().ok().filter(|&n| n >= 1)
    };
    editor.glide_count.clear();
    n
}

/// Resolve a motion and move the cursor to its target (bare movement).
fn apply_glide_move(editor: &mut EditorState, motion: crate::glide::Motion) -> EventResult {
    editor.glide_prefix = None;
    let count = take_count_opt(editor);
    // Soft-wrap: bare j/k follow visual rows. Operators keep logical lines because
    // they go through apply_operator/resolve, which this branch does not touch.
    let tw = editor.text_display_width;
    if editor.soft_wrap
        && tw > 0
        && matches!(
            motion,
            crate::glide::Motion::Up | crate::glide::Motion::Down
        )
    {
        let n = count.unwrap_or(1);
        for _ in 0..n {
            match motion {
                crate::glide::Motion::Down => {
                    editor.cursor.move_down_visual(&editor.buffer.lines, tw)
                }
                crate::glide::Motion::Up => editor.cursor.move_up_visual(&editor.buffer.lines, tw),
                _ => unreachable!(),
            }
        }
        return EventResult::Continue;
    }
    let r = crate::glide::resolve(motion, count, editor);
    editor.cursor = r.cursor; // adopt goal-column state, not just (y, x)
    EventResult::Continue
}

/// Save using the same name-resolution rule as the Save dialog:
/// a non-empty name that differs from the current file → Save As; otherwise
/// save to the current file (or Save As when there is no current file yet).
/// Re-read `git diff` and refresh the review hunks in place, clamping the
/// cursor. Staged/committed hunks drop out of the working-tree diff, so this
/// shrinks the list after a stage. Keeps DiffReview mode (empty list renders a
/// "no changes" surface) rather than bouncing the user out.
fn reload_diff_review(editor: &mut EditorState, dir: &std::path::Path) {
    if let Ok(hunks) = crate::state::diff::load_git_diff(dir) {
        if let Some(dr) = editor.diff_review.as_mut() {
            dr.current = dr.current.min(hunks.len().saturating_sub(1));
            dr.hunks = hunks;
            dr.scroll = 0;
        }
    }
}

fn save_dispatch(editor: &mut EditorState, fname: &str) -> std::io::Result<()> {
    if !fname.is_empty()
        && editor
            .filename
            .as_ref()
            .map(|p| p.to_string_lossy().to_string())
            != Some(fname.to_string())
    {
        editor.save_as(fname)
    } else if editor.filename.is_some() {
        editor.save()
    } else {
        editor.save_as(fname)
    }
}

/// A failed save either asks a question or reports an error — never both.
///
/// "The parent directory does not exist" is the one failure the user can undo
/// from inside the editor, so it becomes a one-line offer instead of an error.
/// Every other failure (permission, read-only mount, a name that is a
/// directory) is nothing cozy can offer to fix, and stays an error.
///
/// ⚠️ The condition is recognised by **type**, not by matching the message —
/// see `file_io::missing_parent_of`.
fn offer_or_error(editor: &mut EditorState, e: std::io::Error, fname: &str, and_exit: bool) {
    if let Some(dir) = crate::file_io::missing_parent_of(&e) {
        editor.create_dir = Some(crate::file_io::CreateDirOffer {
            dir,
            fname: fname.to_string(),
            and_exit,
        });
        return;
    }
    crate::reducer::status::set_error(editor, &e.to_string());
}

pub fn apply_editor_event(editor: &mut EditorState, action: &Action) -> EventResult {
    match action {
        // Navigation
        // Navigation
        // Navigation
        Action::MoveUp => {
            if editor.mode == EditorMode::Glide {
                editor.pending_operator = None; // arrow = plain move, cancels a pending operator
                let n = take_count(editor);
                let tw = editor.text_display_width;
                let visual = editor.soft_wrap && tw > 0;
                for _ in 0..n {
                    if visual {
                        editor.cursor.move_up_visual(&editor.buffer.lines, tw);
                    } else {
                        editor.cursor.move_up(&editor.buffer.lines);
                    }
                }
            } else {
                let tw = editor.text_display_width;
                if editor.soft_wrap && tw > 0 {
                    editor.cursor.move_up_visual(&editor.buffer.lines, tw);
                } else {
                    editor.cursor.move_up(&editor.buffer.lines);
                }
            }
            EventResult::Continue
        }
        Action::MoveDown => {
            if editor.mode == EditorMode::Glide {
                editor.pending_operator = None;
                let n = take_count(editor);
                let tw = editor.text_display_width;
                let visual = editor.soft_wrap && tw > 0;
                for _ in 0..n {
                    if visual {
                        editor.cursor.move_down_visual(&editor.buffer.lines, tw);
                    } else {
                        editor.cursor.move_down(&editor.buffer.lines);
                    }
                }
            } else {
                let tw = editor.text_display_width;
                if editor.soft_wrap && tw > 0 {
                    editor.cursor.move_down_visual(&editor.buffer.lines, tw);
                } else {
                    editor.cursor.move_down(&editor.buffer.lines);
                }
            }
            EventResult::Continue
        }
        Action::MoveLeft => {
            if editor.mode == EditorMode::Glide {
                editor.pending_operator = None;
                let n = take_count(editor);
                for _ in 0..n {
                    editor.cursor.move_left(&editor.buffer.lines);
                }
            } else {
                editor.cursor.move_left(&editor.buffer.lines);
            }
            EventResult::Continue
        }
        Action::MoveRight => {
            if editor.mode == EditorMode::Glide {
                editor.pending_operator = None;
                let n = take_count(editor);
                for _ in 0..n {
                    editor.cursor.move_right(&editor.buffer.lines);
                }
            } else {
                editor.cursor.move_right(&editor.buffer.lines);
            }
            EventResult::Continue
        }
        Action::PageUp => crate::reducer::cursor::page_up(
            &mut editor.cursor,
            &editor.buffer.lines,
            editor.page_size,
        ),
        Action::PageDown => crate::reducer::cursor::page_down(
            &mut editor.cursor,
            &editor.buffer.lines,
            editor.page_size,
        ),
        // Glide motion engine: a motion resolves to a target. With a pending
        // operator it defines the span to act on; otherwise it moves the cursor.
        Action::GlideMove(m) => {
            editor.glide_prefix = None;
            editor.glide_find_pending = None;
            // Remember a to-char motion so `.`/`,` can replay it.
            if let Some(f) = m.as_find() {
                editor.last_find = Some(f);
            }
            let count = take_count_opt(editor);
            if let Some(op) = editor.pending_operator.take() {
                crate::reducer::operator::apply_operator(editor, op, *m, count)
            } else {
                let r = crate::glide::resolve(*m, count, editor);
                editor.cursor = r.cursor; // adopt goal-column state, not just (y, x)
                EventResult::Continue
            }
        }
        Action::SetOperator(op) => {
            editor.pending_operator = Some(*op);
            EventResult::Continue
        }
        Action::SetFindPending(kind) => {
            editor.glide_find_pending = Some(*kind);
            EventResult::Continue
        }
        Action::ToggleCase => {
            let n = take_count(editor);
            let y = editor.cursor.y;
            // Snapshot BEFORE mutating so the whole `[count]~` is one undo step.
            // (save_snapshot captures the current buffer; calling it after the
            // edit would record the already-toggled state and break undo.)
            if editor.cursor.x < editor.buffer.lines[y].len() {
                crate::reducer::helper::mark_modified(editor);
            }
            for _ in 0..n {
                let x = editor.cursor.x;
                let line = &editor.buffer.lines[y];
                if x >= line.len() {
                    break;
                }
                let ch = line[x..].chars().next().unwrap();
                let toggled: String = if ch.is_lowercase() {
                    ch.to_uppercase().collect()
                } else if ch.is_uppercase() {
                    ch.to_lowercase().collect()
                } else {
                    ch.to_string()
                };
                editor.buffer.lines[y].replace_range(x..x + ch.len_utf8(), &toggled);
                editor.cursor.x = (x + toggled.len()).min(editor.buffer.lines[y].len());
            }
            EventResult::Continue
        }
        Action::ClearOperator => {
            editor.pending_operator = None;
            editor.glide_prefix = None;
            editor.glide_find_pending = None;
            editor.glide_count.clear();
            EventResult::Continue
        }
        Action::ChangeToLineEnd => {
            let count = take_count_opt(editor);
            crate::reducer::operator::apply_operator(
                editor,
                crate::glide::Operator::Change,
                crate::glide::Motion::LineEnd,
                count,
            )
        }
        Action::YankLine => {
            let count = take_count_opt(editor);
            crate::reducer::operator::apply_operator(
                editor,
                crate::glide::Operator::Yank,
                crate::glide::Motion::CurrentLine,
                count,
            )
        }
        // PageTop/PageBottom remain as config-bindable ([keys]) aliases.
        Action::PageTop => apply_glide_move(editor, crate::glide::Motion::ScreenTop),
        Action::PageBottom => apply_glide_move(editor, crate::glide::Motion::ScreenBottom),
        Action::Home => {
            editor.glide_prefix = None;
            crate::reducer::cursor::move_home(&mut editor.cursor)
        }
        Action::End => {
            editor.glide_prefix = None; // mirror Home: don't leave a dangling `g` prefix
            crate::reducer::cursor::move_end(&mut editor.cursor, &editor.buffer.lines)
        }

        // Undo/Redo
        Action::Undo => {
            editor.undo();
            EventResult::Continue
        }
        Action::Redo => {
            editor.redo();
            EventResult::Continue
        }

        // Mode Switching
        Action::EnterMode(mode) => {
            editor.enter_mode(*mode);
            EventResult::Continue
        }

        // Execution
        Action::Save(fname) => {
            match save_dispatch(editor, fname) {
                Ok(_) => {
                    let display_name = editor
                        .filename
                        .as_ref()
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_else(|| fname.to_string());
                    // enter_mode() clears status_message, so switch to Edit FIRST,
                    // then set the success message so it survives into Edit mode.
                    editor.enter_mode(editor.home_mode());
                    crate::reducer::status::set_success(editor, "Saved", &display_name);
                }
                Err(e) => {
                    offer_or_error(editor, e, fname, false);
                }
            }
            EventResult::Continue
        }
        // Ctrl+X → Enter ("Save and Exit"): save first, exit only on success.
        // On failure (e.g. empty/invalid filename) stay in Quit mode and show the
        // error — the user can fix the name inline, or Ctrl+Q to discard / Esc to cancel.
        Action::SaveAndExit(fname) => match save_dispatch(editor, fname) {
            Ok(_) => EventResult::Exit,
            Err(e) => {
                offer_or_error(editor, e, fname, true);
                EventResult::Continue
            }
        },
        Action::Open(fname) => {
            let result = editor.open_file(fname);
            // enter_mode() clears status_message, so switch to the resting mode
            // FIRST, then set the outcome message so it survives the transition.
            editor.enter_mode(editor.home_mode());
            match result {
                Ok(_) => {
                    crate::reducer::status::set_success(editor, "Opened", fname);
                }
                Err(e) => {
                    crate::reducer::status::set_error(editor, &e.to_string());
                }
            }
            EventResult::Continue
        }

        // System
        Action::Quit => EventResult::Exit,
        Action::Cancel => {
            editor.glide_prefix = None;
            editor.mode = editor.home_mode();
            editor.status_message = None;
            editor.search_matches.clear();
            editor.search_current = 0;
            EventResult::Continue
        }
        Action::ReloadConfig => {
            use crate::state::Config;
            let config_path = Config::user_config_path(None);
            let config = match config_path.as_ref() {
                Some(path) if path.exists() => match Config::load_from_path(path) {
                    Ok(config) => config,
                    Err(e) => {
                        crate::reducer::status::set_error(editor, &e.to_string());
                        return EventResult::Continue;
                    }
                },
                _ => Config::load(),
            };
            editor.page_size = config.page_size;
            editor.shortcut_map = crate::shortcuts::build_shortcut_map(config.keys.as_ref());
            editor.config = config;
            crate::reducer::status::set_info(editor, "Config reloaded");
            EventResult::Continue
        }
        Action::ToggleLineNumbers => {
            let current = editor.line_numbers_visible();
            editor.show_line_numbers_runtime = Some(!current);
            let status = if !current { "on" } else { "off" };
            crate::reducer::status::set_info(editor, &format!("Line numbers: {}", status));
            EventResult::Continue
        }
        Action::ToggleWrap => {
            editor.soft_wrap = !editor.soft_wrap;
            let status = if editor.soft_wrap { "on" } else { "off" };
            crate::reducer::status::set_info(editor, &format!("Soft wrap: {}", status));
            EventResult::Continue
        }
        Action::ToggleFooter => {
            editor.footer_visible_runtime = !editor.footer_visible_runtime;
            let status = if editor.footer_visible_runtime {
                "on"
            } else {
                "off"
            };
            crate::reducer::status::set_info(editor, &format!("Footer: {}", status));
            EventResult::Continue
        }
        Action::ToggleMarkdownPreview => {
            if editor.mode == EditorMode::Markdown {
                editor.enter_mode(editor.home_mode());
            } else {
                editor.enter_mode(EditorMode::Markdown);
            }
            EventResult::Continue
        }
        Action::ToggleDiffReview => {
            if editor.mode == EditorMode::DiffReview {
                editor.diff_review = None;
                editor.enter_mode(editor.home_mode());
            } else {
                let dir = editor._working_dir.clone();
                match crate::state::diff::load_git_diff(&dir) {
                    Ok(hunks) if !hunks.is_empty() => {
                        editor.diff_review = Some(crate::state::DiffReviewState {
                            hunks,
                            current: 0,
                            scroll: 0,
                        });
                        editor.enter_mode(EditorMode::DiffReview);
                    }
                    Ok(_) => crate::reducer::status::set_info(
                        editor,
                        "No changes to review (git diff is empty)",
                    ),
                    Err(e) => {
                        crate::reducer::status::set_error(editor, &format!("git diff failed: {e}"))
                    }
                }
            }
            EventResult::Continue
        }
        Action::DiffToggleApprove => {
            let summary = editor.diff_review.as_mut().map(|dr| {
                if let Some(h) = dr.hunks.get_mut(dr.current) {
                    h.approved = !h.approved;
                }
                (dr.approved_count(), dr.hunks.len())
            });
            if let Some((approved, total)) = summary {
                crate::reducer::status::set_info(
                    editor,
                    &format!("Approved {approved}/{total} hunks"),
                );
            }
            EventResult::Continue
        }
        Action::DiffApproveAll => {
            // Toggle: approve everything, or clear when already all-approved.
            let summary = editor.diff_review.as_mut().map(|dr| {
                let all = !dr.hunks.is_empty() && dr.hunks.iter().all(|h| h.approved);
                for h in &mut dr.hunks {
                    h.approved = !all;
                }
                (dr.approved_count(), dr.hunks.len())
            });
            if let Some((approved, total)) = summary {
                crate::reducer::status::set_info(
                    editor,
                    &format!("Approved {approved}/{total} hunks"),
                );
            }
            EventResult::Continue
        }
        Action::DiffStageApproved => {
            let dir = editor._working_dir.clone();
            let hunks = editor.diff_review.as_ref().map(|dr| dr.hunks.clone());
            if let Some(hunks) = hunks {
                match crate::state::diff::stage_approved(&dir, &hunks) {
                    Ok(0) => crate::reducer::status::set_info(
                        editor,
                        "No approved hunks to stage (Space to approve, a for all)",
                    ),
                    Ok(n) => {
                        // Staged hunks leave the working-tree `git diff`, so reload
                        // to drop them from the review; the list shrinks naturally.
                        reload_diff_review(editor, &dir);
                        crate::reducer::status::set_info(
                            editor,
                            &format!("Staged {n} hunk(s) — commit them with git"),
                        );
                    }
                    Err(e) => {
                        crate::reducer::status::set_error(editor, &format!("git apply failed: {e}"))
                    }
                }
            }
            EventResult::Continue
        }
        Action::DiffCommitApproved => {
            // Standalone "review → commit" terminus: needs approved hunks to
            // commit. (Use `s` instead to stage for an external/git commit.)
            let approved = editor
                .diff_review
                .as_ref()
                .map(|dr| dr.approved_count())
                .unwrap_or(0);
            if approved == 0 {
                crate::reducer::status::set_info(
                    editor,
                    "Nothing approved to commit (Space/a to approve)",
                );
            } else {
                editor.enter_mode(EditorMode::DiffCommitMsg);
            }
            EventResult::Continue
        }
        Action::DiffCommitConfirm => {
            let dir = editor._working_dir.clone();
            let msg = editor.commit_msg_buffer.trim().to_string();
            if msg.is_empty() {
                crate::reducer::status::set_error(editor, "Commit message required");
                return EventResult::Continue; // stay in DiffCommitMsg
            }
            let hunks = editor.diff_review.as_ref().map(|dr| dr.hunks.clone());
            // Stage the approved hunks, then commit the index.
            let staged = hunks
                .as_ref()
                .map(|h| crate::state::diff::stage_approved(&dir, h))
                .unwrap_or(Ok(0));
            match staged {
                Ok(_) => match crate::state::diff::commit_index(&dir, &msg) {
                    Ok(sha) => {
                        reload_diff_review(editor, &dir);
                        editor.enter_mode(EditorMode::DiffReview);
                        crate::reducer::status::set_info(
                            editor,
                            &format!("Committed {sha} — P to push, or push with git"),
                        );
                    }
                    Err(e) => {
                        editor.enter_mode(EditorMode::DiffReview);
                        crate::reducer::status::set_error(
                            editor,
                            &format!("commit failed (changes are staged): {e}"),
                        );
                    }
                },
                Err(e) => {
                    editor.enter_mode(EditorMode::DiffReview);
                    crate::reducer::status::set_error(editor, &format!("git apply failed: {e}"));
                }
            }
            EventResult::Continue
        }
        Action::DiffPush => {
            let dir = editor._working_dir.clone();
            match crate::state::diff::push_current(&dir) {
                Ok(summary) => {
                    crate::reducer::status::set_info(editor, &format!("Pushed: {summary}"))
                }
                Err(e) => crate::reducer::status::set_error(editor, &format!("push failed: {e}")),
            }
            EventResult::Continue
        }
        Action::DeleteLine => {
            if editor.mode == EditorMode::Glide && !editor.glide_count.is_empty() {
                // Ndd: delete N lines from cursor as one undo snapshot
                let n = take_count(editor);
                editor.glide_prefix = None;
                crate::reducer::helper::mark_modified(editor);
                let y = editor.cursor.y;
                let end = (y + n).min(editor.buffer.lines.len());
                let removed = editor.buffer.lines[y..end].join("\n");
                crate::reducer::clipboard::set_register_linewise(editor, removed);
                editor.buffer.lines.drain(y..end);
                if editor.buffer.lines.is_empty() {
                    editor.buffer.lines.push(String::new());
                }
                if editor.cursor.y >= editor.buffer.lines.len() {
                    editor.cursor.y = editor.buffer.lines.len() - 1;
                }
                editor.cursor.x = 0;
                editor.set_status_message(
                    format!("{} lines deleted", n),
                    StatusKind::Success,
                    false,
                );
                EventResult::Continue
            } else {
                editor.glide_prefix = None;
                editor.glide_count.clear();
                crate::reducer::clipboard::cut_line(editor)
            }
        }

        // Enter (fallthrough — Edit mode newline; other modes handled in keymap/mod.rs)
        Action::Enter => EventResult::Continue,

        Action::GotoLine(n) => {
            let last = editor.buffer.lines.len().saturating_sub(1);
            editor.cursor.y = n.saturating_sub(1).min(last);
            editor.cursor.x = 0;
            editor.enter_mode(editor.home_mode());
            crate::reducer::status::set_info(editor, &format!("Jumped to line {}", n));
            EventResult::Continue
        }

        Action::GlideJoin => {
            let n = take_count(editor).saturating_sub(1).max(1);
            let lines = &mut editor.buffer.lines;
            let y = editor.cursor.y;
            if y + 1 < lines.len() {
                crate::reducer::helper::mark_modified(editor);
                for _ in 0..n {
                    let y = editor.cursor.y;
                    if y + 1 >= editor.buffer.lines.len() {
                        break;
                    }
                    let join_x = editor.buffer.lines[y].len();
                    let next = editor.buffer.lines.remove(y + 1);
                    let trimmed = next.trim_start();
                    if !trimmed.is_empty() {
                        editor.buffer.lines[y].push(' ');
                        editor.buffer.lines[y].push_str(trimmed);
                    }
                    editor.cursor.x = join_x;
                }
            }
            EventResult::Continue
        }
        // D = d$ : delete from the cursor to end of line, via the operator engine.
        Action::DeleteToLineEnd => {
            let count = take_count_opt(editor);
            crate::reducer::operator::apply_operator(
                editor,
                crate::glide::Operator::Delete,
                crate::glide::Motion::LineEnd,
                count,
            )
        }
        Action::PasteRegister(after) => crate::reducer::clipboard::paste_register(editor, *after),

        Action::SetGlidePrefix(c) => {
            editor.glide_prefix = *c;
            if c.is_none() {
                editor.glide_count.clear();
            }
            EventResult::Continue
        }
        Action::GlideDigit(c) => {
            editor.glide_count.push(*c);
            EventResult::Continue
        }
        Action::GlideInsert => {
            editor.enter_mode(EditorMode::Edit);
            EventResult::Continue
        }
        Action::GlideInsertLineStart => {
            editor.cursor.move_line_start();
            editor.cursor.x = crate::state::cursor::first_non_whitespace_byte(
                &editor.buffer.lines[editor.cursor.y],
            );
            editor.enter_mode(EditorMode::Edit);
            EventResult::Continue
        }
        Action::GlideAppend => {
            editor.cursor.move_right(&editor.buffer.lines);
            editor.enter_mode(EditorMode::Edit);
            EventResult::Continue
        }
        Action::GlideAppendEnd => {
            editor.cursor.move_line_end(&editor.buffer.lines);
            editor.enter_mode(EditorMode::Edit);
            EventResult::Continue
        }
        Action::GlideOpenLine => {
            editor.save_snapshot();
            let y = editor.cursor.y;
            editor.buffer.lines.insert(y + 1, String::new());
            editor.cursor.y = y + 1;
            editor.cursor.x = 0;
            editor.modified = true;
            editor.enter_mode(EditorMode::Edit);
            EventResult::Continue
        }
        Action::GlideOpenLineAbove => {
            editor.save_snapshot();
            let y = editor.cursor.y;
            editor.buffer.lines.insert(y, String::new());
            editor.cursor.x = 0;
            editor.modified = true;
            editor.enter_mode(EditorMode::Edit);
            EventResult::Continue
        }

        _ => EventResult::Continue,
    }
}
