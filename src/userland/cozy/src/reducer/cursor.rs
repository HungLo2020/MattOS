use crate::reducer::EventResult;
use crate::state::Cursor;

pub fn page_up(cursor: &mut Cursor, lines: &[String], page_size: usize) -> EventResult {
    cursor.page_up(lines, page_size);
    EventResult::Continue
}

pub fn page_down(cursor: &mut Cursor, lines: &[String], page_size: usize) -> EventResult {
    cursor.page_down(lines, page_size);
    EventResult::Continue
}

pub fn move_home(cursor: &mut Cursor) -> EventResult {
    cursor.move_home();
    EventResult::Continue
}

pub fn move_end(cursor: &mut Cursor, lines: &[String]) -> EventResult {
    cursor.move_end(lines);
    EventResult::Continue
}
