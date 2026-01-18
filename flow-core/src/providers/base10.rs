//! Base10 provider for Whisper transcription + OpenRouter completion
//!
//! Combined transcription and completion in a single worker request.
//! API keys handled by Cloudflare Worker secrets.

use async_trait::async_trait;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{debug, error};

use crate::error::{Error, Result};

use super::{TranscriptionProvider, TranscriptionRequest, TranscriptionResponse};

const BASE10_PROXY_URL: &str = "https://base10-proxy.test-j.workers.dev";

/// Base10 transcription provider (with integrated completion)
pub struct Base10TranscriptionProvider {
    client: Client,
}

impl Base10TranscriptionProvider {
    pub fn new(_api_key: Option<String>) -> Self {
        Self {
            client: Client::new(),
        }
    }
}

#[derive(Debug, Serialize)]
struct WorkerRequest {
    whisper_input: WhisperInput,
    completion: WorkerCompletionParams,
}

#[derive(Debug, Serialize)]
struct WhisperInput {
    audio: AudioInput,
    whisper_params: WhisperParams,
}

#[derive(Debug, Serialize)]
struct AudioInput {
    audio_b64: String,
}

#[derive(Debug, Serialize)]
struct WhisperParams {
    audio_language: String,
}

#[derive(Debug, Serialize)]
struct WorkerCompletionParams {
    mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    app_context: Option<String>,
    shortcuts_triggered: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    voice_instruction: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WorkerResponse {
    transcription: String,
    text: String,
    #[serde(default)]
    language: Option<String>,
}

#[async_trait]
impl TranscriptionProvider for Base10TranscriptionProvider {
    fn name(&self) -> &'static str {
        "Auto (Cloud)"
    }

    async fn transcribe(&self, request: TranscriptionRequest) -> Result<TranscriptionResponse> {
        let wav_data = pcm_to_wav(&request.audio, request.sample_rate, 1);
        let audio_base64 = STANDARD.encode(&wav_data);
        let language = request.language.as_deref().unwrap_or("auto").to_string();

        // Completion params are required
        let completion = request.completion.ok_or_else(|| {
            Error::Transcription("Completion params required for auto mode".to_string())
        })?;

        let worker_request = WorkerRequest {
            whisper_input: WhisperInput {
                audio: AudioInput {
                    audio_b64: audio_base64,
                },
                whisper_params: WhisperParams {
                    audio_language: language,
                },
            },
            completion: WorkerCompletionParams {
                mode: completion.mode,
                app_context: completion.app_context,
                shortcuts_triggered: completion.shortcuts_triggered,
                voice_instruction: completion.voice_instruction,
            },
        };

        debug!("Sending combined transcription+completion request to worker");

        let response = self
            .client
            .post(BASE10_PROXY_URL)
            .json(&worker_request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            error!("Worker error: {} - {}", status, error_text);
            return Err(Error::Transcription(format!(
                "Worker error: {} - {}",
                status, error_text
            )));
        }

        let worker_response: WorkerResponse = response.json().await?;

        let samples = request.audio.len() / 2;
        let duration_ms = (samples as u64 * 1000) / request.sample_rate as u64;

        Ok(TranscriptionResponse {
            text: worker_response.transcription,
            confidence: None,
            language: worker_response.language,
            duration_ms,
            segments: None,
            completed_text: Some(worker_response.text),
        })
    }

    fn is_configured(&self) -> bool {
        true
    }
}

fn pcm_to_wav(pcm: &[u8], sample_rate: u32, channels: u16) -> Vec<u8> {
    let bits_per_sample: u16 = 16;
    let byte_rate = sample_rate * u32::from(channels) * u32::from(bits_per_sample) / 8;
    let block_align = channels * bits_per_sample / 8;
    let data_size = pcm.len() as u32;
    let file_size = 36 + data_size;

    let mut wav = Vec::with_capacity(44 + pcm.len());

    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&file_size.to_le_bytes());
    wav.extend_from_slice(b"WAVE");
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&channels.to_le_bytes());
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&byte_rate.to_le_bytes());
    wav.extend_from_slice(&block_align.to_le_bytes());
    wav.extend_from_slice(&bits_per_sample.to_le_bytes());
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
        let pcm = vec![0u8; 32000];
        let wav = pcm_to_wav(&pcm, 16000, 1);

        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(&wav[12..16], b"fmt ");
        assert_eq!(&wav[36..40], b"data");
        assert_eq!(wav.len(), 44 + 32000);
    }

    #[test]
    fn test_provider_always_configured() {
        let provider = Base10TranscriptionProvider::new(None);
        assert!(provider.is_configured());
    }
}
