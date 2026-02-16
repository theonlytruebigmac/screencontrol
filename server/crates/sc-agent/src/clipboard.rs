//! Cross-platform clipboard access for remote desktop sessions.
//!
//! Uses the `arboard` crate to read/write the system clipboard.
//! Supports text clipboard content for bidirectional sync between
//! the agent machine and the web console.

/// Read the current clipboard text content.
/// Returns `None` if the clipboard is empty or contains non-text data.
pub fn get_clipboard_text() -> Option<String> {
    match arboard::Clipboard::new() {
        Ok(mut clipboard) => match clipboard.get_text() {
            Ok(text) if !text.is_empty() => Some(text),
            Ok(_) => None,
            Err(arboard::Error::ContentNotAvailable) => None,
            Err(e) => {
                tracing::debug!("Clipboard read error: {}", e);
                None
            }
        },
        Err(e) => {
            tracing::warn!("Failed to create clipboard context: {}", e);
            None
        }
    }
}

/// Set the clipboard text content on the agent machine.
pub fn set_clipboard_text(text: &str) -> bool {
    match arboard::Clipboard::new() {
        Ok(mut clipboard) => match clipboard.set_text(text) {
            Ok(_) => {
                tracing::debug!("Clipboard set ({} chars)", text.len());
                true
            }
            Err(e) => {
                tracing::warn!("Failed to set clipboard: {}", e);
                false
            }
        },
        Err(e) => {
            tracing::warn!("Failed to create clipboard context: {}", e);
            false
        }
    }
}

/// Clipboard change watcher — tracks the last known content to detect changes.
pub struct ClipboardWatcher {
    last_content: Option<String>,
}

impl ClipboardWatcher {
    pub fn new() -> Self {
        Self {
            last_content: get_clipboard_text(),
        }
    }

    /// Check if the clipboard has changed since the last poll.
    /// Returns the new content if changed, `None` otherwise.
    pub fn poll_change(&mut self) -> Option<String> {
        let current = get_clipboard_text();
        if current != self.last_content {
            self.last_content = current.clone();
            current
        } else {
            None
        }
    }
}
