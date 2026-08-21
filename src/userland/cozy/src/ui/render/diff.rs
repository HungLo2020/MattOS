//! Session diff review rendering.
//!
//! Read-only full-screen view, modeled on the Markdown preview. Renders the
//! parsed `git diff` hunk by hunk: `+` lines green, `-` lines red, the file
//! header (`+++`/`---` collapsed into one divider) and `@@` hunk header are
//! shown per hunk. The current hunk is highlighted; approved hunks carry a `✓`
//! gutter. The whole surface is the "read == approve" gate.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph},
};

use crate::state::{DiffLineKind, EditorState};

pub fn render_diff_review(editor: &mut EditorState, f: &mut Frame, area: Rect) {
    let Some(dr) = editor.diff_review.as_ref() else {
        return;
    };

    if dr.hunks.is_empty() {
        let msg = Line::from(Span::styled(
            "  No changes to review.",
            Style::default().fg(Color::DarkGray),
        ));
        f.render_widget(Paragraph::new(msg), area);
        return;
    }

    let current = dr.current;

    // Flatten hunks into owned display rows: (line, hunk_index). Owned so the
    // immutable borrow of `editor` ends before we touch `scroll` mutably.
    let mut rows: Vec<(Line<'static>, usize)> = Vec::new();
    let mut current_row = 0usize;
    for (hi, h) in dr.hunks.iter().enumerate() {
        if hi == current {
            current_row = rows.len();
        }
        let (mark, mark_color) = if h.approved {
            ("✓", Color::Green)
        } else {
            ("·", Color::DarkGray)
        };
        let header = Line::from(vec![
            Span::styled(
                format!("{mark} "),
                Style::default().fg(mark_color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{}  ", h.file),
                Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(h.header.clone(), Style::default().fg(Color::Cyan)),
        ]);
        rows.push((header, hi));
        for l in &h.lines {
            let (sign, color) = match l.kind {
                DiffLineKind::Add => ('+', Color::Green),
                DiffLineKind::Del => ('-', Color::Red),
                DiffLineKind::Context => (' ', Color::Gray),
            };
            let text = format!("  {sign}{}", l.text);
            rows.push((
                Line::from(Span::styled(text, Style::default().fg(color))),
                hi,
            ));
        }
    }

    let total = rows.len();
    let height = (area.height as usize).max(1);

    // Keep the current hunk's header visible (scroll like the Markdown view).
    let mut scroll = editor.diff_review.as_ref().map(|d| d.scroll).unwrap_or(0);
    if current_row < scroll {
        scroll = current_row;
    } else if current_row >= scroll + height {
        scroll = current_row + 1 - height;
    }
    let max_scroll = total.saturating_sub(height);
    scroll = scroll.min(max_scroll);
    if let Some(d) = editor.diff_review.as_mut() {
        d.scroll = scroll;
    }

    for row in 0..area.height {
        let idx = scroll + row as usize;
        let Some((line, hunk_idx)) = rows.get(idx) else {
            break;
        };
        let row_area = Rect {
            x: area.x,
            y: area.y + row,
            width: area.width,
            height: 1,
        };
        let mut line = line.clone();
        if *hunk_idx == current {
            f.render_widget(
                Block::default().style(Style::default().bg(Color::Rgb(40, 44, 52))),
                row_area,
            );
            line = line.style(Style::default().bg(Color::Rgb(40, 44, 52)));
        }
        f.render_widget(Paragraph::new(line), row_area);
    }
}
