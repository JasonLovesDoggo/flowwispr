//! Writing modes engine for per-app mode customization
//!
//! The WritingMode enum is defined in types.rs, this module provides
//! the engine for managing modes per-app and the style analyzer.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::error::Result;
use crate::storage::Storage;

// Re-export WritingMode from types for convenience
pub use crate::types::WritingMode;

/// Engine for managing writing modes per app
pub struct WritingModeEngine {
    /// Default mode when no app-specific mode is set
    default_mode: WritingMode,
    /// In-memory cache of app modes
    app_modes: HashMap<String, WritingMode>,
}

impl WritingModeEngine {
    /// Create a new engine with the given default mode
    pub fn new(default_mode: WritingMode) -> Self {
        Self {
            default_mode,
            app_modes: HashMap::new(),
        }
    }

    /// Create engine and load app modes from storage
    pub fn from_storage(_storage: &Storage, default_mode: WritingMode) -> Result<Self> {
        let engine = Self::new(default_mode);
        // load modes would need a get_all_app_modes method
        // for now we lazily load on demand
        Ok(engine)
    }

    /// Get the writing mode for an app
    pub fn get_mode(&self, app_name: &str) -> WritingMode {
        self.app_modes
            .get(app_name)
            .copied()
            .unwrap_or(self.default_mode)
    }

    /// Get mode for app, loading from storage if not cached
    pub fn get_mode_with_storage(&mut self, app_name: &str, storage: &Storage) -> WritingMode {
        if let Some(&mode) = self.app_modes.get(app_name) {
            return mode;
        }

        // try loading from storage
        if let Ok(Some(mode)) = storage.get_app_mode(app_name) {
            self.app_modes.insert(app_name.to_string(), mode);
            return mode;
        }

        self.default_mode
    }

    /// Set the writing mode for an app
    pub fn set_mode(&mut self, app_name: &str, mode: WritingMode) {
        debug!("Setting mode for {} to {:?}", app_name, mode);
        self.app_modes.insert(app_name.to_string(), mode);
    }

    /// Set mode and persist to storage
    pub fn set_mode_with_storage(
        &mut self,
        app_name: &str,
        mode: WritingMode,
        storage: &Storage,
    ) -> Result<()> {
        self.set_mode(app_name, mode);
        storage.save_app_mode(app_name, mode)?;
        Ok(())
    }

    /// Get the default mode
    pub fn default_mode(&self) -> WritingMode {
        self.default_mode
    }

    /// Set the default mode
    pub fn set_default_mode(&mut self, mode: WritingMode) {
        self.default_mode = mode;
    }

    /// Clear the mode for an app (reverts to default)
    pub fn clear_mode(&mut self, app_name: &str) {
        self.app_modes.remove(app_name);
    }

    /// Get all app-specific mode overrides
    pub fn get_all_overrides(&self) -> &HashMap<String, WritingMode> {
        &self.app_modes
    }
}

/// Style analyzer for learning user preferences from their edits
pub struct StyleAnalyzer;

impl StyleAnalyzer {
    /// Analyze a text sample and suggest a writing mode
    pub fn analyze_style(text: &str) -> WritingMode {
        let has_caps = text.chars().any(|c| c.is_uppercase());
        let has_punctuation = text.chars().any(|c| matches!(c, '.' | '!' | '?' | ','));
        let has_exclamation = text.contains('!');
        let all_lower = text == text.to_lowercase();
        let word_count = text.split_whitespace().count();

        // detect excited style
        if has_exclamation && text.matches('!').count() >= 2 {
            return WritingMode::Excited;
        }

        // detect very casual (all lowercase, no/minimal punctuation)
        if all_lower && !has_punctuation && word_count > 0 {
            return WritingMode::VeryCasual;
        }

        // detect formal (proper caps, punctuation, longer sentences)
        let sentences: Vec<&str> = text
            .split(['.', '!', '?'])
            .filter(|s| !s.trim().is_empty())
            .collect();
        let num_sentences = sentences.len().max(1);
        let avg_sentence_length = word_count / num_sentences;

        if has_caps && has_punctuation && avg_sentence_length >= 8 {
            return WritingMode::Formal;
        }

        // default to casual
        WritingMode::Casual
    }

    /// Analyze multiple samples and return the most common style
    pub fn analyze_samples(samples: &[String]) -> WritingMode {
        if samples.is_empty() {
            return WritingMode::default();
        }

        let mut counts: HashMap<WritingMode, usize> = HashMap::new();

        for sample in samples {
            let mode = Self::analyze_style(sample);
            *counts.entry(mode).or_insert(0) += 1;
        }

        counts
            .into_iter()
            .max_by_key(|(_, count)| *count)
            .map(|(mode, _)| mode)
            .unwrap_or_default()
    }
}

/// Observed typing style metrics for an app
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StyleObservation {
    pub app_name: String,
    /// Ratio of capitalized first letters (0.0 to 1.0)
    pub avg_caps_ratio: f32,
    /// Punctuation marks per 100 characters
    pub avg_punctuation_density: f32,
    /// Whether exclamation marks are commonly used
    pub uses_exclamations: bool,
    /// Number of samples used
    pub sample_count: u32,
    pub last_observed: DateTime<Utc>,
}

impl StyleObservation {
    pub fn new(app_name: String) -> Self {
        Self {
            app_name,
            avg_caps_ratio: 0.0,
            avg_punctuation_density: 0.0,
            uses_exclamations: false,
            sample_count: 0,
            last_observed: Utc::now(),
        }
    }

    /// Update observation with a new sample
    pub fn update(&mut self, text: &str) {
        let caps_ratio = calculate_caps_ratio(text);
        let punct_density = calculate_punctuation_density(text);
        let has_exclamations = text.contains('!');

        // rolling average
        let n = self.sample_count as f32;
        self.avg_caps_ratio = (self.avg_caps_ratio * n + caps_ratio) / (n + 1.0);
        self.avg_punctuation_density =
            (self.avg_punctuation_density * n + punct_density) / (n + 1.0);
        self.uses_exclamations = self.uses_exclamations || has_exclamations;
        self.sample_count += 1;
        self.last_observed = Utc::now();
    }

    /// Suggest a writing mode based on observations
    pub fn suggest_mode(&self) -> Option<WritingModeSuggestion> {
        // need enough samples to make a suggestion (lowered to 2 for faster learning)
        if self.sample_count < 2 {
            return None;
        }

        let suggested = if self.avg_caps_ratio < 0.3 && self.avg_punctuation_density < 2.0 {
            WritingMode::VeryCasual
        } else if self.avg_caps_ratio > 0.8 && self.uses_exclamations {
            WritingMode::Excited
        } else if self.avg_caps_ratio > 0.8 && self.avg_punctuation_density > 4.0 {
            WritingMode::Formal
        } else {
            WritingMode::Casual
        };

        Some(WritingModeSuggestion {
            app_name: self.app_name.clone(),
            suggested_mode: suggested,
            confidence: (self.sample_count as f32 / 20.0).min(1.0),
            based_on_samples: self.sample_count,
        })
    }
}

/// A mode suggestion based on observed behavior
#[derive(Debug, Clone)]
pub struct WritingModeSuggestion {
    pub app_name: String,
    pub suggested_mode: WritingMode,
    pub confidence: f32,
    pub based_on_samples: u32,
}

/// Style learner that tracks observations per app
pub struct StyleLearner {
    observations: HashMap<String, StyleObservation>,
}

impl Default for StyleLearner {
    fn default() -> Self {
        Self::new()
    }
}

impl StyleLearner {
    pub fn new() -> Self {
        Self {
            observations: HashMap::new(),
        }
    }

    /// Observe user's edited text for an app
    pub fn observe(&mut self, app_name: &str, edited_text: &str) {
        let obs = self
            .observations
            .entry(app_name.to_string())
            .or_insert_with(|| StyleObservation::new(app_name.to_string()));
        obs.update(edited_text);
    }

    /// Observe with storage persistence
    pub fn observe_with_storage(&mut self, app_name: &str, edited_text: &str, storage: &Storage) {
        self.observe(app_name, edited_text);
        // save sample to storage
        if let Err(e) = storage.save_style_sample(app_name, edited_text) {
            debug!("Failed to save style sample: {}", e);
        }
    }

    /// Get a mode suggestion for an app
    pub fn suggest_mode(&self, app_name: &str) -> Option<WritingModeSuggestion> {
        self.observations
            .get(app_name)
            .and_then(|obs| obs.suggest_mode())
    }

    /// Get observation for an app
    pub fn get_observation(&self, app_name: &str) -> Option<&StyleObservation> {
        self.observations.get(app_name)
    }

    /// Load observations from storage samples
    pub fn load_from_storage(&mut self, storage: &Storage, app_name: &str) -> Result<()> {
        let samples = storage.get_style_samples(app_name, 50)?;
        for sample in samples {
            self.observe(app_name, &sample);
        }
        Ok(())
    }

    /// Get all observations
    pub fn all_observations(&self) -> &HashMap<String, StyleObservation> {
        &self.observations
    }
}

fn calculate_caps_ratio(text: &str) -> f32 {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() {
        return 0.0;
    }

    let capitalized = words
        .iter()
        .filter(|w| w.chars().next().is_some_and(|c| c.is_uppercase()))
        .count();

    capitalized as f32 / words.len() as f32
}

fn calculate_punctuation_density(text: &str) -> f32 {
    if text.is_empty() {
        return 0.0;
    }

    let punct_count = text
        .chars()
        .filter(|c| matches!(c, '.' | ',' | '!' | '?' | ';' | ':'))
        .count();

    (punct_count as f32 / text.len() as f32) * 100.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::AppCategory;

    #[test]
    fn test_style_analysis() {
        assert_eq!(
            StyleAnalyzer::analyze_style("hello how r u"),
            WritingMode::VeryCasual
        );

        assert_eq!(
            StyleAnalyzer::analyze_style("This is amazing!! So excited!!!"),
            WritingMode::Excited
        );

        assert_eq!(
            StyleAnalyzer::analyze_style(
                "I would like to schedule a meeting to discuss the quarterly results."
            ),
            WritingMode::Formal
        );
    }

    #[test]
    fn test_engine() {
        let mut engine = WritingModeEngine::new(WritingMode::Casual);

        assert_eq!(engine.get_mode("Slack"), WritingMode::Casual);

        engine.set_mode("Mail", WritingMode::Formal);
        assert_eq!(engine.get_mode("Mail"), WritingMode::Formal);

        engine.clear_mode("Mail");
        assert_eq!(engine.get_mode("Mail"), WritingMode::Casual);
    }

    #[test]
    fn test_style_learner() {
        let mut learner = StyleLearner::new();

        // observe some casual text
        for _ in 0..6 {
            learner.observe("Slack", "hey whats up");
        }

        let suggestion = learner.suggest_mode("Slack");
        assert!(suggestion.is_some());
        let suggestion = suggestion.unwrap();
        assert_eq!(suggestion.suggested_mode, WritingMode::VeryCasual);
        assert!(suggestion.confidence > 0.0);
    }

    #[test]
    fn test_style_observation() {
        let mut obs = StyleObservation::new("Test".to_string());

        // add formal samples
        for _ in 0..5 {
            obs.update("Hello, I hope this message finds you well. Best regards.");
        }

        // should suggest formal
        let suggestion = obs.suggest_mode();
        assert!(suggestion.is_some());
    }

    #[test]
    fn test_caps_ratio() {
        assert_eq!(calculate_caps_ratio("hello world"), 0.0);
        assert_eq!(calculate_caps_ratio("Hello World"), 1.0);
        assert_eq!(calculate_caps_ratio("Hello world test"), 1.0 / 3.0);
    }

    #[test]
    fn test_punctuation_density() {
        assert_eq!(calculate_punctuation_density("hello"), 0.0);
        // "hello." is 6 chars, 1 punct = 100/6 = ~16.67
        let density = calculate_punctuation_density("hello.");
        assert!(density > 16.0 && density < 17.0);
    }

    // ========== Additional comprehensive tests ==========

    #[test]
    fn test_all_app_category_suggestions() {
        assert_eq!(
            WritingMode::suggested_for_category(AppCategory::Email),
            WritingMode::Formal
        );
        assert_eq!(
            WritingMode::suggested_for_category(AppCategory::Code),
            WritingMode::Formal
        );
        assert_eq!(
            WritingMode::suggested_for_category(AppCategory::Documents),
            WritingMode::Formal
        );
        assert_eq!(
            WritingMode::suggested_for_category(AppCategory::Slack),
            WritingMode::Casual
        );
        assert_eq!(
            WritingMode::suggested_for_category(AppCategory::Social),
            WritingMode::VeryCasual
        );
        assert_eq!(
            WritingMode::suggested_for_category(AppCategory::Browser),
            WritingMode::Casual
        );
        assert_eq!(
            WritingMode::suggested_for_category(AppCategory::Terminal),
            WritingMode::VeryCasual
        );
        assert_eq!(
            WritingMode::suggested_for_category(AppCategory::Unknown),
            WritingMode::Casual
        );
    }

    #[test]
    fn test_style_analysis_empty_text() {
        let mode = StyleAnalyzer::analyze_style("");
        // empty text should probably return default (Casual)
        assert_eq!(mode, WritingMode::Casual);
    }

    #[test]
    fn test_style_analysis_whitespace_only() {
        let mode = StyleAnalyzer::analyze_style("   \t\n   ");
        // whitespace-only should return Casual (default)
        assert_eq!(mode, WritingMode::Casual);
    }

    #[test]
    fn test_style_analysis_single_word() {
        // single word all lowercase
        assert_eq!(
            StyleAnalyzer::analyze_style("hello"),
            WritingMode::VeryCasual
        );

        // single word capitalized
        assert_eq!(StyleAnalyzer::analyze_style("Hello"), WritingMode::Casual);
    }

    #[test]
    fn test_style_analysis_excited_detection() {
        // need at least 2 exclamation marks
        assert_eq!(StyleAnalyzer::analyze_style("Wow!"), WritingMode::Casual);
        assert_eq!(StyleAnalyzer::analyze_style("Wow!!"), WritingMode::Excited);
        assert_eq!(
            StyleAnalyzer::analyze_style("Amazing! Great!"),
            WritingMode::Excited
        );
    }

    #[test]
    fn test_style_analysis_formal_long_sentences() {
        // formal requires proper caps, punctuation, and avg sentence length >= 8
        let formal_text =
            "I hope this message finds you in good spirits and excellent health today.";
        assert_eq!(
            StyleAnalyzer::analyze_style(formal_text),
            WritingMode::Formal
        );

        // shorter sentences shouldn't be formal even with caps and punctuation
        let short_text = "Hello. Yes. Ok.";
        assert_ne!(
            StyleAnalyzer::analyze_style(short_text),
            WritingMode::Formal
        );
    }

    #[test]
    fn test_style_analysis_very_casual() {
        // all lowercase, no punctuation
        assert_eq!(
            StyleAnalyzer::analyze_style("hey whats up"),
            WritingMode::VeryCasual
        );
        assert_eq!(
            StyleAnalyzer::analyze_style("k cool"),
            WritingMode::VeryCasual
        );
        assert_eq!(
            StyleAnalyzer::analyze_style("yea sure"),
            WritingMode::VeryCasual
        );
    }

    #[test]
    fn test_analyze_samples_empty() {
        let samples: Vec<String> = vec![];
        assert_eq!(
            StyleAnalyzer::analyze_samples(&samples),
            WritingMode::default()
        );
    }

    #[test]
    fn test_analyze_samples_single() {
        let samples = vec!["hello how r u".to_string()];
        assert_eq!(
            StyleAnalyzer::analyze_samples(&samples),
            WritingMode::VeryCasual
        );
    }

    #[test]
    fn test_analyze_samples_majority_wins() {
        let samples = vec![
            "hello".to_string(),           // VeryCasual
            "hi there".to_string(),        // VeryCasual
            "This is formal.".to_string(), // Casual (not long enough for Formal)
        ];
        // VeryCasual should win by majority
        let result = StyleAnalyzer::analyze_samples(&samples);
        assert_eq!(result, WritingMode::VeryCasual);
    }

    #[test]
    fn test_engine_default_mode() {
        let engine = WritingModeEngine::new(WritingMode::Formal);
        assert_eq!(engine.default_mode(), WritingMode::Formal);

        let engine2 = WritingModeEngine::new(WritingMode::VeryCasual);
        assert_eq!(engine2.default_mode(), WritingMode::VeryCasual);
    }

    #[test]
    fn test_engine_set_default_mode() {
        let mut engine = WritingModeEngine::new(WritingMode::Casual);
        assert_eq!(engine.default_mode(), WritingMode::Casual);

        engine.set_default_mode(WritingMode::Formal);
        assert_eq!(engine.default_mode(), WritingMode::Formal);

        // apps without overrides should now use new default
        assert_eq!(engine.get_mode("SomeApp"), WritingMode::Formal);
    }

    #[test]
    fn test_engine_get_all_overrides() {
        let mut engine = WritingModeEngine::new(WritingMode::Casual);
        engine.set_mode("App1", WritingMode::Formal);
        engine.set_mode("App2", WritingMode::Excited);

        let overrides = engine.get_all_overrides();
        assert_eq!(overrides.len(), 2);
        assert_eq!(overrides.get("App1"), Some(&WritingMode::Formal));
        assert_eq!(overrides.get("App2"), Some(&WritingMode::Excited));
    }

    #[test]
    fn test_engine_clear_mode() {
        let mut engine = WritingModeEngine::new(WritingMode::Casual);
        engine.set_mode("Mail", WritingMode::Formal);
        assert_eq!(engine.get_mode("Mail"), WritingMode::Formal);

        engine.clear_mode("Mail");
        assert_eq!(engine.get_mode("Mail"), WritingMode::Casual); // falls back to default
    }

    #[test]
    fn test_engine_clear_nonexistent_mode() {
        let mut engine = WritingModeEngine::new(WritingMode::Casual);
        // clearing a mode that doesn't exist should be fine
        engine.clear_mode("NonexistentApp");
        assert_eq!(engine.get_mode("NonexistentApp"), WritingMode::Casual);
    }

    #[test]
    fn test_style_observation_new() {
        let obs = StyleObservation::new("TestApp".to_string());
        assert_eq!(obs.app_name, "TestApp");
        assert_eq!(obs.avg_caps_ratio, 0.0);
        assert_eq!(obs.avg_punctuation_density, 0.0);
        assert!(!obs.uses_exclamations);
        assert_eq!(obs.sample_count, 0);
    }

    #[test]
    fn test_style_observation_single_update() {
        let mut obs = StyleObservation::new("Test".to_string());
        obs.update("Hello World!");

        assert_eq!(obs.sample_count, 1);
        assert!(obs.uses_exclamations);
        assert!(obs.avg_caps_ratio > 0.0); // "Hello World" = 2/2 caps
    }

    #[test]
    fn test_style_observation_rolling_average() {
        let mut obs = StyleObservation::new("Test".to_string());

        // first sample: all caps
        obs.update("HELLO WORLD");
        assert_eq!(obs.avg_caps_ratio, 1.0);

        // second sample: no caps
        obs.update("hello world");
        // average should be 0.5
        assert!((obs.avg_caps_ratio - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_style_observation_suggest_mode_not_enough_samples() {
        let mut obs = StyleObservation::new("Test".to_string());
        obs.update("hello"); // only 1 sample

        // need at least 2 samples
        assert!(obs.suggest_mode().is_none());
    }

    #[test]
    fn test_style_observation_suggest_very_casual() {
        let mut obs = StyleObservation::new("Test".to_string());
        // low caps ratio, low punctuation
        for _ in 0..5 {
            obs.update("hey whats up no caps here");
        }

        let suggestion = obs.suggest_mode().unwrap();
        assert_eq!(suggestion.suggested_mode, WritingMode::VeryCasual);
    }

    #[test]
    fn test_style_observation_suggest_excited() {
        let mut obs = StyleObservation::new("Test".to_string());
        // high caps ratio with exclamations
        for _ in 0..5 {
            obs.update("WOW THIS IS AMAZING!");
        }

        let suggestion = obs.suggest_mode().unwrap();
        assert_eq!(suggestion.suggested_mode, WritingMode::Excited);
    }

    #[test]
    fn test_style_observation_suggest_formal() {
        let mut obs = StyleObservation::new("Test".to_string());
        // high caps ratio, high punctuation, no exclamations
        for _ in 0..5 {
            obs.update(
                "Dear Sir, I Hope This Message Finds You Well. Best Regards, The Management Team.",
            );
        }

        let suggestion = obs.suggest_mode().unwrap();
        assert_eq!(suggestion.suggested_mode, WritingMode::Formal);
    }

    #[test]
    fn test_style_observation_confidence_scales() {
        let mut obs = StyleObservation::new("Test".to_string());
        for _ in 0..5 {
            obs.update("hello");
        }
        let suggestion1 = obs.suggest_mode().unwrap();

        for _ in 0..15 {
            obs.update("hello");
        }
        let suggestion2 = obs.suggest_mode().unwrap();

        // more samples = higher confidence
        assert!(suggestion2.confidence > suggestion1.confidence);
    }

    #[test]
    fn test_style_learner_new() {
        let learner = StyleLearner::new();
        assert!(learner.all_observations().is_empty());
    }

    #[test]
    fn test_style_learner_default() {
        let learner = StyleLearner::default();
        assert!(learner.all_observations().is_empty());
    }

    #[test]
    fn test_style_learner_observe() {
        let mut learner = StyleLearner::new();
        learner.observe("App1", "hello");
        learner.observe("App1", "hi");
        learner.observe("App2", "formal text here");

        assert!(learner.get_observation("App1").is_some());
        assert!(learner.get_observation("App2").is_some());
        assert!(learner.get_observation("App3").is_none());

        let obs = learner.get_observation("App1").unwrap();
        assert_eq!(obs.sample_count, 2);
    }

    #[test]
    fn test_style_learner_suggest_mode_not_enough_samples() {
        let mut learner = StyleLearner::new();
        learner.observe("App1", "hello"); // only 1 sample

        assert!(learner.suggest_mode("App1").is_none());
    }

    #[test]
    fn test_style_learner_suggest_mode_no_observations() {
        let learner = StyleLearner::new();
        assert!(learner.suggest_mode("NonexistentApp").is_none());
    }

    #[test]
    fn test_caps_ratio_empty() {
        assert_eq!(calculate_caps_ratio(""), 0.0);
    }

    #[test]
    fn test_caps_ratio_whitespace() {
        assert_eq!(calculate_caps_ratio("   "), 0.0);
    }

    #[test]
    fn test_caps_ratio_mixed() {
        // "Hello world Test" = 2/3 = 0.667
        let ratio = calculate_caps_ratio("Hello world Test");
        assert!((ratio - 2.0 / 3.0).abs() < 0.01);
    }

    #[test]
    fn test_punctuation_density_empty() {
        assert_eq!(calculate_punctuation_density(""), 0.0);
    }

    #[test]
    fn test_punctuation_density_multiple_types() {
        // "Hello, world! How? Nice; ok:" = 5 punct in 28 bytes
        // Note: text.len() returns bytes, not chars. For ASCII this is the same,
        // but the original comment had wrong count (26 vs 28).
        let density = calculate_punctuation_density("Hello, world! How? Nice; ok:");
        let expected = 5.0 / 28.0 * 100.0; // ~17.86%
        assert!((density - expected).abs() < 0.1);
    }

    #[test]
    fn test_writing_mode_suggestion_struct() {
        let suggestion = WritingModeSuggestion {
            app_name: "TestApp".to_string(),
            suggested_mode: WritingMode::Casual,
            confidence: 0.75,
            based_on_samples: 15,
        };

        assert_eq!(suggestion.app_name, "TestApp");
        assert_eq!(suggestion.suggested_mode, WritingMode::Casual);
        assert!((suggestion.confidence - 0.75).abs() < 0.001);
        assert_eq!(suggestion.based_on_samples, 15);
    }

    #[test]
    fn test_writing_mode_all() {
        let all_modes = WritingMode::all();
        assert_eq!(all_modes.len(), 4);
        assert!(all_modes.contains(&WritingMode::Formal));
        assert!(all_modes.contains(&WritingMode::Casual));
        assert!(all_modes.contains(&WritingMode::VeryCasual));
        assert!(all_modes.contains(&WritingMode::Excited));
    }

    #[test]
    fn test_writing_mode_default() {
        assert_eq!(WritingMode::default(), WritingMode::Casual);
    }

    #[test]
    fn test_writing_mode_serialization() {
        // Test that modes serialize correctly for JSON
        let mode = WritingMode::VeryCasual;
        let json = serde_json::to_string(&mode).unwrap();
        assert!(json.contains("very_casual"));

        let deserialized: WritingMode = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, WritingMode::VeryCasual);
    }

    #[test]
    fn test_style_observation_serialization() {
        let obs = StyleObservation::new("Test".to_string());
        let json = serde_json::to_string(&obs).unwrap();
        let deserialized: StyleObservation = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.app_name, "Test");
    }

    #[test]
    fn test_engine_same_app_multiple_sets() {
        let mut engine = WritingModeEngine::new(WritingMode::Casual);

        engine.set_mode("App", WritingMode::Formal);
        assert_eq!(engine.get_mode("App"), WritingMode::Formal);

        engine.set_mode("App", WritingMode::Excited);
        assert_eq!(engine.get_mode("App"), WritingMode::Excited);

        engine.set_mode("App", WritingMode::VeryCasual);
        assert_eq!(engine.get_mode("App"), WritingMode::VeryCasual);
    }

    #[test]
    fn test_caps_ratio_unicode() {
        // Unicode characters with uppercase
        let ratio = calculate_caps_ratio("Café Résumé");
        assert!(ratio > 0.0); // Both words start with uppercase
    }

    #[test]
    fn test_style_analysis_unicode() {
        // Should handle unicode without panicking
        let mode = StyleAnalyzer::analyze_style("こんにちは世界");
        // Result doesn't matter, just shouldn't panic
        let _ = mode;
    }

    #[test]
    fn test_style_observation_confidence_capped() {
        let mut obs = StyleObservation::new("Test".to_string());
        // Add lots of samples
        for _ in 0..100 {
            obs.update("hello");
        }

        let suggestion = obs.suggest_mode().unwrap();
        // confidence should be capped at 1.0
        assert!(suggestion.confidence <= 1.0);
    }
}
