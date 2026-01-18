//! Error types for Flow

use thiserror::Error;

/// Result type alias using FlowWhispr's Error type
pub type Result<T> = std::result::Result<T, Error>;

/// All possible errors in FlowWhispr
#[derive(Error, Debug)]
pub enum Error {
    #[error("Audio error: {0}")]
    Audio(String),

    #[error("Transcription failed: {0}")]
    Transcription(String),

    #[error("Completion failed: {0}")]
    Completion(String),

    #[error("Storage error: {0}")]
    Storage(#[from] rusqlite::Error),

    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Invalid configuration: {0}")]
    Config(String),

    #[error("Provider not configured: {0}")]
    ProviderNotConfigured(String),

    #[error("Feature requires subscription tier: {0}")]
    SubscriptionRequired(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
