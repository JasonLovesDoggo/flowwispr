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

    // ========== Additional comprehensive tests ==========

    #[test]
    fn test_empty_name() {
        let classifier = ContactClassifier::new();

        let input = ContactInput {
            name: "".to_string(),
            organization: String::new(),
        };

        // Empty name should fall through to FormalNeutral
        let result = classifier.classify(&input);
        assert_eq!(result, ContactCategory::FormalNeutral);
    }

    #[test]
    fn test_whitespace_only_name() {
        let classifier = ContactClassifier::new();

        let input = ContactInput {
            name: "   \t\n   ".to_string(),
            organization: String::new(),
        };

        let result = classifier.classify(&input);
        assert_eq!(result, ContactCategory::FormalNeutral);
    }

    #[test]
    fn test_all_partner_keywords() {
        let classifier = ContactClassifier::new();

        let partner_terms = vec![
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

        for term in partner_terms {
            let input = ContactInput {
                name: term.to_string(),
                organization: String::new(),
            };
            assert_eq!(
                classifier.classify(&input),
                ContactCategory::Partner,
                "Partner keyword '{}' not detected",
                term
            );
        }
    }

    #[test]
    fn test_all_family_keywords() {
        let classifier = ContactClassifier::new();

        let family_terms = vec![
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
        ];

        for term in family_terms {
            let input = ContactInput {
                name: term.to_string(),
                organization: String::new(),
            };
            assert_eq!(
                classifier.classify(&input),
                ContactCategory::CloseFamily,
                "Family keyword '{}' not detected",
                term
            );
        }
    }

    #[test]
    fn test_all_professional_titles() {
        let classifier = ContactClassifier::new();

        let professional_titles = vec![
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
    }

    #[test]
    fn test_professional_credentials() {
        let classifier = ContactClassifier::new();

        let credentials = vec![
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
    }

    #[test]
    fn test_professional_credentials_after_comma() {
        let classifier = ContactClassifier::new();

        let input = ContactInput {
            name: "Smith, MD".to_string(),
            organization: String::new(),
        };
        assert_eq!(classifier.classify(&input), ContactCategory::Professional);
    }

    #[test]
    fn test_ice_prefix_contacts() {
        let classifier = ContactClassifier::new();

        let ice_contacts = vec![
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
    }

    #[test]
    fn test_all_partner_emojis() {
        let classifier = ContactClassifier::new();

        let partner_emojis = vec![
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
    }

    #[test]
    fn test_all_casual_emojis() {
        let classifier = ContactClassifier::new();

        let casual_emojis = vec![
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
    }

    #[test]
    fn test_informal_descriptors() {
        let classifier = ContactClassifier::new();

        let informal = vec![
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
    }

    #[test]
    fn test_all_lowercase_name_is_casual() {
        let classifier = ContactClassifier::new();

        // all lowercase names (without other indicators) should be casual
        let input = ContactInput {
            name: "john".to_string(),
            organization: String::new(),
        };
        assert_eq!(classifier.classify(&input), ContactCategory::CasualPeer);
    }

    #[test]
    fn test_proper_case_name_is_formal() {
        let classifier = ContactClassifier::new();

        // properly cased name without other indicators should be formal
        let input = ContactInput {
            name: "John".to_string(),
            organization: String::new(),
        };
        assert_eq!(classifier.classify(&input), ContactCategory::FormalNeutral);
    }

    #[test]
    fn test_case_insensitive_keywords() {
        let classifier = ContactClassifier::new();

        // Partner keywords should be case-insensitive
        let inputs = vec![
            ("BAE", ContactCategory::Partner),
            ("Bae", ContactCategory::Partner),
            ("MOM", ContactCategory::CloseFamily),
            ("Mom", ContactCategory::CloseFamily),
            ("DR. SMITH", ContactCategory::Professional),
            ("Dr. smith", ContactCategory::Professional),
        ];

        for (name, expected) in inputs {
            let input = ContactInput {
                name: name.to_string(),
                organization: String::new(),
            };
            assert_eq!(
                classifier.classify(&input),
                expected,
                "Case insensitivity failed for '{}'",
                name
            );
        }
    }

    #[test]
    fn test_priority_partner_over_family() {
        // If someone is named "Mom" but has a heart emoji, partner wins
        let classifier = ContactClassifier::new();

        let input = ContactInput {
            name: "❤️ Mom".to_string(), // unlikely but tests priority
            organization: String::new(),
        };
        assert_eq!(classifier.classify(&input), ContactCategory::Partner);
    }

    #[test]
    fn test_contact_cache_operations() {
        let classifier = ContactClassifier::new();

        // Create and upsert a contact
        let contact = Contact::new(
            "Test Contact".to_string(),
            Some("Test Org".to_string()),
            ContactCategory::Professional,
        );
        classifier.upsert_contact(contact.clone());

        // Retrieve it
        let retrieved = classifier.get_contact("Test Contact");
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.name, "Test Contact");
        assert_eq!(retrieved.category, ContactCategory::Professional);

        // Get non-existent
        assert!(classifier.get_contact("Nonexistent").is_none());
    }

    #[test]
    fn test_get_or_create_contact() {
        let classifier = ContactClassifier::new();

        let input = ContactInput {
            name: "New Person".to_string(),
            organization: "Some Company".to_string(),
        };

        // First call creates
        let contact1 = classifier.get_or_create_contact(&input);
        assert_eq!(contact1.name, "New Person");
        assert_eq!(contact1.category, ContactCategory::Professional);

        // Store it
        classifier.upsert_contact(contact1.clone());

        // Second call retrieves existing
        let contact2 = classifier.get_or_create_contact(&input);
        assert_eq!(contact2.id, contact1.id);
    }

    #[test]
    fn test_record_interaction() {
        let classifier = ContactClassifier::new();

        let contact = Contact::new(
            "Interacted".to_string(),
            None,
            ContactCategory::FormalNeutral,
        );
        classifier.upsert_contact(contact);

        // Record interaction
        classifier.record_interaction("Interacted");

        // Check it was recorded
        let retrieved = classifier.get_contact("Interacted").unwrap();
        assert_eq!(retrieved.frequency, 1);
        assert!(retrieved.last_contacted.is_some());
    }

    #[test]
    fn test_get_frequent_contacts() {
        let classifier = ContactClassifier::new();

        // Create contacts with different frequencies
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

    #[test]
    fn test_batch_classification_empty() {
        let classifier = ContactClassifier::new();
        let inputs: Vec<ContactInput> = vec![];

        let result = classifier.classify_batch(&inputs);
        assert!(result.is_empty());
    }

    #[test]
    fn test_batch_classification_json_empty() {
        let classifier = ContactClassifier::new();
        let inputs: Vec<ContactInput> = vec![];

        let json = classifier.classify_batch_json(&inputs);
        assert_eq!(json, "{}");
    }

    #[test]
    fn test_default_impl() {
        let classifier = ContactClassifier::default();
        // Should create a working classifier
        let input = ContactInput {
            name: "Mom".to_string(),
            organization: String::new(),
        };
        assert_eq!(classifier.classify(&input), ContactCategory::CloseFamily);
    }

    #[test]
    fn test_contact_input_deserialization() {
        let json = r#"{"name": "Test", "organization": ""}"#;
        let input: ContactInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.name, "Test");
        assert_eq!(input.organization, "");

        // organization should be optional (default to empty)
        let json2 = r#"{"name": "Test2"}"#;
        let input2: ContactInput = serde_json::from_str(json2).unwrap();
        assert_eq!(input2.name, "Test2");
        assert_eq!(input2.organization, "");
    }

    #[test]
    fn test_classification_result_serialization() {
        let result = ClassificationResult {
            name: "Test".to_string(),
            category: ContactCategory::Partner,
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("Test"));
        assert!(json.contains("partner"));
    }

    #[test]
    fn test_special_characters_in_name() {
        let classifier = ContactClassifier::new();

        let input = ContactInput {
            name: "O'Brien & Co.".to_string(),
            organization: String::new(),
        };
        // Should not panic, should classify as formal neutral
        let result = classifier.classify(&input);
        assert_eq!(result, ContactCategory::FormalNeutral);
    }

    #[test]
    fn test_unicode_name() {
        let classifier = ContactClassifier::new();

        let input = ContactInput {
            name: "日本語".to_string(), // Japanese characters
            organization: String::new(),
        };
        // BUG EXPOSURE: Japanese characters have no uppercase, so the "all lowercase" check
        // treats them as casual. This classifies non-Latin names incorrectly as CasualPeer
        // when they should be FormalNeutral.
        let result = classifier.classify(&input);
        assert_eq!(result, ContactCategory::CasualPeer); // Documents buggy behavior
    }

    #[test]
    fn test_very_long_name() {
        let classifier = ContactClassifier::new();

        let input = ContactInput {
            name: "A".repeat(1000),
            organization: String::new(),
        };
        // Should not panic
        let _ = classifier.classify(&input);
    }

    #[test]
    fn test_name_with_only_emojis() {
        let classifier = ContactClassifier::new();

        let input = ContactInput {
            name: "❤️💕💖".to_string(), // only partner emojis
            organization: String::new(),
        };
        assert_eq!(classifier.classify(&input), ContactCategory::Partner);

        let input2 = ContactInput {
            name: "🔥🍺🎮".to_string(), // only casual emojis
            organization: String::new(),
        };
        assert_eq!(classifier.classify(&input2), ContactCategory::CasualPeer);
    }

    #[test]
    fn test_embedded_keyword() {
        // BUG EXPOSURE: Keywords match anywhere in the name
        let classifier = ContactClassifier::new();

        // "mother" is embedded in "grandmother" - both should match family
        let input = ContactInput {
            name: "grandmother".to_string(),
            organization: String::new(),
        };
        assert_eq!(classifier.classify(&input), ContactCategory::CloseFamily);

        // But what about "lovelock" containing "love"?
        let input2 = ContactInput {
            name: "Lovelock".to_string(), // surname containing "love"
            organization: String::new(),
        };
        // This will incorrectly classify as Partner because "love" is found
        assert_eq!(classifier.classify(&input2), ContactCategory::Partner);
        // BUG: Surname "Lovelock" should probably be FormalNeutral
    }
}
