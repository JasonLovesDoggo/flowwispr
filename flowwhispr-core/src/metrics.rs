//! Metrics and analytics collection
//!
//! Non-blocking event tracking with batched persistence for usage analytics.

use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;
use std::time::{Duration, Instant};

use parking_lot::RwLock;
use tracing::{debug, error, warn};

use crate::error::Result;
use crate::storage::Storage;
use crate::types::{AnalyticsEvent, AppContext, EventType, WritingMode};

/// Metrics collector for non-blocking event tracking
pub struct MetricsCollector {
    sender: Sender<TrackedEvent>,
    /// Session stats maintained in memory
    session_stats: RwLock<SessionStats>,
}

/// Internal event wrapper with metadata
struct TrackedEvent {
    event: AnalyticsEvent,
}

/// In-memory session statistics
#[derive(Debug, Default)]
pub struct SessionStats {
    pub transcriptions_count: u64,
    pub total_duration_ms: u64,
    pub shortcuts_triggered: u64,
    pub corrections_applied: u64,
    pub mode_changes: u64,
    pub session_start: Option<Instant>,
}

impl SessionStats {
    pub fn new() -> Self {
        Self {
            session_start: Some(Instant::now()),
            ..Default::default()
        }
    }

    /// Get session duration in seconds
    pub fn session_duration_secs(&self) -> u64 {
        self.session_start
            .map(|s| s.elapsed().as_secs())
            .unwrap_or(0)
    }
}

impl MetricsCollector {
    /// Create a new metrics collector with background persistence
    pub fn new(storage: Storage, _device_id: String) -> Self {
        let (sender, receiver) = channel();

        // spawn background thread for batched persistence
        thread::spawn(move || {
            Self::process_events(receiver, storage);
        });

        Self {
            sender,
            session_stats: RwLock::new(SessionStats::new()),
        }
    }

    /// Track a transcription started event
    pub fn track_transcription_started(&self, app_context: Option<AppContext>) {
        let mut event = AnalyticsEvent::new(EventType::TranscriptionStarted, serde_json::json!({}));
        event.app_context = app_context;
        self.track(event);
    }

    /// Track a transcription completed event
    pub fn track_transcription_completed(
        &self,
        duration_ms: u64,
        word_count: u32,
        app_context: Option<AppContext>,
    ) {
        let mut event = AnalyticsEvent::new(
            EventType::TranscriptionCompleted,
            serde_json::json!({
                "duration_ms": duration_ms,
                "word_count": word_count,
            }),
        );
        event.app_context = app_context;

        // update session stats
        {
            let mut stats = self.session_stats.write();
            stats.transcriptions_count += 1;
            stats.total_duration_ms += duration_ms;
        }

        self.track(event);
    }

    /// Track a transcription failed event
    pub fn track_transcription_failed(&self, error: &str, app_context: Option<AppContext>) {
        let mut event = AnalyticsEvent::new(
            EventType::TranscriptionFailed,
            serde_json::json!({
                "error": error,
            }),
        );
        event.app_context = app_context;
        self.track(event);
    }

    /// Track a shortcut triggered event
    pub fn track_shortcut_triggered(&self, trigger: &str, expansion_chars: usize) {
        let event = AnalyticsEvent::new(
            EventType::ShortcutTriggered,
            serde_json::json!({
                "trigger": trigger,
                "expansion_chars": expansion_chars,
            }),
        );

        {
            let mut stats = self.session_stats.write();
            stats.shortcuts_triggered += 1;
        }

        self.track(event);
    }

    /// Track a correction applied event
    pub fn track_correction_applied(&self, original: &str, corrected: &str, confidence: f32) {
        let event = AnalyticsEvent::new(
            EventType::CorrectionApplied,
            serde_json::json!({
                "original": original,
                "corrected": corrected,
                "confidence": confidence,
            }),
        );

        {
            let mut stats = self.session_stats.write();
            stats.corrections_applied += 1;
        }

        self.track(event);
    }

    /// Track a mode changed event
    pub fn track_mode_changed(&self, app_name: &str, old_mode: WritingMode, new_mode: WritingMode) {
        let event = AnalyticsEvent::new(
            EventType::ModeChanged,
            serde_json::json!({
                "app_name": app_name,
                "old_mode": format!("{:?}", old_mode),
                "new_mode": format!("{:?}", new_mode),
            }),
        );

        {
            let mut stats = self.session_stats.write();
            stats.mode_changes += 1;
        }

        self.track(event);
    }

    /// Track an app switched event
    pub fn track_app_switched(&self, app_context: AppContext) {
        let mut event = AnalyticsEvent::new(
            EventType::AppSwitched,
            serde_json::json!({
                "app_name": &app_context.app_name,
                "category": format!("{:?}", app_context.category),
            }),
        );
        event.app_context = Some(app_context);
        self.track(event);
    }

    /// Track settings updated event
    pub fn track_settings_updated(&self, setting: &str, old_value: &str, new_value: &str) {
        let event = AnalyticsEvent::new(
            EventType::SettingsUpdated,
            serde_json::json!({
                "setting": setting,
                "old_value": old_value,
                "new_value": new_value,
            }),
        );
        self.track(event);
    }

    /// Get current session stats
    pub fn session_stats(&self) -> SessionStats {
        let stats = self.session_stats.read();
        SessionStats {
            transcriptions_count: stats.transcriptions_count,
            total_duration_ms: stats.total_duration_ms,
            shortcuts_triggered: stats.shortcuts_triggered,
            corrections_applied: stats.corrections_applied,
            mode_changes: stats.mode_changes,
            session_start: stats.session_start,
        }
    }

    /// Internal: queue an event for background persistence
    fn track(&self, event: AnalyticsEvent) {
        let tracked = TrackedEvent { event };

        if let Err(e) = self.sender.send(tracked) {
            warn!("Failed to queue analytics event: {}", e);
        }
    }

    /// Background event processor with batching
    fn process_events(receiver: Receiver<TrackedEvent>, storage: Storage) {
        let mut batch = Vec::with_capacity(100);
        let flush_interval = Duration::from_secs(30);
        let mut last_flush = Instant::now();

        loop {
            match receiver.recv_timeout(Duration::from_secs(1)) {
                Ok(event) => {
                    batch.push(event);

                    // flush if batch is full or interval elapsed
                    if batch.len() >= 100 || last_flush.elapsed() > flush_interval {
                        Self::flush_batch(&storage, &mut batch);
                        last_flush = Instant::now();
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    if !batch.is_empty() && last_flush.elapsed() > flush_interval {
                        Self::flush_batch(&storage, &mut batch);
                        last_flush = Instant::now();
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    // channel closed, flush remaining and exit
                    if !batch.is_empty() {
                        Self::flush_batch(&storage, &mut batch);
                    }
                    debug!("Metrics collector shutting down");
                    break;
                }
            }
        }
    }

    fn flush_batch(storage: &Storage, batch: &mut Vec<TrackedEvent>) {
        if batch.is_empty() {
            return;
        }

        debug!("Flushing {} analytics events", batch.len());

        for tracked in batch.drain(..) {
            if let Err(e) = storage.save_event(&tracked.event) {
                error!("Failed to save analytics event: {}", e);
            }
        }
    }
}

/// User-facing aggregated statistics
#[derive(Debug, Clone, Default)]
pub struct UserStats {
    pub total_transcriptions: u64,
    pub total_words_dictated: u64,
    pub total_duration_ms: u64,
    pub shortcuts_expanded: u64,
    pub corrections_applied: u64,
    pub corrections_learned: u64,
}

impl UserStats {
    /// Calculate stats from storage
    pub fn from_storage(storage: &Storage) -> Result<Self> {
        let transcription_count = storage.get_transcription_count()?;
        let total_duration_ms = storage.get_total_transcription_time_ms()?;

        // estimate words from duration (rough: 150 wpm)
        let total_words_dictated = (total_duration_ms / 1000 / 60) * 150;

        Ok(Self {
            total_transcriptions: transcription_count,
            total_words_dictated,
            total_duration_ms,
            ..Default::default()
        })
    }

    /// Estimated time saved assuming typing speed of 40 WPM
    pub fn estimated_time_saved_minutes(&self) -> u64 {
        // if user dictates at 150 wpm and types at 40 wpm, they save about 73% of time
        let typing_time_ms = (self.total_words_dictated as f64 / 40.0 * 60.0 * 1000.0) as u64;
        let dictation_time_ms = self.total_duration_ms;

        if typing_time_ms > dictation_time_ms {
            (typing_time_ms - dictation_time_ms) / 60000
        } else {
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_stats() {
        let stats = SessionStats::new();
        assert_eq!(stats.transcriptions_count, 0);
        assert!(stats.session_start.is_some());
    }

    #[test]
    fn test_user_stats_time_saved() {
        let stats = UserStats {
            total_transcriptions: 100,
            total_words_dictated: 3000,        // 3000 words
            total_duration_ms: 20 * 60 * 1000, // 20 minutes of dictation
            ..Default::default()
        };

        // typing 3000 words at 40 wpm = 75 minutes
        // dictation took 20 minutes
        // saved ~55 minutes
        let saved = stats.estimated_time_saved_minutes();
        assert!(saved > 50);
    }
}
