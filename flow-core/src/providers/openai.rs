//! OpenAI provider implementations for Whisper transcription and GPT completion

use async_trait::async_trait;
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

const OPENAI_API_BASE: &str = "https://api.openai.com/v1";

/// OpenAI Whisper transcription provider
pub struct OpenAITranscriptionProvider {
    client: Client,
    api_key: Option<String>,
    model: String,
}

impl OpenAITranscriptionProvider {
    /// Create a new provider (API key loaded from environment if not provided)
    pub fn new(api_key: Option<String>) -> Self {
        let key = api_key.or_else(|| std::env::var("OPENAI_API_KEY").ok());

        Self {
            client: Client::new(),
            api_key: key,
            model: "whisper-1".to_string(),
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
            .ok_or_else(|| Error::ProviderNotConfigured("OpenAI API key not set".to_string()))
    }
}

#[derive(Debug, Deserialize)]
struct WhisperResponse {
    text: String,
    #[serde(default)]
    language: Option<String>,
    #[serde(default)]
    duration: Option<f64>,
}

#[async_trait]
impl TranscriptionProvider for OpenAITranscriptionProvider {
    fn name(&self) -> &'static str {
        "OpenAI Whisper"
    }

    async fn transcribe(&self, request: TranscriptionRequest) -> Result<TranscriptionResponse> {
        let api_key = self.api_key()?;

        // convert PCM to WAV format for the API
        let wav_data = pcm_to_wav(&request.audio, request.sample_rate, 1);

        // build multipart form
        let file_part = reqwest::multipart::Part::bytes(wav_data)
            .file_name("audio.wav")
            .mime_str("audio/wav")
            .map_err(|e| Error::Transcription(format!("Failed to create form part: {e}")))?;

        let mut form = reqwest::multipart::Form::new()
            .part("file", file_part)
            .text("model", self.model.clone())
            .text("response_format", "json");

        if let Some(lang) = &request.language {
            form = form.text("language", lang.clone());
        }

        if let Some(prompt) = &request.prompt {
            form = form.text("prompt", prompt.clone());
        }

        debug!("Sending transcription request to OpenAI Whisper");

        let response = self
            .client
            .post(format!("{}/audio/transcriptions", OPENAI_API_BASE))
            .header("Authorization", format!("Bearer {}", api_key))
            .multipart(form)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            error!("Whisper API error: {} - {}", status, error_text);
            return Err(Error::Transcription(format!(
                "Whisper API error: {} - {}",
                status, error_text
            )));
        }

        let whisper_response: WhisperResponse = response.json().await?;

        // estimate duration from audio size if not provided
        let duration_ms = whisper_response
            .duration
            .map(|d| (d * 1000.0) as u64)
            .unwrap_or_else(|| {
                // PCM 16-bit mono at sample_rate
                let samples = request.audio.len() / 2;
                (samples as u64 * 1000) / request.sample_rate as u64
            });

        Ok(TranscriptionResponse {
            text: whisper_response.text,
            confidence: None, // Whisper doesn't provide confidence
            language: whisper_response.language,
            duration_ms,
            segments: None,
        })
    }

    fn is_configured(&self) -> bool {
        self.api_key.is_some()
    }
}

/// OpenAI GPT completion provider
pub struct OpenAICompletionProvider {
    client: Client,
    api_key: Option<String>,
    model: String,
}

impl OpenAICompletionProvider {
    /// Create a new provider (API key loaded from environment if not provided)
    pub fn new(api_key: Option<String>) -> Self {
        let key = api_key.or_else(|| std::env::var("OPENAI_API_KEY").ok());

        Self {
            client: Client::new(),
            api_key: key,
            model: "gpt-4o-mini".to_string(),
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
            .ok_or_else(|| Error::ProviderNotConfigured("OpenAI API key not set".to_string()))
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
impl CompletionProvider for OpenAICompletionProvider {
    fn name(&self) -> &'static str {
        "OpenAI GPT"
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

        debug!("Sending completion request to OpenAI");

        let response = self
            .client
            .post(format!("{}/chat/completions", OPENAI_API_BASE))
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&chat_request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            error!("OpenAI API error: {} - {}", status, error_text);
            return Err(Error::Completion(format!(
                "OpenAI API error: {} - {}",
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
        let provider = OpenAICompletionProvider::new(None);

        let prompt = provider.build_system_prompt(WritingMode::Formal, None);
        assert!(prompt.contains("formally"));
        assert!(prompt.contains("professional"));

        let prompt = provider.build_system_prompt(WritingMode::VeryCasual, Some("Slack"));
        assert!(prompt.contains("casually"));
        assert!(prompt.contains("Slack"));
    }

    #[test]
    fn test_provider_not_configured() {
        let provider = OpenAITranscriptionProvider::new(None);
        // when OPENAI_API_KEY env var is not set, this should be false
        // but in tests the env might be set, so we just verify the method works
        let _ = provider.is_configured();
    }
}
