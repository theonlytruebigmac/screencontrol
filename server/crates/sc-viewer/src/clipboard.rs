//! System clipboard integration.
//!
//! Syncs clipboard content between the local system and the remote machine.

use arboard::Clipboard;
use tracing::{debug, info, warn};

/// Clipboard manager for bidirectional clipboard sync.
pub struct ClipboardManager {
    clipboard: Option<Clipboard>,
    last_content: String,
}

impl ClipboardManager {
    pub fn new() -> Self {
        let clipboard = match Clipboard::new() {
            Ok(cb) => {
                info!("Clipboard integration initialized");
                Some(cb)
            }
            Err(e) => {
                warn!("Clipboard not available: {}", e);
                None
            }
        };

        Self {
            clipboard,
            last_content: String::new(),
        }
    }

    /// Check if the local clipboard has changed and return the new content.
    pub fn poll_local_changes(&mut self) -> Option<String> {
        let clipboard = self.clipboard.as_mut()?;

        match clipboard.get_text() {
            Ok(text) => {
                if text != self.last_content && !text.is_empty() {
                    self.last_content = text.clone();
                    debug!("Local clipboard changed: {} chars", text.len());
                    Some(text)
                } else {
                    None
                }
            }
            Err(_) => None,
        }
    }

    /// Set the local clipboard to content received from the remote machine.
    pub fn set_remote_content(&mut self, text: &str) {
        if let Some(ref mut clipboard) = self.clipboard {
            match clipboard.set_text(text) {
                Ok(()) => {
                    self.last_content = text.to_string();
                    debug!("Clipboard set from remote: {} chars", text.len());
                }
                Err(e) => {
                    warn!("Failed to set clipboard: {}", e);
                }
            }
        }
    }
}
