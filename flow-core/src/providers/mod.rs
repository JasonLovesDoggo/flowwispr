//! Provider abstraction layer for transcription and completion services
//!
//! Supports pluggable providers for cloud (OpenAI, ElevenLabs, Anthropic, Gemini) and local services.
mod auto;
mod completion;
mod gemini;
mod local_whisper;
mod openai;
mod openrouter;
mod streaming;
mod transcription;

pub use auto::{
    AutoTranscriptionProvider, CorrectionPair, CorrectionValidation, validate_corrections,
};
pub use completion::{CompletionProvider, CompletionRequest, CompletionResponse, TokenUsage};
pub use gemini::{GeminiCompletionProvider, GeminiTranscriptionProvider};
pub use local_whisper::{LocalWhisperTranscriptionProvider, WhisperModel};
pub use openai::{OpenAICompletionProvider, OpenAITranscriptionProvider};
pub use openrouter::OpenRouterCompletionProvider;
pub use streaming::{
    CompletionChunk, CompletionStream, StreamingCompletionProvider, collect_stream,
};
pub use transcription::{
    CompletionParams as TranscriptionCompletionParams, TranscriptionProvider, TranscriptionRequest,
    TranscriptionResponse,
};
