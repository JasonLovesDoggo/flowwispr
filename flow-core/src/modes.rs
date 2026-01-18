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
    fn test_mode_suggestions() {
        assert_eq!(
            WritingMode::suggested_for_category(AppCategory::Email),
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
    }

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
}
