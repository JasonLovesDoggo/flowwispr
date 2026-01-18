//! Contact categorization engine for context-aware transcription

use crate::types::{Contact, ContactCategory};
use aho_corasick::AhoCorasick;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// Input for contact classification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContactInput {
    pub name: String,
    #[serde(default)]
    pub organization: String,
}

/// Result of contact classification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassificationResult {
    pub name: String,
    pub category: ContactCategory,
}

/// Contact classification engine with rule-based heuristics
pub struct ContactClassifier {
    /// Pattern matchers for efficient keyword detection
    partner_patterns: AhoCorasick,
    family_patterns: AhoCorasick,
    professional_patterns: AhoCorasick,
    professional_suffixes: AhoCorasick,
    casual_emojis: Vec<char>,
    partner_emojis: Vec<char>,

    /// In-memory contact cache
    contacts: Arc<RwLock<HashMap<String, Contact>>>,
}

impl ContactClassifier {
    pub fn new() -> Self {
        // Partner terms of endearment (case-insensitive)
        let partner_keywords = vec![
            "bae",
            "hubby",
            "wife",
            "wifey",
            "husband",
            "my love",
            "baby",
            "babe",
            "love",
            "honey",
            "sweetheart",
            "darling",
            "dear",
            "sweetie",
            "boo",
        ];

        // Family titles (case-insensitive)
        let family_keywords = vec![
            "mom",
            "dad",
            "mama",
            "papa",
            "mother",
            "father",
            "grandma",
            "grandpa",
            "grandmother",
            "grandfather",
            "aunt",
            "uncle",
            "sister",
            "brother",
            "sis",
            "bro",
            "cousin",
            "nephew",
            "niece",
            "ice mom",
            "ice dad",
            "ice mama",
            "ice papa",
            "ice aunt",
            "ice uncle",
            "ice grandmother",
            "ice grandfather",
        ];

        // Professional titles (case-insensitive)
        let professional_keywords = vec![
            "dr.",
            "dr ",
            "prof.",
            "prof ",
            "professor",
            "boss",
            "manager",
            "coach",
            "director",
            "vp",
            "ceo",
            "cto",
            "cfo",
            "coo",
            "president",
            "supervisor",
            "lead",
            "senior",
            "jr.",
            "sr.",
            "attorney",
            "lawyer",
        ];

        // Professional credentials (case-insensitive)
        let professional_suffixes = vec![
            "md", "phd", "cpa", "esq", "dds", "jd", "mba", "rn", "dvm", "do",
        ];

        // Casual emojis (non-romantic)
        let casual_emojis = vec![
            '🔥', '🍻', '🤪', '🍕', '🎮', '⚽', '🏀', '🎸', '🎉', '💪', '🤘', '🍺', '🎯', '🚀',
            '💯', '👊', '🤙', '😎', '🏆',
        ];

        // Romantic emojis
        let partner_emojis = vec![
            '❤', '💕', '💖', '💗', '💘', '💝', '💞', '💟', '💙', '💚', '💛', '🧡', '💜', '🖤',
            '🤍', '🤎', '💋', '💍', '💑', '💏', '👩', '👨', '❣',
        ];

        Self {
            partner_patterns: AhoCorasick::new(partner_keywords).unwrap(),
            family_patterns: AhoCorasick::new(family_keywords).unwrap(),
            professional_patterns: AhoCorasick::new(professional_keywords).unwrap(),
            professional_suffixes: AhoCorasick::new(professional_suffixes).unwrap(),
            casual_emojis,
            partner_emojis,
            contacts: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Classify a single contact using strict ordering heuristics
    /// CRITICAL: Partner detection has ABSOLUTE HIGHEST PRIORITY and overrides everything
    pub fn classify(&self, input: &ContactInput) -> ContactCategory {
        let name_lower = input.name.to_lowercase();
        let name_trimmed = input.name.trim();

        // RULE 1: Partner detection (romantic emojis + terms of endearment)
        // HIGHEST PRIORITY - overrides organization field and all other indicators
        if self.has_partner_emoji(name_trimmed) || self.partner_patterns.is_match(&name_lower) {
            return ContactCategory::Partner;
        }

        // RULE 2: Close Family detection (familial titles + ICE)
        if self.family_patterns.is_match(&name_lower) {
            return ContactCategory::CloseFamily;
        }

        // RULE 3: Professional detection (organization OR professional titles/credentials)
        if !input.organization.is_empty() {
            return ContactCategory::Professional;
        }

        if self.professional_patterns.is_match(&name_lower)
            || self.has_professional_suffix(&name_lower)
        {
            return ContactCategory::Professional;
        }

        // RULE 4: Casual / Peer detection (casual emojis + informal formatting)
        if self.has_casual_emoji(name_trimmed) || self.is_casual_nickname(name_trimmed) {
            return ContactCategory::CasualPeer;
        }

        // RULE 5: Formal / Neutral (default fallback)
        ContactCategory::FormalNeutral
    }

    /// Classify multiple contacts and return JSON mapping
    pub fn classify_batch(&self, inputs: &[ContactInput]) -> HashMap<String, ContactCategory> {
        inputs
            .iter()
            .map(|input| (input.name.clone(), self.classify(input)))
            .collect()
    }

    /// Classify batch and return JSON-serializable result
    pub fn classify_batch_json(&self, inputs: &[ContactInput]) -> String {
        let result = self.classify_batch(inputs);
        serde_json::to_string(&result).unwrap_or_else(|_| "{}".to_string())
    }

    /// Check if name contains romantic emoji
    fn has_partner_emoji(&self, name: &str) -> bool {
        name.chars().any(|c| self.partner_emojis.contains(&c))
    }

    /// Check if name contains casual emoji (non-romantic)
    fn has_casual_emoji(&self, name: &str) -> bool {
        name.chars().any(|c| self.casual_emojis.contains(&c))
    }

    /// Check if name ends with professional credential suffix
    fn has_professional_suffix(&self, name_lower: &str) -> bool {
        // Look for ", MD" or " PhD" patterns
        let words: Vec<&str> = name_lower.split_whitespace().collect();
        if let Some(last) = words.last() {
            let cleaned = last.trim_matches(|c: char| !c.is_alphanumeric());
            if self.professional_suffixes.is_match(cleaned) {
                return true;
            }
        }

        // Check after comma (e.g., "Smith, MD")
        if let Some(after_comma) = name_lower.split(',').nth(1) {
            let trimmed = after_comma.trim();
            if self.professional_suffixes.is_match(trimmed) {
                return true;
            }
        }

        false
    }

    /// Check if name looks like casual nickname
    fn is_casual_nickname(&self, name: &str) -> bool {
        // Check for informal descriptors first
        let name_lower = name.to_lowercase();
        let informal_descriptors = ["from gym", "roommate", "lol", "haha", "buddy", "pal"];
        let has_informal_descriptor = informal_descriptors.iter().any(|d| name_lower.contains(d));

        if has_informal_descriptor {
            return true;
        }

        // Check if name is all lowercase (original string, not lowercased)
        // This catches things like "dave" or "mike" but not "Dave" or "John Smith"
        let has_letters = name.chars().any(|c| c.is_alphabetic());
        let all_lowercase = has_letters && name.chars().all(|c| !c.is_uppercase());

        all_lowercase
    }

    /// Store or update contact in cache
    pub fn upsert_contact(&self, contact: Contact) {
        let mut contacts = self.contacts.write();
        contacts.insert(contact.name.clone(), contact);
    }

    /// Get contact by name
    pub fn get_contact(&self, name: &str) -> Option<Contact> {
        let contacts = self.contacts.read();
        contacts.get(name).cloned()
    }

    /// Get or create contact from input
    pub fn get_or_create_contact(&self, input: &ContactInput) -> Contact {
        if let Some(existing) = self.get_contact(&input.name) {
            return existing;
        }

        let category = self.classify(input);
        let organization = if input.organization.is_empty() {
            None
        } else {
            Some(input.organization.clone())
        };

        Contact::new(input.name.clone(), organization, category)
    }

    /// Record interaction with contact
    pub fn record_interaction(&self, name: &str) {
        let mut contacts = self.contacts.write();
        if let Some(contact) = contacts.get_mut(name) {
            contact.record_interaction();
        }
    }

    /// Get all contacts sorted by frequency
    pub fn get_frequent_contacts(&self, limit: usize) -> Vec<Contact> {
        let contacts = self.contacts.read();
        let mut sorted: Vec<Contact> = contacts.values().cloned().collect();
        sorted.sort_by(|a, b| b.frequency.cmp(&a.frequency));
        sorted.truncate(limit);
        sorted
    }
}

impl Default for ContactClassifier {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_partner_classification() {
        let classifier = ContactClassifier::new();

        let cases = vec![
            ContactInput {
                name: "Bae".to_string(),
                organization: String::new(),
            },
            ContactInput {
                name: "❤️ Alex".to_string(),
                organization: String::new(),
            },
            ContactInput {
                name: "My Love".to_string(),
                organization: String::new(),
            },
            ContactInput {
                name: "Hubby 💍".to_string(),
                organization: String::new(),
            },
        ];

        for case in cases {
            assert_eq!(
                classifier.classify(&case),
                ContactCategory::Partner,
                "Failed for: {}",
                case.name
            );
        }
    }

    #[test]
    fn test_partner_overrides_organization() {
        let classifier = ContactClassifier::new();

        // CRITICAL: Partner indicators must override organization field
        let cases = vec![
            ContactInput {
                name: "Bae".to_string(),
                organization: "Acme Corp".to_string(),
            },
            ContactInput {
                name: "❤️ Alex".to_string(),
                organization: "Tech Inc".to_string(),
            },
            ContactInput {
                name: "My Love".to_string(),
                organization: "Business LLC".to_string(),
            },
            ContactInput {
                name: "Hubby 💍".to_string(),
                organization: "Company XYZ".to_string(),
            },
        ];

        for case in cases {
            assert_eq!(
                classifier.classify(&case),
                ContactCategory::Partner,
                "Partner detection MUST override organization field. Failed for: {} at {}",
                case.name,
                case.organization
            );
        }
    }

    #[test]
    fn test_close_family_classification() {
        let classifier = ContactClassifier::new();

        let cases = vec![
            ContactInput {
                name: "Mom".to_string(),
                organization: String::new(),
            },
            ContactInput {
                name: "Dad".to_string(),
                organization: String::new(),
            },
            ContactInput {
                name: "ICE Mom".to_string(),
                organization: String::new(),
            },
            ContactInput {
                name: "Grandma".to_string(),
                organization: String::new(),
            },
        ];

        for case in cases {
            assert_eq!(
                classifier.classify(&case),
                ContactCategory::CloseFamily,
                "Failed for: {}",
                case.name
            );
        }
    }

    #[test]
    fn test_professional_classification() {
        let classifier = ContactClassifier::new();

        // CRITICAL: Organization field presence
        let case1 = ContactInput {
            name: "Sarah".to_string(),
            organization: "Acme Inc".to_string(),
        };
        assert_eq!(classifier.classify(&case1), ContactCategory::Professional);

        // Professional titles
        let cases = vec![
            ContactInput {
                name: "Dr. Smith".to_string(),
                organization: String::new(),
            },
            ContactInput {
                name: "Prof. Johnson".to_string(),
                organization: String::new(),
            },
            ContactInput {
                name: "John Smith, MD".to_string(),
                organization: String::new(),
            },
            ContactInput {
                name: "Jane Doe PhD".to_string(),
                organization: String::new(),
            },
        ];

        for case in cases {
            assert_eq!(
                classifier.classify(&case),
                ContactCategory::Professional,
                "Failed for: {}",
                case.name
            );
        }
    }

    #[test]
    fn test_casual_peer_classification() {
        let classifier = ContactClassifier::new();

        let cases = vec![
            ContactInput {
                name: "dave from gym".to_string(),
                organization: String::new(),
            },
            ContactInput {
                name: "Mike 🍺".to_string(),
                organization: String::new(),
            },
            ContactInput {
                name: "alex lol".to_string(),
                organization: String::new(),
            },
        ];

        for case in cases {
            assert_eq!(
                classifier.classify(&case),
                ContactCategory::CasualPeer,
                "Failed for: {}",
                case.name
            );
        }
    }

    #[test]
    fn test_formal_neutral_classification() {
        let classifier = ContactClassifier::new();

        let cases = vec![
            ContactInput {
                name: "John Smith".to_string(),
                organization: String::new(),
            },
            ContactInput {
                name: "Uber Driver".to_string(),
                organization: String::new(),
            },
            ContactInput {
                name: "Plumber".to_string(),
                organization: String::new(),
            },
        ];

        for case in cases {
            assert_eq!(
                classifier.classify(&case),
                ContactCategory::FormalNeutral,
                "Failed for: {}",
                case.name
            );
        }
    }

    #[test]
    fn test_batch_classification() {
        let classifier = ContactClassifier::new();

        let inputs = vec![
            ContactInput {
                name: "Mom".to_string(),
                organization: String::new(),
            },
            ContactInput {
                name: "❤️ Alex".to_string(),
                organization: String::new(),
            },
            ContactInput {
                name: "Sarah".to_string(),
                organization: "Acme Inc".to_string(),
            },
            ContactInput {
                name: "dave from gym".to_string(),
                organization: String::new(),
            },
            ContactInput {
                name: "John Smith".to_string(),
                organization: String::new(),
            },
        ];

        let result = classifier.classify_batch(&inputs);

        assert_eq!(result.get("Mom"), Some(&ContactCategory::CloseFamily));
        assert_eq!(result.get("❤️ Alex"), Some(&ContactCategory::Partner));
        assert_eq!(result.get("Sarah"), Some(&ContactCategory::Professional));
        assert_eq!(
            result.get("dave from gym"),
            Some(&ContactCategory::CasualPeer)
        );
        assert_eq!(
            result.get("John Smith"),
            Some(&ContactCategory::FormalNeutral)
        );
    }

    #[test]
    fn test_json_serialization() {
        let classifier = ContactClassifier::new();

        let inputs = vec![
            ContactInput {
                name: "Mom".to_string(),
                organization: String::new(),
            },
            ContactInput {
                name: "Sarah Work".to_string(),
                organization: "Acme Inc".to_string(),
            },
        ];

        let json = classifier.classify_batch_json(&inputs);
        let parsed: HashMap<String, ContactCategory> = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.get("Mom"), Some(&ContactCategory::CloseFamily));
        assert_eq!(
            parsed.get("Sarah Work"),
            Some(&ContactCategory::Professional)
        );
    }
}
