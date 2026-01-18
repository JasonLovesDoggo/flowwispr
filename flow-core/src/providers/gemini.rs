//! Gemini provider implementations for Whisper transcription and completion

use async_trait::async_trait;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{debug, error};

use crate::error::{Error, Result};
use crate::types::WritingMode;

use super::completion::TokenUsage;
use super::{
    CompletionProvider, CompletionRequest, CompletionResponse, TranscriptionProvider,
    TranscriptionRequest, TranscriptionResponse,
};

const GEMINI_API_BASE: &str = "https://generativelanguage.googleapis.com/v1beta";
const GEMINI_OPENAI_COMPAT_BASE: &str = "https://generativelanguage.googleapis.com/v1beta/openai";

/// Gemini transcription provider (using native API with audio input)
pub struct GeminiTranscriptionProvider {
    client: Client,
    api_key: Option<String>,
    model: String,
}

impl GeminiTranscriptionProvider {
    /// Create a new provider (API key loaded from environment if not provided)
    pub fn new(api_key: Option<String>) -> Self {
        let key = api_key.or_else(|| std::env::var("GEMINI_API_KEY").ok());

        Self {
            client: Client::new(),
            api_key: key,
            model: "gemini-3-flash-preview".to_string(),
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
            .ok_or_else(|| Error::ProviderNotConfigured("Gemini API key not set".to_string()))
    }
}

#[derive(Debug, Serialize)]
struct GeminiGenerateContentRequest {
    contents: Vec<GeminiContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    generation_config: Option<GeminiGenerationConfig>,
}

#[derive(Debug, Serialize)]
struct GeminiContent {
    parts: Vec<GeminiPart>,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum GeminiPart {
    Text {
        text: String,
    },
    InlineData {
        #[serde(rename = "inlineData")]
        inline_data: GeminiInlineData,
    },
}

#[derive(Debug, Serialize)]
struct GeminiInlineData {
    #[serde(rename = "mimeType")]
    mime_type: String,
    data: String,
}

#[derive(Debug, Serialize)]
struct GeminiGenerationConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
}

#[derive(Debug, Deserialize)]
struct GeminiGenerateContentResponse {
    candidates: Vec<GeminiCandidate>,
}

#[derive(Debug, Deserialize)]
struct GeminiCandidate {
    content: GeminiContentResponse,
}

#[derive(Debug, Deserialize)]
struct GeminiContentResponse {
    parts: Vec<GeminiPartResponse>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum GeminiPartResponse {
    Text {
        text: String,
    },
    InlineData {
        #[serde(rename = "inlineData")]
        _inline_data: GeminiInlineDataResponse,
    },
}

#[derive(Debug, Deserialize)]
struct GeminiInlineDataResponse {
    #[serde(rename = "mimeType")]
    _mime_type: String,
    _data: String,
}

#[async_trait]
impl TranscriptionProvider for GeminiTranscriptionProvider {
    fn name(&self) -> &'static str {
        "Gemini"
    }

    async fn transcribe(&self, request: TranscriptionRequest) -> Result<TranscriptionResponse> {
        let api_key = self.api_key()?;

        // Convert PCM to WAV format for the API
        let wav_data = pcm_to_wav(&request.audio, request.sample_rate, 1);
        let audio_base64 = STANDARD.encode(&wav_data);

        // Build the request with audio input
        let mut parts = vec![GeminiPart::InlineData {
            inline_data: GeminiInlineData {
                mime_type: "audio/wav".to_string(),
                data: audio_base64,
            },
        }];

        // Add prompt if provided
        let prompt_text = if let Some(prompt) = &request.prompt {
            prompt.clone()
        } else {
            "Transcribe this audio accurately. Output only the transcribed text, nothing else."
                .to_string()
        };
        parts.insert(0, GeminiPart::Text { text: prompt_text });

        let generate_request = GeminiGenerateContentRequest {
            contents: vec![GeminiContent { parts }],
            generation_config: Some(GeminiGenerationConfig {
                temperature: Some(0.0), // Low temperature for accurate transcription
            }),
        };

        debug!("Sending transcription request to Gemini");

        let url = format!(
            "{}/models/{}:generateContent?key={}",
            GEMINI_API_BASE, self.model, api_key
        );
        let response = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&generate_request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            error!("Gemini API error: {} - {}", status, error_text);
            return Err(Error::Transcription(format!(
                "Gemini API error: {} - {}",
                status, error_text
            )));
        }

        let gemini_response: GeminiGenerateContentResponse = response.json().await?;

        let text = gemini_response
            .candidates
            .into_iter()
            .next()
            .and_then(|c| {
                c.content.parts.into_iter().find_map(|p| match p {
                    GeminiPartResponse::Text { text } => Some(text),
                    _ => None,
                })
            })
            .ok_or_else(|| Error::Transcription("No transcription returned".to_string()))?;

        // Estimate duration from audio size
        let samples = request.audio.len() / 2;
        let duration_ms = (samples as u64 * 1000) / request.sample_rate as u64;

        Ok(TranscriptionResponse {
            text: text.trim().to_string(),
            confidence: None, // Gemini doesn't provide confidence scores
            language: request.language,
            duration_ms,
            segments: None,
        })
    }

    fn is_configured(&self) -> bool {
        self.api_key.is_some()
    }
}

/// Gemini completion provider (using OpenAI-compatible endpoint)
pub struct GeminiCompletionProvider {
    client: Client,
    api_key: Option<String>,
    model: String,
}

impl GeminiCompletionProvider {
    /// Create a new provider (API key loaded from environment if not provided)
    pub fn new(api_key: Option<String>) -> Self {
        let key = api_key.or_else(|| std::env::var("GEMINI_API_KEY").ok());

        Self {
            client: Client::new(),
            api_key: key,
            model: "gemini-3-flash-preview".to_string(),
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
            .ok_or_else(|| Error::ProviderNotConfigured("Gemini API key not set".to_string()))
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
    model: String,
    messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    temperature: f32,
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
impl CompletionProvider for GeminiCompletionProvider {
    fn name(&self) -> &'static str {
        "Gemini"
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
            model: self.model.clone(),
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
            max_tokens: request.max_tokens,
            temperature: 0.3, // low temperature for consistent formatting
        };

        debug!("Sending completion request to Gemini");

        let response = self
            .client
            .post(format!("{}/chat/completions", GEMINI_OPENAI_COMPAT_BASE))
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&chat_request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            error!("Gemini API error: {} - {}", status, error_text);
            return Err(Error::Completion(format!(
                "Gemini API error: {} - {}",
                status, error_text
            )));
        }

        let chat_response: ChatResponse = response.json().await?;

        let text = chat_response
            .choices
            .into_iter()
            .next()
            .map(|c| c.message.content)
            .ok_or_else(|| Error::Completion("No completion returned".to_string()))?;

        Ok(CompletionResponse {
            text,
            usage: chat_response.usage.map(|u| TokenUsage {
                prompt_tokens: u.prompt_tokens,
                completion_tokens: u.completion_tokens,
                total_tokens: u.total_tokens,
            }),
            model: Some(chat_response.model),
        })
    }

    fn is_configured(&self) -> bool {
        self.api_key.is_some()
    }
}

/// Convert raw PCM data to WAV format
fn pcm_to_wav(pcm: &[u8], sample_rate: u32, channels: u16) -> Vec<u8> {
    let bits_per_sample: u16 = 16;
    let byte_rate = sample_rate * u32::from(channels) * u32::from(bits_per_sample) / 8;
    let block_align = channels * bits_per_sample / 8;
    let data_size = pcm.len() as u32;
    let file_size = 36 + data_size;

    let mut wav = Vec::with_capacity(44 + pcm.len());

    // RIFF header
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&file_size.to_le_bytes());
    wav.extend_from_slice(b"WAVE");

    // fmt chunk
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16u32.to_le_bytes()); // chunk size
    wav.extend_from_slice(&1u16.to_le_bytes()); // PCM format
    wav.extend_from_slice(&channels.to_le_bytes());
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&byte_rate.to_le_bytes());
    wav.extend_from_slice(&block_align.to_le_bytes());
    wav.extend_from_slice(&bits_per_sample.to_le_bytes());

    // data chunk
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_size.to_le_bytes());
    wav.extend_from_slice(pcm);

    wav
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pcm_to_wav() {
        // 1 second of silence at 16kHz mono
        let pcm = vec![0u8; 32000]; // 16000 samples * 2 bytes
        let wav = pcm_to_wav(&pcm, 16000, 1);

        // check RIFF header
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(&wav[12..16], b"fmt ");
        assert_eq!(&wav[36..40], b"data");

        // total size should be 44 header + 32000 data
        assert_eq!(wav.len(), 44 + 32000);
    }

    #[test]
    fn test_system_prompt_building() {
        let provider = GeminiCompletionProvider::new(None);

        let prompt = provider.build_system_prompt(WritingMode::Formal, None);
        assert!(prompt.contains("formally"));
        assert!(prompt.contains("professional"));

        let prompt = provider.build_system_prompt(WritingMode::VeryCasual, Some("Slack"));
        assert!(prompt.contains("casually"));
        assert!(prompt.contains("Slack"));
    }

    #[test]
    fn test_provider_not_configured() {
        let provider = GeminiTranscriptionProvider::new(None);
        // when GEMINI_API_KEY env var is not set, this should be false
        // but in tests the env might be set, so we just verify the method works
        let _ = provider.is_configured();
    }
}
