pub mod file;
pub mod highlight;
// Regex highlighter: the fallback engine when the `treesitter` feature is off.
#[cfg(not(feature = "treesitter"))]
pub mod syntax;
pub mod terminal;
pub mod unicode;
pub mod wrap;
