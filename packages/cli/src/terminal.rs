//! Raw-mode stdin handler and stdout writer for PTY stream bridging.
//!
//! Puts the terminal into raw mode for the duration of a session bridge,
//! then restores it on drop — even if the process panics.

use std::io::{self, Write};

use anyhow::Result;
use crossterm::terminal;

/// RAII guard that enables raw mode on construction and disables it on drop.
pub struct RawMode;

impl RawMode {
    pub fn enter() -> Result<Self> {
        terminal::enable_raw_mode()?;
        Ok(RawMode)
    }
}

impl Drop for RawMode {
    fn drop(&mut self) {
        // Best-effort — nothing to do if it fails.
        let _ = terminal::disable_raw_mode();
    }
}

/// Write `data` directly to stdout, flushing after each chunk.
pub fn write_stdout(data: &[u8]) -> Result<()> {
    let mut out = io::stdout().lock();
    out.write_all(data)?;
    out.flush()?;
    Ok(())
}
