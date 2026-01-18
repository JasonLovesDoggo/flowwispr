//! Base10 provider for Whisper transcription
//!
//! Base10 is a transcription-only provider (no completion support).
//! Requests are proxied through a Cloudflare Worker that handles authentication.

use async_trait::async_trait;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{debug, error};

use crate::error::{Error, Result};

use super::{TranscriptionProvider, TranscriptionRequest, TranscriptionResponse};

const BASE10_PROXY_URL: &str = "https://base10-proxy.test-j.workers.dev";

/// Base10 Whisper transcription provider
pub struct Base10TranscriptionProvider {
    client: Client,
}

impl Base10TranscriptionProvider {
    /// Create a new provider (API key handled by proxy)
    pub fn new(_api_key: Option<String>) -> Self {
        Self {
            client: Client::new(),
        }
    }
}

#[derive(Debug, Serialize)]
struct Base10Request {
    whisper_input: WhisperInput,
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

#[derive(Debug, Deserialize)]
struct Base10Response {
    segments: Option<Vec<TranscriptionSegment>>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    language: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TranscriptionSegment {
    text: String,
    #[serde(default)]
    #[allow(dead_code)]
    start: Option<f64>,
    #[serde(default)]
    #[allow(dead_code)]
    end: Option<f64>,
}

#[async_trait]
impl TranscriptionProvider for Base10TranscriptionProvider {
    fn name(&self) -> &'static str {
        "Base10 Whisper"
    }

    async fn transcribe(&self, request: TranscriptionRequest) -> Result<TranscriptionResponse> {
        // Convert PCM to WAV format and base64 encode
        let wav_data = pcm_to_wav(&request.audio, request.sample_rate, 1);
        let audio_base64 = STANDARD.encode(&wav_data);

        // Build the request
        let language = request.language.as_deref().unwrap_or("auto").to_string();
        let base10_request = Base10Request {
            whisper_input: WhisperInput {
                audio: AudioInput {
                    audio_b64: audio_base64,
                },
                whisper_params: WhisperParams {
                    audio_language: language,
                },
            },
        };

        debug!("Sending transcription request to Base10 proxy");

        let response = self
            .client
            .post(BASE10_PROXY_URL)
            .json(&base10_request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            error!("Base10 API error: {} - {}", status, error_text);
            return Err(Error::Transcription(format!(
                "Base10 API error: {} - {}",
                status, error_text
            )));
        }

        let base10_response: Base10Response = response.json().await?;

        // Extract text from segments or direct text field
        let text = if let Some(segments) = &base10_response.segments {
            segments
                .iter()
                .map(|s| s.text.as_str())
                .collect::<Vec<_>>()
                .join("")
                .trim()
                .to_string()
        } else if let Some(text) = base10_response.text {
            text.trim().to_string()
        } else {
            return Err(Error::Transcription(
                "No transcription returned from Base10".to_string(),
            ));
        };

        // Estimate duration from audio size
        let samples = request.audio.len() / 2;
        let duration_ms = (samples as u64 * 1000) / request.sample_rate as u64;

        Ok(TranscriptionResponse {
            text,
            confidence: None,
            language: base10_response.language,
            duration_ms,
            segments: None,
        })
    }

    fn is_configured(&self) -> bool {
        true // proxy handles authentication
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
    fn test_provider_always_configured() {
        // proxy handles auth, so always configured
        let provider = Base10TranscriptionProvider::new(None);
        assert!(provider.is_configured());
    }
}
