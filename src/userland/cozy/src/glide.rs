//! Glide-mode motion engine (data-oriented).
//!
//! A `Motion` is "just a coordinate": [`resolve`] computes where it lands from
//! the current cursor without mutating the buffer. Bare movement sets the
//! cursor to that target; operators (later phase) act over the span to it — the
//! same motion definition serves both uses.

use crate::state::EditorState;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Motion {
    Left,
    Right,
    Up,
    Down,
    WordForward,
    WordBackward,
    WordEnd,
    /// WORD motions (whitespace-delimited; punctuation is part of the WORD): `W`/`B`/`E`.
    WordForwardBig,
    WordBackwardBig,
    WordEndBig,
    /// Find the next occurrence of a char on the current line (`f<char>`).
    FindChar(char),
    /// Up to (one before) the next occurrence of a char (`t<char>`).
    TillChar(char),
    /// Backward to the previous occurrence of a char (`F<char>`).
    FindCharBack(char),
    /// Backward to one char after the previous occurrence (`T<char>`).
    TillCharBack(char),
    LineStart,
    /// First non-whitespace char of the current line (`^`).
    LineStartNonBlank,
    LineEnd,
    FileTop,
    FileBottom,
    ScreenTop,
    ScreenMiddle,
    ScreenBottom,
    NextLineNonBlank,
    PrevLineNonBlank,
    /// The current line (and count-1 following lines): the operand of `dd`/`yy`/`cc`.
    CurrentLine,
}

/// A Glide operator: a verb that acts over the span a motion describes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Operator {
    Delete,
    Change,
    Yank,
}

impl Operator {
    /// The Glide key that starts this operator (and repeats it for linewise: `dd`).
    pub fn key(self) -> char {
        match self {
            Operator::Delete => 'd',
            Operator::Change => 'c',
            Operator::Yank => 'y',
        }
    }
}

/// Which find-char family a pending key belongs to. The next typed char becomes
/// the target, producing the corresponding [`Motion`]. Shared by `f`/`t`/`F`/`T`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FindKind {
    Find,     // f: forward, onto the char
    Till,     // t: forward, one before the char
    FindBack, // F: backward, onto the char
    TillBack, // T: backward, one after the char
}

impl FindKind {
    /// Build the motion that lands on/around `c`.
    pub fn motion(self, c: char) -> Motion {
        match self {
            FindKind::Find => Motion::FindChar(c),
            FindKind::Till => Motion::TillChar(c),
            FindKind::FindBack => Motion::FindCharBack(c),
            FindKind::TillBack => Motion::TillCharBack(c),
        }
    }

    /// The forward sibling in the same find/till family (for `.`).
    pub fn forward(self) -> FindKind {
        match self {
            FindKind::Find | FindKind::FindBack => FindKind::Find,
            FindKind::Till | FindKind::TillBack => FindKind::Till,
        }
    }

    /// The backward sibling in the same find/till family (for `,`).
    pub fn backward(self) -> FindKind {
        match self {
            FindKind::Find | FindKind::FindBack => FindKind::FindBack,
            FindKind::Till | FindKind::TillBack => FindKind::TillBack,
        }
    }
}

/// How a motion's span is interpreted when applied with an operator.
/// `CharExclusive`: up to (not including) the target. `CharInclusive`: through the
/// target char (`e`, `f`). `Linewise`: whole lines.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MotionKind {
    CharExclusive,
    CharInclusive,
    Linewise,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MotionResult {
    /// Target position as (line_y, byte_x).
    pub target: (usize, usize),
    /// The fully-moved cursor, carrying goal-column state for pure-motion commits.
    /// Operators use `target` instead and must not adopt this.
    pub cursor: crate::state::cursor::Cursor,
    pub kind: MotionKind,
}

impl Motion {
    /// If this is a to-char motion, its find/till family and target char.
    /// Used to remember the last `>`/`<`/`t`/`T` for `.`/`,` repeat.
    pub fn as_find(self) -> Option<(FindKind, char)> {
        match self {
            Motion::FindChar(c) => Some((FindKind::Find, c)),
            Motion::TillChar(c) => Some((FindKind::Till, c)),
            Motion::FindCharBack(c) => Some((FindKind::FindBack, c)),
            Motion::TillCharBack(c) => Some((FindKind::TillBack, c)),
            _ => None,
        }
    }

    pub fn kind(self) -> MotionKind {
        use Motion::*;
        match self {
            Up | Down | FileTop | FileBottom | ScreenTop | ScreenMiddle | ScreenBottom
            | NextLineNonBlank | PrevLineNonBlank | CurrentLine => MotionKind::Linewise,
            // Inclusive: the operator acts through the target char (e, E, f, t).
            WordEnd | WordEndBig | FindChar(_) | TillChar(_) => MotionKind::CharInclusive,
            // Backward find (F/T): the target is the lower bound of the span, so a
            // half-open [target, cursor) already includes it — exclusive is correct.
            Left | Right | WordForward | WordBackward | WordForwardBig | WordBackwardBig
            | FindCharBack(_) | TillCharBack(_) | LineStart | LineStartNonBlank | LineEnd => {
                MotionKind::CharExclusive
            }
        }
    }
}

/// Resolve a motion to its target position from the current cursor.
///
/// `count` is `Some(n)` only when the user typed a count prefix (`None` = a bare
/// key). The computation delegates to the existing `Cursor` movement methods on
/// a throwaway copy, so the result is identical to the pre-engine code.
pub fn resolve(motion: Motion, count: Option<usize>, editor: &EditorState) -> MotionResult {
    let n = count.unwrap_or(1).max(1);
    let lines = &editor.buffer.lines;
    let mut c = editor.cursor; // Cursor is Copy

    match motion {
        Motion::Left => {
            for _ in 0..n {
                c.move_left(lines);
            }
        }
        Motion::Right => {
            for _ in 0..n {
                c.move_right(lines);
            }
        }
        Motion::Up => {
            for _ in 0..n {
                c.move_up(lines);
            }
        }
        Motion::Down => {
            for _ in 0..n {
                c.move_down(lines);
            }
        }
        Motion::WordForward => {
            for _ in 0..n {
                c.move_word_forward(lines);
            }
        }
        Motion::WordBackward => {
            for _ in 0..n {
                c.move_word_backward(lines);
            }
        }
        Motion::WordEnd => {
            for _ in 0..n {
                c.move_word_end(lines);
            }
        }
        Motion::WordForwardBig => {
            for _ in 0..n {
                c.move_word_forward_big(lines);
            }
        }
        Motion::WordBackwardBig => {
            for _ in 0..n {
                c.move_word_backward_big(lines);
            }
        }
        Motion::WordEndBig => {
            for _ in 0..n {
                c.move_word_end_big(lines);
            }
        }
        // Find the n-th occurrence of `target` after the cursor on this line.
        Motion::FindChar(target) => {
            let line = &lines[c.y];
            let mut found = c.x;
            let mut remaining = n;
            for (b, ch) in line.char_indices() {
                if b <= c.x {
                    continue;
                }
                if ch == target {
                    remaining -= 1;
                    if remaining == 0 {
                        found = b;
                        break;
                    }
                }
            }
            c.x = found; // unchanged if not found -> operator becomes a no-op
        }
        // Like FindChar but stop one char before the target (`t`).
        Motion::TillChar(target) => {
            let line = &lines[c.y];
            let mut found_b = None;
            let mut remaining = n;
            for (b, ch) in line.char_indices() {
                if b <= c.x {
                    continue;
                }
                if ch == target {
                    remaining -= 1;
                    if remaining == 0 {
                        found_b = Some(b);
                        break;
                    }
                }
            }
            if let Some(b) = found_b {
                c.x = line[..b]
                    .char_indices()
                    .next_back()
                    .map(|(i, _)| i)
                    .unwrap_or(c.x);
            }
        }
        // Backward find: n-th occurrence of `target` before the cursor (`F`).
        Motion::FindCharBack(target) => {
            let line = &lines[c.y];
            let mut found = c.x;
            let mut remaining = n;
            for (b, ch) in line.char_indices().rev() {
                if b >= c.x {
                    continue;
                }
                if ch == target {
                    remaining -= 1;
                    if remaining == 0 {
                        found = b;
                        break;
                    }
                }
            }
            c.x = found;
        }
        // Backward till: one char after the n-th previous occurrence (`T`).
        Motion::TillCharBack(target) => {
            let line = &lines[c.y];
            let mut found_b = None;
            let mut remaining = n;
            for (b, ch) in line.char_indices().rev() {
                if b >= c.x {
                    continue;
                }
                if ch == target {
                    remaining -= 1;
                    if remaining == 0 {
                        found_b = Some(b);
                        break;
                    }
                }
            }
            if let Some(b) = found_b {
                let ch = line[b..].chars().next().unwrap();
                c.x = b + ch.len_utf8();
            }
        }
        Motion::LineStart => c.move_line_start(),
        Motion::LineStartNonBlank => {
            c.x = crate::state::cursor::first_non_whitespace_byte(&lines[c.y])
        }
        Motion::LineEnd => c.move_line_end(lines),
        // `Ngg` jumps to line N (1-indexed), mirroring `NG`; bare `gg` -> line 1.
        Motion::FileTop => match count {
            Some(line) => {
                let last = lines.len().saturating_sub(1);
                c.y = line.saturating_sub(1).min(last);
                c.x = 0;
            }
            None => {
                c.y = 0;
                c.x = 0;
            }
        },
        // `NG` jumps to line N (1-indexed); bare `G` goes to the last line.
        Motion::FileBottom => match count {
            Some(line) => {
                let last = lines.len().saturating_sub(1);
                c.y = line.saturating_sub(1).min(last);
                c.x = 0;
            }
            None => c.move_file_bottom(lines),
        },
        Motion::ScreenTop => c.move_page_top(editor.scroll_offset, lines),
        Motion::ScreenMiddle => c.move_page_middle(editor.scroll_offset, editor.page_size, lines),
        Motion::ScreenBottom => c.move_page_bottom(editor.scroll_offset, editor.page_size, lines),
        Motion::NextLineNonBlank => {
            for _ in 0..n {
                c.move_next_line_non_ws(lines);
            }
        }
        Motion::PrevLineNonBlank => {
            for _ in 0..n {
                c.move_prev_line_non_ws(lines);
            }
        }
        // `Ndd`/`Nyy`: the current line plus the next n-1 lines.
        Motion::CurrentLine => {
            let last = lines.len().saturating_sub(1);
            c.y = (c.y + n - 1).min(last);
            c.x = 0;
        }
    }

    MotionResult {
        target: (c.y, c.x),
        cursor: c,
        kind: motion.kind(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{EditorState, TextBuffer};

    fn editor_with(lines: &[&str]) -> EditorState {
        let mut e = EditorState::new(None);
        e.buffer = TextBuffer::from_lines(lines.iter().map(|s| s.to_string()).collect());
        e
    }

    #[test]
    fn word_forward_matches_cursor_method() {
        let e = editor_with(&["alpha beta gamma"]);
        let r = resolve(Motion::WordForward, None, &e);
        assert_eq!(r.target, (0, 6)); // start of "beta"
        let r3 = resolve(Motion::WordForward, Some(2), &e);
        assert_eq!(r3.target, (0, 11)); // two words -> start of "gamma"
    }

    #[test]
    fn file_bottom_count_jumps_to_line() {
        let e = editor_with(&["1", "2", "3", "4", "5"]);
        assert_eq!(resolve(Motion::FileBottom, None, &e).target, (4, 0)); // G -> last
        assert_eq!(resolve(Motion::FileBottom, Some(3), &e).target, (2, 0)); // 3G -> line 3
        assert_eq!(resolve(Motion::FileTop, None, &e).target, (0, 0)); // gg -> first
        assert_eq!(resolve(Motion::FileTop, Some(3), &e).target, (2, 0)); // 3gg -> line 3
    }

    #[test]
    fn resolve_does_not_move_the_real_cursor() {
        let mut e = editor_with(&["hello world"]);
        e.cursor.x = 0;
        let _ = resolve(Motion::LineEnd, None, &e);
        assert_eq!((e.cursor.y, e.cursor.x), (0, 0)); // pure: real cursor untouched
    }

    #[test]
    fn motion_kind_classification() {
        assert_eq!(Motion::Down.kind(), MotionKind::Linewise);
        assert_eq!(Motion::WordForward.kind(), MotionKind::CharExclusive);
        assert_eq!(Motion::LineEnd.kind(), MotionKind::CharExclusive);
        assert_eq!(Motion::CurrentLine.kind(), MotionKind::Linewise);
        assert_eq!(Motion::TillChar(')').kind(), MotionKind::CharInclusive);
        assert_eq!(Motion::FindCharBack('x').kind(), MotionKind::CharExclusive);
    }

    #[test]
    fn small_word_stops_at_punctuation() {
        // `w` (small word) stops at the '(' between "foo" and "bar".
        let mut e = editor_with(&["foo(bar)"]);
        e.cursor.x = 0;
        assert_eq!(resolve(Motion::WordForward, None, &e).target, (0, 3)); // onto '('
        // `W` (WORD) skips the whole "foo(bar)" run -> end of line.
        assert_eq!(resolve(Motion::WordForwardBig, None, &e).target, (0, 8));
    }

    #[test]
    fn caret_lands_on_first_non_blank() {
        let mut e = editor_with(&["    hello"]);
        e.cursor.x = 7;
        assert_eq!(resolve(Motion::LineStartNonBlank, None, &e).target, (0, 4));
    }

    #[test]
    fn till_and_find_back_targets() {
        let mut e = editor_with(&["hello)world"]);
        e.cursor.x = 0;
        // t) lands one char before ')'.
        assert_eq!(resolve(Motion::TillChar(')'), None, &e).target, (0, 4));
        // F backward from the 'w' onto the previous 'o'.
        e.cursor.x = 6; // on 'w'
        assert_eq!(resolve(Motion::FindCharBack('o'), None, &e).target, (0, 4));
        // T backward lands one char after the found 'h'.
        e.cursor.x = 6;
        assert_eq!(resolve(Motion::TillCharBack('h'), None, &e).target, (0, 1));
    }
}
