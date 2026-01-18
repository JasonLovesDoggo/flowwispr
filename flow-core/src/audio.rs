//! Audio capture module using CPAL for cross-platform audio input

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, Sample, SampleFormat, SizedSample, Stream, StreamConfig};
use parking_lot::Mutex;
use std::sync::Arc;
use tracing::{debug, error, info};

use crate::AudioData;
use crate::error::{Error, Result};

/// Audio capture configuration
#[derive(Debug, Clone)]
pub struct AudioCaptureConfig {
    /// Sample rate in Hz (default: 16000 for speech recognition)
    pub sample_rate: u32,
    /// Number of channels (default: 1 for mono)
    pub channels: u16,
    /// Buffer size in samples
    pub buffer_size: usize,
}

impl Default for AudioCaptureConfig {
    fn default() -> Self {
        Self {
            sample_rate: 16000,
            channels: 1,
            buffer_size: 4096,
        }
    }
}

/// State of the audio capture
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureState {
    Idle,
    Recording,
    Paused,
}

/// Handles audio capture from the default input device
pub struct AudioCapture {
    device: Device,
    config: AudioCaptureConfig,
    stream_config: StreamConfig,
    input_channels: u16,
    sample_format: SampleFormat,
    state: Arc<Mutex<CaptureState>>,
    buffer: Arc<Mutex<Vec<f32>>>,
    stream: Option<Stream>,
}

impl AudioCapture {
    /// Create a new AudioCapture with default settings
    pub fn new() -> Result<Self> {
        Self::with_config(AudioCaptureConfig::default())
    }

    /// Create a new AudioCapture with custom configuration
    pub fn with_config(config: AudioCaptureConfig) -> Result<Self> {
        let host = cpal::default_host();

        let device = host
            .default_input_device()
            .ok_or_else(|| Error::Audio("No input device available".to_string()))?;

        // note: device.name() is deprecated in cpal 0.17+, but works
        #[allow(deprecated)]
        let device_name = device.name().unwrap_or_else(|_| "Unknown".to_string());
        info!("Using input device: {}", device_name);

        let supported_configs: Vec<_> = device
            .supported_input_configs()
            .map_err(|e| Error::Audio(format!("Failed to get supported configs: {e}")))?
            .collect();

        if supported_configs.is_empty() {
            return Err(Error::Audio("No supported input configs".to_string()));
        }

        let (supported_config, input_channels, sample_format, sample_rate) =
            select_supported_config(&supported_configs, config.sample_rate, config.channels)
                .ok_or_else(|| Error::Audio("No supported input config found".to_string()))?;

        let stream_config = supported_config.config();

        let mut config = config;
        config.sample_rate = sample_rate;
        config.channels = 1;

        debug!(
            "Stream config: {:?} (input channels: {}, format: {:?})",
            stream_config, input_channels, sample_format
        );

        Ok(Self {
            device,
            config,
            stream_config,
            input_channels,
            sample_format,
            state: Arc::new(Mutex::new(CaptureState::Idle)),
            buffer: Arc::new(Mutex::new(Vec::new())),
            stream: None,
        })
    }

    /// Start recording audio
    pub fn start(&mut self) -> Result<()> {
        if *self.state.lock() == CaptureState::Recording {
            return Ok(());
        }

        let buffer = Arc::clone(&self.buffer);
        let state = Arc::clone(&self.state);

        // clear buffer
        buffer.lock().clear();

        let err_fn = |err| error!("Audio stream error: {}", err);

        let stream = match self.sample_format {
            SampleFormat::F32 => self.build_stream::<f32>(buffer, state, err_fn)?,
            SampleFormat::I16 => self.build_stream::<i16>(buffer, state, err_fn)?,
            SampleFormat::U16 => self.build_stream::<u16>(buffer, state, err_fn)?,
            SampleFormat::I24 => self.build_stream::<cpal::I24>(buffer, state, err_fn)?,
            SampleFormat::U24 => self.build_stream::<cpal::U24>(buffer, state, err_fn)?,
            SampleFormat::I32 => self.build_stream::<i32>(buffer, state, err_fn)?,
            SampleFormat::U32 => self.build_stream::<u32>(buffer, state, err_fn)?,
            SampleFormat::I8 => self.build_stream::<i8>(buffer, state, err_fn)?,
            SampleFormat::U8 => self.build_stream::<u8>(buffer, state, err_fn)?,
            SampleFormat::F64 => self.build_stream::<f64>(buffer, state, err_fn)?,
            SampleFormat::I64 => self.build_stream::<i64>(buffer, state, err_fn)?,
            SampleFormat::U64 => self.build_stream::<u64>(buffer, state, err_fn)?,
            _ => {
                return Err(Error::Audio(format!(
                    "Unsupported sample format: {:?}",
                    self.sample_format
                )));
            }
        };

        stream
            .play()
            .map_err(|e| Error::Audio(format!("Failed to start stream: {e}")))?;

        self.stream = Some(stream);
        *self.state.lock() = CaptureState::Recording;

        info!("Audio capture started");
        Ok(())
    }

    /// Stop recording and return the captured audio data
    pub fn stop(&mut self) -> Result<AudioData> {
        *self.state.lock() = CaptureState::Idle;

        // drop the stream to stop recording
        self.stream = None;

        let samples = std::mem::take(&mut *self.buffer.lock());
        let audio_data = self.samples_to_pcm(&samples);

        info!("Audio capture stopped, {} bytes captured", audio_data.len());
        Ok(audio_data)
    }

    /// Stop recording without draining the buffer
    pub fn stop_stream(&mut self) -> Result<()> {
        *self.state.lock() = CaptureState::Idle;
        self.stream = None;
        info!("Audio capture stopped (buffer retained)");
        Ok(())
    }

    /// Drain buffered audio into PCM data without touching the stream
    pub fn take_buffered_audio(&mut self) -> AudioData {
        let samples = std::mem::take(&mut *self.buffer.lock());
        self.samples_to_pcm(&samples)
    }

    /// Pause recording (keeps stream alive but stops buffering)
    pub fn pause(&mut self) {
        *self.state.lock() = CaptureState::Paused;
        debug!("Audio capture paused");
    }

    /// Resume recording after pause
    pub fn resume(&mut self) {
        *self.state.lock() = CaptureState::Recording;
        debug!("Audio capture resumed");
    }

    /// Get current capture state
    pub fn state(&self) -> CaptureState {
        *self.state.lock()
    }

    /// Get current buffer duration in milliseconds
    pub fn buffer_duration_ms(&self) -> u64 {
        let samples = self.buffer.lock().len();
        (samples as u64 * 1000) / (self.config.sample_rate as u64 * self.config.channels as u64)
    }

    /// Current capture sample rate
    pub fn sample_rate(&self) -> u32 {
        self.config.sample_rate
    }

    /// Get current audio level (RMS amplitude) from the last 50ms of audio
    /// Returns a value between 0.0 and 1.0
    pub fn current_audio_level(&self) -> f32 {
        let buffer = self.buffer.lock();
        if buffer.is_empty() {
            return 0.0;
        }

        // Calculate how many samples represent 50ms
        let samples_per_50ms = (self.config.sample_rate as usize / 20).max(1);
        let start_idx = buffer.len().saturating_sub(samples_per_50ms);
        let recent_samples = &buffer[start_idx..];

        // Calculate RMS (root mean square) for perceived loudness
        let sum_squares: f32 = recent_samples.iter().map(|&s| s * s).sum();
        let rms = (sum_squares / recent_samples.len() as f32).sqrt();

        // Amplify a bit for visual effect (typical speech is quite quiet)
        (rms * 3.0).min(1.0)
    }

    fn build_stream<T>(
        &self,
        buffer: Arc<Mutex<Vec<f32>>>,
        state: Arc<Mutex<CaptureState>>,
        err_fn: impl FnMut(cpal::StreamError) + Send + 'static,
    ) -> Result<Stream>
    where
        T: Sample + SizedSample,
        f32: cpal::FromSample<T>,
    {
        let channels = self.input_channels as usize;
        let stream_config = self.stream_config.clone();

        self.device
            .build_input_stream(
                &stream_config,
                move |data: &[T], _: &cpal::InputCallbackInfo| {
                    if *state.lock() != CaptureState::Recording {
                        return;
                    }

                    if channels == 1 {
                        buffer
                            .lock()
                            .extend(data.iter().map(|sample| sample.to_sample::<f32>()));
                    } else {
                        let mut buf = buffer.lock();
                        for frame in data.chunks_exact(channels) {
                            let mut sum = 0.0f32;
                            for sample in frame {
                                sum += sample.to_sample::<f32>();
                            }
                            buf.push(sum / channels as f32);
                        }
                    }
                },
                err_fn,
                None,
            )
            .map_err(|e| Error::Audio(format!("Failed to build stream: {e}")))
    }

    /// Convert f32 samples to 16-bit PCM bytes
    fn samples_to_pcm(&self, samples: &[f32]) -> AudioData {
        samples
            .iter()
            .flat_map(|&sample| {
                // clamp and convert to i16
                let clamped = sample.clamp(-1.0, 1.0);
                let pcm = (clamped * 32767.0) as i16;
                pcm.to_le_bytes()
            })
            .collect()
    }
}

impl Drop for AudioCapture {
    fn drop(&mut self) {
        *self.state.lock() = CaptureState::Idle;
        self.stream = None;
    }
}

fn select_supported_config(
    ranges: &[cpal::SupportedStreamConfigRange],
    preferred_rate: u32,
    preferred_channels: u16,
) -> Option<(cpal::SupportedStreamConfig, u16, SampleFormat, u32)> {
    let preferred_formats = [
        SampleFormat::F32,
        SampleFormat::I16,
        SampleFormat::U16,
        SampleFormat::I32,
        SampleFormat::U32,
        SampleFormat::F64,
        SampleFormat::I24,
        SampleFormat::U24,
        SampleFormat::I8,
        SampleFormat::U8,
        SampleFormat::I64,
        SampleFormat::U64,
    ];

    for format in preferred_formats {
        let mut candidates: Vec<_> = ranges
            .iter()
            .copied()
            .filter(|range| {
                range.sample_format() == format && range.channels() == preferred_channels
            })
            .collect();

        if candidates.is_empty() {
            candidates = ranges
                .iter()
                .copied()
                .filter(|range| range.sample_format() == format)
                .collect();
        }

        if candidates.is_empty() {
            continue;
        }

        let best = candidates
            .into_iter()
            .min_by_key(|range| sample_rate_distance(*range, preferred_rate))?;

        let sample_rate = choose_sample_rate(best, preferred_rate);
        let supported = best.with_sample_rate(sample_rate);

        return Some((supported, best.channels(), format, sample_rate));
    }

    None
}

fn sample_rate_distance(range: cpal::SupportedStreamConfigRange, preferred_rate: u32) -> u32 {
    let min_rate = range.min_sample_rate();
    let max_rate = range.max_sample_rate();
    if preferred_rate < min_rate {
        min_rate.saturating_sub(preferred_rate)
    } else if preferred_rate > max_rate {
        preferred_rate.saturating_sub(max_rate)
    } else {
        0
    }
}

fn choose_sample_rate(range: cpal::SupportedStreamConfigRange, preferred_rate: u32) -> u32 {
    let min_rate = range.min_sample_rate();
    let max_rate = range.max_sample_rate();
    if preferred_rate < min_rate {
        min_rate
    } else if preferred_rate > max_rate {
        max_rate
    } else {
        preferred_rate
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = AudioCaptureConfig::default();
        assert_eq!(config.sample_rate, 16000);
        assert_eq!(config.channels, 1);
    }

    #[test]
    fn test_samples_to_pcm() {
        // this test doesn't need audio hardware, just validates PCM conversion logic
        // test conversion manually
        let samples = [0.0f32, 0.5, -0.5, 1.0, -1.0];
        let pcm: Vec<u8> = samples
            .iter()
            .flat_map(|&sample| {
                let clamped = sample.clamp(-1.0, 1.0);
                let pcm = (clamped * 32767.0) as i16;
                pcm.to_le_bytes()
            })
            .collect();

        // 5 samples * 2 bytes each = 10 bytes
        assert_eq!(pcm.len(), 10);

        // check silence (0.0 -> 0)
        assert_eq!(i16::from_le_bytes([pcm[0], pcm[1]]), 0);

        // check 0.5 -> ~16383
        let half_pos = i16::from_le_bytes([pcm[2], pcm[3]]);
        assert!((half_pos - 16383).abs() < 2);

        // check -0.5 -> ~-16383
        let half_neg = i16::from_le_bytes([pcm[4], pcm[5]]);
        assert!((half_neg + 16383).abs() < 2);
    }
}
