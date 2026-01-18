//! Completion provider trait and types

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::modes::WritingMode;

/// Request for text completion/formatting
#[derive(Debug, Clone)]
pub struct CompletionRequest {
    /// Raw transcribed text to process
    pub text: String,
    /// Writing mode to apply
    pub mode: WritingMode,
    /// Optional system prompt override
    pub system_prompt: Option<String>,
    /// Context about the target application
    pub app_context: Option<String>,
    /// Max tokens to generate
    pub max_tokens: Option<u32>,
    /// Instruction to preserve shortcut text word-for-word
    pub shortcut_preservation: Option<String>,
}

impl CompletionRequest {
    pub fn new(text: String, mode: WritingMode) -> Self {
        Self {
            text,
            mode,
            system_prompt: None,
            app_context: None,
            max_tokens: None,
            shortcut_preservation: None,
        }
    }

    pub fn with_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }

    pub fn with_app_context(mut self, context: impl Into<String>) -> Self {
        self.app_context = Some(context.into());
        self
    }

    pub fn with_max_tokens(mut self, max: u32) -> Self {
        self.max_tokens = Some(max);
        self
    }

    pub fn with_shortcut_preservation(mut self, instruction: impl Into<String>) -> Self {
        self.shortcut_preservation = Some(instruction.into());
        self
    }
}

/// Response from completion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionResponse {
    /// Processed/formatted text
    pub text: String,
    /// Token usage information
    pub usage: Option<TokenUsage>,
    /// Model used for completion
    pub model: Option<String>,
}

/// Token usage statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// Trait for completion/formatting providers
#[async_trait]
pub trait CompletionProvider: Send + Sync {
    /// Get the provider name
    fn name(&self) -> &'static str;

    /// Process text with the given mode
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse>;

    /// Check if the provider is configured and ready
    fn is_configured(&self) -> bool;
}
