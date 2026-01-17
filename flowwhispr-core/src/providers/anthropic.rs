//! Anthropic Claude completion provider

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{debug, error};

use crate::error::{Error, Result};
use crate::types::WritingMode;

use super::completion::TokenUsage;
use super::{CompletionProvider, CompletionRequest, CompletionResponse};

const ANTHROPIC_API_BASE: &str = "https://api.anthropic.com/v1";
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// Anthropic Claude completion provider
pub struct AnthropicCompletionProvider {
    client: Client,
    api_key: Option<String>,
    model: String,
}

impl AnthropicCompletionProvider {
    /// Create a new provider (API key loaded from environment if not provided)
    pub fn new(api_key: Option<String>) -> Self {
        let key = api_key.or_else(|| std::env::var("ANTHROPIC_API_KEY").ok());

        Self {
            client: Client::new(),
            api_key: key,
            model: "claude-sonnet-4-20250514".to_string(),
        }
    }

    /// Set the model to use
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    fn api_key(&self) -> Result<&str> {
        self.api_key
            .as_deref()
            .ok_or_else(|| Error::ProviderNotConfigured("Anthropic API key not set".to_string()))
    }

    fn build_system_prompt(&self, mode: WritingMode, app_context: Option<&str>) -> String {
        let mut prompt = String::from(
            "You are a dictation assistant. Your job is to take raw transcribed speech \
             and format it appropriately. Preserve the user's intended meaning while \
             applying the requested formatting style. Output ONLY the formatted text, \
             nothing else - no explanations, no quotes, no metadata.\n\n",
        );

        prompt.push_str("Formatting style: ");
        prompt.push_str(mode.prompt_modifier());

        if let Some(context) = app_context {
            prompt.push_str("\n\nContext: The user is typing in ");
            prompt.push_str(context);
            prompt.push_str(". Adjust formatting appropriately for this context.");
        }

        prompt
    }
}

#[derive(Debug, Serialize)]
struct MessageRequest {
    model: String,
    max_tokens: u32,
    system: String,
    messages: Vec<Message>,
}

#[derive(Debug, Serialize)]
struct Message {
    role: String,
    content: String,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct MessageResponse {
    content: Vec<ContentBlock>,
    model: String,
    usage: Usage,
    #[serde(default)]
    stop_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ContentBlock {
    #[serde(rename = "type")]
    content_type: String,
    text: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Usage {
    input_tokens: u32,
    output_tokens: u32,
}

#[async_trait]
impl CompletionProvider for AnthropicCompletionProvider {
    fn name(&self) -> &'static str {
        "Anthropic Claude"
    }

    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        let api_key = self.api_key()?;

        let system_prompt = request.system_prompt.unwrap_or_else(|| {
            self.build_system_prompt(request.mode, request.app_context.as_deref())
        });

        let message_request = MessageRequest {
            model: self.model.clone(),
            max_tokens: request.max_tokens.unwrap_or(1024),
            system: system_prompt,
            messages: vec![Message {
                role: "user".to_string(),
                content: request.text,
            }],
        };

        debug!("Sending completion request to Anthropic Claude");

        let response = self
            .client
            .post(format!("{}/messages", ANTHROPIC_API_BASE))
            .header("x-api-key", api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("content-type", "application/json")
            .json(&message_request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            error!("Anthropic API error: {} - {}", status, error_text);
            return Err(Error::Completion(format!(
                "Anthropic API error: {} - {}",
                status, error_text
            )));
        }

        let message_response: MessageResponse = response.json().await?;

        // extract text from content blocks
        let text = message_response
            .content
            .into_iter()
            .filter_map(|block| {
                if block.content_type == "text" {
                    block.text
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("");

        if text.is_empty() {
            return Err(Error::Completion("No text content in response".to_string()));
        }

        Ok(CompletionResponse {
            text,
            usage: Some(TokenUsage {
                prompt_tokens: message_response.usage.input_tokens,
                completion_tokens: message_response.usage.output_tokens,
                total_tokens: message_response.usage.input_tokens
                    + message_response.usage.output_tokens,
            }),
            model: Some(message_response.model),
        })
    }

    fn is_configured(&self) -> bool {
        self.api_key.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_prompt_building() {
        let provider = AnthropicCompletionProvider::new(None);

        let prompt = provider.build_system_prompt(WritingMode::Formal, None);
        assert!(prompt.contains("formally"));
        assert!(prompt.contains("professional"));

        let prompt = provider.build_system_prompt(WritingMode::VeryCasual, Some("Slack"));
        assert!(prompt.contains("casually"));
        assert!(prompt.contains("Slack"));
    }

    #[test]
    fn test_provider_creation() {
        let provider = AnthropicCompletionProvider::new(Some("test-key".to_string()));
        assert!(provider.is_configured());
        assert_eq!(provider.name(), "Anthropic Claude");
    }

    #[test]
    fn test_custom_model() {
        let provider = AnthropicCompletionProvider::new(None).with_model("claude-3-opus-20240229");
        assert_eq!(provider.model, "claude-3-opus-20240229");
    }
}
