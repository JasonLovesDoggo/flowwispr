//! Transcription provider trait and types

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::AudioData;
use crate::error::Result;

/// Request for transcription
#[derive(Debug, Clone)]
pub struct TranscriptionRequest {
    /// Raw audio data (16-bit PCM)
    pub audio: AudioData,
    /// Sample rate of the audio
    pub sample_rate: u32,
    /// Optional language hint (ISO 639-1 code, e.g., "en")
    pub language: Option<String>,
    /// Optional prompt to guide transcription
    pub prompt: Option<String>,
}

impl TranscriptionRequest {
    pub fn new(audio: AudioData, sample_rate: u32) -> Self {
        Self {
            audio,
            sample_rate,
            language: None,
            prompt: None,
        }
    }

    pub fn with_language(mut self, language: impl Into<String>) -> Self {
        self.language = Some(language.into());
        self
    }

    pub fn with_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.prompt = Some(prompt.into());
        self
    }
}

/// Response from transcription
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptionResponse {
    /// Transcribed text
    pub text: String,
    /// Confidence score (0.0 - 1.0) if available
    pub confidence: Option<f32>,
    /// Detected language if available
    pub language: Option<String>,
    /// Duration of audio in milliseconds
    pub duration_ms: u64,
    /// Individual word segments if available
    pub segments: Option<Vec<TranscriptionSegment>>,
}

/// A segment of transcribed text with timing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptionSegment {
    pub text: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub confidence: Option<f32>,
}

/// Trait for transcription providers
#[async_trait]
pub trait TranscriptionProvider: Send + Sync {
    /// Get the provider name
    fn name(&self) -> &'static str;

    /// Transcribe audio to text
    async fn transcribe(&self, request: TranscriptionRequest) -> Result<TranscriptionResponse>;

    /// Check if the provider is configured and ready
    fn is_configured(&self) -> bool;
}
