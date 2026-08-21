// A clipboard handle that lives for the whole process. On X11/Wayland the
// clipboard contents are served by the owning connection, so a throwaway
// `Clipboard::new()` per call relinquishes ownership the instant it drops.
// Holding one handle alive for the session keeps the selection owned and
// pasteable.
#[cfg(feature = "clipboard")]
thread_local! {
    static CLIPBOARD: std::cell::RefCell<Option<arboard::Clipboard>> =
        const { std::cell::RefCell::new(None) };
}

/// Run `f` with the process-lifetime clipboard, lazily creating it. Returns
/// `None` when no clipboard is available (e.g. headless / no display).
#[cfg(feature = "clipboard")]
fn with_clipboard<R>(f: impl FnOnce(&mut arboard::Clipboard) -> R) -> Option<R> {
    CLIPBOARD.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot.is_none() {
            *slot = arboard::Clipboard::new().ok();
        }
        slot.as_mut().map(f)
    })
}

#[cfg(feature = "clipboard")]
pub(crate) fn set_text(text: &str) {
    let _ = with_clipboard(|cb| cb.set_text(text));
}

#[cfg(not(feature = "clipboard"))]
pub(crate) fn set_text(_text: &str) {}

#[cfg(feature = "clipboard")]
pub(crate) fn get_text() -> ClipboardRead {
    match with_clipboard(|cb| cb.get_text()) {
        Some(Ok(text)) => ClipboardRead::Text(text),
        _ => ClipboardRead::Failed,
    }
}

#[cfg(not(feature = "clipboard"))]
pub(crate) fn get_text() -> ClipboardRead {
    ClipboardRead::Unavailable
}

pub(crate) enum ClipboardRead {
    #[cfg(feature = "clipboard")]
    Text(String),
    #[cfg(feature = "clipboard")]
    Failed,
    #[cfg(not(feature = "clipboard"))]
    Unavailable,
}
