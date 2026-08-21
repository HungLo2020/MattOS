use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::state::{EditorMode, EditorState, ReplaceFocus};

fn clamp_cursor(f: &Frame, x: u16, y: u16) -> (u16, u16) {
    let a = f.area();
    (
        x.min(a.right().saturating_sub(1)),
        y.min(a.bottom().saturating_sub(1)),
    )
}

// ── public entry points ───────────────────────────────────────────────────────

pub fn render_shortcuts(editor: &EditorState, f: &mut Frame, area: Rect) {
    if area.height == 0 {
        return;
    }
    f.render_widget(Paragraph::new("").style(shortcut_bar_style(editor)), area);
    match editor.mode {
        EditorMode::Welcome => {}
        EditorMode::Glide => render_glide_shortcuts(editor, f, area),
        EditorMode::Edit => render_edit_shortcuts(editor, f, area),
        EditorMode::Search => render_search_shortcuts(editor, f, area),
        EditorMode::Replace => render_replace_shortcuts(editor, f, area),
        EditorMode::Save => render_save_shortcuts(editor, f, area),
        EditorMode::Open => render_open_shortcuts(editor, f, area),
        EditorMode::Help => render_read_shortcuts(editor, f, area),
        EditorMode::Quit => render_quit_shortcuts(editor, f, area),
        EditorMode::Goto => render_goto_shortcuts(editor, f, area),
        EditorMode::Browse => render_browse_shortcuts(editor, f, area),
        EditorMode::Markdown => render_read_shortcuts(editor, f, area),
        EditorMode::DiffReview => render_diff_review_shortcuts(editor, f, area),
        EditorMode::DiffCommitMsg => render_diff_commit_shortcuts(editor, f, area),
        EditorMode::Command => render_command_shortcuts(editor, f, area),
    }
}

fn shortcut_bar_style(editor: &EditorState) -> Style {
    Style::default().bg(config_color(
        editor.config.footer_bg.as_deref(),
        Color::Rgb(34, 34, 38),
    ))
}

pub fn render_status_bar(editor: &EditorState, f: &mut Frame, area: Rect) {
    // A pending recovery owns the line: it is a question, and the user cannot
    // edit until they answer it. One line, two keys — vim's swap dialog is a
    // wall of text, and cozy's whole pitch is that you can type like nano.
    if let Some(recovery) = &editor.recovery {
        let offer = format!(
            " Unsaved changes from {} — [Enter] restore  [Esc] discard",
            crate::swap::describe_age(recovery.age)
        );
        let line = Line::from(vec![Span::raw(compose_status(
            &offer,
            "",
            area.width as usize,
        ))]);
        f.render_widget(Paragraph::new(line).style(status_bar_style(editor)), area);
        return;
    }

    // A save waiting on "create the directory?" owns the line for the same
    // reason: it is a question, and the answer has a side effect on disk.
    // ⚠️ The directory is spelled out in full — the whole point of asking is
    // that the user can see it is `.shh` before a typo becomes a directory.
    if let Some(offer) = &editor.create_dir {
        let text = format!(
            " Create directory {}? — [Enter] create  [Esc] cancel",
            offer.dir.display()
        );
        let line = Line::from(vec![Span::raw(compose_status(
            &text,
            "",
            area.width as usize,
        ))]);
        f.render_widget(Paragraph::new(line).style(status_bar_style(editor)), area);
        return;
    }

    let status = inline_status(editor);
    let narrow = area.width < 50;
    let wide = area.width >= 80;
    let row = editor.cursor.y + 1;
    // Two zones: `left` flush-left (name / mode + transient message),
    // `right` flush-right (cursor position). Modes without a position leave
    // the right zone empty and keep the whole line left-aligned.
    let (left, right) = match editor.mode {
        EditorMode::Welcome => (" cozy".to_string(), String::new()),
        EditorMode::Help => {
            let mut pend = String::new();
            pend.push_str(&editor.glide_count);
            if let Some(p) = editor.glide_prefix {
                pend.push(p);
            }
            let hint = if pend.is_empty() {
                String::new()
            } else {
                format!(" [{}]", pend)
            };
            let total = editor.help_rendered_line_count.max(1);
            let pos = (editor.help_cursor_line + 1).min(total);
            (
                format!(" Help{}{}", hint, status),
                format!("{}/{} ", pos, total),
            )
        }
        EditorMode::Edit => {
            let name = editor
                .filename
                .as_ref()
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                .unwrap_or("[No Name]")
                .to_string();
            (
                format!(" Edit: {}{}", name, status),
                if narrow {
                    String::new()
                } else {
                    position_str(editor, row)
                },
            )
        }
        EditorMode::Glide => {
            let mut pend = String::new();
            pend.push_str(&editor.glide_count);
            if let Some(op) = editor.pending_operator {
                pend.push(op.key());
            }
            if let Some(p) = editor.glide_prefix {
                pend.push(p);
            }
            let hint = if pend.is_empty() {
                String::new()
            } else {
                format!(" [{}]", pend)
            };
            (
                format!(" Glide{}{}", hint, status),
                if narrow {
                    String::new()
                } else {
                    position_str(editor, row)
                },
            )
        }
        EditorMode::Search => (
            format!(
                " Find:{}{}{}",
                search_mode_label(editor.search_mode, wide),
                match_count_str(editor),
                status
            ),
            String::new(),
        ),
        EditorMode::Replace => (
            format!(
                " Replace:{}{}{}",
                search_mode_label(editor.search_mode, wide),
                match_count_str(editor),
                status
            ),
            String::new(),
        ),
        EditorMode::Save => (format!(" Save{}", status), String::new()),
        EditorMode::Open => (format!(" Open{}", status), String::new()),
        EditorMode::Goto => (format!(" Goto: {}", editor.goto_line_buffer), String::new()),
        EditorMode::Quit => (format!(" Exit{}", status), String::new()),
        EditorMode::Command => (format!(" Command: {}", editor.command_query), String::new()),
        EditorMode::Browse => match editor.browse_tree.as_ref() {
            Some(tree) => {
                let left = if tree.filtering || !tree.filter.is_empty() {
                    format!(" Browse  /{}", tree.filter)
                } else {
                    let name = tree
                        .root
                        .file_name()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_else(|| tree.root.to_string_lossy().to_string());
                    format!(" Browse  {}", name)
                };
                let vis = tree.visible_nodes();
                let pos = vis
                    .iter()
                    .position(|&i| i == tree.selected)
                    .map(|p| p + 1)
                    .unwrap_or(0);
                (left, format!("{}/{} ", pos, vis.len()))
            }
            None => (" Browse".to_string(), String::new()),
        },
        EditorMode::Markdown => {
            let mut pend = String::new();
            pend.push_str(&editor.glide_count);
            if let Some(p) = editor.glide_prefix {
                pend.push(p);
            }
            let hint = if pend.is_empty() {
                String::new()
            } else {
                format!(" [{}]", pend)
            };
            let total = crate::ui::render::markdown::markdown_line_count(editor);
            let pos = (editor.markdown_cursor_line + 1).min(total.max(1));
            (
                format!(" Markdown{}{}", hint, status),
                format!("{}/{} ", pos, total.max(1)),
            )
        }
        EditorMode::DiffReview => {
            let (cur, total, approved) = editor
                .diff_review
                .as_ref()
                .map(|d| (d.current + 1, d.hunks.len(), d.approved_count()))
                .unwrap_or((0, 0, 0));
            (
                format!(" DiffReview {}/{} approved{}", approved, total, status),
                format!("hunk {}/{} ", cur, total.max(1)),
            )
        }
        EditorMode::DiffCommitMsg => {
            let approved = editor
                .diff_review
                .as_ref()
                .map(|d| d.approved_count())
                .unwrap_or(0);
            (
                format!(" Commit {} hunk(s){}", approved, status),
                String::new(),
            )
        }
    };
    let label = compose_status(&left, &right, area.width as usize);
    f.render_widget(Paragraph::new(label).style(status_bar_style(editor)), area);
}

fn status_bar_style(editor: &EditorState) -> Style {
    Style::default()
        .bg(config_color(
            editor.config.status_bar_bg.as_deref(),
            Color::DarkGray,
        ))
        .fg(config_color(
            editor.config.status_bar_fg.as_deref(),
            Color::White,
        ))
}

/// Right-zone cursor position, e.g. `L12 C5 ` (1-based line and display
/// column, VSCode-style; trailing space keeps it off the edge).
fn position_str(editor: &EditorState, row: usize) -> String {
    let col = editor
        .buffer
        .lines
        .get(editor.cursor.y)
        .map(|line| {
            let x = editor.cursor.x.min(line.len());
            UnicodeWidthStr::width(&line[..x]) + 1
        })
        .unwrap_or(1);
    format!("L{} C{} ", row, col)
}

/// Compose a two-zone status line: `left` flush-left, `right` flush-right,
/// padded to `width`. When the two would overlap, the left zone is truncated
/// so the position (right zone) always stays visible.
fn compose_status(left: &str, right: &str, width: usize) -> String {
    if right.is_empty() {
        return left.to_string();
    }
    let rw = UnicodeWidthStr::width(right);
    let avail = width.saturating_sub(rw + 1);
    let left = truncate_to_width(left, avail);
    let lw = UnicodeWidthStr::width(left.as_str());
    let pad = width.saturating_sub(lw + rw);
    format!("{}{}{}", left, " ".repeat(pad), right)
}

/// Truncate `s` to at most `max` display columns, respecting char boundaries.
fn truncate_to_width(s: &str, max: usize) -> String {
    if UnicodeWidthStr::width(s) <= max {
        return s.to_string();
    }
    let mut out = String::new();
    let mut w = 0;
    for ch in s.chars() {
        let cw = UnicodeWidthChar::width(ch).unwrap_or(0);
        if w + cw > max {
            break;
        }
        out.push(ch);
        w += cw;
    }
    out
}

// ── key label styling ─────────────────────────────────────────────────────────

/// The shortcut key itself, as a bold accent (no background) so it stands out
/// from the gutter and status bar instead of sharing their gray fill.
fn key_span(editor: &EditorState, key: &str) -> Span<'static> {
    Span::styled(
        key.to_string(),
        Style::default()
            .fg(config_color(
                editor.config.footer_key_fg.as_deref(),
                Color::Cyan,
            ))
            .add_modifier(Modifier::BOLD),
    )
}

/// The dimmed description that follows a key.
fn desc_span(editor: &EditorState, desc: &str) -> Span<'static> {
    Span::styled(
        format!(" {}", desc),
        Style::default().fg(config_color(
            editor.config.footer_fg.as_deref(),
            Color::Gray,
        )),
    )
}

fn shortcut_line(editor: &EditorState, narrow: bool, pairs: &[(&str, &str)]) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    for (i, (key, desc)) in pairs.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw("   "));
        }
        let key = if narrow {
            key.replace("Ctrl+", "^")
        } else {
            key.to_string()
        };
        spans.push(key_span(editor, &key));
        spans.push(desc_span(editor, desc));
    }
    Line::from(spans)
}

fn compact_line(editor: &EditorState, pairs: &[(&str, &str)]) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    for (i, (key, desc)) in pairs.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw(" "));
        }
        spans.push(key_span(editor, key));
        spans.push(desc_span(editor, desc));
    }
    Line::from(spans)
}

fn config_color(value: Option<&str>, fallback: Color) -> Color {
    value.and_then(parse_color).unwrap_or(fallback)
}

fn parse_color(value: &str) -> Option<Color> {
    let value = value.trim();
    if let Some(hex) = value.strip_prefix('#') {
        return parse_hex_color(hex);
    }

    match value.to_ascii_lowercase().as_str() {
        "black" => Some(Color::Black),
        "red" => Some(Color::Red),
        "green" => Some(Color::Green),
        "yellow" => Some(Color::Yellow),
        "blue" => Some(Color::Blue),
        "magenta" => Some(Color::Magenta),
        "cyan" => Some(Color::Cyan),
        "gray" | "grey" => Some(Color::Gray),
        "darkgray" | "darkgrey" => Some(Color::DarkGray),
        "white" => Some(Color::White),
        "lightred" => Some(Color::LightRed),
        "lightgreen" => Some(Color::LightGreen),
        "lightyellow" => Some(Color::LightYellow),
        "lightblue" => Some(Color::LightBlue),
        "lightmagenta" => Some(Color::LightMagenta),
        "lightcyan" => Some(Color::LightCyan),
        _ => None,
    }
}

fn parse_hex_color(hex: &str) -> Option<Color> {
    if hex.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some(Color::Rgb(r, g, b))
}

// ── layout helpers ────────────────────────────────────────────────────────────

fn narrow_layout(area: Rect) -> std::rc::Rc<[Rect]> {
    Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(area)
}

fn wide_layout(area: Rect) -> std::rc::Rc<[Rect]> {
    Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(area)
}

fn row(area: Rect, offset: u16) -> Rect {
    Rect::new(area.x, area.y + offset, area.width, 1)
}

// ── misc helpers ──────────────────────────────────────────────────────────────

fn search_mode_label(mode: crate::state::SearchMode, wide: bool) -> &'static str {
    use crate::state::SearchMode::*;
    match (mode, wide) {
        (MatchCase, false) => "MC",
        (MatchCase, true) => "Case",
        (Regex, false) => "Rx",
        (Regex, true) => "Regex",
        (ByWord, false) => "Wrd",
        (ByWord, true) => "Word",
    }
}

fn match_count_str(editor: &EditorState) -> String {
    if editor.search_matches.is_empty() {
        if editor.search_buffer.is_empty() {
            String::new()
        } else {
            " 0".to_string()
        }
    } else {
        format!(
            " {}/{}",
            editor.search_current + 1,
            editor.search_matches.len()
        )
    }
}

fn inline_status(editor: &EditorState) -> String {
    if let Some(msg) = &editor.status_message {
        if editor.should_show_status() {
            return format!("   {}", msg);
        }
    }
    String::new()
}

// ── shortcut renderers ────────────────────────────────────────────────────────

fn render_glide_shortcuts(editor: &EditorState, f: &mut Frame, area: Rect) {
    if area.width < 50 {
        if area.height <= 1 {
            f.render_widget(
                Paragraph::new(compact_line(
                    editor,
                    &[("Esc", "Ret"), ("i", "Edit"), ("dd", "Del")],
                )),
                row(area, 0),
            );
            return;
        }
        let layout = narrow_layout(area);
        f.render_widget(
            Paragraph::new(compact_line(
                editor,
                &[("hjkl", "Move"), ("w/b", "Wrd"), ("0/$", "Ln")],
            )),
            layout[0],
        );
        f.render_widget(
            Paragraph::new(compact_line(
                editor,
                &[("gg/G", "Fil"), ("H/M/L", "Scr"), ("i/a", "Ins")],
            )),
            layout[1],
        );
        f.render_widget(
            Paragraph::new(compact_line(
                editor,
                &[("Esc", "Ret"), ("x", "Del"), ("dd", "DelLn")],
            )),
            layout[2],
        );
        f.render_widget(
            Paragraph::new(compact_line(
                editor,
                &[("f/r", "Srch"), ("+/-", "Ln↕"), (".", "Rep")],
            )),
            layout[3],
        );
    } else {
        let narrow = area.width < 80;
        let layout = wide_layout(area);
        f.render_widget(
            Paragraph::new(shortcut_line(
                editor,
                narrow,
                &[
                    ("hjkl/←↓↑→", "Move"),
                    ("w/b", "Word"),
                    ("0/$", "Line"),
                    ("gg/G", "File Top/Bot"),
                    ("H/M/L", "Scr Hi/Mid/Low"),
                ],
            )),
            layout[0],
        );
        f.render_widget(
            Paragraph::new(shortcut_line(
                editor,
                narrow,
                &[
                    ("i/I/a/A", "Edit"),
                    ("x/X", "Del Char"),
                    ("dd/D", "Del Line/End"),
                    ("f/r", "Find/Replace"),
                    ("Esc", "Return"),
                ],
            )),
            layout[1],
        );
    }
}

/// 幅が足りない段で **`^X Exit` の枠を `^Q Quit` に譲る**かどうか。
///
/// `^X`(`EnterExit`) と `^S`(`EnterSave`) は**同じ `File:` プロンプトに着く**
/// （`EditorState::enter_mode` が `Save | Quit` を同じ腕で処理する）。違うのは Enter を
/// 押した後だけなので、狭い footer に両方載せると **1 枠が実質重複**する。
/// 一方 `^Q`(`ForceQuit`) は**保存せず出る**＝ここでしか辿り着けない唯一の出口で、
/// しかも**電話の中で一番要る**（シェルへ落ちるのが容易でない）。
/// ∴ 枠が足りないときは `^X` ではなく `^Q` を見せる。
///
/// ⚠️ **鍵は消していない。** `^X` は今までどおり効く —— 案内から外れるだけ。
/// ⚠️ 4 行入る段（幅 50 未満・高さ 2 行以上）は**両方載る**ので入れ替えない。
fn quit_takes_the_exit_slot(area: Rect) -> bool {
    area.width < 80
}

fn render_edit_shortcuts(editor: &EditorState, f: &mut Frame, area: Rect) {
    if area.width < 50 {
        if area.height <= 1 {
            // 1 行しか無い ＝ 3 枠。`^X` を `^Q` に譲る（上の doc）。
            f.render_widget(
                Paragraph::new(compact_line(
                    editor,
                    &[("^S", "Save"), ("^Q", "Quit"), ("^D", "Md")],
                )),
                row(area, 0),
            );
            return;
        }
        let layout = narrow_layout(area);
        f.render_widget(
            Paragraph::new(compact_line(
                editor,
                &[("^S", "Save"), ("^B", "Browse"), ("^X", "Exit")],
            )),
            layout[0],
        );
        f.render_widget(
            Paragraph::new(compact_line(
                editor,
                &[("^F", "Find"), ("^R", "Repl"), ("^H", "Help")],
            )),
            layout[1],
        );
        f.render_widget(
            Paragraph::new(compact_line(
                editor,
                &[("^K", "Cut"), ("^J", "Jump"), ("^G", "Glide")],
            )),
            layout[2],
        );
        f.render_widget(
            Paragraph::new(compact_line(
                editor,
                &[("^Z", "Undo"), ("^Q", "Quit"), ("^D", "Md")],
            )),
            layout[3],
        );
    } else {
        let narrow = area.width < 80;
        let layout = wide_layout(area);
        // 狭いときは `Ctrl+X Exit` の枠を `Ctrl+Q Quit` に譲る（`quit_takes_the_exit_slot`）。
        let third = if quit_takes_the_exit_slot(area) {
            ("Ctrl+Q", "Quit")
        } else {
            ("Ctrl+X", "Exit")
        };
        f.render_widget(
            Paragraph::new(shortcut_line(
                editor,
                narrow,
                &[
                    ("Ctrl+S", "Save"),
                    ("Ctrl+B", "Browse"),
                    third,
                    ("Ctrl+F", "Find"),
                    ("Ctrl+R", "Replace"),
                ],
            )),
            layout[0],
        );
        f.render_widget(
            Paragraph::new(shortcut_line(
                editor,
                narrow,
                &[
                    ("Ctrl+H", "Help"),
                    ("Ctrl+K", "Cut Line"),
                    ("Ctrl+J", "Jump"),
                    ("Ctrl+G", "Glide"),
                    ("Ctrl+Z", "Undo"),
                ],
            )),
            layout[1],
        );
    }
}

fn render_browse_shortcuts(editor: &EditorState, f: &mut Frame, area: Rect) {
    let filtering = editor
        .browse_tree
        .as_ref()
        .map(|t| t.filtering)
        .unwrap_or(false);
    if area.width < 50 {
        if area.height <= 1 {
            let pairs = if filtering {
                &[("Type", "Fltr"), ("Enter", "OK"), ("Esc", "Clr")][..]
            } else {
                &[("↑↓", "Move"), ("→", "Open"), ("Esc", "Exit")][..]
            };
            f.render_widget(Paragraph::new(compact_line(editor, pairs)), row(area, 0));
            return;
        }
        let layout = narrow_layout(area);
        if filtering {
            f.render_widget(
                Paragraph::new(compact_line(editor, &[("Type", "Filter"), ("Enter", "OK")])),
                layout[0],
            );
            f.render_widget(
                Paragraph::new(compact_line(editor, &[("Esc", "Clear")])),
                layout[1],
            );
        } else {
            f.render_widget(
                Paragraph::new(compact_line(
                    editor,
                    &[("↑↓", "Move"), ("→", "Open"), ("←", "Back")],
                )),
                layout[0],
            );
            f.render_widget(
                Paragraph::new(compact_line(editor, &[("/", "Filter"), ("Esc", "Exit")])),
                layout[1],
            );
        }
    } else {
        let narrow = area.width < 80;
        let layout = wide_layout(area);
        if filtering {
            let line = shortcut_line(
                editor,
                narrow,
                &[("Type", "Filter"), ("Enter", "Confirm"), ("Esc", "Clear")],
            );
            f.render_widget(Paragraph::new(line), layout[0]);
        } else {
            f.render_widget(
                Paragraph::new(shortcut_line(
                    editor,
                    narrow,
                    &[("↑↓", "Move"), ("→", "Open"), ("←", "Back")],
                )),
                layout[0],
            );
            f.render_widget(
                Paragraph::new(shortcut_line(
                    editor,
                    narrow,
                    &[
                        ("j/k", "Move"),
                        ("l", "Open"),
                        ("h", "Back"),
                        ("/", "Filter"),
                        ("Esc", "Exit"),
                    ],
                )),
                layout[1],
            );
        }
    }
}

/// Shortcut footer shared by the read-only scrolling views (Markdown preview and
/// Help), so both advertise the same navigation keys.
fn render_read_shortcuts(editor: &EditorState, f: &mut Frame, area: Rect) {
    if area.width < 50 {
        if area.height <= 1 {
            f.render_widget(
                Paragraph::new(compact_line(
                    editor,
                    &[("jk", "Move"), ("Spc/b", "Page"), ("Esc", "Ret")],
                )),
                row(area, 0),
            );
            return;
        }
        let layout = narrow_layout(area);
        f.render_widget(
            Paragraph::new(compact_line(
                editor,
                &[("jk/↑↓", "Move"), ("gg/G", "Top/Bot")],
            )),
            layout[0],
        );
        f.render_widget(
            Paragraph::new(compact_line(editor, &[("H/M/L", "Screen")])),
            layout[1],
        );
        f.render_widget(
            Paragraph::new(compact_line(editor, &[("Spc/b · PgUp/Dn", "Page")])),
            layout[2],
        );
        f.render_widget(
            Paragraph::new(compact_line(editor, &[("Esc", "Return")])),
            layout[3],
        );
    } else {
        let narrow = area.width < 80;
        let layout = wide_layout(area);
        f.render_widget(
            Paragraph::new(shortcut_line(
                editor,
                narrow,
                &[
                    ("jk/↑↓", "Move"),
                    ("gg/G", "Top/Bottom"),
                    ("H/M/L", "Scr Hi/Mid/Low"),
                ],
            )),
            layout[0],
        );
        f.render_widget(
            Paragraph::new(shortcut_line(
                editor,
                narrow,
                &[("Spc/f/b · PgUp/PgDn", "Page"), ("Esc", "Return")],
            )),
            layout[1],
        );
    }
}

fn render_diff_commit_shortcuts(editor: &EditorState, f: &mut Frame, area: Rect) {
    // Append-only commit-message prompt (no left/right cursor, like Goto): show
    // the message and park the caret at its end; a hint line below.
    let buf = &editor.commit_msg_buffer;
    let prefix = "Commit message: ";
    f.render_widget(Paragraph::new(format!("{prefix}{buf}")), row(area, 0));
    if area.height > 1 {
        f.render_widget(
            Paragraph::new(shortcut_line(
                editor,
                area.width < 80,
                &[("Enter", "Commit"), ("Esc", "Cancel")],
            )),
            row(area, 1),
        );
    }
    let col = area.x
        + UnicodeWidthStr::width(prefix) as u16
        + UnicodeWidthStr::width(buf.as_str()) as u16;
    let (cx, cy) = clamp_cursor(f, col, area.y);
    f.set_cursor_position((cx, cy));
}

fn render_diff_review_shortcuts(editor: &EditorState, f: &mut Frame, area: Rect) {
    if area.width < 50 {
        f.render_widget(
            Paragraph::new(compact_line(
                editor,
                &[
                    ("jk", "Hunk"),
                    ("Spc", "Apprv"),
                    ("a", "All"),
                    ("s", "Stage"),
                    ("c", "Commit"),
                    ("P", "Push"),
                    ("Esc", "Close"),
                ],
            )),
            row(area, 0),
        );
    } else {
        let narrow = area.width < 80;
        f.render_widget(
            Paragraph::new(shortcut_line(
                editor,
                narrow,
                &[
                    ("jk/↑↓", "Hunk"),
                    ("Space", "Approve"),
                    ("a", "Approve all"),
                    ("s", "Stage"),
                    ("c", "Commit"),
                    ("P", "Push"),
                    ("q/Esc", "Close"),
                ],
            )),
            row(area, 0),
        );
    }
}

fn render_command_shortcuts(editor: &EditorState, f: &mut Frame, area: Rect) {
    let query_prefix = "Command: ";
    f.render_widget(
        Paragraph::new(format!("{}{}", query_prefix, editor.command_query)),
        row(area, 0),
    );

    let matches = crate::commands::filtered_commands(&editor.command_query);
    let action_rows = if area.width < 50 {
        if area.height >= 4 { 2 } else { 0 }
    } else if area.height >= 3 {
        1
    } else {
        0
    };
    let rows = area.height.saturating_sub(1 + action_rows).min(8) as usize;
    if matches.is_empty() {
        if area.height > 1 {
            f.render_widget(Paragraph::new("  No commands"), row(area, 1));
        }
    } else {
        let start = editor
            .command_selected
            .saturating_sub(rows.saturating_sub(1));
        for (row_idx, command) in matches.iter().skip(start).take(rows).enumerate() {
            let index = start + row_idx;
            let marker = if index == editor.command_selected {
                "> "
            } else {
                "  "
            };
            f.render_widget(
                Paragraph::new(format!("{}{}", marker, command.label)),
                row(area, row_idx as u16 + 1),
            );
        }
    }

    if area.width < 50 {
        if action_rows == 2 {
            f.render_widget(
                Paragraph::new(compact_line(
                    editor,
                    &[("↑↓/jk", "Select"), ("Enter", "Run")],
                )),
                row(area, area.height - 2),
            );
            f.render_widget(
                Paragraph::new(compact_line(
                    editor,
                    &[("Tab", "Complete"), ("Esc", "Return")],
                )),
                row(area, area.height - 1),
            );
        }
    } else if action_rows == 1 {
        f.render_widget(
            Paragraph::new(shortcut_line(
                editor,
                area.width < 80,
                &[
                    ("↑↓/jk", "Select"),
                    ("Enter", "Run"),
                    ("Tab", "Complete"),
                    ("Esc", "Return"),
                ],
            )),
            row(area, area.height - 1),
        );
    }

    let col = area.x
        + UnicodeWidthStr::width(query_prefix) as u16
        + UnicodeWidthStr::width(editor.command_query.as_str()) as u16;
    let (cx, cy) = clamp_cursor(f, col, area.y);
    f.set_cursor_position((cx, cy));
}

fn render_search_shortcuts(editor: &EditorState, f: &mut Frame, area: Rect) {
    let prefix = "Find: [";
    let line_find = format!("{}{}]", prefix, editor.search_buffer);
    let buf = &editor.search_buffer;
    let before = &buf[..editor.search_cursor.min(buf.len())];
    let col =
        area.x + UnicodeWidthStr::width(prefix) as u16 + UnicodeWidthStr::width(before) as u16;

    f.render_widget(Paragraph::new(line_find), row(area, 0));
    if area.height <= 1 {
        let (cx, cy) = clamp_cursor(f, col, area.y);
        f.set_cursor_position((cx, cy));
        return;
    }
    let layout = wide_layout(area);
    if area.width < 50 {
        f.render_widget(
            Paragraph::new(compact_line(
                editor,
                &[("^N", "Next"), ("^P", "Prev"), ("^T", "Tog")],
            )),
            layout[1],
        );
    } else {
        let narrow = area.width < 80;
        f.render_widget(
            Paragraph::new(shortcut_line(
                editor,
                narrow,
                &[
                    ("Ctrl+N", "Next"),
                    ("Ctrl+P", "Prev"),
                    ("Ctrl+T", "Toggle"),
                    ("Esc", "Return"),
                ],
            )),
            layout[1],
        );
    }
    let (cx, cy) = clamp_cursor(f, col, area.y);
    f.set_cursor_position((cx, cy));
}

fn render_replace_shortcuts(editor: &EditorState, f: &mut Frame, area: Rect) {
    // ASCII marker (width 1) instead of ▶ (U+25B6): ambiguous-width glyph
    // throws off cursor-column math on CJK terminals.
    let (q_mark, r_mark) = match editor.replace_focus {
        ReplaceFocus::Query => ("> ", "  "),
        ReplaceFocus::Replace => ("  ", "> "),
    };
    let sc = editor.search_cursor;

    if area.width < 50 {
        if area.height <= 3 {
            f.render_widget(
                Paragraph::new(format!("{}Find: [{}]", q_mark, editor.search_buffer)),
                row(area, 0),
            );
            if area.height > 1 {
                f.render_widget(
                    Paragraph::new(format!("{}Replace: [{}]", r_mark, editor.replace_buffer)),
                    row(area, 1),
                );
            }
            if area.height > 2 {
                f.render_widget(
                    Paragraph::new(compact_line(
                        editor,
                        &[("Tab", "Sw"), ("Enter", "Repl"), ("^R", "All")],
                    )),
                    row(area, 2),
                );
            }
            let (col, cursor_row) = match editor.replace_focus {
                ReplaceFocus::Query => {
                    let prefix = format!("{}Find: [", q_mark);
                    let before = &editor.search_buffer[..sc.min(editor.search_buffer.len())];
                    (
                        area.x
                            + UnicodeWidthStr::width(prefix.as_str()) as u16
                            + UnicodeWidthStr::width(before) as u16,
                        area.y,
                    )
                }
                ReplaceFocus::Replace => {
                    let prefix = format!("{}Replace: [", r_mark);
                    let before = &editor.replace_buffer[..sc.min(editor.replace_buffer.len())];
                    (
                        area.x
                            + UnicodeWidthStr::width(prefix.as_str()) as u16
                            + UnicodeWidthStr::width(before) as u16,
                        area.y + 1,
                    )
                }
            };
            let (cx, cy) = clamp_cursor(f, col, cursor_row);
            f.set_cursor_position((cx, cy));
            return;
        }
        let layout = narrow_layout(area);
        f.render_widget(
            Paragraph::new(format!("{}Find: [{}]", q_mark, editor.search_buffer)),
            layout[0],
        );
        f.render_widget(
            Paragraph::new(format!("{}Replace: [{}]", r_mark, editor.replace_buffer)),
            layout[1],
        );
        f.render_widget(
            Paragraph::new(compact_line(
                editor,
                &[("Tab", "Sw"), ("^N", "Next"), ("^P", "Prev")],
            )),
            layout[2],
        );
        f.render_widget(
            Paragraph::new(compact_line(
                editor,
                &[("Enter", "Repl"), ("^R", "All"), ("^T", "Tog")],
            )),
            layout[3],
        );
        let (col, row) = match editor.replace_focus {
            ReplaceFocus::Query => {
                let prefix = format!("{}Find: [", q_mark);
                let before = &editor.search_buffer[..sc.min(editor.search_buffer.len())];
                (
                    area.x
                        + UnicodeWidthStr::width(prefix.as_str()) as u16
                        + UnicodeWidthStr::width(before) as u16,
                    area.y,
                )
            }
            ReplaceFocus::Replace => {
                let prefix = format!("{}Replace: [", r_mark);
                let before = &editor.replace_buffer[..sc.min(editor.replace_buffer.len())];
                (
                    area.x
                        + UnicodeWidthStr::width(prefix.as_str()) as u16
                        + UnicodeWidthStr::width(before) as u16,
                    area.y + 1,
                )
            }
        };
        let (cx, cy) = clamp_cursor(f, col, row);
        f.set_cursor_position((cx, cy));
    } else {
        let fields = format!(
            "{}Find: [{}]   {}Replace: [{}]",
            q_mark, editor.search_buffer, r_mark, editor.replace_buffer
        );
        // Tab swaps the two fields above and Ctrl+T toggles search mode, so
        // both hints live on the field row rather than crowding the action
        // row below (which otherwise overflows and clips "Esc Return").
        let line_fields = Line::from(vec![
            Span::raw(fields),
            Span::raw("   "),
            key_span(editor, "Tab"),
            desc_span(editor, "Switch"),
            Span::raw("   "),
            key_span(editor, "Ctrl+T"),
            desc_span(editor, "Toggle"),
        ]);
        let col = match editor.replace_focus {
            ReplaceFocus::Query => {
                let prefix = format!("{}Find: [", q_mark);
                let before = &editor.search_buffer[..sc.min(editor.search_buffer.len())];
                area.x
                    + UnicodeWidthStr::width(prefix.as_str()) as u16
                    + UnicodeWidthStr::width(before) as u16
            }
            ReplaceFocus::Replace => {
                let prefix = format!(
                    "{}Find: [{}]   {}Replace: [",
                    q_mark, editor.search_buffer, r_mark
                );
                let before = &editor.replace_buffer[..sc.min(editor.replace_buffer.len())];
                area.x
                    + UnicodeWidthStr::width(prefix.as_str()) as u16
                    + UnicodeWidthStr::width(before) as u16
            }
        };
        let narrow = area.width < 80;
        let layout = wide_layout(area);
        f.render_widget(Paragraph::new(line_fields), layout[0]);
        f.render_widget(
            Paragraph::new(shortcut_line(
                editor,
                narrow,
                &[
                    ("Ctrl+N", "Next"),
                    ("Ctrl+P", "Prev"),
                    ("Enter", "Replace"),
                    ("Ctrl+R", "All"),
                    ("Esc", "Return"),
                ],
            )),
            layout[1],
        );
        let (cx, cy) = clamp_cursor(f, col, area.y);
        f.set_cursor_position((cx, cy));
    }
}

fn render_save_shortcuts(editor: &EditorState, f: &mut Frame, area: Rect) {
    let buf = &editor.save_filename_buffer;
    if area.width < 50 {
        if area.height <= 2 {
            let (col, cursor_row) =
                render_inline_file_prompt(f, row(area, 0), "File: ", buf, editor.filename_cursor);
            if area.height > 1 {
                f.render_widget(
                    Paragraph::new(compact_line(editor, &[("Enter", "Save"), ("Esc", "Ret")])),
                    row(area, 1),
                );
            }
            let (cx, cy) = clamp_cursor(f, col, cursor_row);
            f.set_cursor_position((cx, cy));
            return;
        }
        let layout = narrow_layout(area);
        let (col, row) =
            render_narrow_file_prompt(f, layout[0], layout[1], buf, editor.filename_cursor);
        f.render_widget(
            Paragraph::new(compact_line(editor, &[("Enter", "Save"), ("Esc", "Ret")])),
            layout[2],
        );
        let (cx, cy) = clamp_cursor(f, col, row);
        f.set_cursor_position((cx, cy));
    } else {
        let prefix = "File: [";
        let before = &buf[..editor.filename_cursor.min(buf.len())];
        let col = area.x + prefix.len() as u16 + UnicodeWidthStr::width(before) as u16;
        let layout = wide_layout(area);
        f.render_widget(Paragraph::new(format!("{}{}]", prefix, buf)), layout[0]);
        let narrow = area.width < 80;
        f.render_widget(
            Paragraph::new(shortcut_line(
                editor,
                narrow,
                &[("Enter", "Save"), ("Esc", "Return")],
            )),
            layout[1],
        );
        let (cx, cy) = clamp_cursor(f, col, area.y);
        f.set_cursor_position((cx, cy));
    }
}

fn render_open_shortcuts(editor: &EditorState, f: &mut Frame, area: Rect) {
    let buf = &editor.open_filename_buffer;
    if area.width < 50 {
        if area.height <= 2 {
            let (col, cursor_row) =
                render_inline_file_prompt(f, row(area, 0), "File: ", buf, editor.filename_cursor);
            if area.height > 1 {
                f.render_widget(
                    Paragraph::new(compact_line(editor, &[("Enter", "Open"), ("Esc", "Ret")])),
                    row(area, 1),
                );
            }
            let (cx, cy) = clamp_cursor(f, col, cursor_row);
            f.set_cursor_position((cx, cy));
            return;
        }
        let layout = narrow_layout(area);
        let (col, row) =
            render_narrow_file_prompt(f, layout[0], layout[1], buf, editor.filename_cursor);
        f.render_widget(
            Paragraph::new(compact_line(editor, &[("Enter", "Open"), ("Esc", "Ret")])),
            layout[2],
        );
        let (cx, cy) = clamp_cursor(f, col, row);
        f.set_cursor_position((cx, cy));
    } else {
        let prefix = "File: [";
        let before = &buf[..editor.filename_cursor.min(buf.len())];
        let col = area.x + prefix.len() as u16 + UnicodeWidthStr::width(before) as u16;
        let layout = wide_layout(area);
        f.render_widget(Paragraph::new(format!("{}{}]", prefix, buf)), layout[0]);
        let narrow = area.width < 80;
        f.render_widget(
            Paragraph::new(shortcut_line(
                editor,
                narrow,
                &[("Enter", "Open"), ("Esc", "Return")],
            )),
            layout[1],
        );
        let (cx, cy) = clamp_cursor(f, col, area.y);
        f.set_cursor_position((cx, cy));
    }
}

fn render_goto_shortcuts(editor: &EditorState, f: &mut Frame, area: Rect) {
    let total = editor.buffer.lines.len();
    let line_goto = format!("Line: [{}]   (1-{})", editor.goto_line_buffer, total);
    f.render_widget(Paragraph::new(line_goto), row(area, 0));
    if area.height <= 1 {
        return;
    }
    let layout = wide_layout(area);
    if area.width < 50 {
        f.render_widget(
            Paragraph::new(compact_line(editor, &[("Enter", "Jump"), ("Esc", "Ret")])),
            layout[1],
        );
    } else {
        let narrow = area.width < 80;
        f.render_widget(
            Paragraph::new(shortcut_line(
                editor,
                narrow,
                &[("Enter", "Jump"), ("Esc", "Return")],
            )),
            layout[1],
        );
    }
}

fn render_quit_shortcuts(editor: &EditorState, f: &mut Frame, area: Rect) {
    let buf = &editor.save_filename_buffer;
    if area.width < 50 {
        if area.height <= 3 {
            let (col, cursor_row) =
                render_inline_file_prompt(f, row(area, 0), "File: ", buf, editor.filename_cursor);
            if area.height > 1 {
                f.render_widget(
                    Paragraph::new(compact_line(editor, &[("Enter", "Save+Exit")])),
                    row(area, 1),
                );
            }
            if area.height > 2 {
                f.render_widget(
                    Paragraph::new(compact_line(editor, &[("^Q", "Quit"), ("Esc", "Ret")])),
                    row(area, 2),
                );
            }
            let (cx, cy) = clamp_cursor(f, col, cursor_row);
            f.set_cursor_position((cx, cy));
            return;
        }
        let layout = narrow_layout(area);
        let (col, row) =
            render_narrow_file_prompt(f, layout[0], layout[1], buf, editor.filename_cursor);
        f.render_widget(
            Paragraph::new(compact_line(editor, &[("Enter", "Save+Exit")])),
            layout[2],
        );
        f.render_widget(
            Paragraph::new(compact_line(editor, &[("^Q", "Quit"), ("Esc", "Ret")])),
            layout[3],
        );
        let (cx, cy) = clamp_cursor(f, col, row);
        f.set_cursor_position((cx, cy));
    } else {
        let prefix = "Filename: [";
        let narrow = area.width < 80;
        let layout = wide_layout(area);
        f.render_widget(Paragraph::new(format!("{}{}]", prefix, buf)), layout[0]);
        f.render_widget(
            Paragraph::new(shortcut_line(
                editor,
                narrow,
                &[
                    ("Enter", "Save and Exit"),
                    ("Ctrl+Q", "Quit"),
                    ("Esc", "Return"),
                ],
            )),
            layout[1],
        );
        let before = &buf[..editor.filename_cursor.min(buf.len())];
        let col = area.x + prefix.len() as u16 + UnicodeWidthStr::width(before) as u16;
        let (cx, cy) = clamp_cursor(f, col, area.y);
        f.set_cursor_position((cx, cy));
    }
}

fn render_narrow_file_prompt(
    f: &mut Frame,
    label_area: Rect,
    input_area: Rect,
    buf: &str,
    cursor: usize,
) -> (u16, u16) {
    f.render_widget(Paragraph::new("File:"), label_area);
    f.render_widget(Paragraph::new(narrow_file_input_line(buf)), input_area);

    let before = &buf[..cursor.min(buf.len())];
    (
        input_area.x + 1 + UnicodeWidthStr::width(before) as u16,
        input_area.y,
    )
}

fn render_inline_file_prompt(
    f: &mut Frame,
    area: Rect,
    prefix: &str,
    buf: &str,
    cursor: usize,
) -> (u16, u16) {
    let label = format!("{}[{}]", prefix, buf);
    f.render_widget(Paragraph::new(label), area);
    let before = &buf[..cursor.min(buf.len())];
    (
        area.x + UnicodeWidthStr::width(prefix) as u16 + 1 + UnicodeWidthStr::width(before) as u16,
        area.y,
    )
}

fn narrow_file_input_line(buf: &str) -> String {
    format!("[{}]", buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    fn line_width(line: &Line<'_>) -> usize {
        line.spans
            .iter()
            .map(|span| UnicodeWidthStr::width(span.content.as_ref()))
            .sum()
    }

    fn assert_compact_fits(editor: &EditorState, pairs: &[(&str, &str)]) {
        let line = compact_line(editor, pairs);
        assert!(
            line_width(&line) <= 26,
            "expected {:?} to fit iPhone width, got {} columns",
            pairs,
            line_width(&line)
        );
    }

    fn render_footer_lines(editor: &EditorState, width: u16, height: u16) -> Vec<String> {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| render_shortcuts(editor, f, Rect::new(0, 0, width, height)))
            .unwrap();

        let buffer = terminal.backend().buffer();
        (0..height)
            .map(|y| {
                (0..width)
                    .map(|x| buffer.cell((x, y)).unwrap().symbol())
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect()
    }

    fn editor_in_mode(mode: EditorMode) -> EditorState {
        let mut editor = EditorState::new(Some("smoke.txt".to_string()));
        editor.enter_mode(mode);
        editor
    }

    #[test]
    fn a_cramped_footer_advertises_quit_instead_of_exit() {
        // ⭐ `^X`(EnterExit) と `^S`(EnterSave) は**同じ `File:` プロンプト**に着く
        // （`enter_mode` が `Save | Quit` を同じ腕で処理する）ので、枠が足りないときに
        // 両方載せると 1 枠が実質重複する。`^Q`(ForceQuit) は保存せず出る唯一の道で、
        // **電話の中で一番要る**（シェルへ落ちるのが容易でない）。
        // 2026-08-04 に iOS 実機を使っていて指摘された。
        let editor = EditorState::new(Some("smoke.txt".to_string()));

        // 50–79 列（2 行）: `^X` の枠を `^Q` が取る。
        let narrow = render_footer_lines(&editor, 79, 2).join("\n");
        assert!(
            narrow.contains("^Q"),
            "narrow footer needs the quit key: {narrow:?}"
        );
        assert!(
            !narrow.contains("^X"),
            "narrow footer should not spend a slot on the near-duplicate: {narrow:?}"
        );

        // 1 行しか無い段（3 枠）も同じ。
        let one_row = render_footer_lines(&editor, 40, 1).join("\n");
        assert!(one_row.contains("^Q"), "{one_row:?}");
        assert!(!one_row.contains("^X"), "{one_row:?}");

        // ⭐ **対の主張**: 幅が足りれば `Ctrl+X` は戻る。片方だけだと
        // 「どこでも Q に置き換える」実装でも緑になる。
        let wide = render_footer_lines(&editor, 100, 2).join("\n");
        assert!(
            wide.contains("Ctrl+X"),
            "wide footer keeps nano's exit: {wide:?}"
        );

        // 4 行入る段は元から両方載っているので入れ替えていない。
        let four_rows = render_footer_lines(&editor, 40, 4).join("\n");
        assert!(
            four_rows.contains("^X") && four_rows.contains("^Q"),
            "the four-row footer has room for both: {four_rows:?}"
        );
    }

    #[test]
    fn narrow_edit_status_omits_cursor_position() {
        let editor = EditorState::new(Some("smoke.txt".to_string()));
        let (left, right) = match editor.mode {
            EditorMode::Edit => {
                let name = editor
                    .filename
                    .as_ref()
                    .and_then(|p| p.file_name())
                    .and_then(|n| n.to_str())
                    .unwrap_or("[No Name]")
                    .to_string();
                (format!(" Edit: {}", name), String::new())
            }
            _ => unreachable!(),
        };

        let label = compose_status(&left, &right, 26);
        assert_eq!(label, " Edit: smoke.txt");
        assert!(!label.contains("L1 C1"));
    }

    #[test]
    fn compact_edit_primary_shortcuts_fit_iphone_width() {
        let editor = EditorState::new(Some("smoke.txt".to_string()));

        assert_compact_fits(&editor, &[("^S", "Save"), ("^B", "Browse"), ("^X", "Exit")]);
    }

    #[test]
    fn narrow_shortcut_rows_fit_iphone_width() {
        let editor = EditorState::new(Some("smoke.txt".to_string()));
        let rows: &[&[(&str, &str)]] = &[
            &[("^S", "Save"), ("^X", "Exit"), ("^D", "Md")],
            &[("Esc", "Ret"), ("i", "Edit"), ("dd", "Del")],
            &[("Type", "Fltr"), ("Enter", "OK"), ("Esc", "Clr")],
            &[("↑↓", "Move"), ("→", "Open"), ("Esc", "Exit")],
            &[("jk", "Move"), ("Pg", "Page"), ("Esc", "Ret")],
            &[("hjkl", "Move"), ("w/b", "Wrd"), ("0/$", "Ln")],
            &[("gg/G", "Fil"), ("H/M/L", "Scr"), ("i/a", "Ins")],
            &[("Esc", "Ret"), ("x", "Del"), ("dd", "DelLn")],
            &[("f/r", "Srch"), ("+/-", "Ln↕"), (".", "Rep")],
            &[("^F", "Find"), ("^R", "Repl"), ("^H", "Help")],
            &[("^K", "Cut"), ("^J", "Jump"), ("^G", "Glide")],
            &[("^Z", "Undo"), ("^Q", "Quit"), ("^D", "Md")],
            &[("Type", "Filter"), ("Enter", "OK")],
            &[("Esc", "Clear")],
            &[("↑↓", "Move"), ("→", "Open"), ("←", "Back")],
            &[("/", "Filter"), ("Esc", "Exit")],
            &[("jk/↑↓", "Move"), ("gg/G", "Top/Bot")],
            &[("H/M/L", "Screen")],
            &[("PgUp/PgDn", "Page")],
            &[("Esc", "Return")],
            &[("↑↓/jk", "Select"), ("Enter", "Run")],
            &[("Tab", "Complete"), ("Esc", "Return")],
            &[("^N", "Next"), ("^P", "Prev"), ("^T", "Tog")],
            &[("Tab", "Sw"), ("^N", "Next"), ("^P", "Prev")],
            &[("Tab", "Sw"), ("Enter", "Repl"), ("^R", "All")],
            &[("Enter", "Repl"), ("^R", "All"), ("^T", "Tog")],
            &[("Enter", "Save"), ("Esc", "Ret")],
            &[("Enter", "Open"), ("Esc", "Ret")],
            &[("Enter", "Jump"), ("Esc", "Ret")],
            &[("Enter", "Save+Exit")],
            &[("^Q", "Quit"), ("Esc", "Ret")],
        ];

        for row in rows {
            assert_compact_fits(&editor, row);
        }
    }

    #[test]
    fn one_row_narrow_edit_shows_primary_actions() {
        let editor = editor_in_mode(EditorMode::Edit);

        // ⚠️ 2026-08-04: 3 枠目が `^X Exit` から `^Q Quit` になった。理由は
        // `quit_takes_the_exit_slot` の doc（`^X` は `^S` と同じプロンプトに着くので
        // 狭いところでは枠の無駄・`^Q` は保存せず出る唯一の道）。
        // ⭐ 幅は変わっていない（どちらも 7 桁）ので、iPhone 幅の主張は無傷。
        assert_eq!(
            render_footer_lines(&editor, 26, 1),
            vec!["^S Save ^Q Quit ^D Md".to_string()]
        );
    }

    #[test]
    fn one_row_narrow_navigation_modes_show_primary_actions() {
        let glide = editor_in_mode(EditorMode::Glide);
        let browse = editor_in_mode(EditorMode::Browse);
        let markdown = editor_in_mode(EditorMode::Markdown);

        assert_eq!(
            render_footer_lines(&glide, 26, 1),
            vec!["Esc Ret i Edit dd Del".to_string()]
        );
        assert_eq!(
            render_footer_lines(&browse, 26, 1),
            vec!["↑↓ Move → Open Esc Exit".to_string()]
        );
        assert_eq!(
            render_footer_lines(&markdown, 26, 1),
            vec!["jk Move Spc/b Page Esc Ret".to_string()]
        );
    }

    #[test]
    fn one_row_narrow_input_modes_keep_input_visible() {
        let search = editor_in_mode(EditorMode::Search);
        let goto = editor_in_mode(EditorMode::Goto);

        assert_eq!(
            render_footer_lines(&search, 26, 1),
            vec!["Find: []".to_string()]
        );
        assert_eq!(
            render_footer_lines(&goto, 26, 1),
            vec!["Line: []   (1-1)".to_string()]
        );
    }

    #[test]
    fn three_row_narrow_replace_keeps_find_replace_and_primary_actions() {
        let replace = editor_in_mode(EditorMode::Replace);

        assert_eq!(
            render_footer_lines(&replace, 26, 3),
            vec![
                "> Find: []".to_string(),
                "  Replace: []".to_string(),
                "Tab Sw Enter Repl ^R All".to_string(),
            ]
        );
    }

    #[test]
    fn parse_color_accepts_named_colors() {
        assert_eq!(parse_color("cyan"), Some(Color::Cyan));
        assert_eq!(parse_color("darkgrey"), Some(Color::DarkGray));
        assert_eq!(parse_color("LIGHTYELLOW"), Some(Color::LightYellow));
    }

    #[test]
    fn parse_color_accepts_true_color_hex() {
        assert_eq!(parse_color("#222226"), Some(Color::Rgb(34, 34, 38)));
    }

    #[test]
    fn parse_color_rejects_invalid_values() {
        assert_eq!(parse_color("not-a-color"), None);
        assert_eq!(parse_color("#22222"), None);
        assert_eq!(parse_color("#zzzzzz"), None);
    }
}
