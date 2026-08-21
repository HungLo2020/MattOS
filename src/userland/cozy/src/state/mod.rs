pub mod buffer;
pub mod cursor;
pub mod diff;
pub mod editor;
pub mod key;

pub use self::buffer::TextBuffer;
pub use self::cursor::Cursor;
pub use self::diff::{DiffLineKind, DiffReviewState};
pub use self::editor::{
    Config, EditorMode, EditorState, Register, ReplaceFocus, SearchMode, StatusKind, YankHighlight,
};
