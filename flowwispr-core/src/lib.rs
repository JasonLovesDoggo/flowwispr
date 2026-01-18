//! FlowWhispr Core - Voice dictation engine with AI-powered completions
//!
//! A cloud-first dictation engine with provider abstraction for transcription and completions,
//! self-learning typo correction, voice shortcuts, and writing mode customization.

pub mod apps;
pub mod audio;
pub mod error;
pub mod ffi;
pub mod learning;
pub mod metrics;
pub mod modes;
pub mod providers;
pub mod shortcuts;
pub mod storage;
pub mod types;
pub mod whisper_models;

pub use error::{Error, Result};
pub use types::*;

/// Re-export the main engine components for convenience
pub use apps::{AppRegistry, AppTracker};
pub use audio::AudioCapture;
pub use learning::LearningEngine;
pub use metrics::{MetricsCollector, SessionStats, UserStats};
pub use modes::WritingModeEngine;
pub use providers::{CompletionProvider, TranscriptionProvider};
pub use shortcuts::ShortcutsEngine;
pub use storage::Storage;
