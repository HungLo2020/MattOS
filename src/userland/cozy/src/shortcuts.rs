use crate::state::key::{KeyCode, KeyModifiers};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EditorAction {
    EnterSave,
    EnterSaveAs,
    EnterOpen,
    EnterBrowse,
    EnterExit,
    ForceQuit, // Ctrl+Q - Quit immediately without saving
    EnterSearch,
    EnterReplace,
    ReplaceAll,
    Enter,
    MoveLeft,
    MoveRight,
    MoveUp,
    MoveDown,
    PageUp,
    PageDown,
    PageTop,
    PageBottom,
    Home,
    End,
    ToggleSearchMode,
    ReloadConfig,
    EnterHelp,
    Undo,
    Redo,
    Cancel,
    ToggleLineNumbers,
    ToggleWrap,
    ToggleFooter,
    DeleteLine,
    Paste,
    EnterGoto,
    EnterGlide,
    ToggleMarkdownPreview,
    ToggleDiffReview,
    EnterCommand,
}

#[derive(Clone)]
pub struct Shortcut {
    pub key: KeyCode,
    pub modifiers: KeyModifiers,
    pub action: EditorAction,
    pub label: &'static str,
}

// Shortcut constructor helper - makes definitions more compact
fn sc(
    key: KeyCode,
    modifiers: KeyModifiers,
    action: EditorAction,
    label: &'static str,
) -> Shortcut {
    Shortcut {
        key,
        modifiers,
        action,
        label,
    }
}

// File operation shortcuts
fn file_shortcuts() -> Vec<Shortcut> {
    vec![
        sc(
            KeyCode::Char('s'),
            KeyModifiers::CONTROL,
            EditorAction::EnterSave,
            "Ctrl+S Save...",
        ),
        sc(
            KeyCode::Char('s'),
            KeyModifiers::CONTROL | KeyModifiers::SHIFT,
            EditorAction::EnterSaveAs,
            "Ctrl+Shift+S Save As...",
        ),
        sc(
            KeyCode::Char('o'),
            KeyModifiers::CONTROL,
            EditorAction::EnterOpen,
            "Ctrl+O Open...",
        ),
        sc(
            KeyCode::Char('b'),
            KeyModifiers::CONTROL,
            EditorAction::EnterBrowse,
            "Ctrl+B Browse...",
        ),
        // F3 is a tmux-safe Browse fallback: tmux's default prefix is Ctrl+B,
        // which it swallows before cozy ever sees it. Same shape as the F1/Ctrl+H
        // and F2/Ctrl+D fallbacks for keys terminals or multiplexers intercept.
        sc(
            KeyCode::F(3),
            KeyModifiers::NONE,
            EditorAction::EnterBrowse,
            "F3 Browse...",
        ),
        sc(
            KeyCode::Char('x'),
            KeyModifiers::CONTROL,
            EditorAction::EnterExit,
            "Ctrl+X Exit...",
        ),
    ]
}

// Search and replace shortcuts
fn search_shortcuts() -> Vec<Shortcut> {
    vec![
        sc(
            KeyCode::Char('f'),
            KeyModifiers::CONTROL,
            EditorAction::EnterSearch,
            "Ctrl+F Find...",
        ),
        sc(
            KeyCode::Char('r'),
            KeyModifiers::CONTROL,
            EditorAction::EnterReplace,
            "Ctrl+R Replace...",
        ),
        sc(
            KeyCode::Char('q'),
            KeyModifiers::CONTROL,
            EditorAction::ForceQuit,
            "Ctrl+Q Quit without saving",
        ),
        sc(
            KeyCode::Char('t'),
            KeyModifiers::CONTROL,
            EditorAction::ToggleSearchMode,
            "Ctrl+T Toggle",
        ),
    ]
}

// Navigation shortcuts
fn navigation_shortcuts() -> Vec<Shortcut> {
    vec![
        sc(
            KeyCode::Char('a'),
            KeyModifiers::CONTROL,
            EditorAction::Home,
            "Ctrl+A Home",
        ),
        sc(
            KeyCode::Char('e'),
            KeyModifiers::CONTROL,
            EditorAction::End,
            "Ctrl+E End",
        ),
        sc(
            KeyCode::PageUp,
            KeyModifiers::NONE,
            EditorAction::PageUp,
            "PgUp Prev",
        ),
        sc(
            KeyCode::PageDown,
            KeyModifiers::NONE,
            EditorAction::PageDown,
            "PgDn Next",
        ),
    ]
}

// Edit operation shortcuts
fn edit_shortcuts() -> Vec<Shortcut> {
    vec![
        sc(
            KeyCode::Char('z'),
            KeyModifiers::CONTROL,
            EditorAction::Undo,
            "Ctrl+Z Undo",
        ),
        sc(
            KeyCode::Char('y'),
            KeyModifiers::CONTROL,
            EditorAction::Redo,
            "Ctrl+Y Redo",
        ),
        sc(
            KeyCode::Char('v'),
            KeyModifiers::CONTROL,
            EditorAction::Paste,
            "Ctrl+V Paste",
        ),
    ]
}

// Utility shortcuts
fn utility_shortcuts() -> Vec<Shortcut> {
    vec![
        sc(
            KeyCode::Char('h'),
            KeyModifiers::CONTROL,
            EditorAction::EnterHelp,
            "Ctrl+H Help",
        ),
        sc(
            KeyCode::Char('p'),
            KeyModifiers::CONTROL,
            EditorAction::EnterCommand,
            "Ctrl+P Command",
        ),
        // F1 is an unambiguous Help fallback: Ctrl+H sends the Backspace byte
        // (0x08) on some terminals and can be swallowed there.
        sc(
            KeyCode::F(1),
            KeyModifiers::NONE,
            EditorAction::EnterHelp,
            "F1 Help",
        ),
        sc(
            KeyCode::Char('l'),
            KeyModifiers::CONTROL,
            EditorAction::ToggleLineNumbers,
            "Ctrl+L LineNo",
        ),
        sc(
            KeyCode::Char('w'),
            KeyModifiers::CONTROL,
            EditorAction::ToggleWrap,
            "Ctrl+W Wrap",
        ),
        sc(
            KeyCode::Char('u'),
            KeyModifiers::CONTROL,
            EditorAction::ToggleFooter,
            "Ctrl+U Footer",
        ),
        sc(
            KeyCode::Char('k'),
            KeyModifiers::CONTROL,
            EditorAction::DeleteLine,
            "Ctrl+K Cut Line",
        ),
        sc(
            KeyCode::Char('j'),
            KeyModifiers::CONTROL,
            EditorAction::EnterGoto,
            "Ctrl+J Jump",
        ),
        sc(
            KeyCode::Char('g'),
            KeyModifiers::CONTROL,
            EditorAction::EnterGlide,
            "Ctrl+G Glide",
        ),
        sc(
            KeyCode::F(2),
            KeyModifiers::NONE,
            EditorAction::ToggleMarkdownPreview,
            "F2 Markdown",
        ),
        sc(
            KeyCode::Char('d'),
            KeyModifiers::CONTROL,
            EditorAction::ToggleMarkdownPreview,
            "Ctrl+D Markdown",
        ),
        sc(
            KeyCode::F(4),
            KeyModifiers::NONE,
            EditorAction::ToggleDiffReview,
            "F4 Diff",
        ),
    ]
}

// Cancel/Exit shortcuts
fn cancel_shortcuts() -> Vec<Shortcut> {
    vec![
        sc(
            KeyCode::Esc,
            KeyModifiers::NONE,
            EditorAction::Cancel,
            "Esc Cancel",
        ),
        // Ctrl+[ == Esc at the byte level: under the legacy protocol it already
        // arrives as Esc, but under the kitty keyboard protocol it splits off as
        // Ctrl+[. Bind it so the vim-style cancel keeps working either way.
        sc(
            KeyCode::Char('['),
            KeyModifiers::CONTROL,
            EditorAction::Cancel,
            "Ctrl+[ Cancel",
        ),
    ]
}

// Internal shortcuts (arrow keys, Enter, Esc)
fn internal_shortcuts() -> Vec<Shortcut> {
    vec![
        sc(
            KeyCode::Up,
            KeyModifiers::NONE,
            EditorAction::MoveUp,
            "↑ Up",
        ),
        sc(
            KeyCode::Down,
            KeyModifiers::NONE,
            EditorAction::MoveDown,
            "↓ Down",
        ),
        sc(
            KeyCode::Left,
            KeyModifiers::NONE,
            EditorAction::MoveLeft,
            "← Left",
        ),
        sc(
            KeyCode::Right,
            KeyModifiers::NONE,
            EditorAction::MoveRight,
            "→ Right",
        ),
        sc(
            KeyCode::Enter,
            KeyModifiers::NONE,
            EditorAction::Enter,
            "Enter",
        ),
    ]
}

pub fn get_shortcuts() -> Vec<Shortcut> {
    [
        file_shortcuts(),
        search_shortcuts(),
        navigation_shortcuts(),
        edit_shortcuts(),
        utility_shortcuts(),
        cancel_shortcuts(),
        internal_shortcuts(),
    ]
    .concat()
}

pub fn shortcut_map() -> HashMap<(KeyCode, KeyModifiers), EditorAction> {
    let mut map = HashMap::new();
    for shortcut in get_shortcuts() {
        map.insert((shortcut.key, shortcut.modifiers), shortcut.action);
    }
    map
}

pub fn footer_labels() -> Vec<String> {
    get_shortcuts()
        .iter()
        .map(|s| s.label.to_string())
        .collect()
}

/// `"ctrl+s"` / `"alt+enter"` / `"pageup"` 等のキー文字列を (KeyCode, KeyModifiers) に変換する
pub fn parse_key_str(s: &str) -> Option<(KeyCode, KeyModifiers)> {
    let lower = s.to_lowercase();
    let parts: Vec<&str> = lower.split('+').collect();
    // split_last() → (last_element, rest): key は末尾、修飾キーは残り
    let (key_token, modifier_tokens) = parts.split_last()?;
    let key_token: &str = key_token;
    let mut mods = KeyModifiers::NONE;
    for m in modifier_tokens {
        match *m {
            "ctrl" => mods |= KeyModifiers::CONTROL,
            "shift" => mods |= KeyModifiers::SHIFT,
            "alt" => mods |= KeyModifiers::ALT,
            _ => return None,
        }
    }
    let code = match key_token {
        "enter" => KeyCode::Enter,
        "esc" => KeyCode::Esc,
        "backspace" => KeyCode::Backspace,
        "delete" => KeyCode::Delete,
        "pageup" => KeyCode::PageUp,
        "pagedown" => KeyCode::PageDown,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "tab" => KeyCode::Tab,
        "f1" => KeyCode::F(1),
        "f2" => KeyCode::F(2),
        "f3" => KeyCode::F(3),
        "f4" => KeyCode::F(4),
        "f5" => KeyCode::F(5),
        "f6" => KeyCode::F(6),
        "f7" => KeyCode::F(7),
        "f8" => KeyCode::F(8),
        "f9" => KeyCode::F(9),
        "f10" => KeyCode::F(10),
        "f11" => KeyCode::F(11),
        "f12" => KeyCode::F(12),
        c if c.chars().count() == 1 => KeyCode::Char(c.chars().next()?),
        _ => return None,
    };
    Some((code, mods))
}

/// アクション名文字列 → EditorAction
pub fn action_from_name(name: &str) -> Option<EditorAction> {
    match name {
        "enter_save" => Some(EditorAction::EnterSave),
        "enter_save_as" => Some(EditorAction::EnterSaveAs),
        "enter_open" => Some(EditorAction::EnterOpen),
        "enter_browse" => Some(EditorAction::EnterBrowse),
        "enter_exit" => Some(EditorAction::EnterExit),
        "force_quit" => Some(EditorAction::ForceQuit),
        "enter_search" => Some(EditorAction::EnterSearch),
        "enter_replace" => Some(EditorAction::EnterReplace),
        "replace_all" => Some(EditorAction::ReplaceAll),
        "enter_help" => Some(EditorAction::EnterHelp),
        "undo" => Some(EditorAction::Undo),
        "redo" => Some(EditorAction::Redo),
        "cancel" => Some(EditorAction::Cancel),
        "page_up" => Some(EditorAction::PageUp),
        "page_down" => Some(EditorAction::PageDown),
        "page_top" => Some(EditorAction::PageTop),
        "page_bottom" => Some(EditorAction::PageBottom),
        "home" => Some(EditorAction::Home),
        "end" => Some(EditorAction::End),
        "delete_line" => Some(EditorAction::DeleteLine),
        "toggle_line_numbers" => Some(EditorAction::ToggleLineNumbers),
        "toggle_wrap" => Some(EditorAction::ToggleWrap),
        "toggle_footer" => Some(EditorAction::ToggleFooter),
        "reload_config" => Some(EditorAction::ReloadConfig),
        "enter_goto" => Some(EditorAction::EnterGoto),
        "enter_glide" => Some(EditorAction::EnterGlide),
        "toggle_markdown" => Some(EditorAction::ToggleMarkdownPreview),
        "toggle_diff_review" => Some(EditorAction::ToggleDiffReview),
        "enter_command" => Some(EditorAction::EnterCommand),
        "paste" => Some(EditorAction::Paste),
        _ => None,
    }
}

/// デフォルトショートカットマップに config.toml の [keys] 上書きを適用して返す
pub fn build_shortcut_map(
    overrides: Option<&HashMap<String, String>>,
) -> HashMap<(KeyCode, KeyModifiers), EditorAction> {
    let mut map = shortcut_map();
    let Some(ov) = overrides else { return map };
    for (action_name, key_str) in ov {
        let Some(action) = action_from_name(action_name) else {
            eprintln!("warning: unknown action '{}' in [keys] config", action_name);
            continue;
        };
        let Some(key) = parse_key_str(key_str) else {
            eprintln!("warning: cannot parse key '{}' in [keys] config", key_str);
            continue;
        };
        // 既存のこのアクションへの割り当てを除去してから上書き
        map.retain(|_, v| v != &action);
        map.insert(key, action);
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_preview_has_function_key_and_ctrl_fallback() {
        let map = shortcut_map();

        assert_eq!(
            map.get(&(KeyCode::F(2), KeyModifiers::NONE)),
            Some(&EditorAction::ToggleMarkdownPreview)
        );
        assert_eq!(
            map.get(&(KeyCode::Char('d'), KeyModifiers::CONTROL)),
            Some(&EditorAction::ToggleMarkdownPreview)
        );
    }

    #[test]
    fn browse_has_ctrl_b_and_f3_fallback() {
        let map = shortcut_map();

        assert_eq!(
            map.get(&(KeyCode::Char('b'), KeyModifiers::CONTROL)),
            Some(&EditorAction::EnterBrowse)
        );
        // F3 is reachable inside tmux, where Ctrl+B is the multiplexer prefix.
        assert_eq!(
            map.get(&(KeyCode::F(3), KeyModifiers::NONE)),
            Some(&EditorAction::EnterBrowse)
        );
    }

    #[test]
    fn footer_toggle_has_default_shortcut() {
        let map = shortcut_map();

        assert_eq!(
            map.get(&(KeyCode::Char('u'), KeyModifiers::CONTROL)),
            Some(&EditorAction::ToggleFooter)
        );
    }

    #[test]
    fn footer_toggle_binding_can_be_overridden() {
        let overrides = HashMap::from([("toggle_footer".to_string(), "f6".to_string())]);
        let map = build_shortcut_map(Some(&overrides));

        assert_eq!(
            map.get(&(KeyCode::F(6), KeyModifiers::NONE)),
            Some(&EditorAction::ToggleFooter)
        );
        assert_eq!(map.get(&(KeyCode::Char('u'), KeyModifiers::CONTROL)), None);
    }
}
