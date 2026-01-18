//! Local Whisper provider using Candle with Metal acceleration

use crate::error::{Error, Result};
use async_trait::async_trait;
use candle_core::{Device, IndexOp, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::whisper::{self as m, audio, Config};
use hf_hub::{api::sync::Api, Repo, RepoType};
use parking_lot::Mutex;
use std::path::PathBuf;
use std::sync::Arc;
use tokenizers::Tokenizer;
use tracing::{debug, info};

use super::{TranscriptionProvider, TranscriptionRequest, TranscriptionResponse};

// Include the mel filter bytes (80 mel bins for Whisper)
const MEL_FILTER_BYTES: &[u8] = include_bytes!("../../melfilters.bytes");

/// Whisper model sizes
#[derive(Debug, Clone, Copy)]
pub enum WhisperModel {
    Tiny,
    Base,
    Small,
}

impl WhisperModel {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "tiny" => Some(WhisperModel::Tiny),
            "base" => Some(WhisperModel::Base),
            "small" => Some(WhisperModel::Small),
            _ => None,
        }
    }

    pub fn model_id(&self) -> (&'static str, &'static str) {
        match self {
            WhisperModel::Tiny => ("openai/whisper-tiny.en", "refs/pr/15"),
            WhisperModel::Base => ("openai/whisper-base.en", "refs/pr/13"),
            WhisperModel::Small => ("openai/whisper-small.en", "refs/pr/10"),
        }
    }

    pub fn size_mb(&self) -> usize {
        match self {
            WhisperModel::Tiny => 39,
            WhisperModel::Base => 142,
            WhisperModel::Small => 466,
        }
    }
}

/// Whisper engine state
struct WhisperEngine {
    model: m::model::Whisper,
    tokenizer: Tokenizer,
    config: Config,
    device: Device,
    mel_filters: Vec<f32>,
}

impl WhisperEngine {
    async fn new(model_size: WhisperModel, models_dir: &PathBuf) -> Result<Self> {
        std::fs::create_dir_all(models_dir).map_err(|e| {
            Error::Transcription(format!("Failed to create models directory: {}", e))
        })?;

        info!("Initializing Whisper {:?} model", model_size);

        // Download model files if needed
        let (config_path, tokenizer_path, weights_path) =
            Self::ensure_model_files(model_size, models_dir).await?;

        // Load config
        let config: Config = serde_json::from_str(
            &std::fs::read_to_string(&config_path)
                .map_err(|e| Error::Transcription(format!("Failed to read config: {}", e)))?,
        )
        .map_err(|e| Error::Transcription(format!("Failed to parse config: {}", e)))?;

        // Load tokenizer
        let tokenizer = Tokenizer::from_file(tokenizer_path)
            .map_err(|e| Error::Transcription(format!("Failed to load tokenizer: {}", e)))?;

        // Setup device - try Metal (Apple Silicon GPU) first, fallback to CPU
        let device = if cfg!(target_os = "macos") {
            match Device::new_metal(0) {
                Ok(device) => {
                    info!("Using Metal (GPU) acceleration");
                    device
                }
                Err(_) => {
                    info!("Metal not available, using CPU");
                    Device::Cpu
                }
            }
        } else {
            Device::Cpu
        };

        // Load model weights
        info!("Loading model weights...");
        let vb = unsafe {
            VarBuilder::from_mmaped_safetensors(&[weights_path], m::DTYPE, &device).map_err(
                |e| Error::Transcription(format!("Failed to load weights: {}", e)),
            )?
        };
        let model = m::model::Whisper::load(&vb, config.clone())
            .map_err(|e| Error::Transcription(format!("Failed to load model: {}", e)))?;

        // Load mel filters
        let mut mel_filters = vec![0f32; MEL_FILTER_BYTES.len() / 4];
        <byteorder::LittleEndian as byteorder::ByteOrder>::read_f32_into(
            MEL_FILTER_BYTES,
            &mut mel_filters,
        );

        info!("Whisper model loaded successfully");

        Ok(Self {
            model,
            tokenizer,
            config,
            device,
            mel_filters,
        })
    }

    async fn ensure_model_files(
        model_size: WhisperModel,
        models_dir: &PathBuf,
    ) -> Result<(PathBuf, PathBuf, PathBuf)> {
        let (model_id, revision) = model_size.model_id();
        let model_name = model_id.split('/').last().unwrap();

        let config_path = models_dir.join(format!("{}-config.json", model_name));
        let tokenizer_path = models_dir.join(format!("{}-tokenizer.json", model_name));
        let weights_path = models_dir.join(format!("{}-model.safetensors", model_name));

        // Check if all files exist
        if config_path.exists() && tokenizer_path.exists() && weights_path.exists() {
            info!("Model files already cached");
            return Ok((config_path, tokenizer_path, weights_path));
        }

        // Download from HuggingFace
        info!(
            "Downloading {} model files ({}MB)...",
            model_id,
            model_size.size_mb()
        );

        let api = Api::new()
            .map_err(|e| Error::Transcription(format!("Failed to init HuggingFace API: {}", e)))?;
        let repo = api.repo(Repo::with_revision(
            model_id.to_string(),
            RepoType::Model,
            revision.to_string(),
        ));

        // Download config
        info!("Downloading config.json");
        let config_file = repo
            .get("config.json")
            .map_err(|e| Error::Transcription(format!("Failed to download config: {}", e)))?;
        std::fs::copy(&config_file, &config_path)
            .map_err(|e| Error::Transcription(format!("Failed to save config: {}", e)))?;

        // Download tokenizer
        info!("Downloading tokenizer.json");
        let tokenizer_file = repo
            .get("tokenizer.json")
            .map_err(|e| Error::Transcription(format!("Failed to download tokenizer: {}", e)))?;
        std::fs::copy(&tokenizer_file, &tokenizer_path)
            .map_err(|e| Error::Transcription(format!("Failed to save tokenizer: {}", e)))?;

        // Download model weights
        info!("Downloading model weights (this may take a while)");
        let weights_file = repo
            .get("model.safetensors")
            .map_err(|e| Error::Transcription(format!("Failed to download weights: {}", e)))?;
        std::fs::copy(&weights_file, &weights_path)
            .map_err(|e| Error::Transcription(format!("Failed to save weights: {}", e)))?;

        info!("Model downloaded successfully");

        Ok((config_path, tokenizer_path, weights_path))
    }

    fn transcribe_pcm(&mut self, pcm_data: &[f32]) -> Result<String> {
        debug!("Transcribing {} samples", pcm_data.len());

        // Convert to mel spectrogram
        let mel = audio::pcm_to_mel(&self.config, pcm_data, &self.mel_filters);
        let mel_len = mel.len();
        let mel = Tensor::from_vec(
            mel,
            (
                1,
                self.config.num_mel_bins,
                mel_len / self.config.num_mel_bins,
            ),
            &self.device,
        )
        .map_err(|e| Error::Transcription(format!("Failed to create mel tensor: {}", e)))?;

        // Decode audio
        let segments = self
            .decode_audio(&mel)
            .map_err(|e| Error::Transcription(format!("Failed to decode audio: {}", e)))?;

        // Join segments
        let text = segments.join(" ");
        Ok(text.trim().to_string())
    }

    fn decode_audio(&mut self, mel: &Tensor) -> Result<Vec<String>> {
        let (_, _, content_frames) = mel
            .dims3()
            .map_err(|e| Error::Transcription(format!("Invalid mel dimensions: {}", e)))?;
        let mut segments = Vec::new();
        let mut seek = 0;

        // Get token IDs
        let sot_token = self.token_id(m::SOT_TOKEN)?;
        let transcribe_token = self.token_id(m::TRANSCRIBE_TOKEN)?;
        let eot_token = self.token_id(m::EOT_TOKEN)?;
        let no_timestamps_token = self.token_id(m::NO_TIMESTAMPS_TOKEN)?;

        while seek < content_frames {
            let segment_size = usize::min(content_frames - seek, m::N_FRAMES);
            let mel_segment = mel
                .narrow(2, seek, segment_size)
                .map_err(|e| Error::Transcription(format!("Failed to narrow mel: {}", e)))?;

            let audio_features = self
                .model
                .encoder
                .forward(&mel_segment, true)
                .map_err(|e| Error::Transcription(format!("Encoder failed: {}", e)))?;

            // Decode with greedy decoding
            let mut tokens = vec![sot_token, transcribe_token, no_timestamps_token];
            let max_tokens = self.config.max_target_positions / 2;

            for i in 0..max_tokens {
                let tokens_t = Tensor::new(tokens.as_slice(), &self.device)
                    .map_err(|e| Error::Transcription(format!("Failed to create tokens: {}", e)))?
                    .unsqueeze(0)
                    .map_err(|e| Error::Transcription(format!("Failed to unsqueeze: {}", e)))?;

                let decoder_output = self
                    .model
                    .decoder
                    .forward(&tokens_t, &audio_features, i == 0)
                    .map_err(|e| Error::Transcription(format!("Decoder failed: {}", e)))?;

                let (_, seq_len, _) = decoder_output.dims3().map_err(|e| {
                    Error::Transcription(format!("Invalid decoder output dims: {}", e))
                })?;

                let tail = decoder_output
                    .i((..1, seq_len - 1..))
                    .map_err(|e| Error::Transcription(format!("Failed to index tail: {}", e)))?;

                let logits = self
                    .model
                    .decoder
                    .final_linear(&tail)
                    .map_err(|e| Error::Transcription(format!("Failed final linear: {}", e)))?
                    .i(0)
                    .map_err(|e| Error::Transcription(format!("Failed to index: {}", e)))?
                    .i(0)
                    .map_err(|e| Error::Transcription(format!("Failed to index: {}", e)))?;

                // Get next token (greedy)
                let next_token = logits
                    .argmax(0)
                    .map_err(|e| Error::Transcription(format!("Failed argmax: {}", e)))?
                    .to_scalar::<u32>()
                    .map_err(|e| Error::Transcription(format!("Failed to_scalar: {}", e)))?;

                if next_token == eot_token {
                    break;
                }

                tokens.push(next_token);
            }

            // Decode tokens to text
            let text = self
                .tokenizer
                .decode(&tokens[3..], true)
                .map_err(|e| Error::Transcription(format!("Failed to decode tokens: {}", e)))?;

            if !text.trim().is_empty() {
                segments.push(text.trim().to_string());
            }

            seek += segment_size;
        }

        Ok(segments)
    }

    fn token_id(&self, token: &str) -> Result<u32> {
        self.tokenizer
            .token_to_id(token)
            .ok_or_else(|| Error::Transcription(format!("Token not found: {}", token)))
    }
}

/// Local Whisper transcription provider with Metal acceleration
pub struct LocalWhisperTranscriptionProvider {
    engine: Arc<Mutex<Option<WhisperEngine>>>,
    model_size: WhisperModel,
    models_dir: PathBuf,
}

impl LocalWhisperTranscriptionProvider {
    /// Create a new provider with a model size
    pub fn new(model_size: WhisperModel, models_dir: PathBuf) -> Self {
        Self {
            engine: Arc::new(Mutex::new(None)),
            model_size,
            models_dir,
        }
    }

    /// Load the model (call once before first use)
    pub async fn load_model(&self) -> Result<()> {
        let engine = WhisperEngine::new(self.model_size, &self.models_dir).await?;
        *self.engine.lock() = Some(engine);
        Ok(())
    }

    /// Check if model is loaded
    pub fn is_model_loaded(&self) -> bool {
        self.engine.lock().is_some()
    }

    /// Resample audio using linear interpolation
    fn resample_audio(samples: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
        if from_rate == to_rate {
            return samples.to_vec();
        }

        let ratio = to_rate as f32 / from_rate as f32;
        let output_len = (samples.len() as f32 * ratio) as usize;
        let mut output = Vec::with_capacity(output_len);

        for i in 0..output_len {
            let src_pos = i as f32 / ratio;
            let src_idx = src_pos as usize;
            let frac = src_pos - src_idx as f32;

            if src_idx + 1 < samples.len() {
                let sample = samples[src_idx] * (1.0 - frac) + samples[src_idx + 1] * frac;
                output.push(sample);
            } else if src_idx < samples.len() {
                output.push(samples[src_idx]);
            }
        }

        output
    }

    /// Convert PCM bytes (16-bit little-endian) to f32 normalized audio
    fn pcm_bytes_to_f32(audio_bytes: &[u8]) -> Vec<f32> {
        let mut samples = Vec::with_capacity(audio_bytes.len() / 2);
        for chunk in audio_bytes.chunks_exact(2) {
            let sample = i16::from_le_bytes([chunk[0], chunk[1]]);
            samples.push(sample as f32 / 32768.0);
        }
        samples
    }
}

#[async_trait]
impl TranscriptionProvider for LocalWhisperTranscriptionProvider {
    fn name(&self) -> &'static str {
        "Local Whisper (Metal)"
    }

    async fn transcribe(&self, request: TranscriptionRequest) -> Result<TranscriptionResponse> {
        // Ensure model is loaded
        if !self.is_model_loaded() {
            self.load_model().await?;
        }

        // Convert audio bytes to f32 format expected by whisper (mono at 16kHz)
        let mut audio_data = Self::pcm_bytes_to_f32(&request.audio);

        // Resample to 16kHz if needed
        if request.sample_rate != 16000 {
            audio_data = Self::resample_audio(&audio_data, request.sample_rate, 16000);
        }

        // Transcribe
        let mut engine_guard = self.engine.lock();
        let engine = engine_guard
            .as_mut()
            .ok_or_else(|| Error::Transcription("Whisper engine not initialized".to_string()))?;

        let text = engine.transcribe_pcm(&audio_data)?;

        debug!("Local Whisper transcription: {}", text);

        Ok(TranscriptionResponse {
            text,
            confidence: None,
            language: Some("en".to_string()),
            duration_ms: request.audio.len() as u64 * 1000 / request.sample_rate as u64,
            segments: None,
        })
    }

    fn is_configured(&self) -> bool {
        self.models_dir.exists()
    }
}
