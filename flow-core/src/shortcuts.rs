//! Voice shortcuts engine using Aho-Corasick for efficient multi-pattern matching
//!
//! Allows users to define trigger phrases that expand to replacement text.
//! Example: "my linkedin" -> "jsn.cam/li"

use aho_corasick::{AhoCorasick, AhoCorasickBuilder, MatchKind};
use parking_lot::RwLock;
use std::collections::HashMap;
use tracing::debug;

use crate::error::Result;
use crate::storage::Storage;
use crate::types::Shortcut;

/// Engine for processing voice shortcuts with O(n) multi-pattern matching
pub struct ShortcutsEngine {
    /// Aho-Corasick automaton for pattern matching
    automaton: RwLock<Option<AhoCorasick>>,
    /// Map from pattern index to shortcut
    shortcuts: RwLock<Vec<Shortcut>>,
    /// Quick lookup by trigger
    trigger_map: RwLock<HashMap<String, usize>>,
}

impl ShortcutsEngine {
    /// Create a new empty shortcuts engine
    pub fn new() -> Self {
        Self {
            automaton: RwLock::new(None),
            shortcuts: RwLock::new(Vec::new()),
            trigger_map: RwLock::new(HashMap::new()),
        }
    }

    /// Create engine and load shortcuts from storage
    pub fn from_storage(storage: &Storage) -> Result<Self> {
        let engine = Self::new();
        let shortcuts = storage.get_enabled_shortcuts()?;
        engine.load_shortcuts(shortcuts);
        Ok(engine)
    }

    /// Load shortcuts and rebuild the automaton
    pub fn load_shortcuts(&self, shortcuts: Vec<Shortcut>) {
        let patterns: Vec<String> = shortcuts
            .iter()
            .map(|s| {
                if s.case_sensitive {
                    s.trigger.clone()
                } else {
                    s.trigger.to_lowercase()
                }
            })
            .collect();

        let automaton = if patterns.is_empty() {
            None
        } else {
            AhoCorasickBuilder::new()
                .match_kind(MatchKind::LeftmostLongest)
                .build(&patterns)
                .ok()
        };

        let trigger_map: HashMap<String, usize> = shortcuts
            .iter()
            .enumerate()
            .map(|(i, s)| (s.trigger.to_lowercase(), i))
            .collect();

        *self.automaton.write() = automaton;
        *self.shortcuts.write() = shortcuts;
        *self.trigger_map.write() = trigger_map;

        debug!(
            "Loaded {} shortcuts into engine",
            self.shortcuts.read().len()
        );
    }

    /// Add a single shortcut
    pub fn add_shortcut(&self, shortcut: Shortcut) {
        let mut shortcuts = self.shortcuts.write();
        shortcuts.push(shortcut);
        drop(shortcuts);
        self.rebuild_automaton();
    }

    /// Remove a shortcut by trigger
    pub fn remove_shortcut(&self, trigger: &str) {
        let trigger_lower = trigger.to_lowercase();
        let mut shortcuts = self.shortcuts.write();
        shortcuts.retain(|s| s.trigger.to_lowercase() != trigger_lower);
        drop(shortcuts);
        self.rebuild_automaton();
    }

    /// Rebuild the automaton from current shortcuts
    fn rebuild_automaton(&self) {
        let shortcuts = self.shortcuts.read();

        let patterns: Vec<String> = shortcuts
            .iter()
            .map(|s| {
                if s.case_sensitive {
                    s.trigger.clone()
                } else {
                    s.trigger.to_lowercase()
                }
            })
            .collect();

        let automaton = if patterns.is_empty() {
            None
        } else {
            AhoCorasickBuilder::new()
                .match_kind(MatchKind::LeftmostLongest)
                .build(&patterns)
                .ok()
        };

        let trigger_map: HashMap<String, usize> = shortcuts
            .iter()
            .enumerate()
            .map(|(i, s)| (s.trigger.to_lowercase(), i))
            .collect();

        drop(shortcuts);

        *self.automaton.write() = automaton;
        *self.trigger_map.write() = trigger_map;
    }

    /// Process text and expand all shortcuts
    /// Returns the processed text and a list of triggered shortcuts
    pub fn process(&self, text: &str) -> (String, Vec<TriggeredShortcut>) {
        let automaton = self.automaton.read();
        let shortcuts = self.shortcuts.read();

        let Some(ref ac) = *automaton else {
            return (text.to_string(), Vec::new());
        };

        // work with lowercase for matching but preserve original positions
        let text_lower = text.to_lowercase();

        // find all matches
        let matches: Vec<_> = ac.find_iter(&text_lower).collect();

        if matches.is_empty() {
            return (text.to_string(), Vec::new());
        }

        let mut triggered = Vec::new();
        let mut result = String::with_capacity(text.len());
        let mut last_end = 0;

        for m in &matches {
            let shortcut = &shortcuts[m.pattern().as_usize()];

            // add text before this match
            result.push_str(&text[last_end..m.start()]);

            // add replacement
            result.push_str(&shortcut.replacement);

            triggered.push(TriggeredShortcut {
                trigger: shortcut.trigger.clone(),
                replacement: shortcut.replacement.clone(),
                position: m.start(),
            });

            last_end = m.end();
        }

        // add remaining text
        result.push_str(&text[last_end..]);

        debug!("Processed {} shortcuts in text", triggered.len());

        (result, triggered)
    }

    /// Check if text contains any shortcuts
    pub fn contains_shortcuts(&self, text: &str) -> bool {
        let automaton = self.automaton.read();
        let Some(ref ac) = *automaton else {
            return false;
        };
        ac.is_match(&text.to_lowercase())
    }

    /// Get all shortcuts
    pub fn get_all(&self) -> Vec<Shortcut> {
        self.shortcuts.read().clone()
    }

    /// Get shortcut count
    pub fn count(&self) -> usize {
        self.shortcuts.read().len()
    }
}

impl Default for ShortcutsEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// A shortcut that was triggered during processing
#[derive(Debug, Clone)]
pub struct TriggeredShortcut {
    pub trigger: String,
    pub replacement: String,
    pub position: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shortcut_expansion() {
        let engine = ShortcutsEngine::new();

        engine.add_shortcut(Shortcut::new(
            "my linkedin".to_string(),
            "jsn.cam/li".to_string(),
        ));
        engine.add_shortcut(Shortcut::new(
            "my email".to_string(),
            "jason@example.com".to_string(),
        ));

        let (result, triggered) = engine.process("check out my linkedin and send to my email");

        assert_eq!(result, "check out jsn.cam/li and send to jason@example.com");
        assert_eq!(triggered.len(), 2);
        assert_eq!(triggered[0].trigger, "my linkedin");
        assert_eq!(triggered[1].trigger, "my email");
    }

    #[test]
    fn test_case_insensitive() {
        let engine = ShortcutsEngine::new();

        engine.add_shortcut(Shortcut::new(
            "My GitHub".to_string(),
            "github.com/jasonlovesdoggo/flow".to_string(),
        ));

        let (result, triggered) = engine.process("visit MY GITHUB for code");

        assert_eq!(result, "visit github.com/jasonlovesdoggo/flow for code");
        assert_eq!(triggered.len(), 1);
    }

    #[test]
    fn test_no_shortcuts() {
        let engine = ShortcutsEngine::new();

        let (result, triggered) = engine.process("hello world");

        assert_eq!(result, "hello world");
        assert!(triggered.is_empty());
    }

    #[test]
    fn test_overlapping_patterns() {
        let engine = ShortcutsEngine::new();

        engine.add_shortcut(Shortcut::new("foo".to_string(), "X".to_string()));
        engine.add_shortcut(Shortcut::new("foobar".to_string(), "Y".to_string()));

        // leftmost longest should prefer "foobar"
        let (result, _) = engine.process("test foobar here");
        assert_eq!(result, "test Y here");
    }

    #[test]
    fn test_contains_shortcuts() {
        let engine = ShortcutsEngine::new();

        engine.add_shortcut(Shortcut::new("test".to_string(), "X".to_string()));

        assert!(engine.contains_shortcuts("this is a test"));
        assert!(!engine.contains_shortcuts("no match here"));
    }

    #[test]
    fn test_remove_shortcut() {
        let engine = ShortcutsEngine::new();

        engine.add_shortcut(Shortcut::new("foo".to_string(), "X".to_string()));
        assert_eq!(engine.count(), 1);

        engine.remove_shortcut("foo");
        assert_eq!(engine.count(), 0);

        let (result, _) = engine.process("test foo here");
        assert_eq!(result, "test foo here");
    }


    #[test]
    fn test_empty_text_processing() {
        let engine = ShortcutsEngine::new();
        engine.add_shortcut(Shortcut::new("test".to_string(), "TEST".to_string()));

        let (result, triggered) = engine.process("");
        assert_eq!(result, "");
        assert!(triggered.is_empty());
    }

    #[test]
    fn test_whitespace_only_text() {
        let engine = ShortcutsEngine::new();
        engine.add_shortcut(Shortcut::new("test".to_string(), "TEST".to_string()));

        let (result, triggered) = engine.process("   \t\n   ");
        assert_eq!(result, "   \t\n   ");
        assert!(triggered.is_empty());
    }

    #[test]
    fn test_shortcut_at_start_of_text() {
        let engine = ShortcutsEngine::new();
        engine.add_shortcut(Shortcut::new("hello".to_string(), "HELLO".to_string()));

        let (result, triggered) = engine.process("hello world");
        assert_eq!(result, "HELLO world");
        assert_eq!(triggered.len(), 1);
        assert_eq!(triggered[0].position, 0);
    }

    #[test]
    fn test_shortcut_at_end_of_text() {
        let engine = ShortcutsEngine::new();
        engine.add_shortcut(Shortcut::new("world".to_string(), "WORLD".to_string()));

        let (result, triggered) = engine.process("hello world");
        assert_eq!(result, "hello WORLD");
        assert_eq!(triggered.len(), 1);
        assert_eq!(triggered[0].position, 6);
    }

    #[test]
    fn test_shortcut_is_entire_text() {
        let engine = ShortcutsEngine::new();
        engine.add_shortcut(Shortcut::new("hello".to_string(), "HELLO".to_string()));

        let (result, triggered) = engine.process("hello");
        assert_eq!(result, "HELLO");
        assert_eq!(triggered.len(), 1);
    }

    #[test]
    fn test_multiple_same_shortcut() {
        let engine = ShortcutsEngine::new();
        engine.add_shortcut(Shortcut::new("hi".to_string(), "hello".to_string()));

        let (result, triggered) = engine.process("hi there hi again hi");
        assert_eq!(result, "hello there hello again hello");
        assert_eq!(triggered.len(), 3);
    }

    #[test]
    fn test_remove_shortcut_case_insensitive() {
        let engine = ShortcutsEngine::new();
        engine.add_shortcut(Shortcut::new("MyShortcut".to_string(), "X".to_string()));
        assert_eq!(engine.count(), 1);

        // remove with different case
        engine.remove_shortcut("MYSHORTCUT");
        assert_eq!(engine.count(), 0);
    }

    #[test]
    fn test_remove_nonexistent_shortcut() {
        let engine = ShortcutsEngine::new();
        engine.add_shortcut(Shortcut::new("foo".to_string(), "X".to_string()));
        assert_eq!(engine.count(), 1);

        // remove something that doesn't exist
        engine.remove_shortcut("bar");
        assert_eq!(engine.count(), 1); // still has "foo"
    }

    #[test]
    fn test_get_all_shortcuts() {
        let engine = ShortcutsEngine::new();
        engine.add_shortcut(Shortcut::new("foo".to_string(), "X".to_string()));
        engine.add_shortcut(Shortcut::new("bar".to_string(), "Y".to_string()));

        let all = engine.get_all();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_load_shortcuts() {
        let engine = ShortcutsEngine::new();

        let shortcuts = vec![
            Shortcut::new("aaa".to_string(), "AAA".to_string()),
            Shortcut::new("bbb".to_string(), "BBB".to_string()),
        ];

        engine.load_shortcuts(shortcuts);
        assert_eq!(engine.count(), 2);

        let (result, _) = engine.process("test aaa and bbb");
        assert_eq!(result, "test AAA and BBB");
    }

    #[test]
    fn test_load_shortcuts_replaces_existing() {
        let engine = ShortcutsEngine::new();
        engine.add_shortcut(Shortcut::new("old".to_string(), "OLD".to_string()));
        assert_eq!(engine.count(), 1);

        // load new shortcuts should replace
        engine.load_shortcuts(vec![Shortcut::new("new".to_string(), "NEW".to_string())]);
        assert_eq!(engine.count(), 1);

        let (result, _) = engine.process("old and new");
        assert_eq!(result, "old and NEW"); // "old" should not be replaced
    }

    #[test]
    fn test_default_impl() {
        let engine = ShortcutsEngine::default();
        assert_eq!(engine.count(), 0);
    }

    #[test]
    fn test_triggered_shortcut_struct() {
        let triggered = TriggeredShortcut {
            trigger: "my email".to_string(),
            replacement: "test@example.com".to_string(),
            position: 10,
        };

        assert_eq!(triggered.trigger, "my email");
        assert_eq!(triggered.replacement, "test@example.com");
        assert_eq!(triggered.position, 10);
    }

    #[test]
    fn test_shortcut_with_special_characters() {
        let engine = ShortcutsEngine::new();
        engine.add_shortcut(Shortcut::new(
            "c++".to_string(),
            "C++ programming language".to_string(),
        ));

        let (result, triggered) = engine.process("I love c++ development");
        assert_eq!(result, "I love C++ programming language development");
        assert_eq!(triggered.len(), 1);
    }

    #[test]
    fn test_shortcut_with_numbers() {
        let engine = ShortcutsEngine::new();
        engine.add_shortcut(Shortcut::new(
            "24/7".to_string(),
            "twenty-four seven".to_string(),
        ));

        let (result, triggered) = engine.process("we are available 24/7");
        assert_eq!(result, "we are available twenty-four seven");
        assert_eq!(triggered.len(), 1);
    }

    #[test]
    fn test_shortcut_with_unicode() {
        let engine = ShortcutsEngine::new();
        engine.add_shortcut(Shortcut::new("café".to_string(), "coffee shop".to_string()));

        let (result, triggered) = engine.process("let's meet at the café");
        assert_eq!(result, "let's meet at the coffee shop");
        assert_eq!(triggered.len(), 1);
    }

    #[test]
    fn test_shortcut_replacement_contains_trigger() {
        // edge case: replacement contains the trigger text
        let engine = ShortcutsEngine::new();
        engine.add_shortcut(Shortcut::new("hi".to_string(), "hi there".to_string()));

        let (result, triggered) = engine.process("say hi");
        // should not infinitely expand
        assert_eq!(result, "say hi there");
        assert_eq!(triggered.len(), 1);
    }

    #[test]
    fn test_shortcut_empty_replacement() {
        let engine = ShortcutsEngine::new();
        engine.add_shortcut(Shortcut::new("remove me".to_string(), "".to_string()));

        let (result, triggered) = engine.process("please remove me from text");
        assert_eq!(result, "please  from text");
        assert_eq!(triggered.len(), 1);
    }

    #[test]
    fn test_shortcut_multiline_text() {
        let engine = ShortcutsEngine::new();
        engine.add_shortcut(Shortcut::new(
            "sig".to_string(),
            "Best regards,\nJohn".to_string(),
        ));

        let (result, triggered) = engine.process("Thanks!\nsig");
        assert_eq!(result, "Thanks!\nBest regards,\nJohn");
        assert_eq!(triggered.len(), 1);
    }

    #[test]
    fn test_adjacent_shortcuts() {
        let engine = ShortcutsEngine::new();
        engine.add_shortcut(Shortcut::new("aa".to_string(), "X".to_string()));
        engine.add_shortcut(Shortcut::new("bb".to_string(), "Y".to_string()));

        let (result, triggered) = engine.process("aabb");
        // both should be matched
        assert_eq!(result, "XY");
        assert_eq!(triggered.len(), 2);
    }

    #[test]
    fn test_contains_shortcuts_case_insensitive() {
        let engine = ShortcutsEngine::new();
        engine.add_shortcut(Shortcut::new("TeSt".to_string(), "X".to_string()));

        assert!(engine.contains_shortcuts("this is a TEST"));
        assert!(engine.contains_shortcuts("test"));
        assert!(engine.contains_shortcuts("TEST"));
    }

    #[test]
    fn test_shortcut_partial_word_match() {
        // BUG EXPOSURE: Shortcuts match anywhere in text, not just word boundaries
        let engine = ShortcutsEngine::new();
        engine.add_shortcut(Shortcut::new("test".to_string(), "X".to_string()));

        // "testing" contains "test" - this will match even though it's partial
        let (result, triggered) = engine.process("testing the system");
        // This exposes that shortcuts match anywhere, not at word boundaries
        assert_eq!(result, "Xing the system"); // possibly undesired behavior
        assert_eq!(triggered.len(), 1);
    }

    #[test]
    fn test_shortcut_very_long_trigger() {
        let engine = ShortcutsEngine::new();
        let long_trigger = "a".repeat(1000);
        let replacement = "short".to_string();
        engine.add_shortcut(Shortcut::new(long_trigger.clone(), replacement.clone()));

        let text = format!("before {} after", long_trigger);
        let (result, triggered) = engine.process(&text);
        assert_eq!(result, "before short after");
        assert_eq!(triggered.len(), 1);
    }

    #[test]
    fn test_shortcut_very_long_replacement() {
        let engine = ShortcutsEngine::new();
        let long_replacement = "b".repeat(1000);
        engine.add_shortcut(Shortcut::new("short".to_string(), long_replacement.clone()));

        let (result, triggered) = engine.process("replace short here");
        let expected = format!("replace {} here", long_replacement);
        assert_eq!(result, expected);
        assert_eq!(triggered.len(), 1);
    }

    #[test]
    fn test_empty_shortcuts_list() {
        let engine = ShortcutsEngine::new();
        engine.load_shortcuts(vec![]);

        assert_eq!(engine.count(), 0);
        let (result, triggered) = engine.process("some text");
        assert_eq!(result, "some text");
        assert!(triggered.is_empty());
    }

    #[test]
    fn test_shortcut_case_sensitive_flag() {
        // The Shortcut struct has a case_sensitive field, test its behavior
        let engine = ShortcutsEngine::new();
        let mut shortcut = Shortcut::new("CaseSensitive".to_string(), "X".to_string());
        shortcut.case_sensitive = true;
        engine.load_shortcuts(vec![shortcut]);

        // BUG EXPOSURE: The case_sensitive flag doesn't work properly.
        // When case_sensitive=true, the pattern is stored as-is ("CaseSensitive"),
        // but the process() method always lowercases the input before matching.
        // So "CaseSensitive" in input becomes "casesensitive" which doesn't match
        // the pattern "CaseSensitive".
        //
        // The fix would be to not lowercase input when doing case-sensitive matching.

        let (result, triggered) = engine.process("this is casesensitive here");
        // lowercase doesn't match (correct for case-sensitive)
        assert_eq!(result, "this is casesensitive here");
        assert!(triggered.is_empty());

        let (result2, triggered2) = engine.process("this is CaseSensitive here");
        // BUG: exact case also doesn't match because input gets lowercased
        assert_eq!(result2, "this is CaseSensitive here"); // Documents buggy behavior
        assert!(triggered2.is_empty()); // Should be 1 if working correctly
    }

    #[test]
    fn test_rebuild_automaton_maintains_consistency() {
        let engine = ShortcutsEngine::new();
        engine.add_shortcut(Shortcut::new("foo".to_string(), "X".to_string()));

        // process once
        let (result1, _) = engine.process("test foo here");
        assert_eq!(result1, "test X here");

        // add another shortcut (triggers rebuild)
        engine.add_shortcut(Shortcut::new("bar".to_string(), "Y".to_string()));

        // both should work
        let (result2, _) = engine.process("test foo and bar here");
        assert_eq!(result2, "test X and Y here");
    }
}
