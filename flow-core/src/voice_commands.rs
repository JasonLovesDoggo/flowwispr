//! Voice command detection and extraction
//!
//! Detects "Hey Flow" wake phrase and extracts the instruction that follows.

const WAKE_PHRASE: &str = "hey flow";

/// If text starts with "Hey Flow", returns the rest. Otherwise None.
///
/// The instruction is passed to the LLM to parse what's an instruction vs content.
/// This keeps the detection simple and lets the AI handle the nuance.
///
/// # Examples
/// ```
/// use flow_core::voice_commands::extract_voice_command;
///
/// assert_eq!(
///     extract_voice_command("Hey Flow, reject him politely"),
///     Some("reject him politely".to_string())
/// );
/// assert_eq!(
///     extract_voice_command("hey flow say thanks"),
///     Some("say thanks".to_string())
/// );
/// assert_eq!(
///     extract_voice_command("Hello world"),
///     None
/// );
/// ```
pub fn extract_voice_command(text: &str) -> Option<String> {
    let lower = text.to_lowercase();
    if lower.starts_with(WAKE_PHRASE) {
        let rest = text[WAKE_PHRASE.len()..].trim_start_matches([',', ' ']);
        if !rest.is_empty() {
            return Some(rest.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use crate::voice_commands::extract_voice_command;

    #[test]
    fn test_basic_wake_phrase() {
        assert_eq!(
            extract_voice_command("Hey Flow, reject him politely"),
            Some("reject him politely".to_string())
        );
    }

    #[test]
    fn test_lowercase_wake_phrase() {
        assert_eq!(
            extract_voice_command("hey flow say thanks"),
            Some("say thanks".to_string())
        );
    }

    #[test]
    fn test_mixed_case_wake_phrase() {
        assert_eq!(
            extract_voice_command("Hey flow, make this professional"),
            Some("make this professional".to_string())
        );
    }

    #[test]
    fn test_no_wake_phrase() {
        assert_eq!(extract_voice_command("Hello world"), None);
    }

    #[test]
    fn test_wake_phrase_in_middle() {
        assert_eq!(extract_voice_command("So I said hey flow"), None);
    }

    #[test]
    fn test_empty_after_wake_phrase() {
        assert_eq!(extract_voice_command("Hey Flow, "), None);
        assert_eq!(extract_voice_command("Hey Flow,"), None);
    }

    #[test]
    fn test_instruction_with_content() {
        assert_eq!(
            extract_voice_command(
                "Hey Flow, make this shorter. I wanted to follow up on our conversation"
            ),
            Some("make this shorter. I wanted to follow up on our conversation".to_string())
        );
    }

    #[test]
    fn test_translate_command() {
        assert_eq!(
            extract_voice_command("Hey Flow, translate to Spanish. See you tomorrow"),
            Some("translate to Spanish. See you tomorrow".to_string())
        );
    }
}
