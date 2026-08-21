use unicode_width::UnicodeWidthChar;

/// Returns the display column width of the substring `s[..byte_pos]`.
/// Byte pos must be on a char boundary.
pub fn display_width_up_to(s: &str, byte_pos: usize) -> usize {
    s[..byte_pos.min(s.len())]
        .chars()
        .map(|c| c.width().unwrap_or(0))
        .sum()
}
