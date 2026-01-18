//! OpenRouter provider implementation for LLM completion

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{debug, error};

use crate::error::{Error, Result};
use crate::types::WritingMode;

use super::completion::TokenUsage;
use super::{CompletionProvider, CompletionRequest, CompletionResponse};

const OPENROUTER_API_BASE: &str = "https://openrouter.ai/api/v1";

/// OpenRouter completion provider
pub struct OpenRouterCompletionProvider {
    client: Client,
    api_key: Option<String>,
    models: Vec<String>,
}

impl OpenRouterCompletionProvider {
    /// Create a new provider (API key loaded from environment if not provided)
    pub fn new(api_key: Option<String>) -> Self {
        let key = api_key.or_else(|| std::env::var("OPENROUTER_API_KEY").ok());

        Self {
            client: Client::new(),
            api_key: key,
            models: vec![
                "meta-llama/llama-4-maverick:nitro".to_string(),
                "openai/gpt-oss-120b:nitro".to_string(),
            ],
        }
    }

    /// Set the models to use (with fallbacks)
    pub fn with_models(mut self, models: Vec<String>) -> Self {
        self.models = models;
        self
    }

    /// Set a single model to use
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.models = vec![model.into()];
        self
    }

    fn api_key(&self) -> Result<&str> {
        self.api_key
            .as_deref()
            .ok_or_else(|| Error::ProviderNotConfigured("OpenRouter API key not set".to_string()))
    }

    fn build_system_prompt(&self, mode: WritingMode, app_context: Option<&str>) -> String {
        let mut prompt = String::from(
            "You are a dictation assistant. Your job is to take raw transcribed speech \
             and format it appropriately. Preserve the user's intended meaning while \
             applying the requested formatting style. Output ONLY the formatted text, \
             nothing else.\n\n",
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
struct ChatRequest {
    models: Vec<String>,
    messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    temperature: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    provider: Option<ProviderConfig>,
}

#[derive(Debug, Serialize)]
struct ProviderConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    allow_fallbacks: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sort: Option<SortConfig>,
}

#[derive(Debug, Serialize)]
struct SortConfig {
    by: String,
    partition: String,
}

#[derive(Debug, Serialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
    usage: Option<ChatUsage>,
    model: String,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatMessageResponse,
}

#[derive(Debug, Deserialize)]
struct ChatMessageResponse {
    content: String,
}

#[derive(Debug, Deserialize)]
struct ChatUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

#[async_trait]
impl CompletionProvider for OpenRouterCompletionProvider {
    fn name(&self) -> &'static str {
        "OpenRouter"
    }

    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        let api_key = self.api_key()?;

        let mut system_prompt = request.system_prompt.unwrap_or_else(|| {
            self.build_system_prompt(request.mode, request.app_context.as_deref())
        });

        // Add shortcut preservation instruction if present
        if let Some(preservation) = request.shortcut_preservation {
            system_prompt.push_str(&preservation);
        }

        let chat_request = ChatRequest {
            models: self.models.clone(),
            messages: vec![
                ChatMessage {
                    role: "system".to_string(),
                    content: system_prompt,
                },
                ChatMessage {
                    role: "user".to_string(),
                    content: request.text,
                },
            ],
            max_tokens: Some(1000),
            temperature: 0.3,
            provider: Some(ProviderConfig {
                allow_fallbacks: Some(true),
                sort: Some(SortConfig {
                    by: "throughput".to_string(),
                    partition: "none".to_string(),
                }),
            }),
        };

        debug!(
            "Sending completion request to OpenRouter with models: {:?}",
            self.models
        );

        let response = self
            .client
            .post(format!("{}/chat/completions", OPENROUTER_API_BASE))
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&chat_request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| String::from("Unknown error"));
            error!("OpenRouter API error ({}): {}", status, error_text);
            return Err(Error::Completion(format!(
                "OpenRouter API error ({}): {}",
                status, error_text
            )));
        }

        let chat_response: ChatResponse = response.json().await?;

        let text = chat_response
            .choices
            .first()
            .map(|choice| choice.message.content.clone())
            .ok_or_else(|| Error::Completion("No completion returned".to_string()))?;

        let usage = chat_response.usage.map(|u| TokenUsage {
            prompt_tokens: u.prompt_tokens,
            completion_tokens: u.completion_tokens,
            total_tokens: u.total_tokens,
        });

        debug!("Received completion from OpenRouter");

        Ok(CompletionResponse {
            text,
            usage,
            model: Some(chat_response.model),
        })
    }

    fn is_configured(&self) -> bool {
        self.api_key.is_some()
    }
}
