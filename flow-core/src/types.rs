//! Core types used throughout FlowWhispr

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Unique identifier for transcriptions
pub type TranscriptionId = Uuid;

/// Unique identifier for shortcuts
pub type ShortcutId = Uuid;

/// Unique identifier for corrections
pub type CorrectionId = Uuid;

/// Unique identifier for events
pub type EventId = Uuid;

/// Unique identifier for contacts
pub type ContactId = Uuid;

/// Audio data as raw bytes (16-bit PCM)
pub type AudioData = Vec<u8>;

/// Writing mode that affects transcription style
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WritingMode {
    /// Full capitalization, punctuation, formal language
    Formal,
    /// Sentence case, punctuation, conversational tone
    #[default]
    Casual,
    /// Lowercase, minimal punctuation, very informal
    VeryCasual,
    /// Full caps for emphasis, exclamation marks, energetic tone
    Excited,
}

impl WritingMode {
    /// Get the system prompt modifier for this mode
    pub fn prompt_modifier(&self) -> &'static str {
        match self {
            Self::Formal => {
                "Reformat in formal, professional tone. Replace casual phrases like \"gonna\", \"wanna\", \"5 min\" with proper equivalents like \"going to\", \"want to\", \"five minutes\". Use complete sentences, proper grammar, and polished language. Transform slang into professional alternatives. Output EXACTLY as it would be typed—nothing more, nothing else."
            }
            Self::Casual => {
                "Reformat in friendly, conversational tone. Keep contractions, use natural language, but ensure it's clear and warm. Preserve the intended meaning exactly. Output EXACTLY as it would be typed—do NOT add commentary, responses, or anything beyond the reformatted text."
            }
            Self::VeryCasual => {
                "Reformat in casual texting style. Use lowercase, abbreviations like \"gonna\", \"rn\", \"sry\". Keep it brief and informal like a text to a close friend. Output EXACTLY as it would be typed—nothing else."
            }
            Self::Excited => {
                "Reformat with enthusiasm and warmth. Add exclamation marks where appropriate, express affection. Make it sound excited while preserving the intended meaning. Output EXACTLY as it would be typed—nothing more."
            }
        }
    }

    /// Get all available modes
    pub fn all() -> &'static [WritingMode] {
        &[
            WritingMode::Formal,
            WritingMode::Casual,
            WritingMode::VeryCasual,
            WritingMode::Excited,
        ]
    }

    /// Suggest default mode for an app category
    pub fn suggested_for_category(category: AppCategory) -> Self {
        match category {
            AppCategory::Email => WritingMode::Formal,
            AppCategory::Code => WritingMode::Formal,
            AppCategory::Documents => WritingMode::Formal,
            AppCategory::Slack => WritingMode::Casual,
            AppCategory::Social => WritingMode::VeryCasual,
            AppCategory::Browser => WritingMode::Casual,
            AppCategory::Terminal => WritingMode::VeryCasual,
            AppCategory::Unknown => WritingMode::Casual,
        }
    }
}

/// A single transcription result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transcription {
    pub id: TranscriptionId,
    pub raw_text: String,
    pub processed_text: String,
    pub confidence: f32,
    pub duration_ms: u64,
    pub app_context: Option<AppContext>,
    pub created_at: DateTime<Utc>,
}

/// Status for transcription history entries
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TranscriptionStatus {
    Success,
    Failed,
}

/// A transcription history entry (success or failure)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptionHistoryEntry {
    pub id: TranscriptionId,
    pub status: TranscriptionStatus,
    pub text: String,
    pub raw_text: String,
    pub error: Option<String>,
    pub duration_ms: u64,
    pub app_context: Option<AppContext>,
    pub created_at: DateTime<Utc>,
}

impl TranscriptionHistoryEntry {
    pub fn success(raw_text: String, text: String, duration_ms: u64) -> Self {
        Self {
            id: Uuid::new_v4(),
            status: TranscriptionStatus::Success,
            text,
            raw_text,
            error: None,
            duration_ms,
            app_context: None,
            created_at: Utc::now(),
        }
    }

    pub fn failure(error: String, duration_ms: u64) -> Self {
        Self {
            id: Uuid::new_v4(),
            status: TranscriptionStatus::Failed,
            text: String::new(),
            raw_text: String::new(),
            error: Some(error),
            duration_ms,
            app_context: None,
            created_at: Utc::now(),
        }
    }
}

impl Transcription {
    pub fn new(
        raw_text: String,
        processed_text: String,
        confidence: f32,
        duration_ms: u64,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            raw_text,
            processed_text,
            confidence,
            duration_ms,
            app_context: None,
            created_at: Utc::now(),
        }
    }
}

/// Context about the active application during transcription
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppContext {
    pub app_name: String,
    pub bundle_id: Option<String>,
    pub window_title: Option<String>,
    pub category: AppCategory,
}

/// Categories of applications for mode suggestions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppCategory {
    Email,
    Slack,
    Code,
    Documents,
    Social,
    Browser,
    Terminal,
    Unknown,
}

impl AppCategory {
    /// Infer category from app name or bundle ID
    pub fn from_app(app_name: &str, bundle_id: Option<&str>) -> Self {
        let name_lower = app_name.to_lowercase();
        let bundle_lower = bundle_id.map(|b| b.to_lowercase()).unwrap_or_default();

        if name_lower.contains("mail") || bundle_lower.contains("mail") {
            AppCategory::Email
        } else if name_lower.contains("slack")
            || name_lower.contains("discord")
            || name_lower.contains("teams")
        {
            AppCategory::Slack
        } else if name_lower.contains("code")
            || name_lower.contains("xcode")
            || name_lower.contains("intellij")
            || name_lower.contains("vim")
            || name_lower.contains("nvim")
            || name_lower.contains("cursor")
        {
            AppCategory::Code
        } else if name_lower.contains("pages")
            || name_lower.contains("word")
            || name_lower.contains("docs")
            || name_lower.contains("notion")
        {
            AppCategory::Documents
        } else if name_lower.contains("twitter")
            || name_lower.contains("facebook")
            || name_lower.contains("instagram")
        {
            AppCategory::Social
        } else if name_lower.contains("safari")
            || name_lower.contains("chrome")
            || name_lower.contains("firefox")
            || name_lower.contains("arc")
        {
            AppCategory::Browser
        } else if name_lower.contains("terminal")
            || name_lower.contains("iterm")
            || name_lower.contains("warp")
            || name_lower.contains("kitty")
            || name_lower.contains("alacritty")
        {
            AppCategory::Terminal
        } else {
            AppCategory::Unknown
        }
    }
}

/// User subscription tier for feature gating
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubscriptionTier {
    #[default]
    Free,
    Pro,
    Team,
}

impl SubscriptionTier {
    /// Check if a feature is available at this tier
    pub fn has_feature(&self, feature: Feature) -> bool {
        match feature {
            Feature::BasicTranscription => true,
            Feature::Shortcuts => true,
            Feature::WritingModes => true,
            Feature::TypoLearning => matches!(self, Self::Pro | Self::Team),
            Feature::AppCustomization => matches!(self, Self::Pro | Self::Team),
            Feature::Analytics => matches!(self, Self::Pro | Self::Team),
            Feature::TeamSharing => matches!(self, Self::Team),
            Feature::PrioritySupport => matches!(self, Self::Team),
        }
    }

    /// Get the monthly transcription limit in minutes
    pub fn transcription_limit_minutes(&self) -> Option<u32> {
        match self {
            Self::Free => Some(60),
            Self::Pro => Some(600),
            Self::Team => None, // unlimited
        }
    }
}

/// Features that can be gated by subscription
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Feature {
    BasicTranscription,
    Shortcuts,
    WritingModes,
    TypoLearning,
    AppCustomization,
    Analytics,
    TeamSharing,
    PrioritySupport,
}

/// A voice shortcut that expands trigger phrases into replacement text
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Shortcut {
    pub id: ShortcutId,
    pub trigger: String,
    pub replacement: String,
    pub case_sensitive: bool,
    pub enabled: bool,
    pub use_count: u32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Shortcut {
    pub fn new(trigger: String, replacement: String) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            trigger,
            replacement,
            case_sensitive: false,
            enabled: true,
            use_count: 0,
            created_at: now,
            updated_at: now,
        }
    }
}

/// A learned correction from user edits
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Correction {
    pub id: CorrectionId,
    pub original: String,
    pub corrected: String,
    pub occurrences: u32,
    pub confidence: f32,
    pub source: CorrectionSource,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Correction {
    pub fn new(original: String, corrected: String, source: CorrectionSource) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            original,
            corrected,
            occurrences: 1,
            confidence: 0.5, // starts at 50%
            source,
            created_at: now,
            updated_at: now,
        }
    }

    /// Update confidence using logarithmic scaling
    /// Formula: confidence = 0.5 + 0.5 * (1 - 1/ln(occurrences + e))
    pub fn update_confidence(&mut self) {
        let e = std::f32::consts::E;
        self.confidence = 0.5 + 0.5 * (1.0 - 1.0 / (self.occurrences as f32 + e).ln());
        self.confidence = self.confidence.min(0.99); // cap at 99%
    }
}

/// Source of a learned correction
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CorrectionSource {
    /// User manually corrected the text after transcription
    UserEdit,
    /// Detected from clipboard comparison
    ClipboardDiff,
    /// Imported from external source
    Imported,
}

/// An analytics event for tracking user behavior
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyticsEvent {
    pub id: EventId,
    pub event_type: EventType,
    pub properties: serde_json::Value,
    pub app_context: Option<AppContext>,
    pub created_at: DateTime<Utc>,
}

impl AnalyticsEvent {
    pub fn new(event_type: EventType, properties: serde_json::Value) -> Self {
        Self {
            id: Uuid::new_v4(),
            event_type,
            properties,
            app_context: None,
            created_at: Utc::now(),
        }
    }
}

/// Types of analytics events we track
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    TranscriptionStarted,
    TranscriptionCompleted,
    TranscriptionFailed,
    ShortcutTriggered,
    CorrectionApplied,
    ModeChanged,
    AppSwitched,
    SettingsUpdated,
}

/// Contact social relationship category for context-aware writing
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContactCategory {
    /// Work colleague, professional contact
    Professional,
    /// Parent, child, sibling, close family member
    CloseFamily,
    /// Friend, peer, casual acquaintance
    CasualPeer,
    /// Romantic partner, spouse
    Partner,
    /// Default for unknown or neutral contacts
    FormalNeutral,
}

impl ContactCategory {
    /// Map contact category to suggested writing mode
    pub fn suggested_writing_mode(&self) -> WritingMode {
        match self {
            Self::Professional => WritingMode::Formal,
            Self::CloseFamily => WritingMode::Casual,
            Self::CasualPeer => WritingMode::VeryCasual,
            Self::Partner => WritingMode::Excited,
            Self::FormalNeutral => WritingMode::Formal,
        }
    }

    /// Get all available categories
    pub fn all() -> &'static [ContactCategory] {
        &[
            ContactCategory::Professional,
            ContactCategory::CloseFamily,
            ContactCategory::CasualPeer,
            ContactCategory::Partner,
            ContactCategory::FormalNeutral,
        ]
    }
}

/// A contact entry with metadata and categorization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contact {
    pub id: ContactId,
    pub name: String,
    pub organization: Option<String>,
    pub category: ContactCategory,
    pub frequency: u32,
    pub last_contacted: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Contact {
    pub fn new(name: String, organization: Option<String>, category: ContactCategory) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            name,
            organization,
            category,
            frequency: 0,
            last_contacted: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// Update contact usage stats
    pub fn record_interaction(&mut self) {
        self.frequency += 1;
        self.last_contacted = Some(Utc::now());
        self.updated_at = Utc::now();
    }
}
