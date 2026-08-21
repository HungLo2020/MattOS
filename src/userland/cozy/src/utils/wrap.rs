use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// Split `line` into byte ranges that each fit within `width` display columns.
/// Always returns at least one element.
pub fn wrap_chunks(line: &str, width: usize) -> Vec<(usize, usize)> {
    if width == 0 || line.is_empty() {
        return vec![(0, line.len())];
    }
    let mut chunks = Vec::new();
    let mut chunk_start = 0usize;
    let mut col = 0usize;

    for (i, ch) in line.char_indices() {
        let w = UnicodeWidthChar::width(ch).unwrap_or(1);
        if col + w > width && col > 0 {
            chunks.push((chunk_start, i));
            chunk_start = i;
            col = w;
        } else {
            col += w;
        }
    }
    chunks.push((chunk_start, line.len()));
    chunks
}

/// Number of visual rows a line occupies when wrapped to `width` columns.
pub fn visual_row_count(line: &str, width: usize) -> usize {
    wrap_chunks(line, width).len()
}

/// Byte offset (char boundary) within chunk `[cs, ce)` at visual column
/// `target_vcol` (0 = chunk start). Honors wide chars via unicode-width.
/// When `target_vcol` is past the chunk's content it clamps:
///   - `is_last_chunk`: returns `ce` (the line end), so the cursor may sit one
///     past the final char like a normal end-of-line position.
///   - otherwise: returns the start byte of the last char in the chunk, keeping
///     the cursor on this sub-row instead of spilling onto the next one.
pub fn byte_at_visual_col(
    line: &str,
    cs: usize,
    ce: usize,
    target_vcol: usize,
    is_last_chunk: bool,
) -> usize {
    let chunk = &line[cs..ce];
    if chunk.is_empty() {
        return cs;
    }
    let mut w = 0usize;
    let mut last_char_start = cs;
    for (i, ch) in chunk.char_indices() {
        let cw = UnicodeWidthChar::width(ch).unwrap_or(1);
        let byte = cs + i;
        if target_vcol < w + cw {
            return byte;
        }
        last_char_start = byte;
        w += cw;
    }
    // target_vcol is at or past the chunk's width: clamp.
    if is_last_chunk { ce } else { last_char_start }
}

/// Display column of byte offset `cx` within `line` (whole-line, ignoring wrap).
/// Used by non-wrapped vertical movement to keep a width-aware goal column.
pub fn visual_col(line: &str, cx: usize) -> usize {
    let end = cx.min(line.len());
    UnicodeWidthStr::width(&line[..end])
}

/// Returns `(sub_row, visual_col)` for byte offset `cx` within `line`.
/// `sub_row` is which wrapped chunk (0-indexed); `visual_col` is the display
/// column within that chunk.
pub fn cursor_visual_pos(line: &str, cx: usize, width: usize) -> (usize, usize) {
    let chunks = wrap_chunks(line, width);
    let last = chunks.len().saturating_sub(1);
    for (idx, &(s, e)) in chunks.iter().enumerate() {
        let in_chunk = if idx == last {
            cx >= s
        } else {
            cx >= s && cx < e
        };
        if in_chunk {
            let end = cx.min(line.len());
            let before = if s <= end { &line[s..end] } else { "" };
            return (idx, UnicodeWidthStr::width(before));
        }
    }
    (0, 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_at_col_ascii() {
        let line = "abcdef";
        assert_eq!(byte_at_visual_col(line, 0, 6, 0, true), 0);
        assert_eq!(byte_at_visual_col(line, 0, 6, 3, true), 3);
        // Past the end of the last chunk clamps to line end (one past final char).
        assert_eq!(byte_at_visual_col(line, 0, 6, 10, true), 6);
    }

    #[test]
    fn byte_at_col_non_last_chunk_clamps_to_last_char() {
        // chunk [0,3) = "abc"; overshooting must stay on this sub-row (start of 'c').
        assert_eq!(byte_at_visual_col("abcdef", 0, 3, 10, false), 2);
    }

    #[test]
    fn byte_at_col_wide_chars() {
        // "あいう": each width 2, bytes 0..3, 3..6, 6..9.
        let line = "あいう";
        assert_eq!(byte_at_visual_col(line, 0, 9, 0, true), 0); // あ
        assert_eq!(byte_at_visual_col(line, 0, 9, 2, true), 3); // い
        assert_eq!(byte_at_visual_col(line, 0, 9, 3, true), 3); // mid of い snaps to い start
        assert_eq!(byte_at_visual_col(line, 0, 9, 4, true), 6); // う
        // Past end: last chunk -> line end; non-last -> start of last char (う @ 6).
        assert_eq!(byte_at_visual_col(line, 0, 9, 99, true), 9);
        assert_eq!(byte_at_visual_col(line, 0, 9, 99, false), 6);
    }

    #[test]
    fn byte_at_col_empty_chunk() {
        assert_eq!(byte_at_visual_col("", 0, 0, 0, true), 0);
        assert_eq!(byte_at_visual_col("abc", 3, 3, 5, true), 3);
    }
}
