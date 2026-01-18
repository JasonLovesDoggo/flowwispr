//! Self-learning typo correction engine
//!
//! Learns from user corrections when they edit transcribed text.
//! Uses Jaro-Winkler similarity for fuzzy matching and logarithmic confidence scaling.

use parking_lot::RwLock;
use std::collections::HashMap;
use strsim::jaro_winkler;
use tracing::{debug, info};

use crate::error::Result;
use crate::storage::Storage;
use crate::types::{Correction, CorrectionSource};

/// Minimum similarity threshold for considering a word pair as a typo correction
const MIN_SIMILARITY: f64 = 0.7;

/// Minimum confidence to auto-apply a correction (lowered to 0.55 to trigger at ~3 occurrences instead of ~5)
const MIN_AUTO_APPLY_CONFIDENCE: f32 = 0.55;

/// Maximum word length difference to consider a correction (set to 1 for exact wrong words like "there"/"their")
const MAX_LENGTH_DIFF: usize = 1;

/// Engine for learning and applying typo corrections
pub struct LearningEngine {
    /// In-memory cache of high-confidence corrections (original -> corrected)
    corrections: RwLock<HashMap<String, CachedCorrection>>,
    /// Minimum confidence for auto-applying corrections
    min_confidence: f32,
}

#[derive(Debug, Clone)]
struct CachedCorrection {
    corrected: String,
    confidence: f32,
}

impl LearningEngine {
    /// Create a new learning engine
    pub fn new() -> Self {
        Self {
            corrections: RwLock::new(HashMap::new()),
            min_confidence: MIN_AUTO_APPLY_CONFIDENCE,
        }
    }

    /// Create engine and load corrections from storage
    pub fn from_storage(storage: &Storage) -> Result<Self> {
        let engine = Self::new();
        let corrections = storage.get_corrections(MIN_AUTO_APPLY_CONFIDENCE)?;

        let mut cache = engine.corrections.write();
        for correction in corrections {
            cache.insert(
                correction.original.to_lowercase(),
                CachedCorrection {
                    corrected: correction.corrected,
                    confidence: correction.confidence,
                },
            );
        }
        drop(cache);

        info!(
            "Loaded {} corrections into learning engine",
            engine.corrections.read().len()
        );

        Ok(engine)
    }

    /// Set the minimum confidence threshold for auto-applying corrections
    pub fn set_min_confidence(&mut self, confidence: f32) {
        self.min_confidence = confidence.clamp(0.0, 1.0);
    }

    /// Learn from a before/after text comparison
    /// Detects word-level changes and records them as potential corrections
    pub fn learn_from_edit(
        &self,
        original: &str,
        edited: &str,
        storage: &Storage,
    ) -> Result<Vec<LearnedCorrection>> {
        let original_words: Vec<&str> = original.split_whitespace().collect();
        let edited_words: Vec<&str> = edited.split_whitespace().collect();

        let mut learned = Vec::new();

        // use edit distance alignment to find corresponding words
        let pairs = align_words(&original_words, &edited_words);

        for (orig, edit) in pairs {
            // skip if same
            if orig.eq_ignore_ascii_case(edit) {
                continue;
            }

            // check if this looks like a typo correction (high similarity)
            let similarity = jaro_winkler(orig, edit);

            if similarity >= MIN_SIMILARITY {
                // check length difference
                let len_diff = (orig.len() as isize - edit.len() as isize).unsigned_abs();
                if len_diff > MAX_LENGTH_DIFF {
                    continue;
                }

                // this looks like a typo correction
                let mut correction = Correction::new(
                    orig.to_lowercase(),
                    edit.to_string(),
                    CorrectionSource::UserEdit,
                );

                // save or update in storage (will increment occurrences if exists)
                storage.save_correction(&correction)?;

                // update cache if confidence is high enough
                correction.update_confidence();
                if correction.confidence >= self.min_confidence {
                    let mut cache = self.corrections.write();
                    cache.insert(
                        correction.original.clone(),
                        CachedCorrection {
                            corrected: correction.corrected.clone(),
                            confidence: correction.confidence,
                        },
                    );
                }

                debug!(
                    "Learned correction: '{}' -> '{}' (similarity: {:.2})",
                    orig, edit, similarity
                );

                learned.push(LearnedCorrection {
                    original: orig.to_string(),
                    corrected: edit.to_string(),
                    similarity,
                });
            }
        }

        Ok(learned)
    }

    /// Apply learned corrections to text
    /// Only applies corrections above the confidence threshold
    pub fn apply_corrections(&self, text: &str) -> (String, Vec<AppliedCorrection>) {
        let cache = self.corrections.read();

        if cache.is_empty() {
            return (text.to_string(), Vec::new());
        }

        let mut words: Vec<String> = text.split_whitespace().map(String::from).collect();
        let mut applied = Vec::new();

        for (i, word) in words.iter_mut().enumerate() {
            let word_lower = word.to_lowercase();

            if let Some(correction) = cache.get(&word_lower)
                && correction.confidence >= self.min_confidence
            {
                let original = word.clone();

                // preserve case pattern if possible
                *word = match_case(&correction.corrected, &original);

                applied.push(AppliedCorrection {
                    original,
                    corrected: word.clone(),
                    confidence: correction.confidence,
                    position: i,
                });
            }
        }

        let result = words.join(" ");

        if !applied.is_empty() {
            debug!("Applied {} corrections to text", applied.len());
        }

        (result, applied)
    }

    /// Check if we have a correction for a word
    pub fn has_correction(&self, word: &str) -> bool {
        let cache = self.corrections.read();
        cache.contains_key(&word.to_lowercase())
    }

    /// Get the correction for a word if available
    pub fn get_correction(&self, word: &str) -> Option<String> {
        let cache = self.corrections.read();
        cache
            .get(&word.to_lowercase())
            .filter(|c| c.confidence >= self.min_confidence)
            .map(|c| c.corrected.clone())
    }

    /// Get all cached corrections
    pub fn get_all_corrections(&self) -> Vec<(String, String, f32)> {
        self.corrections
            .read()
            .iter()
            .map(|(orig, c)| (orig.clone(), c.corrected.clone(), c.confidence))
            .collect()
    }

    /// Clear all cached corrections
    pub fn clear_cache(&self) {
        self.corrections.write().clear();
    }

    /// Get the number of cached corrections
    pub fn cache_size(&self) -> usize {
        self.corrections.read().len()
    }

    /// Remove a correction from the cache by original word
    pub fn remove_from_cache(&self, original: &str) {
        self.corrections.write().remove(&original.to_lowercase());
    }

    /// Reload corrections from storage (useful after deleting)
    pub fn reload_from_storage(
        &self,
        storage: &crate::storage::Storage,
    ) -> crate::error::Result<()> {
        let corrections = storage.get_corrections(self.min_confidence)?;

        let mut cache = self.corrections.write();
        cache.clear();
        for correction in corrections {
            cache.insert(
                correction.original.to_lowercase(),
                CachedCorrection {
                    corrected: correction.corrected,
                    confidence: correction.confidence,
                },
            );
        }

        info!("Reloaded {} corrections into learning engine", cache.len());

        Ok(())
    }
}

impl Default for LearningEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// A correction that was learned from user edits
#[derive(Debug, Clone)]
pub struct LearnedCorrection {
    pub original: String,
    pub corrected: String,
    pub similarity: f64,
}

/// A correction that was applied to text
#[derive(Debug, Clone)]
pub struct AppliedCorrection {
    pub original: String,
    pub corrected: String,
    pub confidence: f32,
    pub position: usize,
}

/// Align words from two texts using a simple diff algorithm
fn align_words<'a>(original: &[&'a str], edited: &[&'a str]) -> Vec<(&'a str, &'a str)> {
    let mut pairs = Vec::new();

    // simple approach: match by position with some tolerance for insertions/deletions
    let mut orig_idx = 0;
    let mut edit_idx = 0;

    while orig_idx < original.len() && edit_idx < edited.len() {
        let orig = original[orig_idx];
        let edit = edited[edit_idx];

        // if they're similar enough, consider them a pair
        let sim = jaro_winkler(orig, edit);
        if sim >= 0.5 {
            pairs.push((orig, edit));
            orig_idx += 1;
            edit_idx += 1;
        } else {
            // check if the original word was deleted (next edit word matches next orig word better)
            let skip_orig = if orig_idx + 1 < original.len() {
                jaro_winkler(original[orig_idx + 1], edit) > sim
            } else {
                false
            };

            // check if a word was inserted (current orig matches next edit word better)
            let skip_edit = if edit_idx + 1 < edited.len() {
                jaro_winkler(orig, edited[edit_idx + 1]) > sim
            } else {
                false
            };

            if skip_orig && !skip_edit {
                orig_idx += 1;
            } else if skip_edit && !skip_orig {
                edit_idx += 1;
            } else {
                // no good match, skip both
                orig_idx += 1;
                edit_idx += 1;
            }
        }
    }

    pairs
}

/// Try to match the case pattern of the original word
fn match_case(corrected: &str, original: &str) -> String {
    if original.chars().all(|c| c.is_uppercase()) {
        // all caps
        corrected.to_uppercase()
    } else if original.chars().next().is_some_and(|c| c.is_uppercase())
        && original.chars().skip(1).all(|c| c.is_lowercase())
    {
        // title case
        let mut chars = corrected.chars();
        match chars.next() {
            None => String::new(),
            Some(first) => first
                .to_uppercase()
                .chain(chars.flat_map(|c| c.to_lowercase()))
                .collect(),
        }
    } else {
        // preserve corrected case
        corrected.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_apply_corrections() {
        let engine = LearningEngine::new();

        // manually add a correction to cache
        {
            let mut cache = engine.corrections.write();
            cache.insert(
                "teh".to_string(),
                CachedCorrection {
                    corrected: "the".to_string(),
                    confidence: 0.95,
                },
            );

            cache.insert(
                "recieve".to_string(),
                CachedCorrection {
                    corrected: "receive".to_string(),
                    confidence: 0.9,
                },
            );
        }

        let (result, applied) = engine.apply_corrections("I will recieve teh package");

        assert_eq!(result, "I will receive the package");
        assert_eq!(applied.len(), 2);
    }

    #[test]
    fn test_case_matching() {
        assert_eq!(match_case("the", "TEH"), "THE");
        assert_eq!(match_case("the", "Teh"), "The");
        assert_eq!(match_case("the", "teh"), "the");
    }

    #[test]
    fn test_word_alignment() {
        let original = vec!["I", "recieve", "teh", "mail"];
        let edited = vec!["I", "receive", "the", "mail"];

        let pairs = align_words(&original, &edited);

        assert_eq!(pairs.len(), 4);
        assert_eq!(pairs[1], ("recieve", "receive"));
        assert_eq!(pairs[2], ("teh", "the"));
    }

    #[test]
    fn test_similarity_threshold() {
        // "hello" and "world" are very different
        let sim = jaro_winkler("hello", "world");
        assert!(sim < MIN_SIMILARITY);

        // "recieve" and "receive" are similar
        let sim = jaro_winkler("recieve", "receive");
        assert!(sim >= MIN_SIMILARITY);
    }

    #[test]
    fn test_confidence_below_threshold() {
        let mut engine = LearningEngine::new();
        engine.set_min_confidence(0.9);

        // add a low-confidence correction
        {
            let mut cache = engine.corrections.write();
            cache.insert(
                "foo".to_string(),
                CachedCorrection {
                    corrected: "bar".to_string(),
                    confidence: 0.5, // below threshold
                },
            );
        }

        let (result, applied) = engine.apply_corrections("test foo here");

        // should not be applied
        assert_eq!(result, "test foo here");
        assert!(applied.is_empty());
    }
}
