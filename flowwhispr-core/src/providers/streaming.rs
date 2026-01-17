//! Streaming support for completion providers
//!
//! Provides Server-Sent Events (SSE) parsing and streaming completion traits.

use std::pin::Pin;

use async_trait::async_trait;
use futures::Stream;
use serde::Deserialize;

use crate::error::Result;

use super::{CompletionRequest, CompletionResponse, TokenUsage};

/// A chunk of streamed completion text
#[derive(Debug, Clone)]
pub struct CompletionChunk {
    /// The text content of this chunk
    pub text: String,
    /// Whether this is the final chunk
    pub is_final: bool,
    /// Token usage (only available on final chunk)
    pub usage: Option<TokenUsage>,
}

/// Type alias for the boxed stream of completion chunks
pub type CompletionStream = Pin<Box<dyn Stream<Item = Result<CompletionChunk>> + Send>>;

/// Trait for completion providers that support streaming
#[async_trait]
pub trait StreamingCompletionProvider: Send + Sync {
    /// Get the provider name
    fn name(&self) -> &'static str;

    /// Process text with streaming response
    async fn complete_stream(&self, request: CompletionRequest) -> Result<CompletionStream>;

    /// Check if the provider is configured and ready
    fn is_configured(&self) -> bool;
}

/// Parse a Server-Sent Events line
#[allow(dead_code)]
#[derive(Debug)]
pub struct SseEvent {
    pub event: Option<String>,
    pub data: String,
}

/// Parse SSE data from a line
#[allow(dead_code)]
pub fn parse_sse_line(line: &str) -> Option<SseEvent> {
    let line = line.trim();

    if line.is_empty() || line.starts_with(':') {
        return None;
    }

    if let Some(data) = line.strip_prefix("data: ") {
        Some(SseEvent {
            event: None,
            data: data.to_string(),
        })
    } else {
        line.strip_prefix("event: ").map(|event| SseEvent {
            event: Some(event.to_string()),
            data: String::new(),
        })
    }
}

/// OpenAI streaming response chunk
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct OpenAIStreamChunk {
    pub id: String,
    pub object: String,
    pub choices: Vec<OpenAIStreamChoice>,
    #[serde(default)]
    pub usage: Option<OpenAIStreamUsage>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct OpenAIStreamChoice {
    pub delta: OpenAIDelta,
    pub finish_reason: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct OpenAIDelta {
    #[serde(default)]
    pub content: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct OpenAIStreamUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// Anthropic streaming response events
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum AnthropicStreamEvent {
    #[serde(rename = "message_start")]
    MessageStart { message: AnthropicMessageStart },
    #[serde(rename = "content_block_start")]
    ContentBlockStart {
        index: usize,
        content_block: AnthropicContentBlock,
    },
    #[serde(rename = "content_block_delta")]
    ContentBlockDelta { index: usize, delta: AnthropicDelta },
    #[serde(rename = "content_block_stop")]
    ContentBlockStop { index: usize },
    #[serde(rename = "message_delta")]
    MessageDelta {
        delta: AnthropicMessageDeltaContent,
        usage: AnthropicDeltaUsage,
    },
    #[serde(rename = "message_stop")]
    MessageStop,
    #[serde(rename = "ping")]
    Ping,
    #[serde(rename = "error")]
    Error { error: AnthropicError },
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct AnthropicMessageStart {
    pub id: String,
    pub model: String,
    pub usage: AnthropicUsage,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct AnthropicContentBlock {
    #[serde(rename = "type")]
    pub block_type: String,
    #[serde(default)]
    pub text: String,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct AnthropicDelta {
    #[serde(rename = "type")]
    pub delta_type: String,
    #[serde(default)]
    pub text: String,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct AnthropicMessageDeltaContent {
    pub stop_reason: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct AnthropicUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct AnthropicDeltaUsage {
    pub output_tokens: u32,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct AnthropicError {
    #[serde(rename = "type")]
    pub error_type: String,
    pub message: String,
}

/// Collect a stream into a complete response
pub async fn collect_stream(stream: CompletionStream) -> Result<CompletionResponse> {
    use futures::StreamExt;

    let mut text = String::new();
    let mut usage = None;
    let model = None;

    let mut stream = stream;
    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result?;
        text.push_str(&chunk.text);
        if chunk.is_final {
            usage = chunk.usage;
        }
    }

    Ok(CompletionResponse { text, usage, model })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_sse_line() {
        assert!(parse_sse_line("").is_none());
        assert!(parse_sse_line(": comment").is_none());

        let event = parse_sse_line("data: hello").unwrap();
        assert_eq!(event.data, "hello");
        assert!(event.event.is_none());

        let event = parse_sse_line("event: message").unwrap();
        assert_eq!(event.event, Some("message".to_string()));
    }

    #[test]
    fn test_openai_chunk_deserialize() {
        let json = r#"{
            "id": "chatcmpl-123",
            "object": "chat.completion.chunk",
            "choices": [{
                "delta": {"content": "Hello"},
                "finish_reason": null
            }]
        }"#;

        let chunk: OpenAIStreamChunk = serde_json::from_str(json).unwrap();
        assert_eq!(chunk.choices[0].delta.content, Some("Hello".to_string()));
    }

    #[test]
    fn test_anthropic_event_deserialize() {
        let json = r#"{
            "type": "content_block_delta",
            "index": 0,
            "delta": {"type": "text_delta", "text": "Hello"}
        }"#;

        let event: AnthropicStreamEvent = serde_json::from_str(json).unwrap();
        match event {
            AnthropicStreamEvent::ContentBlockDelta { delta, .. } => {
                assert_eq!(delta.text, "Hello");
            }
            _ => panic!("Wrong event type"),
        }
    }
}
