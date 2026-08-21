use crate::state::Cursor;
use crate::state::TextBuffer;
use serde::Deserialize;
use std::io;
use std::path::PathBuf;
use std::time::Instant;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub page_size: usize,
    pub theme: Option<String>,
    pub show_line_numbers: Option<bool>,
    pub status_duration: Option<u64>,
    pub keys: Option<std::collections::HashMap<String, String>>,
    pub line_number_bg: Option<String>,
    pub line_number_fg: Option<String>,
    pub footer_bg: Option<String>,
    pub footer_key_fg: Option<String>,
    pub footer_fg: Option<String>,
    pub status_bar_bg: Option<String>,
    pub status_bar_fg: Option<String>,
    pub cursor_blink: Option<bool>,
    /// Which mode you rest in. "edit" (default) = type like nano. "glide" =
    /// navigate like vim. Affects every action's return target, not just startup.
    pub default_mode: Option<String>,
}

impl Config {
    pub fn load() -> Self {
        crate::config_io::load()
    }

    /// Load config, optionally overriding the search directory (for iOS sandbox etc.).
    pub fn load_from(config_dir: Option<&PathBuf>) -> Self {
        crate::config_io::load_from(config_dir)
    }

    pub fn user_config_path(config_dir: Option<&PathBuf>) -> Option<PathBuf> {
        crate::config_io::user_config_path(config_dir)
    }

    pub fn load_from_path(path: &std::path::Path) -> io::Result<Self> {
        crate::config_io::load_from_path(path)
    }

    pub fn ensure_default_config_file(config_dir: Option<&PathBuf>) -> io::Result<PathBuf> {
        crate::config_io::ensure_default_config_file(config_dir)
    }

    pub(crate) fn default_values() -> Self {
        Self {
            page_size: 20,
            theme: Some("dark".to_string()),
            show_line_numbers: Some(true),
            status_duration: Some(3),
            line_number_bg: Some("darkgray".to_string()),
            line_number_fg: Some("white".to_string()),
            footer_bg: Some("#222226".to_string()),
            footer_key_fg: Some("cyan".to_string()),
            footer_fg: Some("gray".to_string()),
            status_bar_bg: Some("darkgray".to_string()),
            status_bar_fg: Some("white".to_string()),
            cursor_blink: Some(true),
            keys: None,
            default_mode: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]

pub enum EditorMode {
    Edit,
    Glide,
    Search,
    Replace,
    Save,
    Open,
    Help,
    Quit,
    Welcome,
    Goto,
    Browse,
    Markdown,
    DiffReview,
    DiffCommitMsg,
    Command,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StatusKind {
    Info,
    Success,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SearchMode {
    MatchCase,
    Regex,
    ByWord,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ReplaceFocus {
    Query,
    Replace,
}

pub struct EditorState {
    pub buffer: TextBuffer,
    pub cursor: Cursor,
    pub highlighter: crate::utils::highlight::Highlighter, // syntax highlight cache
    pub filename: Option<PathBuf>,                         // PathBufに変更
    pub _working_dir: PathBuf,                             // 新規: カレントディレクトリ
    pub modified: bool,                                    // 新規: 変更フラグ
    pub mode: EditorMode,                                  // 新規: モード管理
    pub diff_review: Option<crate::state::diff::DiffReviewState>, // 新規: セッション diff レビュー状態
    pub commit_msg_buffer: String, // 新規: diff レビューからのコミットメッセージ入力
    pub save_filename_buffer: String, // 新規: 保存時の入力バッファ
    pub open_filename_buffer: String, // 新規: 読み込み時の入力バッファ
    pub filename_cursor: usize,    // 新規: ファイル名入力カーソル位置 (byte index)
    pub status_message: Option<String>, // Optionに変更
    pub status_timestamp: Option<Instant>, // 名前変更 & Option化
    pub status_persistent: bool,   // 新規: 常時表示フラグ
    pub status_kind: StatusKind,   // 新規: ステータス種別
    pub scroll_offset: usize,      // 新規: スクロール位置
    pub _startup_args: Vec<String>, // 新規: 将来用
    pub _startup_time: Instant,    // 新規: 将来用
    pub _footer_shortcuts: Vec<String>, // 新規: 将来用
    pub shortcut_map: std::collections::HashMap<
        (crate::state::key::KeyCode, crate::state::key::KeyModifiers),
        crate::shortcuts::EditorAction,
    >, // 新規: ショートカットマップ
    pub search_buffer: String,     // 新規: 検索用バッファ
    pub replace_buffer: String,    // 新規: 置換用バッファ
    pub search_cursor: usize,      // 検索/置換入力欄のカーソル位置 (byte index)
    pub replace_focus: ReplaceFocus, // 新規: 置換モードのフォーカス
    pub page_size: usize,          // 新規: ページサイズ
    pub config: Config,            // 新規: 設定
    pub cursor_blink: bool,        // 新規: カーソル点滅フラグ
    pub search_mode: SearchMode,   // 新規: 検索モード
    pub undo_stack: Vec<(TextBuffer, Cursor)>, // 新規: Undoスタック
    pub redo_stack: Vec<(TextBuffer, Cursor)>, // 新規: Redoスタック
    pub last_saved_id: usize,      // 新規: 保存時のスナップショットID（dirty判定用）
    pub help_scroll_offset: usize, // ヘルプ画面のスクロール位置
    pub help_cursor_line: usize,   // ヘルプ画面の現在行（Markdown preview と共通モデル）
    pub help_view_height: usize,   // ヘルプ画面の表示行数
    pub help_rendered_line_count: usize, // ヘルプ画面の折り返し後の行数
    pub markdown_scroll_offset: usize, // Markdown preview のスクロール位置
    pub markdown_cursor_line: usize, // Markdown preview の現在行
    pub markdown_view_height: usize, // Markdown preview の表示行数
    pub markdown_rendered_line_count: usize, // Markdown preview のレンダリング後の行数
    pub show_line_numbers_runtime: Option<bool>, // 新規: ランタイム行番号表示フラグ (Noneならconfigに従う)
    pub goto_line_buffer: String,                // 行ジャンプ入力バッファ
    /// All search match positions: (line_y, byte_start, byte_end)
    pub search_matches: Vec<(usize, usize, usize)>,
    /// Index of the currently focused match in search_matches
    pub search_current: usize,
    /// Whether soft-wrap is active (long lines wrap within the viewport)
    pub soft_wrap: bool,
    /// Runtime visibility for shortcut footers. Prompt input rows remain visible.
    pub footer_visible_runtime: bool,
    /// Text display width (columns after subtracting line-number gutter); set by renderer
    pub text_display_width: usize,
    /// Pending prefix key in Glide mode (e.g. Some('g') while waiting for second 'g')
    pub glide_prefix: Option<char>,
    /// Digit accumulation buffer for Glide count prefix (e.g. "53" for 53j)
    pub glide_count: String,
    /// Unnamed register holding the last delete/yank, for `p`/`P`.
    pub register: Register,
    /// Operator awaiting a motion in Glide mode (`d`/`c`/`y` pressed).
    pub pending_operator: Option<crate::glide::Operator>,
    /// Set while waiting for the target char of a find motion (e.g. after `df`,
    /// `dt`, `dF`, `dT`); the variant records which find family is pending.
    pub glide_find_pending: Option<crate::glide::FindKind>,
    /// The span flashed after a yank, shown until the next keypress.
    pub yank_highlight: Option<YankHighlight>,
    /// The last to-char motion (`>`/`<`/`t`/`T`), replayed by `.` (forward) and
    /// `,` (backward).
    pub last_find: Option<(crate::glide::FindKind, char)>,
    /// The folder tree shown in Browse mode (built on entry, `None` otherwise).
    pub browse_tree: Option<crate::browse::BrowseTree>,
    /// Command palette filter text.
    pub command_query: String,
    /// Selected row within the filtered command palette results.
    pub command_selected: usize,
    /// Where swap files live for this session (`None` = no writable state dir,
    /// so recovery is simply off rather than half-working).
    pub swap_dir: Option<PathBuf>,
    /// The swap file for the open document. Follows Save As.
    pub swap_path: Option<PathBuf>,
    /// Edits since the swap was last written (see `swap::EDITS_PER_WRITE`).
    pub swap_edits: usize,
    /// The buffer has moved on since the swap was written.
    pub swap_dirty: bool,
    /// A swap found at startup, waiting on the user's one-line answer. While
    /// this is set, input is answering the prompt, not editing the buffer.
    pub recovery: Option<crate::swap::Recovery>,
    /// A save that stopped because the target's parent directory does not
    /// exist, waiting on the same kind of one-line answer as `recovery`.
    pub create_dir: Option<crate::file_io::CreateDirOffer>,
}

pub(crate) struct EditorStateInit {
    pub filename: Option<String>,
    pub config_dir: Option<PathBuf>,
    pub working_dir: PathBuf,
    pub startup_args: Vec<String>,
    pub startup_time: Instant,
}

impl EditorStateInit {
    pub(crate) fn from_runtime(filename: Option<String>, config_dir: Option<PathBuf>) -> Self {
        Self {
            filename,
            config_dir,
            working_dir: crate::runtime_env::current_working_dir(),
            startup_args: crate::runtime_env::startup_args(),
            startup_time: Instant::now(),
        }
    }
}

/// The unnamed register: holds cut/yanked text. `linewise` means the content is
/// whole lines (`dd`, `yy`) rather than an inline span (`D`, `dw`).
#[derive(Debug, Clone, Default)]
pub struct Register {
    pub text: String,
    pub linewise: bool,
}

/// The span just yanked, flashed in the buffer so you can see what was copied
/// (yank has no other visible effect). Cleared on the next keypress.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct YankHighlight {
    /// Inclusive start `(line, byte)`.
    pub start: (usize, usize),
    /// Exclusive end `(line, byte)` for charwise; for linewise only `end.0` matters.
    pub end: (usize, usize),
    pub linewise: bool,
}

impl YankHighlight {
    /// Whether the char at `(y, byte)` falls inside this highlight.
    pub fn contains(&self, y: usize, byte: usize) -> bool {
        if self.linewise {
            y >= self.start.0 && y <= self.end.0
        } else {
            (y, byte) >= self.start && (y, byte) < self.end
        }
    }
}

/// Pick a default save name for a new buffer that doesn't clobber an existing
/// file in `dir`: `untitled.txt`, then `untitled (1).txt`, `untitled (2).txt`, …
fn default_save_name(dir: &std::path::Path) -> String {
    let first = "untitled.txt".to_string();
    if !dir.join(&first).exists() {
        return first;
    }
    for n in 1..10_000 {
        let candidate = format!("untitled ({}).txt", n);
        if !dir.join(&candidate).exists() {
            return candidate;
        }
    }
    first
}

impl EditorState {
    #[cfg(test)]
    pub fn new(filename: Option<String>) -> Self {
        Self::new_with_config_dir(filename, None)
    }

    #[cfg(test)]
    pub fn new_with_config_dir(filename: Option<String>, config_dir: Option<&PathBuf>) -> Self {
        Self::from_init(EditorStateInit::from_runtime(filename, config_dir.cloned()))
    }

    /// Effective line-number visibility. Precedence: an explicit runtime toggle
    /// (Ctrl+L) wins; otherwise the config default, but a compact/low-spec host
    /// (`COZY_COMPACT`) hides them by default to shrink the per-frame gutter.
    /// Single source of truth so the renderer and the toggle reducer agree
    /// (otherwise the first toggle on a compact host would be a no-op).
    pub(crate) fn line_numbers_visible(&self) -> bool {
        self.show_line_numbers_runtime.unwrap_or(
            self.config.show_line_numbers.unwrap_or(true) && !crate::runtime_env::compact(),
        )
    }

    pub(crate) fn from_init(init: EditorStateInit) -> Self {
        let config = Config::load_from(init.config_dir.as_ref());
        let startup_document = crate::file_io::load_startup_document(init.filename.as_deref());
        let (lines, path_buf, initial_mode, browse_tree) = match startup_document {
            crate::file_io::StartupDocument::Empty => {
                (vec![String::new()], None, EditorMode::Welcome, None)
            }
            crate::file_io::StartupDocument::File { path, lines } => {
                (lines, Some(path), Self::resolve_home(&config), None)
            }
            crate::file_io::StartupDocument::Directory { tree } => {
                (vec![String::new()], None, EditorMode::Browse, Some(tree))
            }
        };

        Self {
            buffer: TextBuffer::from_lines(lines),
            cursor: Cursor::default(),
            highlighter: {
                let mut h = crate::utils::highlight::Highlighter::new();
                h.set_file(path_buf.as_deref());
                h
            },
            filename: path_buf,
            _working_dir: init.working_dir,
            modified: false,
            mode: initial_mode,
            diff_review: None,
            commit_msg_buffer: String::new(),
            save_filename_buffer: String::new(),
            open_filename_buffer: String::new(),
            filename_cursor: 0,
            status_message: None,
            status_timestamp: None,
            status_persistent: true, // デフォルトは常時表示
            status_kind: StatusKind::Info,
            scroll_offset: 0,
            _startup_args: init.startup_args,
            _startup_time: init.startup_time,
            _footer_shortcuts: crate::shortcuts::footer_labels(),
            shortcut_map: crate::shortcuts::build_shortcut_map(config.keys.as_ref()),
            search_buffer: String::new(),
            replace_buffer: String::new(),
            search_cursor: 0,
            replace_focus: ReplaceFocus::Query,
            page_size: config.page_size,
            config,
            cursor_blink: true,
            search_mode: SearchMode::ByWord,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            last_saved_id: 0,
            help_scroll_offset: 0,
            help_cursor_line: 0,
            help_view_height: 0,
            help_rendered_line_count: 1,
            markdown_scroll_offset: 0,
            markdown_cursor_line: 0,
            markdown_view_height: 0,
            markdown_rendered_line_count: 1,
            show_line_numbers_runtime: None,
            goto_line_buffer: String::new(),
            search_matches: Vec::new(),
            search_current: 0,
            soft_wrap: true,
            footer_visible_runtime: true,
            text_display_width: 0,
            glide_prefix: None,
            glide_count: String::new(),
            register: Register::default(),
            pending_operator: None,
            glide_find_pending: None,
            yank_highlight: None,
            last_find: None,
            browse_tree,
            command_query: String::new(),
            command_selected: 0,
            swap_dir: crate::swap::swap_dir(init.config_dir.as_ref()),
            swap_path: None,
            swap_edits: 0,
            swap_dirty: false,
            recovery: None,
            create_dir: None,
        }
        .with_recovery_offer()
    }

    /// Look for a swap left behind by a session that did not get to finish, and
    /// hold it out as an offer. Nothing is restored without an answer: the buffer
    /// still shows the file, so a wrong swap costs the user one keystroke, not
    /// their document.
    fn with_recovery_offer(mut self) -> Self {
        if let Some(dir) = &self.swap_dir {
            crate::swap::prune(dir);
        }
        self.swap_path = crate::swap::path_for(&self);
        if let (Some(swap), Some(target)) = (&self.swap_path, &self.filename) {
            let target = self.resolve_in_working_dir(target);
            self.recovery = crate::swap::load(swap, &target);
        }
        self
    }

    pub fn save_snapshot(&mut self) {
        self.undo_stack.push((self.buffer.clone(), self.cursor));
        self.redo_stack.clear();
    }

    pub fn undo(&mut self) {
        if let Some((buffer, cursor)) = self.undo_stack.pop() {
            self.redo_stack.push((self.buffer.clone(), self.cursor));
            self.buffer = buffer;
            self.cursor = cursor;
        }
    }

    pub fn redo(&mut self) {
        if let Some((buffer, cursor)) = self.redo_stack.pop() {
            self.undo_stack.push((self.buffer.clone(), self.cursor));
            self.buffer = buffer;
            self.cursor = cursor;
        }
    }

    pub fn set_status_message(&mut self, msg: String, kind: StatusKind, persistent: bool) {
        self.status_message = Some(msg);
        self.status_timestamp = Some(Instant::now());
        self.status_kind = kind;
        self.status_persistent = persistent;
    }

    pub fn should_show_status(&self) -> bool {
        if self.status_persistent {
            return true;
        }
        if let Some(timestamp) = self.status_timestamp {
            let duration = self.config.status_duration.unwrap_or(3);
            if timestamp.elapsed() < std::time::Duration::from_secs(duration) {
                return true;
            }
        }
        false
    }

    /// The resting/return mode the user lands in after an action completes,
    /// resolved from `config.default_mode`. Unknown/missing → Edit (the safe
    /// default that keeps zero hidden state for newcomers).
    pub fn home_mode(&self) -> EditorMode {
        Self::resolve_home(&self.config)
    }

    /// Same resolution as [`home_mode`], usable before `EditorState` exists
    /// (e.g. when picking the startup mode in `new_with_config_dir`).
    pub fn resolve_home(config: &Config) -> EditorMode {
        match config.default_mode.as_deref() {
            Some("glide") => EditorMode::Glide,
            _ => EditorMode::Edit,
        }
    }

    pub fn enter_mode(&mut self, mode: EditorMode) {
        self.mode = mode;
        match mode {
            EditorMode::Save | EditorMode::Quit => {
                // A new, unnamed buffer gets a default name so the user can just
                // hit Enter to drop a memo into the current folder. The default
                // dodges existing files by counting up: untitled (1).txt, etc.
                let dir = self._working_dir.clone();
                // ⚠️ **縮めるのは種を仕込むここ**（描画時ではない）。この文字列は
                // そのまま編集される buffer なので、表示だけ別物にすると
                // `filename_cursor` が指す位置と食い違う。
                // 保存時は `file_io` の `expand_tilde` が戻すので往復する。
                self.save_filename_buffer = self
                    .filename
                    .as_ref()
                    .map(|p| crate::file_io::shorten_home(&p.to_string_lossy()))
                    .unwrap_or_else(|| default_save_name(&dir));
                self.filename_cursor = self.save_filename_buffer.len();
                self.status_message = None;
            }
            EditorMode::Open => {
                self.open_filename_buffer = self
                    .filename
                    .as_ref()
                    .map(|p| crate::file_io::shorten_home(&p.to_string_lossy()))
                    .unwrap_or_default();
                self.filename_cursor = self.open_filename_buffer.len();
                self.status_message = None;
            }
            EditorMode::Search => {
                self.search_buffer.clear();
                self.search_matches.clear();
                self.search_current = 0;
                self.search_mode = crate::state::SearchMode::ByWord;
                self.search_cursor = 0;
                self.status_message = None;
            }
            EditorMode::Replace => {
                self.search_buffer.clear();
                self.replace_buffer.clear();
                self.search_matches.clear();
                self.search_current = 0;
                self.replace_focus = crate::state::ReplaceFocus::Query;
                self.search_cursor = 0;
                self.status_message = None;
            }
            EditorMode::Goto => {
                self.goto_line_buffer.clear();
                self.status_message = None;
            }
            EditorMode::Glide => {
                self.glide_prefix = None;
                self.glide_count.clear();
                self.status_message = None;
            }
            EditorMode::Browse => {
                self.browse_tree = Some(crate::file_io::build_browse_tree(
                    self.filename.as_ref(),
                    &self._working_dir,
                ));
                self.status_message = None;
            }
            EditorMode::Markdown => {
                self.markdown_scroll_offset = 0;
                self.markdown_cursor_line = 0;
                self.markdown_view_height = 0;
                self.markdown_rendered_line_count = 1;
                self.glide_prefix = None;
                self.glide_count.clear();
                self.status_message = None;
            }
            EditorMode::Help => {
                self.help_scroll_offset = 0;
                self.help_cursor_line = 0;
                self.help_view_height = 0;
                self.help_rendered_line_count = 1;
                self.glide_prefix = None;
                self.glide_count.clear();
                self.status_message = None;
            }
            EditorMode::Command => {
                self.command_query.clear();
                self.command_selected = 0;
                self.status_message = None;
            }
            EditorMode::DiffCommitMsg => {
                self.commit_msg_buffer.clear();
                self.status_message = None;
            }
            _ => {
                self.status_message = None;
            }
        }
    }
}

// --- 保存系メソッド ---
impl EditorState {
    /// Resolve a possibly-relative save target against the folder cozy is anchored
    /// to (`_working_dir`). This keeps the actual write in the same folder the
    /// default-name collision check looked at — and the same folder regardless of
    /// the process cwd, which matters once Browse can span multiple repos.
    pub(crate) fn resolve_in_working_dir(&self, path: &std::path::Path) -> PathBuf {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            self._working_dir.join(path)
        }
    }

    /// 通常保存（現在のファイル名に保存）
    pub fn save(&mut self) -> io::Result<()> {
        crate::file_io::save(self)
    }

    /// 名前をつけて保存（新しいファイル名を指定して保存し、記憶する）
    pub fn save_as(&mut self, path: &str) -> io::Result<()> {
        crate::file_io::save_as(self, path)
    }

    /// ファイルを開く
    pub fn open_file(&mut self, path: &str) -> io::Result<()> {
        let result = crate::file_io::open_file(self, path);
        if result.is_ok() {
            self.highlighter.set_file(self.filename.as_deref());
        }
        result
    }

    pub fn adjust_scroll(&mut self, viewport_height: usize) {
        if viewport_height == 0 {
            return;
        }

        // Always scroll up if cursor is above viewport
        if self.cursor.y < self.scroll_offset {
            self.scroll_offset = self.cursor.y;
            return;
        }

        if !self.soft_wrap || self.text_display_width == 0 {
            if self.cursor.y >= self.scroll_offset + viewport_height {
                self.scroll_offset = self.cursor.y - viewport_height + 1;
            }
            return;
        }

        // Soft wrap: check if cursor's visual sub-row is already visible
        let tw = self.text_display_width;
        let mut vrows = 0usize;
        let mut cursor_visible = false;

        for by in self.scroll_offset..=self.cursor.y {
            if by >= self.buffer.lines.len() {
                break;
            }
            let line = &self.buffer.lines[by];
            let lrows = crate::utils::wrap::visual_row_count(line, tw);
            if by == self.cursor.y {
                let (sub, _) = crate::utils::wrap::cursor_visual_pos(line, self.cursor.x, tw);
                cursor_visible = vrows + sub < viewport_height;
                break;
            }
            vrows += lrows;
            if vrows >= viewport_height {
                break;
            }
        }

        if cursor_visible {
            return;
        }

        // Cursor is below viewport: find a scroll_offset that shows cursor near bottom
        let tw = self.text_display_width;
        let cursor_line = &self.buffer.lines[self.cursor.y];
        let (cursor_sub, _) = crate::utils::wrap::cursor_visual_pos(cursor_line, self.cursor.x, tw);

        if cursor_sub >= viewport_height {
            // cursor's sub-row alone fills the viewport; show from cursor.y
            self.scroll_offset = self.cursor.y;
            return;
        }

        // Walk backward from cursor.y, accumulating rows, until viewport is full
        let mut rows_used = cursor_sub + 1;
        let mut new_offset = self.cursor.y;
        let mut y = self.cursor.y;
        while y > 0 && rows_used < viewport_height {
            y -= 1;
            let r = crate::utils::wrap::visual_row_count(&self.buffer.lines[y], tw);
            if rows_used + r <= viewport_height {
                rows_used += r;
                new_offset = y;
            } else {
                break;
            }
        }
        self.scroll_offset = new_offset;
    }
}
