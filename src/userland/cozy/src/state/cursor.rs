use crate::utils::wrap::{byte_at_visual_col, cursor_visual_pos, visual_col, wrap_chunks};

/// Sentinel goal column meaning "end of line" (mirrors vim's `curswant = MAXCOL`).
/// `byte_at_visual_col` clamps any target column past the row to the row end, so
/// passing this value naturally sticks the cursor to each line's end.
const EOL: usize = usize::MAX;

#[derive(Default, Debug, Clone, Copy)]
pub struct Cursor {
    pub x: usize,
    pub y: usize,
    /// Desired display column carried across a run of vertical moves (vim curswant).
    /// `None` = recompute from the current position; `Some(EOL)` = stick to line end.
    goal: Option<usize>,
    /// Position `(x, y)` the cursor occupied right after the last vertical move.
    /// If the live position no longer matches, something else moved the cursor and
    /// the goal is stale — so the next vertical move recomputes it.
    goal_at: Option<(usize, usize)>,
}

// Goal state is transient UI bookkeeping, not part of cursor identity: compare on
// position only so existing equality semantics are unchanged.
impl PartialEq for Cursor {
    fn eq(&self, other: &Self) -> bool {
        self.x == other.x && self.y == other.y
    }
}

pub fn first_non_whitespace_byte(line: &str) -> usize {
    line.char_indices()
        .find(|(_, c)| !c.is_whitespace())
        .map(|(b, _)| b)
        .unwrap_or(0)
}

/// vim's three character classes for small-word (`w`/`e`/`b`) motions: a word
/// boundary occurs wherever the class changes. WORD motions (`W`/`E`/`B`) ignore
/// this and split on whitespace only.
#[derive(PartialEq, Clone, Copy)]
enum CharClass {
    Blank,
    Word, // alphanumeric or underscore (a "keyword" char)
    Punct,
}

fn char_class(c: char) -> CharClass {
    if c.is_whitespace() {
        CharClass::Blank
    } else if c.is_alphanumeric() || c == '_' {
        CharClass::Word
    } else {
        CharClass::Punct
    }
}

impl Cursor {
    /// Refresh the goal column for a vertical move. Keeps the existing goal while
    /// vertical moves run back-to-back (anchor still matches); recomputes it from
    /// the current display column once anything else has moved the cursor.
    fn sync_goal(&mut self, cur_vcol: usize) {
        if self.goal.is_none() || self.goal_at != Some((self.x, self.y)) {
            self.goal = Some(cur_vcol);
        }
    }

    /// Record the post-move position as the goal anchor.
    fn stamp(&mut self) {
        self.goal_at = Some((self.x, self.y));
    }

    pub fn move_up(&mut self, lines: &[String]) {
        if self.y > 0 {
            self.sync_goal(visual_col(&lines[self.y], self.x));
            self.y -= 1;
            let line = &lines[self.y];
            self.x = byte_at_visual_col(line, 0, line.len(), self.goal.unwrap(), true);
            self.stamp();
        }
    }

    pub fn move_down(&mut self, lines: &[String]) {
        if self.y + 1 < lines.len() {
            self.sync_goal(visual_col(&lines[self.y], self.x));
            self.y += 1;
            let line = &lines[self.y];
            self.x = byte_at_visual_col(line, 0, line.len(), self.goal.unwrap(), true);
            self.stamp();
        }
    }

    /// Soft-wrap down: move to the next *visual* row, keeping the visual column.
    /// Steps through the wrapped sub-rows of the current line before descending to
    /// the next logical line. Falls back to logical `move_down` when `tw == 0`.
    pub fn move_down_visual(&mut self, lines: &[String], tw: usize) {
        if tw == 0 {
            self.move_down(lines);
            return;
        }
        let line = &lines[self.y];
        let (sub, vcol) = cursor_visual_pos(line, self.x, tw);
        self.sync_goal(vcol);
        let goal = self.goal.unwrap();
        let chunks = wrap_chunks(line, tw);
        if sub + 1 < chunks.len() {
            let (cs, ce) = chunks[sub + 1];
            let is_last = sub + 1 == chunks.len() - 1;
            self.x = byte_at_visual_col(line, cs, ce, goal, is_last);
        } else if self.y + 1 < lines.len() {
            self.y += 1;
            let next = &lines[self.y];
            let nchunks = wrap_chunks(next, tw);
            let (cs, ce) = nchunks[0];
            let is_last = nchunks.len() == 1;
            self.x = byte_at_visual_col(next, cs, ce, goal, is_last);
        } else {
            return; // already on the last visual row of the buffer -> no-op.
        }
        self.stamp();
    }

    /// Soft-wrap up: mirror of `move_down_visual`.
    pub fn move_up_visual(&mut self, lines: &[String], tw: usize) {
        if tw == 0 {
            self.move_up(lines);
            return;
        }
        let line = &lines[self.y];
        let (sub, vcol) = cursor_visual_pos(line, self.x, tw);
        self.sync_goal(vcol);
        let goal = self.goal.unwrap();
        if sub > 0 {
            let chunks = wrap_chunks(line, tw);
            let (cs, ce) = chunks[sub - 1];
            let is_last = sub - 1 == chunks.len() - 1;
            self.x = byte_at_visual_col(line, cs, ce, goal, is_last);
        } else if self.y > 0 {
            self.y -= 1;
            let prev = &lines[self.y];
            let pchunks = wrap_chunks(prev, tw);
            let (cs, ce) = pchunks[pchunks.len() - 1]; // last sub-row of the previous line
            self.x = byte_at_visual_col(prev, cs, ce, goal, true);
        } else {
            return; // already on the first visual row of the buffer -> no-op.
        }
        self.stamp();
    }

    pub fn move_left(&mut self, lines: &[String]) {
        if self.x > 0 {
            let line = &lines[self.y];
            self.x = line[..self.x]
                .char_indices()
                .next_back()
                .map(|(i, _)| i)
                .unwrap_or(0);
        } else if self.y > 0 {
            self.y -= 1;
            self.x = lines[self.y].len();
        }
    }

    pub fn move_right(&mut self, lines: &[String]) {
        let line = &lines[self.y];
        if self.x < line.len() {
            let ch = line[self.x..].chars().next().unwrap();
            self.x += ch.len_utf8();
        } else if self.y + 1 < lines.len() {
            self.y += 1;
            self.x = 0;
        }
    }

    pub fn move_home(&mut self) {
        self.x = 0;
        self.y = 0;
    }

    pub fn move_end(&mut self, lines: &[String]) {
        if !lines.is_empty() {
            self.y = lines.len() - 1;
            self.x = lines[self.y].len();
            self.goal = Some(EOL); // stick to line ends on subsequent vertical moves
            self.stamp();
        }
    }

    pub fn page_up(&mut self, lines: &[String], page_size: usize) {
        self.sync_goal(visual_col(&lines[self.y], self.x));
        if self.y >= page_size {
            self.y -= page_size;
        } else {
            self.y = 0;
        }
        let line = &lines[self.y];
        self.x = byte_at_visual_col(line, 0, line.len(), self.goal.unwrap(), true);
        self.stamp();
    }

    pub fn page_down(&mut self, lines: &[String], page_size: usize) {
        self.sync_goal(visual_col(&lines[self.y], self.x));
        if self.y + page_size < lines.len() {
            self.y += page_size;
        } else {
            self.y = lines.len().saturating_sub(1);
        }
        let line = &lines[self.y];
        self.x = byte_at_visual_col(line, 0, line.len(), self.goal.unwrap(), true);
        self.stamp();
    }

    /// Screen top (vim H): first visible line, on its first non-blank char.
    pub fn move_page_top(&mut self, scroll_offset: usize, lines: &[String]) {
        if lines.is_empty() {
            return;
        }
        self.y = scroll_offset.min(lines.len() - 1);
        self.x = first_non_whitespace_byte(&lines[self.y]);
    }

    /// Screen middle (vim M): midpoint of the visible range, on first non-blank.
    pub fn move_page_middle(&mut self, scroll_offset: usize, page_size: usize, lines: &[String]) {
        if lines.is_empty() {
            return;
        }
        let last = lines.len() - 1;
        let bottom_visible = (scroll_offset + page_size.saturating_sub(1)).min(last);
        self.y = ((scroll_offset + bottom_visible) / 2).min(last);
        self.x = first_non_whitespace_byte(&lines[self.y]);
    }

    // ── Glide mode movements ──────────────────────────────────────────────────

    pub fn move_line_start(&mut self) {
        self.x = 0;
    }

    pub fn move_line_end(&mut self, lines: &[String]) {
        if self.y < lines.len() {
            self.x = lines[self.y].len();
            self.goal = Some(EOL); // vim `$`: stick to line ends on subsequent vertical moves
            self.stamp();
        }
    }

    pub fn move_file_bottom(&mut self, lines: &[String]) {
        if !lines.is_empty() {
            self.y = lines.len() - 1;
            self.x = 0;
        }
    }

    /// vim `w`: start of the next small-word (stops at punctuation/class change).
    pub fn move_word_forward(&mut self, lines: &[String]) {
        let line = &lines[self.y];
        let chars: Vec<(usize, char)> = line.char_indices().collect();
        let cur = chars
            .iter()
            .position(|&(b, _)| b >= self.x)
            .unwrap_or(chars.len());
        let mut i = cur;
        if i < chars.len() {
            let cls = char_class(chars[i].1);
            if cls != CharClass::Blank {
                while i < chars.len() && char_class(chars[i].1) == cls {
                    i += 1;
                }
            }
            while i < chars.len() && char_class(chars[i].1) == CharClass::Blank {
                i += 1;
            }
        }
        if i < chars.len() {
            self.x = chars[i].0;
        } else if self.y + 1 < lines.len() {
            self.y += 1;
            self.x = first_non_whitespace_byte(&lines[self.y]);
        } else {
            self.x = line.len();
        }
    }

    /// vim `e`: last char of the current/next small-word (class-aware) on this line.
    pub fn move_word_end(&mut self, lines: &[String]) {
        let line = &lines[self.y];
        let chars: Vec<(usize, char)> = line.char_indices().collect();
        if chars.is_empty() {
            return;
        }
        // Index of the char at/after the cursor, then step at least one forward.
        let mut i = chars
            .iter()
            .position(|&(b, _)| b >= self.x)
            .unwrap_or(chars.len() - 1);
        i = (i + 1).min(chars.len() - 1);
        while i < chars.len() && char_class(chars[i].1) == CharClass::Blank {
            i += 1;
        }
        if i >= chars.len() {
            self.x = chars.last().unwrap().0;
            return;
        }
        let cls = char_class(chars[i].1);
        while i + 1 < chars.len() && char_class(chars[i + 1].1) == cls {
            i += 1;
        }
        self.x = chars[i].0;
    }

    /// vim `b`: start of the current/previous small-word (class-aware).
    pub fn move_word_backward(&mut self, lines: &[String]) {
        if self.x == 0 {
            if self.y > 0 {
                self.y -= 1;
                self.x = lines[self.y].len();
            }
            return;
        }
        let line = &lines[self.y];
        let chars: Vec<(usize, char)> = line.char_indices().collect();
        let cur = chars
            .iter()
            .rposition(|&(b, _)| b < self.x)
            .map(|i| i + 1)
            .unwrap_or(0);
        if cur == 0 {
            self.x = 0;
            return;
        }
        let mut i = cur - 1;
        while i > 0 && char_class(chars[i].1) == CharClass::Blank {
            i -= 1;
        }
        if char_class(chars[i].1) == CharClass::Blank {
            self.x = 0;
            return;
        }
        let cls = char_class(chars[i].1);
        while i > 0 && char_class(chars[i - 1].1) == cls {
            i -= 1;
        }
        self.x = chars[i].0;
    }

    /// vim `W`: start of the next WORD (whitespace-delimited; punctuation included).
    pub fn move_word_forward_big(&mut self, lines: &[String]) {
        let line = &lines[self.y];
        let chars: Vec<(usize, char)> = line.char_indices().collect();
        let cur = chars
            .iter()
            .position(|&(b, _)| b >= self.x)
            .unwrap_or(chars.len());
        let mut i = cur;
        while i < chars.len() && !chars[i].1.is_whitespace() {
            i += 1;
        }
        while i < chars.len() && chars[i].1.is_whitespace() {
            i += 1;
        }
        if i < chars.len() {
            self.x = chars[i].0;
        } else if self.y + 1 < lines.len() {
            self.y += 1;
            self.x = first_non_whitespace_byte(&lines[self.y]);
        } else {
            self.x = line.len();
        }
    }

    /// vim `E`: last char of the current/next WORD (whitespace-delimited).
    pub fn move_word_end_big(&mut self, lines: &[String]) {
        let line = &lines[self.y];
        let chars: Vec<(usize, char)> = line.char_indices().collect();
        if chars.is_empty() {
            return;
        }
        let mut i = chars
            .iter()
            .position(|&(b, _)| b >= self.x)
            .unwrap_or(chars.len() - 1);
        i = (i + 1).min(chars.len() - 1);
        while i < chars.len() && chars[i].1.is_whitespace() {
            i += 1;
        }
        if i >= chars.len() {
            self.x = chars.last().unwrap().0;
            return;
        }
        while i + 1 < chars.len() && !chars[i + 1].1.is_whitespace() {
            i += 1;
        }
        self.x = chars[i].0;
    }

    /// vim `B`: start of the current/previous WORD (whitespace-delimited).
    pub fn move_word_backward_big(&mut self, lines: &[String]) {
        if self.x == 0 {
            if self.y > 0 {
                self.y -= 1;
                self.x = lines[self.y].len();
            }
            return;
        }
        let line = &lines[self.y];
        let chars: Vec<(usize, char)> = line.char_indices().collect();
        let cur = chars
            .iter()
            .rposition(|&(b, _)| b < self.x)
            .map(|i| i + 1)
            .unwrap_or(0);
        if cur == 0 {
            self.x = 0;
            return;
        }
        let mut i = cur - 1;
        while i > 0 && chars[i].1.is_whitespace() {
            i -= 1;
        }
        if chars[i].1.is_whitespace() {
            self.x = 0;
            return;
        }
        while i > 0 && !chars[i - 1].1.is_whitespace() {
            i -= 1;
        }
        self.x = chars[i].0;
    }

    pub fn move_next_line_non_ws(&mut self, lines: &[String]) {
        if self.y + 1 < lines.len() {
            self.y += 1;
            self.x = first_non_whitespace_byte(&lines[self.y]);
        }
    }

    pub fn move_prev_line_non_ws(&mut self, lines: &[String]) {
        if self.y > 0 {
            self.y -= 1;
            self.x = first_non_whitespace_byte(&lines[self.y]);
        }
    }

    /// Screen bottom (vim L): last visible line, on its first non-blank char.
    pub fn move_page_bottom(&mut self, scroll_offset: usize, page_size: usize, lines: &[String]) {
        let target_y = scroll_offset + page_size - 1;
        if target_y < lines.len() {
            self.y = target_y;
        } else {
            self.y = lines.len().saturating_sub(1);
        }
        if self.y < lines.len() {
            self.x = first_non_whitespace_byte(&lines[self.y]);
        } else {
            self.x = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Each line has two leading spaces, so first-non-blank column is 2.
    fn lines(n: usize) -> Vec<String> {
        (0..n).map(|i| format!("  line{}", i)).collect()
    }

    // ── sticky / goal column (vim curswant) ──────────────────────────────────

    #[test]
    fn sticky_column_restores_after_short_line() {
        // (A) goal column survives a pass through a short line.
        let ls = vec![
            "abcdefghij".to_string(),
            "xy".to_string(),
            "ABCDEFGHIJ".to_string(),
        ];
        let mut c = Cursor {
            x: 5,
            y: 0,
            ..Default::default()
        }; // col 5 ('f')
        c.move_down(&ls);
        assert_eq!((c.y, c.x), (1, 2)); // "xy": display-clamped to its end
        c.move_down(&ls);
        assert_eq!((c.y, c.x), (2, 5)); // long line again -> back to col 5 (not the end)
    }

    #[test]
    fn sticky_eol_after_line_end_sticks_to_line_ends() {
        // (B) move_line_end ($) sets an EOL goal: vertical moves stick to each end.
        let ls = vec![
            "short".to_string(),
            "a much longer line".to_string(),
            "mid".to_string(),
        ];
        let mut c = Cursor::default();
        c.move_line_end(&ls); // y0, x=5 (end of "short"), goal = EOL
        c.move_down(&ls);
        assert_eq!((c.y, c.x), (1, ls[1].len())); // end of the long line
        c.move_down(&ls);
        assert_eq!((c.y, c.x), (2, ls[2].len())); // end of "mid"
    }

    #[test]
    fn horizontal_move_resets_goal() {
        // A horizontal move between vertical moves re-anchors the goal column.
        let ls = vec![
            "abcdefghij".to_string(),
            "xy".to_string(),
            "ABCDEFGHIJ".to_string(),
        ];
        let mut c = Cursor {
            x: 8,
            y: 0,
            ..Default::default()
        }; // col 8
        c.move_down(&ls);
        assert_eq!((c.y, c.x), (1, 2)); // "xy" end, goal still 8
        c.move_left(&ls); // now at col 1 -> breaks the anchor
        c.move_down(&ls);
        assert_eq!((c.y, c.x), (2, 1)); // recomputed goal = 1, not the original 8
    }

    #[test]
    fn sticky_column_is_display_aware_with_wide_chars() {
        // Non-wrap vertical move keeps the *display* column across wide chars.
        let ls = vec!["aあb".to_string(), "xyzw".to_string()]; // 'a'@0 'あ'@1..4 'b'@4
        let mut c = Cursor {
            x: 4,
            y: 0,
            ..Default::default()
        }; // on 'b', display col 3
        c.move_down(&ls);
        assert_eq!((c.y, c.x), (1, 3)); // display col 3 -> 'w' @ byte 3
    }

    #[test]
    fn screen_motions_full_viewport() {
        let ls = lines(100);
        let mut c = Cursor::default();
        c.move_page_top(10, &ls);
        assert_eq!((c.y, c.x), (10, 2)); // H: first visible line
        c.move_page_bottom(10, 20, &ls);
        assert_eq!((c.y, c.x), (29, 2)); // L: scroll_offset + page_size - 1
        c.move_page_middle(10, 20, &ls);
        assert_eq!((c.y, c.x), (19, 2)); // M: midpoint of visible range
    }

    #[test]
    fn visual_down_steps_into_wrap_then_next_line() {
        // tw 5: "abcdefg" -> ["abcde","fg"], then line "XY".
        let ls = vec!["abcdefg".to_string(), "XY".to_string()];
        let mut c = Cursor {
            x: 0,
            y: 0,
            ..Default::default()
        };
        c.move_down_visual(&ls, 5);
        assert_eq!((c.y, c.x), (0, 5)); // sub1 "fg", vcol0 -> 'f' @ byte5
        c.move_down_visual(&ls, 5);
        assert_eq!((c.y, c.x), (1, 0)); // next logical line, sub0 vcol0
        c.move_down_visual(&ls, 5);
        assert_eq!((c.y, c.x), (1, 0)); // last visual row -> no-op
    }

    #[test]
    fn visual_down_preserves_column() {
        // Regression for the reported bug shape: descend within a wrapped last line.
        let ls = vec!["abcdefghij".to_string()]; // tw5 -> ["abcde","fghij"]
        let mut c = Cursor {
            x: 2,
            y: 0,
            ..Default::default()
        }; // sub0, vcol2 ('c')
        c.move_down_visual(&ls, 5);
        assert_eq!((c.y, c.x), (0, 7)); // sub1 vcol2 -> "fghij"[2]='h' @ byte7
        c.move_down_visual(&ls, 5);
        assert_eq!((c.y, c.x), (0, 7)); // last visual row of the only (last) line -> no-op
    }

    #[test]
    fn visual_up_steps_back_through_wrap_then_prev_line() {
        let ls = vec!["abcde".to_string(), "fghijklmn".to_string()]; // tw5: line1 -> ["fghij","klmn"]
        let mut c = Cursor {
            x: 5,
            y: 1,
            ..Default::default()
        }; // line1 sub1 ('k'), vcol0
        c.move_up_visual(&ls, 5);
        assert_eq!((c.y, c.x), (1, 0)); // sub1 -> sub0, vcol0
        c.move_up_visual(&ls, 5);
        assert_eq!((c.y, c.x), (0, 0)); // prev line's last (only) sub-row, vcol0
        c.move_up_visual(&ls, 5);
        assert_eq!((c.y, c.x), (0, 0)); // first visual row -> no-op
    }

    #[test]
    fn visual_falls_back_to_logical_when_tw_zero() {
        let ls = vec!["abc".to_string(), "de".to_string()];
        let mut c = Cursor {
            x: 1,
            y: 0,
            ..Default::default()
        };
        c.move_down_visual(&ls, 0); // tw==0 -> logical move_down
        assert_eq!((c.y, c.x), (1, 1));
    }

    #[test]
    fn screen_motions_short_buffer_clamps() {
        let ls = lines(5); // fewer lines than the viewport
        let mut c = Cursor::default();
        c.move_page_top(0, &ls);
        assert_eq!(c.y, 0);
        c.move_page_bottom(0, 20, &ls);
        assert_eq!(c.y, 4); // clamped to last line
        c.move_page_middle(0, 20, &ls);
        assert_eq!(c.y, 2); // midpoint of the actual 5 lines
    }
}
