//! macOS Messages.app integration for contact detection

use crate::error::{Error, Result};
use std::process::Command;

/// Detect the active contact name from Messages.app window title
pub struct MessagesDetector;

impl MessagesDetector {
    /// Get the active Messages window contact name using AppleScript
    ///
    /// Returns:
    /// - Ok(Some(name)) if Messages is open and has a window
    /// - Ok(None) if Messages is not running or no window exists
    /// - Err if AppleScript execution fails
    pub fn get_active_contact() -> Result<Option<String>> {
        let script = r#"
            tell application "System Events"
                tell application process "Messages"
                    if exists window 1 then
                        get name of window 1
                    end if
                end tell
            end tell
        "#;

        let output = Command::new("osascript")
            .arg("-e")
            .arg(script)
            .output()
            .map_err(Error::Io)?;

        if !output.status.success() {
            // Messages not running or no window
            return Ok(None);
        }

        let window_title = String::from_utf8_lossy(&output.stdout).trim().to_string();

        if window_title.is_empty() {
            return Ok(None);
        }

        // Normalize the window title
        let normalized = Self::normalize_window_title(&window_title);

        Ok(Some(normalized))
    }

    /// Normalize Messages window title by trimming whitespace
    /// and handling edge cases (group chats, etc.)
    fn normalize_window_title(title: &str) -> String {
        let trimmed = title.trim();

        // Handle group chat titles (comma-separated names or "+ N more")
        // For now, we pass through as-is; classifier will handle as FormalNeutral
        // Future enhancement: could parse and detect individual contacts

        trimmed.to_string()
    }

    /// Check if Messages.app is currently running
    pub fn is_messages_running() -> Result<bool> {
        let script = r#"
            tell application "System Events"
                if exists (processes where name is "Messages") then
                    return true
                else
                    return false
                end if
            end tell
        "#;

        let output = Command::new("osascript")
            .arg("-e")
            .arg(script)
            .output()
            .map_err(Error::Io)?;

        if !output.status.success() {
            return Ok(false);
        }

        let result = String::from_utf8_lossy(&output.stdout)
            .trim()
            .to_lowercase();

        Ok(result == "true")
    }

    /// Get all open conversation window titles
    /// Returns vector of contact names from all open Messages windows
    pub fn get_all_conversations() -> Result<Vec<String>> {
        let script = r#"
            tell application "System Events"
                tell application process "Messages"
                    set windowNames to {}
                    repeat with w in windows
                        set end of windowNames to name of w
                    end repeat
                    return windowNames
                end tell
            end tell
        "#;

        let output = Command::new("osascript")
            .arg("-e")
            .arg(script)
            .output()
            .map_err(Error::Io)?;

        if !output.status.success() {
            return Ok(Vec::new());
        }

        let result = String::from_utf8_lossy(&output.stdout).trim().to_string();

        if result.is_empty() {
            return Ok(Vec::new());
        }

        // AppleScript returns comma-separated list
        let names: Vec<String> = result
            .split(", ")
            .map(Self::normalize_window_title)
            .filter(|s| !s.is_empty())
            .collect();

        Ok(names)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_window_title() {
        assert_eq!(
            MessagesDetector::normalize_window_title("  John Smith  "),
            "John Smith"
        );
        assert_eq!(MessagesDetector::normalize_window_title("Mom"), "Mom");
    }

    #[test]
    #[ignore] // Only run on macOS with Messages.app
    fn test_get_active_contact() {
        let result = MessagesDetector::get_active_contact();
        println!("Active contact: {:?}", result);
        assert!(result.is_ok());
    }

    #[test]
    #[ignore] // Only run on macOS
    fn test_is_messages_running() {
        let result = MessagesDetector::is_messages_running();
        println!("Messages running: {:?}", result);
        assert!(result.is_ok());
    }
}
