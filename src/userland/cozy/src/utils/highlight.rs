//! Syntax highlighting front-end shared by the renderer.
//!
//! The renderer never computes highlights per line per frame anymore. Instead it
//! asks this `Highlighter` to `ensure` the visible window is up to date, then
//! reads precomputed per-line spans via `style_at`. With the `treesitter`
//! feature (default) highlighting comes from a real parse tree, so multi-line
//! strings and block comments are correct; without it, the regex engine in
//! `super::syntax` fills the same cache line by line.

use ratatui::style::Style;
use std::collections::HashMap;
use std::ops::Range;
use std::path::Path;

/// Per-line highlight spans: `(start_byte, end_byte, style)`, may overlap.
type LineSpans = Vec<(usize, usize, Style)>;

pub struct Highlighter {
    ext: Option<String>,
    /// Buffer changed since the cache was built; the next `ensure` recomputes.
    dirty: bool,
    /// line index -> spans (only the `cached_range` window is populated).
    cache: HashMap<usize, LineSpans>,
    cached_range: Range<usize>,
    #[cfg(feature = "treesitter")]
    ts: ts::TsBackend,
}

impl Default for Highlighter {
    fn default() -> Self {
        Self {
            ext: None,
            dirty: true,
            cache: HashMap::new(),
            cached_range: 0..0,
            #[cfg(feature = "treesitter")]
            ts: ts::TsBackend::new(),
        }
    }
}

impl Highlighter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Resolve the language from a file path. Cheap; safe to call on every open.
    pub fn set_file(&mut self, path: Option<&Path>) {
        let ext = path
            .and_then(|p| p.extension())
            .and_then(|e| e.to_str())
            .map(|s| s.to_ascii_lowercase());
        if ext != self.ext {
            self.ext = ext;
            #[cfg(feature = "treesitter")]
            self.ts.set_language(self.ext.as_deref());
        }
        self.mark_dirty();
    }

    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    /// Make sure the cache covers `visible`. Reparses only when the buffer is
    /// dirty; re-queries when the visible window moved.
    pub fn ensure(&mut self, lines: &[String], visible: Range<usize>) {
        if !self.dirty && self.cached_range == visible {
            return;
        }

        #[cfg(feature = "treesitter")]
        {
            if self.dirty {
                self.ts.reparse(lines);
            }
            self.cache.clear();
            self.ts.spans_for(lines, &visible, &mut self.cache);
        }

        #[cfg(not(feature = "treesitter"))]
        {
            self.cache.clear();
            let hl = super::syntax::SyntaxHighlighter::new(self.ext.as_deref());
            for y in visible.clone() {
                if let Some(line) = lines.get(y) {
                    self.cache.insert(y, regex_line_spans(&hl, line));
                }
            }
        }

        self.cached_range = visible;
        self.dirty = false;
    }

    /// Base style for the character at `byte` on line `y`, if highlighted.
    /// Later spans win (highlight precedence), so search from the end.
    pub fn style_at(&self, y: usize, byte: usize) -> Option<Style> {
        self.cache.get(&y).and_then(|spans| {
            spans
                .iter()
                .rev()
                .find(|&&(s, e, _)| byte >= s && byte < e)
                .map(|&(_, _, style)| style)
        })
    }
}

#[cfg(not(feature = "treesitter"))]
fn regex_line_spans(hl: &super::syntax::SyntaxHighlighter, line: &str) -> LineSpans {
    let mut spans = Vec::new();
    let mut pos = 0usize;
    for (text, style) in hl.highlight(line) {
        let len = text.len();
        if style != Style::default() {
            spans.push((pos, pos + len, style));
        }
        pos += len;
    }
    spans
}

#[cfg(all(test, feature = "treesitter"))]
mod tests {
    use super::*;
    use ratatui::style::{Color, Modifier};

    fn to_lines(s: &str) -> Vec<String> {
        s.lines().map(|l| l.to_string()).collect()
    }

    fn fg_at(h: &Highlighter, y: usize, byte: usize) -> Option<Color> {
        h.style_at(y, byte).and_then(|s| s.fg)
    }

    #[test]
    fn rust_keyword_comment_and_multiline_string() {
        // line0: comment, line1: `fn` keyword, lines 2-3: a string literal that
        // spans the newline — the exact case the old regex highlighter got wrong.
        let src = "// note\nfn main() {\n    let s = \"a\n b\";\n}";
        let buf = to_lines(src);
        let mut h = Highlighter::new();
        h.set_file(Some(Path::new("x.rs")));
        h.ensure(&buf, 0..buf.len());

        assert_eq!(fg_at(&h, 0, 0), Some(Color::DarkGray), "comment");
        assert_eq!(fg_at(&h, 1, 0), Some(Color::Magenta), "fn keyword");

        let quote = buf[2].find('"').unwrap();
        assert_eq!(fg_at(&h, 2, quote), Some(Color::Green), "string open");
        // Continuation line: the leading space is *inside* the string literal.
        assert_eq!(fg_at(&h, 3, 0), Some(Color::Green), "string continues");
    }

    #[test]
    fn unknown_extension_has_no_highlight() {
        let buf = to_lines("fn main() {}");
        let mut h = Highlighter::new();
        h.set_file(Some(Path::new("notes.txt")));
        h.ensure(&buf, 0..buf.len());
        assert_eq!(h.style_at(0, 0), None);
    }

    #[test]
    fn go_keyword_and_function() {
        let buf = to_lines("func main() {}");
        let mut h = Highlighter::new();
        h.set_file(Some(Path::new("main.go")));
        h.ensure(&buf, 0..buf.len());
        assert_eq!(fg_at(&h, 0, 0), Some(Color::Magenta), "func keyword");
        let f = buf[0].find("main").unwrap();
        assert_eq!(fg_at(&h, 0, f), Some(Color::LightBlue), "function name");
    }

    #[test]
    fn typescript_inherits_javascript_query() {
        // `const`/`function` come from the JS query; without prepending it TS
        // would only highlight TS-specific nodes (e.g. `: number`).
        let buf = to_lines("function add(a: number) { return a; }");
        let mut h = Highlighter::new();
        h.set_file(Some(Path::new("x.ts")));
        h.ensure(&buf, 0..buf.len());
        assert_eq!(fg_at(&h, 0, 0), Some(Color::Magenta), "function keyword");
        let add = buf[0].find("add").unwrap();
        assert_eq!(fg_at(&h, 0, add), Some(Color::LightBlue), "function name");
    }

    #[test]
    fn markdown_matches_vscode_dark_plus_colors() {
        // Markdown uses two grammars: the heading comes from the block tree,
        // while bold/italic/code/link come from an inline tree. Asserting the
        // inline styles by their document position also proves inline-tree node
        // coordinates land in the whole-document space (so push_span is reused).
        // Colors mirror VS Code's Dark+ theme (dark_vs.json).
        let blue = Color::Rgb(0x56, 0x9C, 0xD6);
        let purple = Color::Rgb(0xC5, 0x86, 0xC0);
        let orange = Color::Rgb(0xCE, 0x91, 0x78);

        let buf = to_lines("# Title\n\n**bold** *it* `code` [lbl](http://x)");
        let mut h = Highlighter::new();
        h.set_file(Some(Path::new("notes.md")));
        h.ensure(&buf, 0..buf.len());

        // Heading text (the `(inline)` after "# ") is blue + bold.
        let title = buf[0].find("Title").unwrap();
        assert_eq!(fg_at(&h, 0, title), Some(blue), "heading color");
        assert!(
            h.style_at(0, title)
                .unwrap()
                .add_modifier
                .contains(Modifier::BOLD),
            "heading bold"
        );

        // **bold** is blue + bold (inner chars; proves inline coordinates).
        let bold = buf[2].find("bold").unwrap();
        let strong = h.style_at(2, bold).expect("strong span");
        assert_eq!(strong.fg, Some(blue), "bold color");
        assert!(
            strong.add_modifier.contains(Modifier::BOLD),
            "bold modifier"
        );

        // *it* is purple + italic.
        let italic = buf[2].find("it").unwrap();
        let emphasis = h.style_at(2, italic).expect("emphasis span");
        assert_eq!(emphasis.fg, Some(purple), "italic color");
        assert!(
            emphasis.add_modifier.contains(Modifier::ITALIC),
            "italic modifier"
        );

        // Inline `code` is orange.
        let code = buf[2].find("code").unwrap();
        assert_eq!(fg_at(&h, 2, code), Some(orange), "inline code color");

        // Link URL keeps the default fg (none) but is underlined, like VS Code.
        let uri = buf[2].find("http").unwrap();
        let link = h.style_at(2, uri).expect("link span");
        assert_eq!(link.fg, None, "link uri keeps default fg");
        assert!(
            link.add_modifier.contains(Modifier::UNDERLINED),
            "link underline"
        );
    }

    #[test]
    fn markdown_list_and_quote_markers() {
        let blue_list = Color::Rgb(0x67, 0x96, 0xE6);
        let green_quote = Color::Rgb(0x6A, 0x99, 0x55);
        let buf = to_lines("- item\n> quote");
        let mut h = Highlighter::new();
        h.set_file(Some(Path::new("notes.md")));
        h.ensure(&buf, 0..buf.len());

        assert_eq!(fg_at(&h, 0, 0), Some(blue_list), "list marker color");
        assert_eq!(fg_at(&h, 1, 0), Some(green_quote), "quote marker color");
    }
}

#[cfg(feature = "treesitter")]
mod ts {
    use super::LineSpans;
    use ratatui::style::{Color, Modifier, Style};
    use std::collections::HashMap;
    use std::ops::Range;
    use tree_sitter::{Language, Node, Parser, Point, Query, QueryCursor, StreamingIterator, Tree};

    pub struct TsBackend {
        engine: Engine,
    }

    /// Most languages parse into a single tree. Markdown is the exception: it
    /// uses two grammars (block structure + inline content) coordinated by
    /// `MarkdownParser`, so it gets its own engine variant.
    enum Engine {
        None,
        Single(SingleEngine),
        // Boxed: `MdEngine` (two grammars + parser) is much larger than the
        // other variants, and there is only ever one engine per editor.
        Markdown(Box<MdEngine>),
    }

    struct SingleEngine {
        parser: Parser,
        tree: Option<Tree>,
        lang: LangConfig,
        /// The exact source the current `tree` was parsed from (`lines` joined
        /// with '\n'), reused by `spans_for` so we don't rebuild it twice.
        source: String,
    }

    /// Markdown's inline trees are parsed with document-global byte ranges
    /// (`MarkdownParser` sets included ranges from the block nodes), so their
    /// node positions share the block tree's coordinate space and `push_span`
    /// works unchanged for both.
    struct MdEngine {
        parser: tree_sitter_md::MarkdownParser,
        tree: Option<tree_sitter_md::MarkdownTree>,
        block: LangConfig,
        inline: LangConfig,
        source: String,
    }

    struct LangConfig {
        query: Query,
        /// capture index -> style.
        styles: Vec<Style>,
    }

    impl TsBackend {
        pub fn new() -> Self {
            Self {
                engine: Engine::None,
            }
        }

        pub fn set_language(&mut self, ext: Option<&str>) {
            self.engine = build_engine(ext);
        }

        pub fn reparse(&mut self, lines: &[String]) {
            match &mut self.engine {
                Engine::None => {}
                Engine::Single(s) => {
                    s.source = lines.join("\n");
                    s.tree = s.parser.parse(&s.source, None);
                }
                Engine::Markdown(m) => {
                    m.source = lines.join("\n");
                    m.tree = m.parser.parse(m.source.as_bytes(), None);
                }
            }
        }

        pub fn spans_for(
            &self,
            lines: &[String],
            visible: &Range<usize>,
            out: &mut HashMap<usize, LineSpans>,
        ) {
            match &self.engine {
                Engine::None => {}
                Engine::Single(s) => {
                    if let Some(tree) = s.tree.as_ref() {
                        collect_spans(
                            &s.lang,
                            tree.root_node(),
                            s.source.as_bytes(),
                            lines,
                            visible,
                            out,
                        );
                    }
                }
                Engine::Markdown(m) => {
                    let Some(tree) = m.tree.as_ref() else {
                        return;
                    };
                    let src = m.source.as_bytes();
                    collect_spans(
                        &m.block,
                        tree.block_tree().root_node(),
                        src,
                        lines,
                        visible,
                        out,
                    );
                    for inline in tree.inline_trees() {
                        collect_spans(&m.inline, inline.root_node(), src, lines, visible, out);
                    }
                }
            }
        }
    }

    /// Resolve a file extension to a highlight engine. Markdown is special-cased
    /// because its two-grammar parser does not fit the single-`Tree` path.
    fn build_engine(ext: Option<&str>) -> Engine {
        if matches!(ext, Some("md") | Some("markdown")) {
            // Custom queries (not the crate's stock highlights) so the capture
            // set maps cleanly onto VS Code's Dark+ Markdown colors: headings,
            // bold/italic, inline code, list/quote markers, link URLs — while
            // leaving fenced-code-block *contents* at the default color, like
            // VS Code does. Capture names here are private to Markdown, so they
            // never collide with the code grammars' shared scopes.
            let block = lang_config(
                &tree_sitter_md::LANGUAGE.into(),
                MD_BLOCK_QUERY,
                md_style_for_capture,
            );
            let inline = lang_config(
                &tree_sitter_md::INLINE_LANGUAGE.into(),
                MD_INLINE_QUERY,
                md_style_for_capture,
            );
            return match (block, inline) {
                (Some(block), Some(inline)) => Engine::Markdown(Box::new(MdEngine {
                    parser: tree_sitter_md::MarkdownParser::default(),
                    tree: None,
                    block,
                    inline,
                    source: String::new(),
                })),
                _ => Engine::None,
            };
        }

        let Some((language, query_src)) = resolve(ext) else {
            return Engine::None;
        };
        let mut parser = Parser::new();
        if parser.set_language(&language).is_err() {
            return Engine::None;
        }
        match lang_config(&language, &query_src, style_for_capture) {
            Some(lang) => Engine::Single(SingleEngine {
                parser,
                tree: None,
                lang,
                source: String::new(),
            }),
            None => Engine::None,
        }
    }

    fn lang_config(
        language: &Language,
        query_src: &str,
        style_fn: fn(&str) -> Style,
    ) -> Option<LangConfig> {
        let query = Query::new(language, query_src).ok()?;
        let styles = query.capture_names().iter().map(|n| style_fn(n)).collect();
        Some(LangConfig { query, styles })
    }

    /// Run one highlight query over `root` and push the styled spans into `out`.
    /// Shared by the single-tree path and by each of Markdown's block/inline
    /// trees, all of which report positions in the same document coordinates.
    fn collect_spans(
        lc: &LangConfig,
        root: Node,
        source: &[u8],
        lines: &[String],
        visible: &Range<usize>,
        out: &mut HashMap<usize, LineSpans>,
    ) {
        let mut cursor = QueryCursor::new();
        cursor.set_point_range(Point::new(visible.start, 0)..Point::new(visible.end, 0));
        let mut captures = cursor.captures(&lc.query, root, source);
        while let Some((m, idx)) = captures.next() {
            let cap = m.captures[*idx];
            let style = lc.styles[cap.index as usize];
            if style == Style::default() {
                continue;
            }
            push_span(
                lines,
                out,
                cap.node.start_position(),
                cap.node.end_position(),
                style,
                visible,
            );
        }
    }

    /// Split a possibly multi-line capture into per-line byte ranges.
    fn push_span(
        lines: &[String],
        out: &mut HashMap<usize, LineSpans>,
        start: Point,
        end: Point,
        style: Style,
        visible: &Range<usize>,
    ) {
        for row in start.row..=end.row {
            if row < visible.start || row >= visible.end {
                continue;
            }
            let line_len = lines.get(row).map(|l| l.len()).unwrap_or(0);
            let s = if row == start.row { start.column } else { 0 };
            let e = if row == end.row { end.column } else { line_len };
            let e = e.min(line_len);
            let s = s.min(e);
            if s < e {
                out.entry(row).or_default().push((s, e, style));
            }
        }
    }

    fn resolve(ext: Option<&str>) -> Option<(Language, String)> {
        Some(match ext? {
            "rs" => (
                tree_sitter_rust::LANGUAGE.into(),
                tree_sitter_rust::HIGHLIGHTS_QUERY.to_string(),
            ),
            "py" | "pyi" => (
                tree_sitter_python::LANGUAGE.into(),
                tree_sitter_python::HIGHLIGHTS_QUERY.to_string(),
            ),
            "js" | "mjs" | "cjs" | "jsx" => (
                tree_sitter_javascript::LANGUAGE.into(),
                tree_sitter_javascript::HIGHLIGHT_QUERY.to_string(),
            ),
            // TypeScript's highlights query only carries TS-specific additions;
            // it inherits the rest from JavaScript, so prepend the JS query.
            "ts" | "mts" | "cts" => (
                tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
                typescript_query(),
            ),
            "tsx" => (
                tree_sitter_typescript::LANGUAGE_TSX.into(),
                typescript_query(),
            ),
            "go" => (
                tree_sitter_go::LANGUAGE.into(),
                tree_sitter_go::HIGHLIGHTS_QUERY.to_string(),
            ),
            "json" => (
                tree_sitter_json::LANGUAGE.into(),
                tree_sitter_json::HIGHLIGHTS_QUERY.to_string(),
            ),
            "toml" => (
                tree_sitter_toml_ng::LANGUAGE.into(),
                tree_sitter_toml_ng::HIGHLIGHTS_QUERY.to_string(),
            ),
            _ => return None,
        })
    }

    fn typescript_query() -> String {
        format!(
            "{}\n{}",
            tree_sitter_javascript::HIGHLIGHT_QUERY,
            tree_sitter_typescript::HIGHLIGHTS_QUERY
        )
    }

    /// Highlight query for Markdown's block grammar. Heading text *and* its `#`
    /// markers share one capture (VS Code colors both blue); list/quote markers
    /// get their own. Code blocks are intentionally not captured so their
    /// contents render at the default color, matching VS Code.
    const MD_BLOCK_QUERY: &str = r#"
        (atx_heading (inline) @heading)
        (setext_heading (paragraph) @heading)
        [
          (atx_h1_marker) (atx_h2_marker) (atx_h3_marker)
          (atx_h4_marker) (atx_h5_marker) (atx_h6_marker)
          (setext_h1_underline) (setext_h2_underline)
        ] @heading
        [
          (list_marker_plus) (list_marker_minus) (list_marker_star)
          (list_marker_dot) (list_marker_parenthesis) (thematic_break)
        ] @list_marker
        (block_quote_marker) @quote
    "#;

    /// Highlight query for Markdown's inline grammar. Each capture spans the
    /// whole construct (delimiters included), matching how VS Code styles e.g.
    /// the `**` of bold the same as its text.
    const MD_INLINE_QUERY: &str = r#"
        (code_span) @code
        (emphasis) @italic
        (strong_emphasis) @bold
        [ (link_destination) (uri_autolink) ] @link
    "#;

    /// Map a Markdown capture (from `MD_BLOCK_QUERY` / `MD_INLINE_QUERY`) to a
    /// style matching VS Code's Dark+ Markdown source colors. True-color RGB is
    /// used so the hues match exactly. `@link` sets only an underline so the
    /// foreground stays the theme default, like VS Code.
    fn md_style_for_capture(name: &str) -> Style {
        match name {
            "heading" | "bold" => Style::default()
                .fg(Color::Rgb(0x56, 0x9C, 0xD6))
                .add_modifier(Modifier::BOLD),
            "italic" => Style::default()
                .fg(Color::Rgb(0xC5, 0x86, 0xC0))
                .add_modifier(Modifier::ITALIC),
            "code" => Style::default().fg(Color::Rgb(0xCE, 0x91, 0x78)),
            "list_marker" => Style::default().fg(Color::Rgb(0x67, 0x96, 0xE6)),
            "quote" => Style::default().fg(Color::Rgb(0x6A, 0x99, 0x55)),
            "link" => Style::default().add_modifier(Modifier::UNDERLINED),
            _ => Style::default(),
        }
    }

    /// Map a code-grammar capture name to a terminal style. The base segment
    /// (before the first '.') carries the meaning, e.g. `keyword.control`.
    fn style_for_capture(name: &str) -> Style {
        let base = name.split('.').next().unwrap_or(name);
        match base {
            "keyword" => Style::default().fg(Color::Magenta),
            "string" | "character" | "char" => Style::default().fg(Color::Green),
            "comment" => Style::default().fg(Color::DarkGray),
            "type" | "constructor" => Style::default().fg(Color::Cyan),
            "number" | "float" | "constant" | "boolean" => Style::default().fg(Color::Yellow),
            // LightBlue, not Blue: plain ANSI blue is too dark to read on a dark
            // background (it looks unhighlighted / "white").
            "function" => Style::default().fg(Color::LightBlue),
            "property" | "attribute" => Style::default().fg(Color::Cyan),
            _ => Style::default(),
        }
    }
}
