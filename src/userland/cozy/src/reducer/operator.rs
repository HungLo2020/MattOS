//! Operator + motion execution.
//!
//! `d`/`c`/`y` are operators that act over the span produced by a motion. The
//! same `glide::resolve` that powers bare movement provides the target here, so
//! every motion automatically works with every operator (`dw`, `d$`, `dj`,
//! `cw`, `yw`, `d3w`, `dd`/`yy`/`cc` via `Motion::CurrentLine`).

use crate::glide::{Motion, MotionKind, Operator, resolve};
use crate::reducer::EventResult;
use crate::reducer::clipboard::{set_register_charwise, set_register_linewise};
use crate::reducer::helper::mark_modified;
use crate::state::cursor::first_non_whitespace_byte;
use crate::state::{EditorMode, EditorState};

/// Read text over the half-open span `[start, end)` (start <= end).
fn read_span(lines: &[String], start: (usize, usize), end: (usize, usize)) -> String {
    let (sy, sx) = start;
    let (ey, ex) = end;
    if sy == ey {
        let line = &lines[sy];
        line[sx.min(line.len())..ex.min(line.len())].to_string()
    } else {
        let mut parts = Vec::new();
        parts.push(lines[sy][sx.min(lines[sy].len())..].to_string());
        parts.extend(lines[(sy + 1)..ey].iter().cloned());
        parts.push(lines[ey][..ex.min(lines[ey].len())].to_string());
        parts.join("\n")
    }
}

/// Delete the half-open span `[start, end)` and return the removed text.
fn delete_span(lines: &mut Vec<String>, start: (usize, usize), end: (usize, usize)) -> String {
    let removed = read_span(lines, start, end);
    let (sy, sx) = start;
    let (ey, ex) = end;
    if sy == ey {
        let sx = sx.min(lines[sy].len());
        let ex = ex.min(lines[sy].len());
        lines[sy].replace_range(sx..ex, "");
    } else {
        let ex = ex.min(lines[ey].len());
        let tail = lines[ey][ex..].to_string();
        let sx = sx.min(lines[sy].len());
        lines[sy].truncate(sx);
        lines[sy].push_str(&tail);
        lines.drain((sy + 1)..=ey);
    }
    removed
}

/// Apply `op` over the span from the cursor to the resolved `motion` target.
pub fn apply_operator(
    editor: &mut EditorState,
    op: Operator,
    motion: Motion,
    count: Option<usize>,
) -> EventResult {
    let r = resolve(motion, count, editor);
    let cur = (editor.cursor.y, editor.cursor.x);
    match r.kind {
        MotionKind::Linewise => operate_linewise(editor, op, cur.0, r.target.0),
        MotionKind::CharExclusive => operate_charwise(editor, op, cur, r.target),
        MotionKind::CharInclusive => {
            if r.target == cur {
                return EventResult::Continue; // motion did not move (e.g. f: char not found)
            }
            // Extend the span by one char so the target itself is included (e, f).
            let end = advance_one_char(&editor.buffer.lines, r.target);
            operate_charwise(editor, op, cur, end)
        }
    }
}

/// The byte position one character past `pos` (clamped to the line end).
fn advance_one_char(lines: &[String], pos: (usize, usize)) -> (usize, usize) {
    let (y, x) = pos;
    if let Some(line) = lines.get(y) {
        if let Some(ch) = line.get(x.min(line.len())..).and_then(|s| s.chars().next()) {
            return (y, (x + ch.len_utf8()).min(line.len()));
        }
    }
    pos
}

fn operate_linewise(editor: &mut EditorState, op: Operator, ya: usize, yb: usize) -> EventResult {
    let a = ya.min(yb);
    let b = ya.max(yb).min(editor.buffer.lines.len().saturating_sub(1));
    let text = editor.buffer.lines[a..=b].join("\n");

    match op {
        Operator::Yank => {
            set_register_linewise(editor, text);
            editor.cursor.y = a;
            let len = editor.buffer.lines[a].len();
            editor.cursor.x = editor.cursor.x.min(len);
            let end_len = editor.buffer.lines[b].len();
            editor.yank_highlight = Some(crate::state::YankHighlight {
                start: (a, 0),
                end: (b, end_len),
                linewise: true,
            });
        }
        Operator::Delete => {
            set_register_linewise(editor, text);
            mark_modified(editor);
            let n = b - a + 1;
            editor.buffer.lines.drain(a..=b);
            if editor.buffer.lines.is_empty() {
                editor.buffer.lines.push(String::new());
            }
            editor.cursor.y = a.min(editor.buffer.lines.len() - 1);
            editor.cursor.x = first_non_whitespace_byte(&editor.buffer.lines[editor.cursor.y]);
            editor.set_status_message(
                format!("Deleted {} line(s)", n),
                crate::state::StatusKind::Success,
                false,
            );
        }
        Operator::Change => {
            set_register_linewise(editor, text);
            mark_modified(editor);
            editor.buffer.lines.drain(a..=b);
            editor.buffer.lines.insert(a, String::new());
            editor.cursor.y = a;
            editor.cursor.x = 0;
            editor.enter_mode(EditorMode::Edit);
        }
    }
    EventResult::Continue
}

fn operate_charwise(
    editor: &mut EditorState,
    op: Operator,
    cur: (usize, usize),
    tgt: (usize, usize),
) -> EventResult {
    let (start, end) = if cur <= tgt { (cur, tgt) } else { (tgt, cur) };
    if start == end {
        return EventResult::Continue; // empty span (e.g. motion did not move)
    }
    match op {
        Operator::Yank => {
            let removed = read_span(&editor.buffer.lines, start, end);
            set_register_charwise(editor, removed);
            editor.cursor.y = start.0;
            editor.cursor.x = start.1;
            editor.yank_highlight = Some(crate::state::YankHighlight {
                start,
                end,
                linewise: false,
            });
        }
        Operator::Delete => {
            mark_modified(editor);
            let removed = delete_span(&mut editor.buffer.lines, start, end);
            set_register_charwise(editor, removed);
            editor.cursor.y = start.0;
            editor.cursor.x = start.1;
        }
        Operator::Change => {
            mark_modified(editor);
            let removed = delete_span(&mut editor.buffer.lines, start, end);
            set_register_charwise(editor, removed);
            editor.cursor.y = start.0;
            editor.cursor.x = start.1;
            editor.enter_mode(EditorMode::Edit);
        }
    }
    EventResult::Continue
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::TextBuffer;

    fn editor_with(lines: &[&str]) -> EditorState {
        let mut e = EditorState::new(None);
        e.buffer = TextBuffer::from_lines(lines.iter().map(|s| s.to_string()).collect());
        e
    }

    #[test]
    fn dw_deletes_word_charwise() {
        let mut e = editor_with(&["alpha beta gamma"]);
        e.cursor.x = 0;
        apply_operator(&mut e, Operator::Delete, Motion::WordForward, None);
        assert_eq!(e.buffer.lines[0], "beta gamma");
        assert!(!e.register.linewise);
        assert_eq!(e.register.text, "alpha ");
    }

    #[test]
    fn d_dollar_deletes_to_eol() {
        let mut e = editor_with(&["hello world"]);
        e.cursor.x = 6; // on 'w'
        apply_operator(&mut e, Operator::Delete, Motion::LineEnd, None);
        assert_eq!(e.buffer.lines[0], "hello ");
    }

    #[test]
    fn dj_deletes_two_lines() {
        let mut e = editor_with(&["one", "two", "three"]);
        e.cursor.y = 0;
        apply_operator(&mut e, Operator::Delete, Motion::Down, None);
        assert_eq!(e.buffer.lines, vec!["three"]);
        assert!(e.register.linewise);
        assert_eq!(e.register.text, "one\ntwo");
    }

    #[test]
    fn dd_via_current_line() {
        let mut e = editor_with(&["a", "b", "c"]);
        e.cursor.y = 1;
        apply_operator(&mut e, Operator::Delete, Motion::CurrentLine, None);
        assert_eq!(e.buffer.lines, vec!["a", "c"]);
    }

    #[test]
    fn n_dd_deletes_n_lines() {
        let mut e = editor_with(&["1", "2", "3", "4"]);
        e.cursor.y = 0;
        apply_operator(&mut e, Operator::Delete, Motion::CurrentLine, Some(3));
        assert_eq!(e.buffer.lines, vec!["4"]);
    }

    #[test]
    fn yank_word_then_paste_keeps_buffer() {
        let mut e = editor_with(&["foo bar"]);
        e.cursor.x = 0;
        apply_operator(&mut e, Operator::Yank, Motion::WordForward, None);
        assert_eq!(e.buffer.lines[0], "foo bar"); // yank does not modify
        assert_eq!(e.register.text, "foo ");
        assert!(!e.register.linewise);
    }

    #[test]
    fn yank_flashes_the_copied_span() {
        let mut e = editor_with(&["foo bar"]);
        e.cursor.x = 0;
        apply_operator(&mut e, Operator::Yank, Motion::WordForward, None);
        let hl = e.yank_highlight.expect("yank should arm the flash");
        assert_eq!((hl.start, hl.end, hl.linewise), ((0, 0), (0, 4), false));
        assert!(hl.contains(0, 2)); // a char inside "foo " is highlighted
        assert!(!hl.contains(0, 4)); // the next word is not
    }

    #[test]
    fn linewise_yank_flashes_whole_lines() {
        let mut e = editor_with(&["one", "two", "three"]);
        e.cursor.y = 0;
        apply_operator(&mut e, Operator::Yank, Motion::CurrentLine, Some(2));
        let hl = e
            .yank_highlight
            .expect("linewise yank should arm the flash");
        assert!(hl.linewise);
        assert!(hl.contains(0, 0) && hl.contains(1, 2));
        assert!(!hl.contains(2, 0)); // the third line is outside a 2-line yank
    }

    #[test]
    fn de_deletes_through_word_end_inclusive() {
        let mut e = editor_with(&["alpha beta"]);
        e.cursor.x = 0;
        apply_operator(&mut e, Operator::Delete, Motion::WordEnd, None);
        assert_eq!(e.buffer.lines[0], " beta"); // "alpha" removed, including last char
        assert_eq!(e.register.text, "alpha");
    }

    #[test]
    fn find_char_forward_deletes_through_target_inclusive() {
        // `d>o`: operate onto (through) the first 'o'.
        let mut e = editor_with(&["hello world"]);
        e.cursor.x = 0;
        apply_operator(&mut e, Operator::Delete, Motion::FindChar('o'), None);
        assert_eq!(e.buffer.lines[0], " world"); // through the first 'o'
        assert_eq!(e.register.text, "hello");
    }

    #[test]
    fn find_char_not_found_is_noop() {
        let mut e = editor_with(&["abc"]);
        e.cursor.x = 0;
        apply_operator(&mut e, Operator::Delete, Motion::FindChar('z'), None);
        assert_eq!(e.buffer.lines[0], "abc"); // 'z' absent -> nothing deleted
    }

    #[test]
    fn dt_stops_before_target_char() {
        let mut e = editor_with(&["hello)world"]);
        e.cursor.x = 0;
        apply_operator(&mut e, Operator::Delete, Motion::TillChar(')'), None);
        assert_eq!(e.buffer.lines[0], ")world"); // "hello" removed, ')' kept
    }

    #[test]
    fn find_char_back_deletes_through_found_char() {
        // `d<X`: operate backward onto the previous 'X'.
        let mut e = editor_with(&["abcXabc"]);
        e.cursor.x = 6; // last 'c'
        apply_operator(&mut e, Operator::Delete, Motion::FindCharBack('X'), None);
        assert_eq!(e.buffer.lines[0], "abcc"); // "Xab" removed (X inclusive)
    }

    #[test]
    #[allow(non_snake_case)] // name mirrors the `dW` Glide motion under test
    fn dW_deletes_whole_big_word() {
        let mut e = editor_with(&["foo(bar) baz"]);
        e.cursor.x = 0;
        apply_operator(&mut e, Operator::Delete, Motion::WordForwardBig, None);
        assert_eq!(e.buffer.lines[0], "baz"); // whole "foo(bar) " WORD removed
    }

    #[test]
    fn change_word_enters_edit_mode() {
        let mut e = editor_with(&["alpha beta"]);
        e.mode = EditorMode::Glide;
        e.cursor.x = 0;
        apply_operator(&mut e, Operator::Change, Motion::WordForward, None);
        assert_eq!(e.buffer.lines[0], "beta");
        assert_eq!(e.mode, EditorMode::Edit);
    }
}
