//! Voice Activity Detection module
//!
//! Provides speech detection to determine when the user starts/stops talking.
//! Currently uses a simple energy-based approach.
//!
//! TODO: Integrate Silero VAD ONNX model for more accurate detection
//! when ort crate reaches stable 2.0.

use crate::error::Result;
use tracing::debug;

/// Sample rate expected by VAD
pub const VAD_SAMPLE_RATE: u32 = 16000;

/// Chunk size for VAD processing (512 samples = 32ms at 16kHz)
pub const VAD_CHUNK_SIZE: usize = 512;

/// Voice Activity Detection state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoiceActivity {
    /// No speech detected
    Silence,
    /// Speech is being detected
    Speech,
}

/// Simple energy-based Voice Activity Detection
///
/// This is a placeholder implementation that uses RMS energy detection.
/// Will be replaced with Silero VAD ONNX model for production use.
pub struct SimpleVad {
    /// Energy threshold for speech detection (RMS)
    threshold: f32,
    /// Minimum consecutive speech chunks before triggering speech start
    min_speech_chunks: usize,
    /// Minimum consecutive silence chunks before triggering speech end
    min_silence_chunks: usize,
    /// Current consecutive speech chunk count
    speech_chunk_count: usize,
    /// Current consecutive silence chunk count
    silence_chunk_count: usize,
    /// Current voice activity state
    current_state: VoiceActivity,
}

impl Default for SimpleVad {
    fn default() -> Self {
        Self::new()
    }
}

impl SimpleVad {
    /// Create a new VAD instance with default settings
    pub fn new() -> Self {
        Self {
            threshold: 0.01,        // RMS threshold (adjust based on mic sensitivity)
            min_speech_chunks: 3,   // ~96ms of speech to trigger
            min_silence_chunks: 15, // ~480ms of silence to end
            speech_chunk_count: 0,
            silence_chunk_count: 0,
            current_state: VoiceActivity::Silence,
        }
    }

    /// Set the energy threshold (0.0 - 1.0, lower = more sensitive)
    pub fn set_threshold(&mut self, threshold: f32) {
        self.threshold = threshold.clamp(0.001, 0.5);
    }

    /// Reset the VAD state (call when starting a new recording)
    pub fn reset(&mut self) {
        self.speech_chunk_count = 0;
        self.silence_chunk_count = 0;
        self.current_state = VoiceActivity::Silence;
        debug!("VAD state reset");
    }

    /// Calculate RMS energy of audio samples
    fn calculate_rms(samples: &[f32]) -> f32 {
        if samples.is_empty() {
            return 0.0;
        }
        let sum_squares: f32 = samples.iter().map(|&s| s * s).sum();
        (sum_squares / samples.len() as f32).sqrt()
    }

    /// Process a chunk of audio samples and return speech probability estimate
    ///
    /// # Arguments
    /// * `samples` - Audio samples (ideally VAD_CHUNK_SIZE = 512 samples at 16kHz)
    ///
    /// # Returns
    /// Estimated speech probability between 0.0 and 1.0
    pub fn process_chunk(&self, samples: &[f32]) -> f32 {
        let rms = Self::calculate_rms(samples);
        // Convert RMS to a 0-1 probability-like score
        // This is a rough approximation; Silero VAD would be much more accurate
        (rms / self.threshold).min(1.0)
    }

    /// Process a chunk and update the voice activity state
    ///
    /// Returns the current voice activity state and whether it just changed
    pub fn update(&mut self, samples: &[f32]) -> Result<(VoiceActivity, bool)> {
        let rms = Self::calculate_rms(samples);
        let is_speech = rms >= self.threshold;

        let previous_state = self.current_state;

        if is_speech {
            self.speech_chunk_count += 1;
            self.silence_chunk_count = 0;

            if self.current_state == VoiceActivity::Silence
                && self.speech_chunk_count >= self.min_speech_chunks
            {
                self.current_state = VoiceActivity::Speech;
                debug!("VAD: Speech started (rms: {:.4})", rms);
            }
        } else {
            self.silence_chunk_count += 1;
            self.speech_chunk_count = 0;

            if self.current_state == VoiceActivity::Speech
                && self.silence_chunk_count >= self.min_silence_chunks
            {
                self.current_state = VoiceActivity::Silence;
                debug!("VAD: Speech ended (rms: {:.4})", rms);
            }
        }

        let state_changed = previous_state != self.current_state;
        Ok((self.current_state, state_changed))
    }

    /// Get the current voice activity state
    pub fn state(&self) -> VoiceActivity {
        self.current_state
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vad_constants() {
        assert_eq!(VAD_SAMPLE_RATE, 16000);
        assert_eq!(VAD_CHUNK_SIZE, 512);
        // 512 samples at 16kHz = 32ms
        let chunk_duration_ms = (VAD_CHUNK_SIZE as f32 / VAD_SAMPLE_RATE as f32) * 1000.0;
        assert!((chunk_duration_ms - 32.0).abs() < 0.1);
    }

    #[test]
    fn test_rms_calculation() {
        // Silence
        let silence = vec![0.0f32; 512];
        assert_eq!(SimpleVad::calculate_rms(&silence), 0.0);

        // Full scale sine wave has RMS of 1/sqrt(2) ≈ 0.707
        let samples: Vec<f32> = (0..512)
            .map(|i| (i as f32 * std::f32::consts::PI * 2.0 / 32.0).sin())
            .collect();
        let rms = SimpleVad::calculate_rms(&samples);
        assert!((rms - 0.707).abs() < 0.01);
    }

    #[test]
    fn test_vad_state_transitions() {
        let mut vad = SimpleVad::new();
        vad.set_threshold(0.01);

        // Start with silence
        assert_eq!(vad.state(), VoiceActivity::Silence);

        // Feed some "speech" (loud samples)
        let speech = vec![0.1f32; 512];
        for _ in 0..5 {
            let (state, _) = vad.update(&speech).unwrap();
            if state == VoiceActivity::Speech {
                break;
            }
        }
        assert_eq!(vad.state(), VoiceActivity::Speech);

        // Feed silence to end speech
        let silence = vec![0.001f32; 512];
        for _ in 0..20 {
            let (state, _) = vad.update(&silence).unwrap();
            if state == VoiceActivity::Silence {
                break;
            }
        }
        assert_eq!(vad.state(), VoiceActivity::Silence);
    }
}
