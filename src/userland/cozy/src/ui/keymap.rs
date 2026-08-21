use crate::action::Action;
use crate::glide::{FindKind, Motion, Operator};
use crate::shortcuts::EditorAction;
use crate::state::key::{KeyCode, KeyModifiers};
use crate::state::{EditorMode, EditorState};

/// Map a Glide key to a bare motion. Shared by plain movement and operator
/// operands (`w` alone moves; `dw` deletes over the same motion).
fn glide_motion(code: KeyCode) -> Option<Motion> {
    Some(match code {
        KeyCode::Char('h') => Motion::Left,
        KeyCode::Char('j') => Motion::Down,
        KeyCode::Char('k') => Motion::Up,
        KeyCode::Char('l') => Motion::Right,
        KeyCode::Char('w') => Motion::WordForward,
        KeyCode::Char('b') => Motion::WordBackward,
        KeyCode::Char('e') => Motion::WordEnd,
        KeyCode::Char('W') => Motion::WordForwardBig,
        KeyCode::Char('B') => Motion::WordBackwardBig,
        KeyCode::Char('E') => Motion::WordEndBig,
        KeyCode::Char('0') => Motion::LineStart,
        KeyCode::Char('^') => Motion::LineStartNonBlank,
        KeyCode::Char('$') => Motion::LineEnd,
        KeyCode::Char('G') => Motion::FileBottom,
        KeyCode::Char('H') => Motion::ScreenTop,
        KeyCode::Char('M') => Motion::ScreenMiddle,
        KeyCode::Char('L') => Motion::ScreenBottom,
        KeyCode::Char('+') => Motion::NextLineNonBlank,
        KeyCode::Char('-') => Motion::PrevLineNonBlank,
        _ => return None,
    })
}

/// True when the modifier combination can produce a printable character.
/// CONTROL and ALT combos are shortcut keys, not text input.
fn is_printable(modifiers: KeyModifiers) -> bool {
    !modifiers.contains(KeyModifiers::CONTROL) && !modifiers.contains(KeyModifiers::ALT)
}

pub struct Keymap;

impl Keymap {
    pub fn map_key_to_action(
        editor: &EditorState,
        code: KeyCode,
        modifiers: KeyModifiers,
    ) -> Option<Action> {
        // 1. Check shortcuts (Global) - handles most cases
        if let Some(editor_action) = editor.shortcut_map.get(&(code, modifiers)) {
            match editor_action {
                EditorAction::EnterSave => return Some(Action::EnterMode(EditorMode::Save)),
                EditorAction::EnterSaveAs => return Some(Action::EnterMode(EditorMode::Save)),
                EditorAction::EnterOpen => return Some(Action::EnterMode(EditorMode::Open)),
                EditorAction::EnterBrowse => return Some(Action::EnterMode(EditorMode::Browse)),
                EditorAction::EnterExit => return Some(Action::EnterMode(EditorMode::Quit)),
                EditorAction::ForceQuit => return Some(Action::Quit),
                EditorAction::EnterSearch => return Some(Action::EnterMode(EditorMode::Search)),
                EditorAction::EnterReplace => {
                    if editor.mode == EditorMode::Replace {
                        return Some(Action::ReplaceAll);
                    }
                    return Some(Action::EnterMode(EditorMode::Replace));
                }
                EditorAction::ReplaceAll => return Some(Action::ReplaceAll),
                EditorAction::EnterHelp => return Some(Action::EnterMode(EditorMode::Help)),
                EditorAction::ReloadConfig => return Some(Action::ReloadConfig),
                EditorAction::PageUp => return Some(Action::PageUp),
                EditorAction::PageDown => return Some(Action::PageDown),
                EditorAction::PageTop => return Some(Action::PageTop),
                EditorAction::PageBottom => return Some(Action::PageBottom),
                EditorAction::ToggleSearchMode => return Some(Action::ToggleSearchMode),
                EditorAction::Home => return Some(Action::Home),
                EditorAction::End => return Some(Action::End),
                EditorAction::Undo => return Some(Action::Undo),
                EditorAction::Redo => return Some(Action::Redo),
                EditorAction::MoveUp => {
                    return Some(if editor.mode == EditorMode::Command {
                        Action::CommandMoveUp
                    } else {
                        Action::MoveUp
                    });
                }
                EditorAction::MoveDown => {
                    return Some(if editor.mode == EditorMode::Command {
                        Action::CommandMoveDown
                    } else {
                        Action::MoveDown
                    });
                }
                EditorAction::MoveLeft => return Some(Action::MoveLeft),
                EditorAction::MoveRight => return Some(Action::MoveRight),
                EditorAction::Cancel => return Some(Action::Cancel),
                EditorAction::ToggleLineNumbers => return Some(Action::ToggleLineNumbers),
                EditorAction::ToggleWrap => return Some(Action::ToggleWrap),
                EditorAction::ToggleFooter => return Some(Action::ToggleFooter),
                EditorAction::DeleteLine => return Some(Action::DeleteLine),
                EditorAction::Enter => {
                    // ⚠️ A pending one-line question owns Enter — it is the *answer*,
                    // not the mode's Enter. Without this, a question raised from a
                    // prompt mode can never be answered: Save mode turns Enter into
                    // another `Action::Save`, which re-runs the save that just asked,
                    // and the offer sits there forever (measured — the reducer tests
                    // passed because they call `reduce(Action::Enter)` directly and
                    // never go through this mapping).
                    if editor.create_dir.is_some() || editor.recovery.is_some() {
                        return Some(Action::Enter);
                    }
                    // Context-dependent Enter handling
                    return match editor.mode {
                        EditorMode::Welcome => Some(Action::EnterMode(editor.home_mode())),
                        EditorMode::Search => Some(Action::SearchNext),
                        EditorMode::Replace => Some(Action::ReplaceCurrent),
                        EditorMode::Save => Some(Action::Save(editor.save_filename_buffer.clone())),
                        EditorMode::Open => Some(Action::Open(editor.open_filename_buffer.clone())),
                        EditorMode::Quit => {
                            Some(Action::SaveAndExit(editor.save_filename_buffer.clone()))
                        }
                        EditorMode::Goto => {
                            let n = editor.goto_line_buffer.parse::<usize>().unwrap_or(1);
                            Some(Action::GotoLine(n))
                        }
                        EditorMode::Command => Some(Action::CommandExecute),
                        // Enter expands a dir / opens a file; while filtering it
                        // confirms the filter (handled by the reducer).
                        EditorMode::Browse => Some(Action::BrowseExpandOrOpen),
                        // In the review, Enter is the commit accelerator; in the
                        // commit prompt it confirms the message.
                        EditorMode::DiffReview => Some(Action::DiffCommitApproved),
                        EditorMode::DiffCommitMsg => Some(Action::DiffCommitConfirm),
                        _ => Some(Action::Enter),
                    };
                }
                EditorAction::EnterGoto => return Some(Action::EnterMode(EditorMode::Goto)),
                EditorAction::Paste => return Some(Action::PasteFromClipboard),
                EditorAction::EnterGlide => return Some(Action::EnterMode(EditorMode::Glide)),
                EditorAction::ToggleMarkdownPreview => return Some(Action::ToggleMarkdownPreview),
                EditorAction::ToggleDiffReview => return Some(Action::ToggleDiffReview),
                EditorAction::EnterCommand => {
                    if editor.mode != EditorMode::Search && editor.mode != EditorMode::Replace {
                        return Some(Action::EnterMode(EditorMode::Command));
                    }
                }
            }
        }

        // 2. Mode-specific handling (only for cases not covered by shortcuts)
        match editor.mode {
            EditorMode::Glide => {
                // Awaiting the target char of a to-char motion (after a bare
                // `>`/`<`/`t`/`T` jump, or those same keys as an operand): the
                // next key is taken as that char.
                if let Some(kind) = editor.glide_find_pending {
                    return match code {
                        KeyCode::Char(c) => Some(Action::GlideMove(kind.motion(c))),
                        _ => Some(Action::ClearOperator),
                    };
                }
                // Multi-key motion prefix (gg). Resolves to a GlideMove the reducer
                // applies as movement, or as an operator operand if one is pending.
                if let Some(prefix) = editor.glide_prefix {
                    return match (prefix, code) {
                        ('g', KeyCode::Char('g')) => Some(Action::GlideMove(Motion::FileTop)),
                        _ => Some(Action::SetGlidePrefix(None)),
                    };
                }
                // Count digits ('0' is a digit only once a count has started).
                if let KeyCode::Char(c) = code {
                    if c.is_ascii_digit() && (c != '0' || !editor.glide_count.is_empty()) {
                        return Some(Action::GlideDigit(c));
                    }
                }
                // Operator pending (d/c/y typed): interpret the next key as the operand.
                if let Some(op) = editor.pending_operator {
                    return match code {
                        // Doubled operator key -> linewise current line(s): dd/yy/cc.
                        KeyCode::Char(c) if c == op.key() => {
                            Some(Action::GlideMove(Motion::CurrentLine))
                        }
                        KeyCode::Char('g') => Some(Action::SetGlidePrefix(Some('g'))), // dgg
                        // To-char operands. `>`/`<` find (onto the char): d>) d<( .
                        // `t`/`T` till (up to, before the char): dt) dT( .
                        KeyCode::Char('>') => Some(Action::SetFindPending(FindKind::Find)),
                        KeyCode::Char('<') => Some(Action::SetFindPending(FindKind::FindBack)),
                        KeyCode::Char('t') => Some(Action::SetFindPending(FindKind::Till)),
                        KeyCode::Char('T') => Some(Action::SetFindPending(FindKind::TillBack)),
                        KeyCode::Esc => Some(Action::ClearOperator),
                        _ => match glide_motion(code) {
                            Some(m) => Some(Action::GlideMove(m)),
                            None => Some(Action::ClearOperator),
                        },
                    };
                }
                // Bare motion.
                if let Some(m) = glide_motion(code) {
                    return Some(Action::GlideMove(m));
                }
                // Operators, edits, paste, insert-entry, mode switches.
                match code {
                    KeyCode::Char('g') => Some(Action::SetGlidePrefix(Some('g'))),
                    KeyCode::Char('d') => Some(Action::SetOperator(Operator::Delete)),
                    KeyCode::Char('c') => Some(Action::SetOperator(Operator::Change)),
                    KeyCode::Char('y') => Some(Action::SetOperator(Operator::Yank)),
                    KeyCode::Char('D') => Some(Action::DeleteToLineEnd),
                    KeyCode::Char('C') => Some(Action::ChangeToLineEnd),
                    KeyCode::Char('Y') => Some(Action::YankLine),
                    KeyCode::Char('J') => Some(Action::GlideJoin),
                    KeyCode::Char('x') => Some(Action::Delete),
                    KeyCode::Char('X') => Some(Action::Backspace),
                    KeyCode::Char('~') => Some(Action::ToggleCase),
                    KeyCode::Char('p') => Some(Action::PasteRegister(true)),
                    KeyCode::Char('P') => Some(Action::PasteRegister(false)),
                    KeyCode::Char('f') => Some(Action::EnterMode(EditorMode::Search)),
                    KeyCode::Char('r') => Some(Action::EnterMode(EditorMode::Replace)),
                    // Bare to-char jump: `>`/`<` land onto the char, `t`/`T` just
                    // before/after it (till). No operator -> the cursor moves.
                    KeyCode::Char('>') => Some(Action::SetFindPending(FindKind::Find)),
                    KeyCode::Char('<') => Some(Action::SetFindPending(FindKind::FindBack)),
                    KeyCode::Char('t') => Some(Action::SetFindPending(FindKind::Till)),
                    KeyCode::Char('T') => Some(Action::SetFindPending(FindKind::TillBack)),
                    // Repeat the last to-char jump: `.` forward, `,` backward
                    // (same char & find/till family; no-op until one has been used).
                    KeyCode::Char('.') => editor
                        .last_find
                        .map(|(k, c)| Action::GlideMove(k.forward().motion(c))),
                    KeyCode::Char(',') => editor
                        .last_find
                        .map(|(k, c)| Action::GlideMove(k.backward().motion(c))),
                    KeyCode::Char('i') => Some(Action::GlideInsert),
                    KeyCode::Char('I') => Some(Action::GlideInsertLineStart),
                    KeyCode::Char('a') => Some(Action::GlideAppend),
                    KeyCode::Char('A') => Some(Action::GlideAppendEnd),
                    KeyCode::Char('o') => Some(Action::GlideOpenLine),
                    KeyCode::Char('O') => Some(Action::GlideOpenLineAbove),
                    _ => None,
                }
            }
            EditorMode::Edit => match code {
                KeyCode::Left => Some(Action::MoveLeft),
                KeyCode::Right => Some(Action::MoveRight),
                KeyCode::Up => Some(Action::MoveUp),
                KeyCode::Down => Some(Action::MoveDown),
                KeyCode::Backspace => Some(Action::Backspace),
                KeyCode::Delete => Some(Action::Delete),
                KeyCode::Char(c) if is_printable(modifiers) => Some(Action::InsertChar(c)),
                _ => None,
            },
            EditorMode::Save | EditorMode::Open | EditorMode::Quit => match code {
                KeyCode::Left => Some(Action::MoveLeft),
                KeyCode::Right => Some(Action::MoveRight),
                KeyCode::Home => Some(Action::Home),
                KeyCode::End => Some(Action::End),
                KeyCode::Delete => Some(Action::Delete),
                KeyCode::Backspace => Some(Action::Backspace),
                KeyCode::Char(c) if is_printable(modifiers) => Some(Action::InsertChar(c)),
                _ => None,
            },
            EditorMode::Search => match code {
                KeyCode::Char('n') if modifiers.contains(KeyModifiers::CONTROL) => {
                    Some(Action::SearchNext)
                }
                KeyCode::Char('p') if modifiers.contains(KeyModifiers::CONTROL) => {
                    Some(Action::SearchPrevious)
                }
                KeyCode::Left => Some(Action::MoveLeft),
                KeyCode::Right => Some(Action::MoveRight),
                KeyCode::Home => Some(Action::Home),
                KeyCode::End => Some(Action::End),
                KeyCode::Delete => Some(Action::Delete),
                KeyCode::Backspace => Some(Action::Backspace),
                KeyCode::Char(c) if is_printable(modifiers) => Some(Action::InsertChar(c)),
                _ => None,
            },
            EditorMode::Replace => match code {
                KeyCode::Char('n') if modifiers.contains(KeyModifiers::CONTROL) => {
                    Some(Action::SearchNext)
                }
                KeyCode::Char('p') if modifiers.contains(KeyModifiers::CONTROL) => {
                    Some(Action::SearchPrevious)
                }
                KeyCode::Tab => Some(Action::SwitchFocus),
                KeyCode::Left => Some(Action::MoveLeft),
                KeyCode::Right => Some(Action::MoveRight),
                KeyCode::Home => Some(Action::Home),
                KeyCode::End => Some(Action::End),
                KeyCode::Delete => Some(Action::Delete),
                KeyCode::Backspace => Some(Action::Backspace),
                KeyCode::Char(c) if is_printable(modifiers) => Some(Action::InsertChar(c)),
                _ => None,
            },
            // Help shares the read-only navigation model with Markdown preview:
            // same keys (j/k, gg/G, H/M/L, counts, Page) drive the shared scroll view.
            EditorMode::Help | EditorMode::Markdown => {
                if let Some(prefix) = editor.glide_prefix {
                    return match (prefix, code) {
                        ('g', KeyCode::Char('g')) => Some(Action::GlideMove(Motion::FileTop)),
                        _ => Some(Action::SetGlidePrefix(None)),
                    };
                }
                if let KeyCode::Char(c) = code {
                    if c.is_ascii_digit() && (c != '0' || !editor.glide_count.is_empty()) {
                        return Some(Action::GlideDigit(c));
                    }
                }
                match code {
                    KeyCode::Char('j') => Some(Action::MoveDown),
                    KeyCode::Char('k') => Some(Action::MoveUp),
                    KeyCode::Char('g') => Some(Action::SetGlidePrefix(Some('g'))),
                    KeyCode::Char('G') => Some(Action::GlideMove(Motion::FileBottom)),
                    KeyCode::Char('H') | KeyCode::Char('h') => {
                        Some(Action::GlideMove(Motion::ScreenTop))
                    }
                    KeyCode::Char('M') | KeyCode::Char('m') => {
                        Some(Action::GlideMove(Motion::ScreenMiddle))
                    }
                    KeyCode::Char('L') | KeyCode::Char('l') => {
                        Some(Action::GlideMove(Motion::ScreenBottom))
                    }
                    // Pager keys reachable from any soft keyboard (no PgUp/PgDn on
                    // mobile): Space/f page forward, b pages back — the less/man idiom.
                    KeyCode::Char(' ') | KeyCode::Char('f') => Some(Action::PageDown),
                    KeyCode::Char('b') => Some(Action::PageUp),
                    _ => None,
                }
            }
            EditorMode::DiffReview => match code {
                KeyCode::Char('j') => Some(Action::MoveDown),
                KeyCode::Char('k') => Some(Action::MoveUp),
                KeyCode::Char(' ') => Some(Action::DiffToggleApprove),
                KeyCode::Char('a') => Some(Action::DiffApproveAll),
                KeyCode::Char('s') => Some(Action::DiffStageApproved),
                KeyCode::Char('c') => Some(Action::DiffCommitApproved),
                KeyCode::Char('P') => Some(Action::DiffPush),
                KeyCode::Char('q') => Some(Action::ToggleDiffReview),
                _ => None,
            },
            EditorMode::DiffCommitMsg => match code {
                KeyCode::Backspace => Some(Action::Backspace),
                KeyCode::Char(c) if is_printable(modifiers) => Some(Action::InsertChar(c)),
                _ => None,
            },
            EditorMode::Goto => match code {
                KeyCode::Backspace => Some(Action::Backspace),
                KeyCode::Char(c) if c.is_ascii_digit() => Some(Action::InsertChar(c)),
                _ => None,
            },
            EditorMode::Command => match code {
                KeyCode::Up => Some(Action::CommandMoveUp),
                KeyCode::Down => Some(Action::CommandMoveDown),
                KeyCode::Tab => Some(Action::CommandComplete),
                KeyCode::Backspace => Some(Action::CommandBackspace),
                KeyCode::Char('k') => Some(Action::CommandMoveUp),
                KeyCode::Char('j') => Some(Action::CommandMoveDown),
                KeyCode::Char(c) if is_printable(modifiers) => Some(Action::CommandInput(c)),
                _ => None,
            },
            EditorMode::Welcome => match code {
                KeyCode::Enter
                | KeyCode::Esc
                | KeyCode::Up
                | KeyCode::Down
                | KeyCode::Left
                | KeyCode::Right
                | KeyCode::Backspace
                | KeyCode::Delete
                | KeyCode::Char(' ') => Some(Action::EnterMode(editor.home_mode())),
                KeyCode::Char(_)
                    if modifiers.contains(KeyModifiers::CONTROL)
                        || modifiers.contains(KeyModifiers::ALT) =>
                {
                    Some(Action::EnterMode(editor.home_mode()))
                }
                _ => None,
            },
            EditorMode::Browse => {
                // Arrow keys + Enter come through the global shortcut map (MoveUp/
                // MoveDown/MoveLeft/MoveRight/Enter) and are interpreted per-mode in
                // the reducer; here we add the Glide-style hjkl keys and `/` filter.
                let filtering = editor
                    .browse_tree
                    .as_ref()
                    .map(|t| t.filtering)
                    .unwrap_or(false);
                if filtering {
                    match code {
                        KeyCode::Backspace => Some(Action::BrowseFilterBackspace),
                        KeyCode::Char(c) if is_printable(modifiers) => {
                            Some(Action::BrowseFilterChar(c))
                        }
                        _ => None,
                    }
                } else {
                    match code {
                        KeyCode::Char('j') => Some(Action::MoveDown),
                        KeyCode::Char('k') => Some(Action::MoveUp),
                        KeyCode::Char('l') => Some(Action::BrowseExpandOrOpen),
                        KeyCode::Char('h') => Some(Action::BrowseCollapseOrParent),
                        KeyCode::Char('G') => Some(Action::PageBottom),
                        KeyCode::Char('g') => Some(Action::PageTop),
                        KeyCode::Char('/') => Some(Action::BrowseStartFilter),
                        _ => None,
                    }
                }
            }
        }
    }
}
