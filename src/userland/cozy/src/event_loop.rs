use crate::input::{self, EventSource, InputEvent};
use crate::reducer::{EventResult, reduce};
use crate::state::EditorState;
use crate::ui::Renderer;
use ratatui::{Terminal, backend::Backend};
use std::io;
use std::time::Duration;

pub fn run<B: Backend>(
    terminal: &mut Terminal<B>,
    editor: &mut EditorState,
    event_src: &mut dyn EventSource,
) -> io::Result<()> {
    let mut needs_redraw = true;

    loop {
        if needs_redraw {
            terminal.draw(|f| {
                let chunks = Renderer::editor_layout(f.area(), editor);
                Renderer::render_body(editor, f, chunks[0]);
                Renderer::render_shortcuts(editor, f, chunks[1]);
                Renderer::render_status_bar(editor, f, chunks[2]);
            })?;
            needs_redraw = false;
        }

        // Poll with longer timeout to avoid clearing IME composition overlays.
        // crossterm receives no events during IME composition (macOS intercepts
        // keystrokes at the Cocoa layer), so longer polling means the IME inline
        // display persists undisturbed until the user confirms the composition.
        if event_src.poll(Duration::from_millis(1000))? {
            match input::map_event(editor, event_src.read()?) {
                InputEvent::Action(action) => {
                    if let EventResult::Exit = reduce(editor, action) {
                        // A deliberate exit: whatever the user wanted to keep is
                        // saved, and whatever they discarded they discarded on
                        // purpose. The swap exists for the exits nobody chose.
                        crate::swap::remove(editor);
                        break;
                    }
                    if editor.modified {
                        editor.swap_dirty = true;
                        editor.swap_edits += 1;
                        // Do not let a long uninterrupted burst of typing outrun
                        // the journal — the idle tick below never fires while the
                        // keys keep coming.
                        if editor.swap_edits >= crate::swap::EDITS_PER_WRITE {
                            crate::swap::write(editor);
                        }
                    }
                    editor.cursor_blink = true;
                    // MVP: any action may have mutated the buffer; reparse on the
                    // next render. Buffer mutations are not funneled through one
                    // method, so a precise dirty signal (revision / InputEdit) is
                    // a follow-up — see cozy-notes treesitter-integration.md.
                    editor.highlighter.mark_dirty();
                    needs_redraw = true;
                }
                InputEvent::Resize(cols, rows) => {
                    let _ = terminal.resize(ratatui::layout::Rect::new(0, 0, cols, rows));
                    needs_redraw = true;
                }
                InputEvent::Flush => {
                    // The one second of typing the idle tick has not written yet
                    // is exactly what a kill would take. Write it while we still
                    // hold the thread.
                    if editor.swap_dirty {
                        crate::swap::write(editor);
                    }
                }
                InputEvent::Ignore => {}
            }
        } else {
            // No key for a second: the user stopped typing, so write the swap
            // now. This is the whole cadence — an iOS kill is announced to
            // nobody, so "on the way out" is not a moment we are given.
            if editor.swap_dirty {
                crate::swap::write(editor);
            }

            // Timeout: toggle cursor blink only if enabled
            if editor.config.cursor_blink.unwrap_or(false) {
                editor.cursor_blink = !editor.cursor_blink;
                needs_redraw = true;
            }
        }
    }
    Ok(())
}
