use std::io::{self, Write};

/// Enable bracketed paste mode
/// Sends control sequence to terminal: \x1b[?2004h
pub fn enable_bracketed_paste() -> io::Result<()> {
    let mut stdout = io::stdout();
    write!(stdout, "\x1b[?2004h")?;
    stdout.flush()?;
    Ok(())
}

/// Disable bracketed paste mode
/// Sends control sequence to terminal: \x1b[?2004l
pub fn disable_bracketed_paste() -> io::Result<()> {
    let mut stdout = io::stdout();
    write!(stdout, "\x1b[?2004l")?;
    stdout.flush()?;
    Ok(())
}
