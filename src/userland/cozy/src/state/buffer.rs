use crate::state::Cursor;

#[derive(Clone)]
pub struct TextBuffer {
    pub lines: Vec<String>,
}

impl TextBuffer {
    pub fn new() -> Self {
        Self {
            lines: vec![String::new()],
        }
    }

    pub fn from_lines(lines: Vec<String>) -> Self {
        if lines.is_empty() {
            Self::new()
        } else {
            Self { lines }
        }
    }

    pub fn insert_char(&mut self, c: char, cursor: &mut Cursor) {
        if let Some(line) = self.lines.get_mut(cursor.y) {
            line.insert(cursor.x, c);
            cursor.x += c.len_utf8();
        }
    }

    pub fn enter(&mut self, cursor: &mut Cursor) {
        if let Some(line) = self.lines.get_mut(cursor.y) {
            let tail = line.split_off(cursor.x);
            self.lines.insert(cursor.y + 1, tail);
            cursor.y += 1;
            cursor.x = 0;
        }
    }

    pub fn backspace(&mut self, cursor: &mut Cursor) {
        if cursor.x > 0 {
            if let Some(line) = self.lines.get_mut(cursor.y) {
                // Find the byte start of the previous character
                let prev = line[..cursor.x]
                    .char_indices()
                    .next_back()
                    .map(|(i, _)| i)
                    .unwrap_or(0);
                line.remove(prev);
                cursor.x = prev;
            }
        } else if cursor.y > 0 {
            let prev_len = self.lines[cursor.y - 1].len();
            let current = self.lines.remove(cursor.y);
            cursor.y -= 1;
            cursor.x = prev_len;
            if let Some(prev_line) = self.lines.get_mut(cursor.y) {
                prev_line.push_str(&current);
            }
        }
    }

    pub fn delete(&mut self, cursor: &mut Cursor) {
        if cursor.y < self.lines.len() {
            let line_len = self.lines[cursor.y].len();
            if cursor.x < line_len {
                self.lines[cursor.y].remove(cursor.x);
            } else if cursor.y + 1 < self.lines.len() {
                let next = self.lines.remove(cursor.y + 1);
                self.lines[cursor.y].push_str(&next);
            }
        }
    }
}
