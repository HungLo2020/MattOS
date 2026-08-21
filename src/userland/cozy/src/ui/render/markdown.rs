use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::Line,
    widgets::{Block, Paragraph},
};
use ratatui_markdown::{markdown::MarkdownRenderer, theme::ThemeConfig};

use crate::state::EditorState;

pub fn render_markdown(editor: &mut EditorState, f: &mut Frame, area: Rect) {
    editor.markdown_view_height = area.height as usize;
    let lines = rendered_markdown_lines(&editor.buffer.lines, area.width);
    editor.markdown_rendered_line_count = lines.len().max(1);
    let max_scroll = lines.len().saturating_sub(area.height as usize);
    let max_line = lines.len().saturating_sub(1);
    editor.markdown_cursor_line = editor.markdown_cursor_line.min(max_line);

    let cursor = editor.markdown_cursor_line;
    let top = editor.markdown_scroll_offset;
    let height = (area.height as usize).max(1);
    if cursor < top {
        editor.markdown_scroll_offset = cursor;
    } else if cursor >= top.saturating_add(height) {
        editor.markdown_scroll_offset = cursor.saturating_sub(height - 1);
    }
    editor.markdown_scroll_offset = editor.markdown_scroll_offset.min(max_scroll);

    for row in 0..area.height {
        let idx = editor.markdown_scroll_offset + row as usize;
        let mut line = lines.get(idx).cloned().unwrap_or_else(|| Line::from(""));
        if idx == editor.markdown_cursor_line {
            f.render_widget(
                Block::default().style(Style::default().bg(Color::DarkGray)),
                Rect {
                    x: area.x,
                    y: area.y + row,
                    width: area.width,
                    height: 1,
                },
            );
            line = line.style(Style::default().bg(Color::DarkGray));
        }
        let row_area = Rect {
            x: area.x,
            y: area.y + row,
            width: area.width,
            height: 1,
        };
        f.render_widget(Paragraph::new(line), row_area);
    }
}

pub fn markdown_line_count(editor: &EditorState) -> usize {
    editor
        .markdown_rendered_line_count
        .max(editor.buffer.lines.len())
        .max(1)
}

fn rendered_markdown_lines(source: &[String], width: u16) -> Vec<Line<'static>> {
    let markdown = source.join("\n");
    let renderer = MarkdownRenderer::new(width.max(1) as usize);
    let blocks = renderer.parse(&markdown);
    renderer.render(&blocks, &ThemeConfig::default())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line_text(line: &Line<'static>) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<Vec<_>>()
            .join("")
    }

    #[test]
    fn heading_renders_without_hash_marker() {
        let lines = rendered_markdown_lines(&["# Title".to_string()], 80);
        assert_eq!(line_text(&lines[0]), "Title");
    }

    #[test]
    fn inline_code_renders_without_backticks() {
        let lines = rendered_markdown_lines(&["Use `cozy` now".to_string()], 80);
        assert_eq!(line_text(&lines[0]), "Use cozy now");
    }

    #[test]
    fn fenced_code_block_preserves_code_content() {
        let lines = rendered_markdown_lines(
            &[
                "```rust".to_string(),
                "fn main() {}".to_string(),
                "```".to_string(),
            ],
            80,
        );

        assert!(lines.iter().any(|line| line_text(line).contains("rust")));
        assert!(
            lines
                .iter()
                .any(|line| line_text(line).contains("fn main() {}"))
        );
    }

    #[test]
    fn paragraph_wraps_to_rendered_lines() {
        let lines = rendered_markdown_lines(
            &["alpha beta gamma delta epsilon zeta eta theta".to_string()],
            20,
        );

        assert!(lines.len() > 1);
    }

    #[test]
    fn mermaid_block_renders_diagram_content() {
        let lines = rendered_markdown_lines(
            &[
                "```mermaid".to_string(),
                "graph TD".to_string(),
                "A[Start] --> B[End]".to_string(),
                "```".to_string(),
            ],
            80,
        );
        let text = lines.iter().map(line_text).collect::<Vec<_>>().join("\n");

        assert!(text.contains("mermaid"));
        assert!(text.contains("Start"));
        assert!(text.contains("End"));
    }
}
