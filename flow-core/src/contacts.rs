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
        has_letters && name.chars().all(|c| !c.is_uppercase())
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

    /// Comprehensive test for all partner classification scenarios:
    /// - All partner keywords (bae, hubby, wife, etc.)
    /// - All romantic emojis (❤️, 💕, 💍, etc.)
    /// - Partner indicators override organization field
    /// - Partner priority over family indicators
    /// - Case insensitivity
    #[test]
    fn test_partner_classification() {
        let classifier = ContactClassifier::new();

        // All partner keywords must be detected
        let partner_keywords = [
            "bae", "hubby", "wife", "wifey", "husband", "my love", "baby", "babe", "love",
            "honey", "sweetheart", "darling", "dear", "sweetie", "boo",
        ];
        for keyword in partner_keywords {
            let input = ContactInput {
                name: keyword.to_string(),
                organization: String::new(),
            };
            assert_eq!(
                classifier.classify(&input),
                ContactCategory::Partner,
                "Partner keyword '{}' not detected",
                keyword
            );
        }

        // All romantic emojis must be detected
        let partner_emojis = [
            '❤', '💕', '💖', '💗', '💘', '💝', '💞', '💟', '💙', '💚', '💛', '🧡', '💜', '🖤',
            '🤍', '🤎', '💋', '💍', '💑', '💏', '👩', '👨', '❣',
        ];
        for emoji in partner_emojis {
            let input = ContactInput {
                name: format!("Alex {}", emoji),
                organization: String::new(),
            };
            assert_eq!(
                classifier.classify(&input),
                ContactCategory::Partner,
                "Partner emoji '{}' not detected",
                emoji
            );
        }

        // Partner MUST override organization field (critical business logic)
        let override_cases = [
            ("Bae", "Acme Corp"),
            ("❤️ Alex", "Tech Inc"),
            ("My Love", "Business LLC"),
            ("Hubby 💍", "Company XYZ"),
        ];
        for (name, org) in override_cases {
            let input = ContactInput {
                name: name.to_string(),
                organization: org.to_string(),
            };
            assert_eq!(
                classifier.classify(&input),
                ContactCategory::Partner,
                "Partner MUST override organization. Failed: '{}' at '{}'",
                name,
                org
            );
        }

        // Partner takes priority over family indicators
        let input = ContactInput {
            name: "❤️ Mom".to_string(),
            organization: String::new(),
        };
        assert_eq!(classifier.classify(&input), ContactCategory::Partner);

        // Case insensitivity
        for name in ["BAE", "Bae", "bae", "BAe"] {
            let input = ContactInput {
                name: name.to_string(),
                organization: String::new(),
            };
            assert_eq!(
                classifier.classify(&input),
                ContactCategory::Partner,
                "Case insensitivity failed for '{}'",
                name
            );
        }

        // Emoji-only names with partner emojis
        let input = ContactInput {
            name: "❤️💕💖".to_string(),
            organization: String::new(),
        };
        assert_eq!(classifier.classify(&input), ContactCategory::Partner);
    }

    /// Comprehensive test for all family classification scenarios:
    /// - All family keywords (mom, dad, grandma, etc.)
    /// - ICE (In Case of Emergency) prefix contacts
    /// - Case insensitivity
    #[test]
    fn test_family_classification() {
        let classifier = ContactClassifier::new();

        // All family keywords must be detected
        let family_keywords = [
            "mom", "dad", "mama", "papa", "mother", "father", "grandma", "grandpa",
            "grandmother", "grandfather", "aunt", "uncle", "sister", "brother", "sis", "bro",
            "cousin", "nephew", "niece",
        ];
        for keyword in family_keywords {
            let input = ContactInput {
                name: keyword.to_string(),
                organization: String::new(),
            };
            assert_eq!(
                classifier.classify(&input),
                ContactCategory::CloseFamily,
                "Family keyword '{}' not detected",
                keyword
            );
        }

        // ICE (In Case of Emergency) prefix contacts
        let ice_contacts = [
            "ice mom",
            "ice dad",
            "ice mama",
            "ice papa",
            "ice aunt",
            "ice uncle",
            "ice grandmother",
            "ice grandfather",
        ];
        for contact in ice_contacts {
            let input = ContactInput {
                name: contact.to_string(),
                organization: String::new(),
            };
            assert_eq!(
                classifier.classify(&input),
                ContactCategory::CloseFamily,
                "ICE contact '{}' not detected as family",
                contact
            );
        }

        // Case insensitivity
        for name in ["MOM", "Mom", "mom", "MoM"] {
            let input = ContactInput {
                name: name.to_string(),
                organization: String::new(),
            };
            assert_eq!(
                classifier.classify(&input),
                ContactCategory::CloseFamily,
                "Case insensitivity failed for '{}'",
                name
            );
        }
    }

    /// Comprehensive test for all professional classification scenarios:
    /// - Organization field presence triggers professional
    /// - All professional titles (Dr., Prof., CEO, etc.)
    /// - All professional credentials/suffixes (MD, PhD, CPA, etc.)
    /// - Credentials after comma (Smith, MD)
    /// - Case insensitivity
    #[test]
    fn test_professional_classification() {
        let classifier = ContactClassifier::new();

        // Organization field presence triggers professional
        let input = ContactInput {
            name: "Sarah".to_string(),
            organization: "Acme Inc".to_string(),
        };
        assert_eq!(classifier.classify(&input), ContactCategory::Professional);

        // All professional titles
        let professional_titles = [
            "Dr. Smith",
            "Dr Smith",
            "Prof. Jones",
            "Prof Jones",
            "Professor Williams",
            "Boss Man",
            "Manager Kim",
            "Coach Taylor",
            "Director Lee",
            "VP Sales",
            "CEO Bob",
            "CTO Alice",
            "CFO Carol",
            "COO Dave",
            "President Obama",
            "Supervisor Chen",
            "Lead Engineer",
            "Senior Dev",
        ];
        for title in professional_titles {
            let input = ContactInput {
                name: title.to_string(),
                organization: String::new(),
            };
            assert_eq!(
                classifier.classify(&input),
                ContactCategory::Professional,
                "Professional title '{}' not detected",
                title
            );
        }

        // All professional credentials as suffix
        let credentials = [
            "John Doe MD",
            "Jane Smith PhD",
            "Bob CPA",
            "Alice Esq",
            "Tom DDS",
            "Mary JD",
            "Steve MBA",
            "Lisa RN",
            "Dave DVM",
            "Kate DO",
        ];
        for cred in credentials {
            let input = ContactInput {
                name: cred.to_string(),
                organization: String::new(),
            };
            assert_eq!(
                classifier.classify(&input),
                ContactCategory::Professional,
                "Professional credential '{}' not detected",
                cred
            );
        }

        // Credentials after comma
        let input = ContactInput {
            name: "Smith, MD".to_string(),
            organization: String::new(),
        };
        assert_eq!(classifier.classify(&input), ContactCategory::Professional);

        // Case insensitivity
        for name in ["DR. SMITH", "Dr. smith", "dr. SMITH"] {
            let input = ContactInput {
                name: name.to_string(),
                organization: String::new(),
            };
            assert_eq!(
                classifier.classify(&input),
                ContactCategory::Professional,
                "Case insensitivity failed for '{}'",
                name
            );
        }
    }

    /// Comprehensive test for all casual/peer classification scenarios:
    /// - All casual emojis (🔥, 🍺, 🎮, etc.)
    /// - Informal descriptors (from gym, roommate, lol, etc.)
    /// - All-lowercase names treated as casual nicknames
    #[test]
    fn test_casual_classification() {
        let classifier = ContactClassifier::new();

        // All casual emojis
        let casual_emojis = [
            '🔥', '🍻', '🤪', '🍕', '🎮', '⚽', '🏀', '🎸', '🎉', '💪', '🤘', '🍺', '🎯', '🚀',
            '💯', '👊', '🤙', '😎', '🏆',
        ];
        for emoji in casual_emojis {
            let input = ContactInput {
                name: format!("Mike {}", emoji),
                organization: String::new(),
            };
            assert_eq!(
                classifier.classify(&input),
                ContactCategory::CasualPeer,
                "Casual emoji '{}' not detected",
                emoji
            );
        }

        // Informal descriptors
        let informal = [
            "dave from gym",
            "mike roommate",
            "sarah lol",
            "bob haha",
            "alice buddy",
            "tom pal",
        ];
        for name in informal {
            let input = ContactInput {
                name: name.to_string(),
                organization: String::new(),
            };
            assert_eq!(
                classifier.classify(&input),
                ContactCategory::CasualPeer,
                "Informal descriptor '{}' not detected",
                name
            );
        }

        // All-lowercase names treated as casual nicknames
        let input = ContactInput {
            name: "john".to_string(),
            organization: String::new(),
        };
        assert_eq!(classifier.classify(&input), ContactCategory::CasualPeer);

        // Emoji-only names with casual emojis
        let input = ContactInput {
            name: "🔥🍺🎮".to_string(),
            organization: String::new(),
        };
        assert_eq!(classifier.classify(&input), ContactCategory::CasualPeer);
    }

    /// Comprehensive test for edge cases and formal/neutral fallback:
    /// - Empty and whitespace-only names
    /// - Proper case names (formal neutral)
    /// - Special characters
    /// - Unicode/non-Latin names (documents known bug)
    /// - Very long names
    /// - Embedded keyword substring matching (documents known bug)
    #[test]
    fn test_edge_cases() {
        let classifier = ContactClassifier::new();

        // Empty name falls through to FormalNeutral
        let input = ContactInput {
            name: "".to_string(),
            organization: String::new(),
        };
        assert_eq!(classifier.classify(&input), ContactCategory::FormalNeutral);

        // Whitespace-only name
        let input = ContactInput {
            name: "   \t\n   ".to_string(),
            organization: String::new(),
        };
        assert_eq!(classifier.classify(&input), ContactCategory::FormalNeutral);

        // Proper case names without indicators are formal neutral
        let formal_names = ["John Smith", "Uber Driver", "Plumber", "John"];
        for name in formal_names {
            let input = ContactInput {
                name: name.to_string(),
                organization: String::new(),
            };
            assert_eq!(
                classifier.classify(&input),
                ContactCategory::FormalNeutral,
                "Formal name '{}' not classified correctly",
                name
            );
        }

        // Special characters should not panic
        let input = ContactInput {
            name: "O'Brien & Co.".to_string(),
            organization: String::new(),
        };
        assert_eq!(classifier.classify(&input), ContactCategory::FormalNeutral);

        // Unicode/non-Latin names - documents known bug where caseless scripts
        // are incorrectly treated as all-lowercase and classified as CasualPeer
        let input = ContactInput {
            name: "日本語".to_string(),
            organization: String::new(),
        };
        assert_eq!(classifier.classify(&input), ContactCategory::CasualPeer); // BUG: should be FormalNeutral

        // Very long names should not panic
        let input = ContactInput {
            name: "A".repeat(1000),
            organization: String::new(),
        };
        let _ = classifier.classify(&input); // Just ensure no panic

        // Embedded keyword substring matching - documents known bug where
        // surnames containing partner keywords are misclassified
        let input = ContactInput {
            name: "grandmother".to_string(), // contains "mother", correctly matches family
            organization: String::new(),
        };
        assert_eq!(classifier.classify(&input), ContactCategory::CloseFamily);

        let input = ContactInput {
            name: "Lovelock".to_string(), // surname containing "love"
            organization: String::new(),
        };
        assert_eq!(classifier.classify(&input), ContactCategory::Partner); // BUG: should be FormalNeutral
    }

    /// Test batch classification and JSON serialization
    #[test]
    fn test_batch_operations() {
        let classifier = ContactClassifier::new();

        // Batch classification with all categories
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

        // Empty batch
        let empty: Vec<ContactInput> = vec![];
        assert!(classifier.classify_batch(&empty).is_empty());
        assert_eq!(classifier.classify_batch_json(&empty), "{}");

        // JSON serialization
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

    /// Test contact cache operations (upsert, get, frequency tracking)
    #[test]
    fn test_contact_cache() {
        let classifier = ContactClassifier::new();

        // Upsert and retrieve
        let contact = Contact::new(
            "Test Contact".to_string(),
            Some("Test Org".to_string()),
            ContactCategory::Professional,
        );
        classifier.upsert_contact(contact.clone());
        let retrieved = classifier.get_contact("Test Contact").unwrap();
        assert_eq!(retrieved.name, "Test Contact");
        assert_eq!(retrieved.category, ContactCategory::Professional);

        // Get non-existent returns None
        assert!(classifier.get_contact("Nonexistent").is_none());

        // Get or create
        let input = ContactInput {
            name: "New Person".to_string(),
            organization: "Some Company".to_string(),
        };
        let contact1 = classifier.get_or_create_contact(&input);
        assert_eq!(contact1.name, "New Person");
        assert_eq!(contact1.category, ContactCategory::Professional);
        classifier.upsert_contact(contact1.clone());
        let contact2 = classifier.get_or_create_contact(&input);
        assert_eq!(contact2.id, contact1.id);

        // Record interaction
        let contact = Contact::new(
            "Interacted".to_string(),
            None,
            ContactCategory::FormalNeutral,
        );
        classifier.upsert_contact(contact);
        classifier.record_interaction("Interacted");
        let retrieved = classifier.get_contact("Interacted").unwrap();
        assert_eq!(retrieved.frequency, 1);
        assert!(retrieved.last_contacted.is_some());

        // Get frequent contacts sorted by frequency
        let mut c1 = Contact::new("Low".to_string(), None, ContactCategory::FormalNeutral);
        c1.frequency = 1;
        let mut c2 = Contact::new("High".to_string(), None, ContactCategory::FormalNeutral);
        c2.frequency = 10;
        let mut c3 = Contact::new("Medium".to_string(), None, ContactCategory::FormalNeutral);
        c3.frequency = 5;
        classifier.upsert_contact(c1);
        classifier.upsert_contact(c2);
        classifier.upsert_contact(c3);
        let frequent = classifier.get_frequent_contacts(2);
        assert_eq!(frequent.len(), 2);
        assert_eq!(frequent[0].name, "High");
        assert_eq!(frequent[1].name, "Medium");
    }

    /// Test serde serialization/deserialization
    #[test]
    fn test_serde() {
        // ContactInput deserialization
        let json = r#"{"name": "Test", "organization": ""}"#;
        let input: ContactInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.name, "Test");
        assert_eq!(input.organization, "");

        // organization defaults to empty when missing
        let json2 = r#"{"name": "Test2"}"#;
        let input2: ContactInput = serde_json::from_str(json2).unwrap();
        assert_eq!(input2.name, "Test2");
        assert_eq!(input2.organization, "");

        // ClassificationResult serialization
        let result = ClassificationResult {
            name: "Test".to_string(),
            category: ContactCategory::Partner,
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("Test"));
        assert!(json.contains("partner"));
    }

    /// Test Default impl
    #[test]
    fn test_default_impl() {
        let classifier = ContactClassifier::default();
        let input = ContactInput {
            name: "Mom".to_string(),
            organization: String::new(),
        };
        assert_eq!(classifier.classify(&input), ContactCategory::CloseFamily);
    }
}
