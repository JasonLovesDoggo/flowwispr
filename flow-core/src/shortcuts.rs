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
}
