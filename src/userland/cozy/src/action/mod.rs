use crate::glide::{FindKind, Motion, Operator};
use crate::state::EditorMode;

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    // Navigation
    MoveUp,
    MoveDown,
    MoveLeft,
    MoveRight,
    PageUp,
    PageDown,
    PageTop,
    PageBottom,
    Home,
    End,

    // Editing
    InsertChar(char),
    InsertString(String), // Paste operation
    Backspace,
    Delete,
    Enter,

    // Mode Switching
    EnterMode(EditorMode),

    // Execution
    Save(String),
    SaveAndExit(String), // Ctrl+X → Enter: save, then exit
    Open(String),
    SearchNext,
    SearchPrevious,
    Undo,
    Redo,
    ReplaceCurrent,
    ReplaceAll,

    SwitchFocus,
    ToggleSearchMode,

    // System
    Quit,              // Force Quit (Ctrl+Q)
    Cancel,            // Esc
    ReloadConfig,      // F5
    ToggleLineNumbers, // Ctrl+L
    DeleteLine,        // Ctrl+K
    GotoLine(usize),   // 行ジャンプ (1-indexed)
    PasteFromClipboard,
    ToggleWrap,
    ToggleMarkdownPreview,
    ToggleDiffReview,  // F4 / Review.SessionDiff: enter or leave session diff review
    DiffToggleApprove, // Space: approve/un-approve the current hunk
    DiffApproveAll,    // a: approve all hunks (or un-approve all when already full)
    DiffStageApproved, // s: stage the approved hunks into git's index (git add)
    DiffCommitApproved, // c/Enter: open the commit-message prompt for approved hunks
    DiffCommitConfirm, // Enter in DiffCommitMsg: stage approved + git commit
    DiffPush,          // P: push the current branch (explicit; never auto after commit)
    ToggleFooter,

    // Command palette
    CommandInput(char),
    CommandBackspace,
    CommandMoveUp,
    CommandMoveDown,
    CommandComplete,
    CommandExecute,

    // Glide mode movement: a motion resolved by the glide engine
    GlideMove(Motion),
    // Paste the unnamed register: true = after cursor/line (p), false = before (P)
    PasteRegister(bool),
    // Operator+motion: d/c/y set a pending operator; the next motion applies it
    SetOperator(Operator),
    ClearOperator,
    SetFindPending(FindKind), // f/t/F/T while an operator is pending: capture next char
    ToggleCase,               // ~ : toggle case of the char under the cursor and move right
    ChangeToLineEnd,          // C = c$
    YankLine,                 // Y = yy

    // Glide mode edit-entry
    DeleteToLineEnd,      // D: delete from cursor to end of line
    GlideJoin,            // J: join current line with next
    GlideInsert,          // i: Edit mode at cursor
    GlideInsertLineStart, // I: first non-whitespace then Edit
    GlideAppend,          // a: move right then Edit
    GlideAppendEnd,       // A: line end then Edit
    GlideOpenLine,        // o: open line below then Edit
    GlideOpenLineAbove,   // O: open line above then Edit

    // Internal: set/clear the Glide prefix key (e.g. 'g' waiting for second 'g')
    SetGlidePrefix(Option<char>),
    // Internal: accumulate a count digit in Glide mode
    GlideDigit(char),

    // Browse mode (folder tree). Up/Down reuse MoveUp/MoveDown; gg/G reuse
    // PageTop/PageBottom — only the tree-specific verbs get their own variants.
    BrowseExpandOrOpen, // l / Enter / →: expand a dir, or open a file into Edit
    BrowseCollapseOrParent, // h / ←: collapse a dir, or move to the parent
    BrowseStartFilter,  // /: begin incremental name filtering
    BrowseFilterChar(char), // a character typed while filtering
    BrowseFilterBackspace, // delete the last filter character
}
