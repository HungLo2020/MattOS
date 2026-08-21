//! Browse モード（フォルダツリー）の全画面描画。
//!
//! `BrowseTree::visible_nodes()` の可視ノード列を行に変換し、選択行を中心にスクロールして
//! 描画する。インデントは深さ、ディレクトリは ▸/▾ で開閉を示す。

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use unicode_width::UnicodeWidthStr;

use crate::state::EditorState;

pub fn render_browse(editor: &EditorState, f: &mut Frame, area: Rect) {
    let Some(tree) = &editor.browse_tree else {
        return;
    };
    let height = area.height as usize;
    let width = area.width as usize;
    if height == 0 {
        return;
    }

    let visible = tree.visible_nodes();
    let sel_pos = visible
        .iter()
        .position(|&i| i == tree.selected)
        .unwrap_or(0);
    // Keep the selected row in view.
    let scroll = if sel_pos < height {
        0
    } else {
        sel_pos - height + 1
    };

    let filtering = !tree.filter.is_empty();
    let mut lines: Vec<Line> = Vec::with_capacity(height);
    for &idx in visible.iter().skip(scroll).take(height) {
        let node = &tree.nodes[idx];
        let indent = "  ".repeat(node.depth);
        let icon = if node.is_dir {
            if filtering || tree.expanded.contains(&idx) {
                "▾ "
            } else {
                "▸ "
            }
        } else {
            "  "
        };
        let mut label = format!("{}{}{}", indent, icon, node.name);

        // Directories stand out (blue, bold); files stay plain white.
        let base = if node.is_dir {
            Style::default()
                .fg(Color::LightBlue)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };
        let style = if idx == tree.selected {
            // Pad the row so the reversed highlight fills the full width; REVERSED
            // keeps the dir/file colour but flips it to the background.
            let w = UnicodeWidthStr::width(label.as_str());
            if w < width {
                label.push_str(&" ".repeat(width - w));
            }
            base.add_modifier(Modifier::REVERSED)
        } else {
            base
        };
        lines.push(Line::from(Span::styled(label, style)));
    }

    f.render_widget(Paragraph::new(lines), area);
}
