use crate::action::Action;
use crate::state::EditorState;
use crate::state::key::{KeyCode, KeyModifiers};
use crate::ui::Keymap;
use crossterm::event::Event;
use std::io;
use std::time::Duration;

/// Abstracts the event source so the loop can run with crossterm or an IPC queue.
pub trait EventSource {
    fn poll(&mut self, timeout: Duration) -> io::Result<bool>;
    fn read(&mut self) -> io::Result<Event>;
}

/// Default implementation that reads directly from crossterm (CLI use).
pub struct CrosstermEventSource;

impl EventSource for CrosstermEventSource {
    fn poll(&mut self, timeout: Duration) -> io::Result<bool> {
        crossterm::event::poll(timeout)
    }

    fn read(&mut self) -> io::Result<Event> {
        crossterm::event::read()
    }
}

pub enum InputEvent {
    Action(Action),
    Resize(u16, u16),
    /// Write the swap now, without waiting for the idle tick.
    ///
    /// Losing focus is the last moment a host can still hand us: on iOS the app
    /// is told it is leaving the screen, and is killed later without a word. A
    /// terminal that reports focus gives us the same moment for free.
    Flush,
    Ignore,
}

pub fn map_event(editor: &EditorState, event: Event) -> InputEvent {
    match event {
        Event::Key(key_event) => match ct_code(key_event.code) {
            Some(code) => {
                let mods = ct_mods(key_event.modifiers);
                Keymap::map_key_to_action(editor, code, mods)
                    .map(InputEvent::Action)
                    .unwrap_or(InputEvent::Ignore)
            }
            None => InputEvent::Ignore,
        },
        Event::Paste(data) => InputEvent::Action(Action::InsertString(data)),
        Event::Resize(cols, rows) => InputEvent::Resize(cols, rows),
        Event::FocusLost => InputEvent::Flush,
        _ => InputEvent::Ignore,
    }
}

fn ct_code(code: crossterm::event::KeyCode) -> Option<KeyCode> {
    use crossterm::event::KeyCode as CT;
    Some(match code {
        CT::Char(c) => KeyCode::Char(c),
        CT::Enter => KeyCode::Enter,
        CT::Esc => KeyCode::Esc,
        CT::Backspace => KeyCode::Backspace,
        CT::Delete => KeyCode::Delete,
        CT::PageUp => KeyCode::PageUp,
        CT::PageDown => KeyCode::PageDown,
        CT::Up => KeyCode::Up,
        CT::Down => KeyCode::Down,
        CT::Left => KeyCode::Left,
        CT::Right => KeyCode::Right,
        CT::Home => KeyCode::Home,
        CT::End => KeyCode::End,
        CT::Tab => KeyCode::Tab,
        CT::F(n) => KeyCode::F(n),
        _ => return None,
    })
}

fn ct_mods(mods: crossterm::event::KeyModifiers) -> KeyModifiers {
    use crossterm::event::KeyModifiers as CT;
    let mut result = KeyModifiers::NONE;
    if mods.contains(CT::CONTROL) {
        result |= KeyModifiers::CONTROL;
    }
    if mods.contains(CT::SHIFT) {
        result |= KeyModifiers::SHIFT;
    }
    if mods.contains(CT::ALT) {
        result |= KeyModifiers::ALT;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::EditorMode;
    use crossterm::event::{KeyCode as CtKeyCode, KeyEvent, KeyModifiers as CtKeyModifiers};

    #[test]
    fn ctrl_p_enters_command_mode() {
        let editor = EditorState::new(None);
        let event = Event::Key(KeyEvent::new(CtKeyCode::Char('p'), CtKeyModifiers::CONTROL));

        assert!(matches!(
            map_event(&editor, event),
            InputEvent::Action(Action::EnterMode(EditorMode::Command))
        ));
    }

    #[test]
    fn paste_maps_to_insert_string() {
        let editor = EditorState::new(None);

        assert_eq!(
            match map_event(&editor, Event::Paste("abc".to_string())) {
                InputEvent::Action(action) => Some(action),
                _ => None,
            },
            Some(Action::InsertString("abc".to_string()))
        );
    }

    #[test]
    fn resize_stays_host_side() {
        let editor = EditorState::new(None);

        assert!(matches!(
            map_event(&editor, Event::Resize(80, 24)),
            InputEvent::Resize(80, 24)
        ));
    }
}
