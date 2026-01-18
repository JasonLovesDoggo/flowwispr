//! FFI layer for Swift integration
//!
//! Provides C-compatible functions that can be called from Swift.
//! Uses opaque pointers and C strings for cross-language compatibility.

// FFI functions necessarily work with raw pointers - this is expected behavior
#![allow(clippy::not_unsafe_ptr_arg_deref)]

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_void};
use std::path::PathBuf;
use std::ptr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use parking_lot::Mutex;
use serde::Serialize;
use tokio::runtime::Runtime;
use tracing::{debug, error};

use crate::apps::AppTracker;
use crate::audio::{AudioCapture, CaptureState};
use crate::contacts::{ContactClassifier, ContactInput};
use crate::learning::LearningEngine;
use crate::macos_messages::MessagesDetector;
use crate::modes::{StyleLearner, WritingMode, WritingModeEngine};
use crate::providers::{
    CompletionProvider, CompletionRequest, GeminiCompletionProvider, GeminiTranscriptionProvider,
    LocalWhisperTranscriptionProvider, OpenAICompletionProvider, OpenAITranscriptionProvider,
    OpenRouterCompletionProvider, TranscriptionProvider, TranscriptionRequest, WhisperModel,
};
use crate::shortcuts::ShortcutsEngine;
use crate::storage::{
    SETTING_COMPLETION_PROVIDER, SETTING_GEMINI_API_KEY, SETTING_LOCAL_WHISPER_MODEL,
    SETTING_OPENAI_API_KEY, SETTING_OPENROUTER_API_KEY, SETTING_USE_LOCAL_TRANSCRIPTION, Storage,
};
use crate::types::{Shortcut, Transcription, TranscriptionHistoryEntry, TranscriptionStatus};

/// Log with timestamp
macro_rules! log_with_time {
    ($($arg:tt)*) => {{
        use std::io::Write;
        let now = chrono::Local::now();
        println!("[{}] {}", now.format("%Y-%m-%d %H:%M:%S%.3f"), format!($($arg)*));
        let _ = std::io::stdout().flush();
    }};
}

/// Opaque handle to the FlowWhispr engine
pub struct FlowWhisprHandle {
    runtime: Runtime,
    storage: Storage,
    audio: Mutex<Option<AudioCapture>>,
    last_audio: Mutex<Option<crate::AudioData>>,
    last_audio_sample_rate: Mutex<Option<u32>>,
    last_error: Mutex<Option<String>>,
    transcription: Arc<dyn TranscriptionProvider>,
    completion: Arc<dyn CompletionProvider>,
    shortcuts: ShortcutsEngine,
    learning: LearningEngine,
    modes: Mutex<WritingModeEngine>,
    app_tracker: AppTracker,
    style_learner: Mutex<StyleLearner>,
    is_model_loading: Arc<AtomicBool>,
    contact_classifier: ContactClassifier,
    /// Captured contact name at recording start (for Messages.app context)
    captured_contact: Mutex<Option<String>>,
}

#[derive(Serialize)]
struct TranscriptionSummary {
    id: String,
    status: TranscriptionStatus,
    text: String,
    raw_text: String,
    error: Option<String>,
    duration_ms: u64,
    created_at: String,
    app_name: Option<String>,
}

/// Result callback type for async operations
pub type ResultCallback = extern "C" fn(success: bool, result: *const c_char, context: *mut c_void);

fn set_last_error(handle: &FlowWhisprHandle, message: impl Into<String>) {
    *handle.last_error.lock() = Some(message.into());
}

/// Check if Whisper model files exist in the models directory
fn check_model_files_exist(model: WhisperModel, models_dir: &std::path::Path) -> bool {
    let (model_id, _) = model.model_id();
    let model_name = model_id.split('/').next_back().unwrap();

    let config_path = models_dir.join(format!("{}-config.json", model_name));
    let tokenizer_path = models_dir.join(format!("{}-tokenizer.json", model_name));
    let weights_path = models_dir.join(format!("{}-model.safetensors", model_name));

    config_path.exists() && tokenizer_path.exists() && weights_path.exists()
}

fn clear_last_error(handle: &FlowWhisprHandle) {
    *handle.last_error.lock() = None;
}

fn estimate_duration_ms(bytes: usize, sample_rate: u32) -> u64 {
    let samples = bytes / 2;
    (samples as u64 * 1000) / sample_rate as u64
}

fn load_persisted_configuration(handle: &mut FlowWhisprHandle) {
    // Load all API keys
    let openai_key = handle
        .storage
        .get_setting(SETTING_OPENAI_API_KEY)
        .ok()
        .flatten();
    let gemini_key = handle
        .storage
        .get_setting(SETTING_GEMINI_API_KEY)
        .ok()
        .flatten();
    let openrouter_key = handle
        .storage
        .get_setting(SETTING_OPENROUTER_API_KEY)
        .ok()
        .flatten();

    // Load saved provider preference
    let saved_provider = handle
        .storage
        .get_setting(SETTING_COMPLETION_PROVIDER)
        .ok()
        .flatten();

    // Log what we found for debugging
    tracing::info!("Loading persisted config:");
    tracing::info!(
        "  OpenAI key: {}",
        if openai_key.is_some() { "SET" } else { "NONE" }
    );
    tracing::info!(
        "  Gemini key: {}",
        if gemini_key.is_some() { "SET" } else { "NONE" }
    );
    tracing::info!(
        "  OpenRouter key: {}",
        if openrouter_key.is_some() {
            "SET"
        } else {
            "NONE"
        }
    );
    tracing::info!("  Saved provider: {:?}", saved_provider);

    // Initialize providers based on saved preference
    match saved_provider.as_deref() {
        Some("gemini") => {
            debug!("Restoring Gemini provider from database");
            handle.transcription = Arc::new(GeminiTranscriptionProvider::new(gemini_key.clone()));
            handle.completion = Arc::new(GeminiCompletionProvider::new(gemini_key));
        }
        Some("openrouter") => {
            debug!("Restoring OpenRouter provider from database");
            // OpenRouter doesn't do transcription, use OpenAI for that
            handle.transcription = Arc::new(OpenAITranscriptionProvider::new(openai_key));
            handle.completion = Arc::new(OpenRouterCompletionProvider::new(openrouter_key));
        }
        _ => {
            // Default to OpenAI or if "openai" was explicitly saved
            debug!("Restoring OpenAI provider from database");
            handle.transcription = Arc::new(OpenAITranscriptionProvider::new(openai_key.clone()));
            handle.completion = Arc::new(OpenAICompletionProvider::new(openai_key));
        }
    }
}

// ============ Lifecycle ============

/// Initialize the FlowWhispr engine
/// Returns an opaque handle that must be passed to all other functions
/// Returns null on failure
#[unsafe(no_mangle)]
pub extern "C" fn flowwispr_init(db_path: *const c_char) -> *mut FlowWhisprHandle {
    let db_path = if db_path.is_null() {
        // default to app support directory
        dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("flowwispr")
            .join("flowwispr.db")
    } else {
        let path_str = match unsafe { CStr::from_ptr(db_path) }.to_str() {
            Ok(s) => s,
            Err(_) => return ptr::null_mut(),
        };
        PathBuf::from(path_str)
    };

    // ensure parent directory exists
    if let Some(parent) = db_path.parent()
        && let Err(e) = std::fs::create_dir_all(parent)
    {
        error!("Failed to create data directory: {}", e);
        return ptr::null_mut();
    }

    let runtime = match Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            error!("Failed to create async runtime: {}", e);
            return ptr::null_mut();
        }
    };

    let storage = match Storage::open(&db_path) {
        Ok(s) => s,
        Err(e) => {
            error!("Failed to open storage: {}", e);
            return ptr::null_mut();
        }
    };

    let shortcuts =
        ShortcutsEngine::from_storage(&storage).unwrap_or_else(|_| ShortcutsEngine::new());
    let learning = LearningEngine::from_storage(&storage).unwrap_or_else(|_| LearningEngine::new());
    let modes = WritingModeEngine::new(WritingMode::Casual);
    let app_tracker = AppTracker::new();
    let style_learner = StyleLearner::new();
    let contact_classifier = ContactClassifier::new();

    let mut handle = FlowWhisprHandle {
        runtime,
        storage,
        audio: Mutex::new(None),
        last_audio: Mutex::new(None),
        last_audio_sample_rate: Mutex::new(None),
        last_error: Mutex::new(None),
        transcription: Arc::new(OpenAITranscriptionProvider::new(None)),
        completion: Arc::new(OpenAICompletionProvider::new(None)),
        shortcuts,
        learning,
        modes: Mutex::new(modes),
        app_tracker,
        style_learner: Mutex::new(style_learner),
        is_model_loading: Arc::new(AtomicBool::new(false)),
        contact_classifier,
        captured_contact: Mutex::new(None),
    };

    load_persisted_configuration(&mut handle);

    // Load transcription mode (local vs remote Whisper)
    let use_local = handle.storage.get_setting(SETTING_USE_LOCAL_TRANSCRIPTION)
        .ok()
        .flatten()
        .map(|s| s == "true")
        .unwrap_or(false);

    if use_local {
        log_with_time!("🔧 [INIT] Loading local Whisper transcription from database");
        let model_str = handle.storage.get_setting(SETTING_LOCAL_WHISPER_MODEL)
            .ok()
            .flatten();
        let model = WhisperModel::all()
            .iter()
            .find(|m| {
                let (id, _) = m.model_id();
                Some(id) == model_str.as_deref()
            })
            .copied()
            .unwrap_or(WhisperModel::Quality);

        // Get models directory
        match crate::whisper_models::get_models_dir() {
            Ok(models_dir) => {
                handle.transcription = Arc::new(LocalWhisperTranscriptionProvider::new(
                    model,
                    models_dir
                ));
                log_with_time!("✅ [INIT] Using local Whisper model: {:?}", model);
            }
            Err(e) => {
                error!("Failed to get models directory: {}", e);
                log_with_time!("⚠️ [INIT] Failed to load local Whisper, falling back to remote: {}", e);
            }
        }
    } else {
        log_with_time!("☁️ [INIT] Using remote transcription (OpenAI Whisper API)");
    }

    debug!("FlowWhispr engine initialized");

    Box::into_raw(Box::new(handle))
}

/// Destroy the FlowWhispr engine and free resources
#[unsafe(no_mangle)]
pub extern "C" fn flowwispr_destroy(handle: *mut FlowWhisprHandle) {
    if !handle.is_null() {
        unsafe {
            drop(Box::from_raw(handle));
        }
        debug!("FlowWhispr engine destroyed");
    }
}

// ============ Audio ============

/// Start audio recording
/// Returns true on success
#[unsafe(no_mangle)]
pub extern "C" fn flowwispr_start_recording(handle: *mut FlowWhisprHandle) -> bool {
    let handle = unsafe { &*handle };

    // Capture the active Messages contact at recording START (before any focus changes)
    // This ensures we get the correct contact context for the transcription
    let current_app = handle.app_tracker.current_app();
    let is_messages = current_app
        .as_ref()
        .map(|ctx| {
            ctx.app_name.to_lowercase().contains("messages")
                || ctx.bundle_id.as_deref() == Some("com.apple.MobileSMS")
        })
        .unwrap_or(false);

    if is_messages {
        match MessagesDetector::get_active_contact() {
            Ok(Some(contact_name)) => {
                debug!(
                    "Captured Messages contact at recording start: {}",
                    contact_name
                );
                *handle.captured_contact.lock() = Some(contact_name);
            }
            Ok(None) => {
                debug!("Messages active but no conversation detected at recording start");
                *handle.captured_contact.lock() = None;
            }
            Err(e) => {
                debug!(
                    "Failed to capture Messages contact at recording start: {}",
                    e
                );
                *handle.captured_contact.lock() = None;
            }
        }
    } else {
        *handle.captured_contact.lock() = None;
    }

    let mut audio_lock = handle.audio.lock();

    // create new audio capture if needed
    if audio_lock.is_none() {
        match AudioCapture::new() {
            Ok(capture) => *audio_lock = Some(capture),
            Err(e) => {
                let message = format!("Failed to create audio capture: {e}");
                error!("{message}");
                set_last_error(handle, message);
                return false;
            }
        }
    }

    if let Some(ref mut capture) = *audio_lock {
        match capture.start() {
            Ok(_) => {
                clear_last_error(handle);
                true
            }
            Err(e) => {
                let message = format!("Failed to start recording: {e}");
                error!("{message}");
                set_last_error(handle, message);
                false
            }
        }
    } else {
        set_last_error(handle, "Audio capture unavailable");
        false
    }
}

/// Stop audio recording and get the duration
/// Returns duration in milliseconds, or 0 on failure
#[unsafe(no_mangle)]
pub extern "C" fn flowwispr_stop_recording(handle: *mut FlowWhisprHandle) -> u64 {
    let handle = unsafe { &*handle };
    let mut audio_lock = handle.audio.lock();

    if let Some(ref mut capture) = *audio_lock {
        let duration = capture.buffer_duration_ms();
        match capture.stop_stream() {
            Ok(_) => {
                clear_last_error(handle);
                duration
            }
            Err(e) => {
                let message = format!("Failed to stop recording: {e}");
                error!("{message}");
                set_last_error(handle, message);
                0
            }
        }
    } else {
        set_last_error(handle, "Audio capture unavailable");
        0
    }
}

/// Check if currently recording
#[unsafe(no_mangle)]
pub extern "C" fn flowwispr_is_recording(handle: *mut FlowWhisprHandle) -> bool {
    let handle = unsafe { &*handle };
    let audio_lock = handle.audio.lock();

    if let Some(ref capture) = *audio_lock {
        capture.state() == CaptureState::Recording
    } else {
        false
    }
}

/// Get current audio level (RMS amplitude) from the recording
/// Returns a value between 0.0 and 1.0, or 0.0 if not recording
#[unsafe(no_mangle)]
pub extern "C" fn flowwispr_get_audio_level(handle: *mut FlowWhisprHandle) -> f32 {
    let handle = unsafe { &*handle };
    let audio_lock = handle.audio.lock();

    if let Some(ref capture) = *audio_lock {
        if capture.state() == CaptureState::Recording {
            capture.current_audio_level()
        } else {
            0.0
        }
    } else {
        0.0
    }
}

// ============ Transcription ============

fn transcribe_with_audio(
    handle: &FlowWhisprHandle,
    audio_data: crate::AudioData,
    sample_rate: u32,
    app_name: Option<String>,
) -> crate::error::Result<String> {
    // Determine writing mode - use contact captured at recording start for Messages
    let mode = if let Some(ref name) = app_name {
        // Check if this is Messages.app
        if name.to_lowercase().contains("messages") || name == "com.apple.MobileSMS" {
            // Use the contact that was captured when recording started
            // This avoids race conditions where the window focus changes during recording
            let captured = handle.captured_contact.lock().clone();

            if let Some(contact_name) = captured {
                debug!("Using captured Messages contact: {}", contact_name);

                // Classify the contact
                let input = ContactInput {
                    name: contact_name.clone(),
                    organization: String::new(),
                };
                let category = handle.contact_classifier.classify(&input);
                let contact_mode = category.suggested_writing_mode();

                debug!(
                    "Contact '{}' classified as {:?}, using mode {:?}",
                    contact_name, category, contact_mode
                );

                // Record the interaction
                handle.contact_classifier.record_interaction(&contact_name);

                contact_mode
            } else {
                debug!("No contact was captured at recording start, using app default");
                let mut modes = handle.modes.lock();
                modes.get_mode_with_storage(name, &handle.storage)
            }
        } else {
            // Not Messages - use app-based mode
            let mut modes = handle.modes.lock();
            modes.get_mode_with_storage(name, &handle.storage)
        }
    } else {
        WritingMode::Casual
    };

    let transcription_provider = Arc::clone(&handle.transcription);
    let completion_provider = Arc::clone(&handle.completion);
    let app_context = handle.app_tracker.current_app();

    let provider_name = std::any::type_name_of_val(&*transcription_provider);
    log_with_time!(
        "🎧 [RUST/TRANSCRIBE] Starting speech-to-text transcription (provider: {})",
        provider_name
    );
    let transcription = handle.runtime.block_on(async {
        let request = TranscriptionRequest::new(audio_data, sample_rate);
        transcription_provider.transcribe(request).await
    })?;
    log_with_time!(
        "✅ [RUST/TRANSCRIBE] Speech-to-text completed - Raw text: {} chars",
        transcription.text.len()
    );

    let (text_with_shortcuts, triggered) = handle.shortcuts.process(&transcription.text);
    let (text_with_corrections, _applied) = handle.learning.apply_corrections(&text_with_shortcuts);

    log_with_time!("🤖 [RUST/AI] Starting AI completion with mode: {:?}", mode);
    let completion_result = handle.runtime.block_on(async {
        let mut completion_request = if let Some(name) = app_name.clone() {
            CompletionRequest::new(text_with_corrections.clone(), mode).with_app_context(name)
        } else {
            CompletionRequest::new(text_with_corrections.clone(), mode)
        };

        // If shortcuts were triggered, add strong instruction to preserve them
        if !triggered.is_empty() {
            let shortcuts_info: Vec<String> = triggered
                .iter()
                .map(|t| format!("\"{}\"", t.replacement))
                .collect();
            let preserve_instruction = format!(
                "\n\n=== CRITICAL INSTRUCTION ===\nThe input text contains voice shortcut expansions that MUST be output exactly as written, word-for-word, with NO modifications, rewording, or style changes whatsoever.\n\nShortcut text to preserve EXACTLY: {}\n\nDo NOT paraphrase, rephrase, or alter these phrases in any way. Copy them verbatim into your output.\n=== END CRITICAL INSTRUCTION ===",
                shortcuts_info.join(", ")
            );
            completion_request = completion_request.with_shortcut_preservation(preserve_instruction);
        }

        completion_provider.complete(completion_request).await
    });

    let processed_text = match completion_result {
        Ok(completion) => {
            log_with_time!(
                "✅ [RUST/AI] AI completion succeeded - Output: {} chars",
                completion.text.len()
            );
            completion.text
        }
        Err(err) => {
            log_with_time!(
                "❌ [RUST/AI] Completion failed, using corrected text: {}",
                err
            );
            text_with_corrections.clone()
        }
    };

    let mut record = Transcription::new(
        transcription.text,
        processed_text.clone(),
        transcription.confidence.unwrap_or(0.0),
        transcription.duration_ms,
    );
    if let Some(context) = app_context {
        record.app_context = Some(context);
    }
    if let Err(e) = handle.storage.save_transcription(&record) {
        error!("Failed to save transcription: {}", e);
    }

    let mut history = TranscriptionHistoryEntry::success(
        record.raw_text.clone(),
        processed_text.clone(),
        record.duration_ms,
    );
    history.app_context = record.app_context.clone();
    if let Err(e) = handle.storage.save_history_entry(&history) {
        error!("Failed to save transcription history: {}", e);
    }

    Ok(processed_text)
}

/// Transcribe the recorded audio and process it
/// Returns the processed text (caller must free with flowwispr_free_string)
/// Returns null on failure
#[unsafe(no_mangle)]
pub extern "C" fn flowwispr_transcribe(
    handle: *mut FlowWhisprHandle,
    app_name: *const c_char,
) -> *mut c_char {
    let handle = unsafe { &*handle };

    // get audio data
    let (audio_data, sample_rate) = {
        let mut audio_lock = handle.audio.lock();
        if let Some(ref mut capture) = *audio_lock {
            if let Err(e) = capture.stop_stream() {
                let message = format!("Failed to stop recording: {e}");
                error!("{message}");
                set_last_error(handle, message);
                return ptr::null_mut();
            }
            let sample_rate = capture.sample_rate();
            let audio_data = capture.take_buffered_audio();
            (audio_data, sample_rate)
        } else {
            set_last_error(handle, "No audio capture available");
            return ptr::null_mut();
        }
    };

    if audio_data.is_empty() {
        set_last_error(handle, "No audio captured");
        return ptr::null_mut();
    }

    // get app name
    let app = if !app_name.is_null() {
        unsafe { CStr::from_ptr(app_name) }
            .to_str()
            .ok()
            .map(String::from)
    } else {
        None
    };

    let duration_ms = estimate_duration_ms(audio_data.len(), sample_rate);
    *handle.last_audio.lock() = Some(audio_data.clone());
    *handle.last_audio_sample_rate.lock() = Some(sample_rate);
    let result = transcribe_with_audio(handle, audio_data, sample_rate, app);

    // Clear the captured contact after transcription (whether success or failure)
    *handle.captured_contact.lock() = None;

    match result {
        Ok(text) => {
            clear_last_error(handle);
            *handle.last_audio.lock() = None;
            *handle.last_audio_sample_rate.lock() = None;
            match CString::new(text) {
                Ok(cstr) => cstr.into_raw(),
                Err(_) => ptr::null_mut(),
            }
        }
        Err(e) => {
            let message = format!("Transcription failed: {e}");
            error!("{message}");
            set_last_error(handle, message.clone());
            let mut history = TranscriptionHistoryEntry::failure(message, duration_ms);
            history.app_context = handle.app_tracker.current_app();
            if let Err(e) = handle.storage.save_history_entry(&history) {
                error!("Failed to save transcription history: {}", e);
            }
            ptr::null_mut()
        }
    }
}

/// Retry the last transcription using cached audio
/// Returns processed text (caller must free with flowwispr_free_string), or null on failure
#[unsafe(no_mangle)]
pub extern "C" fn flowwispr_retry_last_transcription(
    handle: *mut FlowWhisprHandle,
    app_name: *const c_char,
) -> *mut c_char {
    let handle = unsafe { &*handle };
    let (audio_data, sample_rate) = {
        let last_audio = handle.last_audio.lock();
        let last_sample_rate = handle.last_audio_sample_rate.lock();
        match last_audio.as_ref() {
            Some(data) => (data.clone(), last_sample_rate.unwrap_or(16_000)),
            None => {
                set_last_error(handle, "No previous recording to retry");
                return ptr::null_mut();
            }
        }
    };

    let app = if !app_name.is_null() {
        unsafe { CStr::from_ptr(app_name) }
            .to_str()
            .ok()
            .map(String::from)
    } else {
        None
    };

    let duration_ms = estimate_duration_ms(audio_data.len(), sample_rate);
    let result = transcribe_with_audio(handle, audio_data, sample_rate, app);

    match result {
        Ok(text) => {
            clear_last_error(handle);
            *handle.last_audio.lock() = None;
            *handle.last_audio_sample_rate.lock() = None;
            match CString::new(text) {
                Ok(cstr) => cstr.into_raw(),
                Err(_) => ptr::null_mut(),
            }
        }
        Err(e) => {
            let message = format!("Transcription failed: {e}");
            error!("{message}");
            set_last_error(handle, message.clone());
            let mut history = TranscriptionHistoryEntry::failure(message, duration_ms);
            history.app_context = handle.app_tracker.current_app();
            if let Err(e) = handle.storage.save_history_entry(&history) {
                error!("Failed to save transcription history: {}", e);
            }
            ptr::null_mut()
        }
    }
}

// ============ Shortcuts ============

/// Add a voice shortcut
/// Returns true on success
#[unsafe(no_mangle)]
pub extern "C" fn flowwispr_add_shortcut(
    handle: *mut FlowWhisprHandle,
    trigger: *const c_char,
    replacement: *const c_char,
) -> bool {
    if trigger.is_null() || replacement.is_null() {
        return false;
    }

    let handle = unsafe { &*handle };

    let trigger_str = match unsafe { CStr::from_ptr(trigger) }.to_str() {
        Ok(s) => s.to_string(),
        Err(_) => return false,
    };

    let replacement_str = match unsafe { CStr::from_ptr(replacement) }.to_str() {
        Ok(s) => s.to_string(),
        Err(_) => return false,
    };

    let shortcut = Shortcut::new(trigger_str, replacement_str);

    if let Err(e) = handle.storage.save_shortcut(&shortcut) {
        error!("Failed to save shortcut: {}", e);
        return false;
    }

    handle.shortcuts.add_shortcut(shortcut);
    true
}

/// Remove a voice shortcut
/// Returns true on success
#[unsafe(no_mangle)]
pub extern "C" fn flowwispr_remove_shortcut(
    handle: *mut FlowWhisprHandle,
    trigger: *const c_char,
) -> bool {
    if trigger.is_null() {
        return false;
    }

    let handle = unsafe { &*handle };

    let trigger_str = match unsafe { CStr::from_ptr(trigger) }.to_str() {
        Ok(s) => s,
        Err(_) => return false,
    };

    handle.shortcuts.remove_shortcut(trigger_str);
    true
}

/// Get the number of shortcuts
#[unsafe(no_mangle)]
pub extern "C" fn flowwispr_shortcut_count(handle: *mut FlowWhisprHandle) -> usize {
    let handle = unsafe { &*handle };
    handle.shortcuts.count()
}

// ============ Writing Modes ============

/// Set the writing mode for an app
/// mode: 0 = Formal, 1 = Casual, 2 = VeryCasual, 3 = Excited
/// Returns true on success
#[unsafe(no_mangle)]
pub extern "C" fn flowwispr_set_app_mode(
    handle: *mut FlowWhisprHandle,
    app_name: *const c_char,
    mode: u8,
) -> bool {
    if app_name.is_null() {
        return false;
    }

    let handle = unsafe { &*handle };

    let app = match unsafe { CStr::from_ptr(app_name) }.to_str() {
        Ok(s) => s,
        Err(_) => return false,
    };

    let writing_mode = match mode {
        0 => WritingMode::Formal,
        1 => WritingMode::Casual,
        2 => WritingMode::VeryCasual,
        3 => WritingMode::Excited,
        _ => return false,
    };

    let mut modes = handle.modes.lock();
    if let Err(e) = modes.set_mode_with_storage(app, writing_mode, &handle.storage) {
        error!("Failed to save app mode: {}", e);
        return false;
    }

    true
}

/// Get the writing mode for an app
/// Returns: 0 = Formal, 1 = Casual, 2 = VeryCasual, 3 = Excited
#[unsafe(no_mangle)]
pub extern "C" fn flowwispr_get_app_mode(
    handle: *mut FlowWhisprHandle,
    app_name: *const c_char,
) -> u8 {
    if app_name.is_null() {
        return 1; // default to casual
    }

    let handle = unsafe { &*handle };

    let app = match unsafe { CStr::from_ptr(app_name) }.to_str() {
        Ok(s) => s,
        Err(_) => return 1,
    };

    let mut modes = handle.modes.lock();
    let mode = modes.get_mode_with_storage(app, &handle.storage);

    match mode {
        WritingMode::Formal => 0,
        WritingMode::Casual => 1,
        WritingMode::VeryCasual => 2,
        WritingMode::Excited => 3,
    }
}

// ============ Learning ============

/// Report a user edit to learn from
/// Returns true on success
#[unsafe(no_mangle)]
pub extern "C" fn flowwispr_learn_from_edit(
    handle: *mut FlowWhisprHandle,
    original: *const c_char,
    edited: *const c_char,
) -> bool {
    if original.is_null() || edited.is_null() {
        return false;
    }

    let handle = unsafe { &*handle };

    let original_str = match unsafe { CStr::from_ptr(original) }.to_str() {
        Ok(s) => s,
        Err(_) => return false,
    };

    let edited_str = match unsafe { CStr::from_ptr(edited) }.to_str() {
        Ok(s) => s,
        Err(_) => return false,
    };

    match handle
        .learning
        .learn_from_edit(original_str, edited_str, &handle.storage)
    {
        Ok(learned) => {
            debug!("Learned {} corrections from edit", learned.len());
            true
        }
        Err(e) => {
            error!("Failed to learn from edit: {}", e);
            false
        }
    }
}

/// Get the number of learned corrections
#[unsafe(no_mangle)]
pub extern "C" fn flowwispr_correction_count(handle: *mut FlowWhisprHandle) -> usize {
    let handle = unsafe { &*handle };
    handle.learning.cache_size()
}

// ============ Stats ============

/// Get total transcription time in minutes
#[unsafe(no_mangle)]
pub extern "C" fn flowwispr_total_transcription_minutes(handle: *mut FlowWhisprHandle) -> u64 {
    let handle = unsafe { &*handle };
    handle
        .storage
        .get_total_transcription_time_ms()
        .unwrap_or(0)
        / 60000
}

/// Get total transcription count
#[unsafe(no_mangle)]
pub extern "C" fn flowwispr_transcription_count(handle: *mut FlowWhisprHandle) -> u64 {
    let handle = unsafe { &*handle };
    handle.storage.get_transcription_count().unwrap_or(0)
}

// ============ Utilities ============

/// Free a string returned by flowwispr functions
#[unsafe(no_mangle)]
pub extern "C" fn flowwispr_free_string(s: *mut c_char) {
    if !s.is_null() {
        unsafe {
            drop(CString::from_raw(s));
        }
    }
}

/// Check if the transcription provider is configured
#[unsafe(no_mangle)]
pub extern "C" fn flowwispr_is_configured(handle: *mut FlowWhisprHandle) -> bool {
    let handle = unsafe { &*handle };
    handle.transcription.is_configured() && handle.completion.is_configured()
}

// ============ App Tracking ============

/// Set the currently active app (call from Swift when app switches)
/// Returns the suggested writing mode for the app
#[unsafe(no_mangle)]
pub extern "C" fn flowwispr_set_active_app(
    handle: *mut FlowWhisprHandle,
    app_name: *const c_char,
    bundle_id: *const c_char,
    window_title: *const c_char,
) -> u8 {
    if app_name.is_null() {
        return 1; // default to casual
    }

    let handle = unsafe { &*handle };

    let name = match unsafe { CStr::from_ptr(app_name) }.to_str() {
        Ok(s) => s.to_string(),
        Err(_) => return 1,
    };

    let bid = if bundle_id.is_null() {
        None
    } else {
        unsafe { CStr::from_ptr(bundle_id) }
            .to_str()
            .ok()
            .map(String::from)
    };

    let title = if window_title.is_null() {
        None
    } else {
        unsafe { CStr::from_ptr(window_title) }
            .to_str()
            .ok()
            .map(String::from)
    };

    let _context = handle.app_tracker.set_active_app(name, bid, title);

    // return suggested mode
    match handle.app_tracker.suggested_mode() {
        WritingMode::Formal => 0,
        WritingMode::Casual => 1,
        WritingMode::VeryCasual => 2,
        WritingMode::Excited => 3,
    }
}

/// Get the current app's category
/// Returns: 0=Email, 1=Slack, 2=Code, 3=Documents, 4=Social, 5=Browser, 6=Terminal, 7=Unknown
#[unsafe(no_mangle)]
pub extern "C" fn flowwispr_get_app_category(handle: *mut FlowWhisprHandle) -> u8 {
    let handle = unsafe { &*handle };

    use crate::types::AppCategory;
    match handle.app_tracker.current_category() {
        AppCategory::Email => 0,
        AppCategory::Slack => 1,
        AppCategory::Code => 2,
        AppCategory::Documents => 3,
        AppCategory::Social => 4,
        AppCategory::Browser => 5,
        AppCategory::Terminal => 6,
        AppCategory::Unknown => 7,
    }
}

/// Get current app name (caller must free with flowwispr_free_string)
#[unsafe(no_mangle)]
pub extern "C" fn flowwispr_get_current_app(handle: *mut FlowWhisprHandle) -> *mut c_char {
    let handle = unsafe { &*handle };

    match handle.app_tracker.current_app() {
        Some(ctx) => match CString::new(ctx.app_name) {
            Ok(cstr) => cstr.into_raw(),
            Err(_) => ptr::null_mut(),
        },
        None => ptr::null_mut(),
    }
}

// ============ Style Learning ============

/// Report edited text to learn user's style for current app
#[unsafe(no_mangle)]
pub extern "C" fn flowwispr_learn_style(
    handle: *mut FlowWhisprHandle,
    edited_text: *const c_char,
) -> bool {
    if edited_text.is_null() {
        return false;
    }

    let handle = unsafe { &*handle };

    let text = match unsafe { CStr::from_ptr(edited_text) }.to_str() {
        Ok(s) => s,
        Err(_) => return false,
    };

    let app_name = match handle.app_tracker.current_app() {
        Some(ctx) => ctx.app_name,
        None => return false,
    };

    let mut learner = handle.style_learner.lock();
    learner.observe_with_storage(&app_name, text, &handle.storage);

    true
}

/// Get suggested mode based on learned style for current app
/// Returns: 0=Formal, 1=Casual, 2=VeryCasual, 3=Excited, 255=no suggestion
#[unsafe(no_mangle)]
pub extern "C" fn flowwispr_get_style_suggestion(handle: *mut FlowWhisprHandle) -> u8 {
    let handle = unsafe { &*handle };

    let app_name = match handle.app_tracker.current_app() {
        Some(ctx) => ctx.app_name,
        None => return 255,
    };

    let learner = handle.style_learner.lock();
    match learner.suggest_mode(&app_name) {
        Some(suggestion) => match suggestion.suggested_mode {
            WritingMode::Formal => 0,
            WritingMode::Casual => 1,
            WritingMode::VeryCasual => 2,
            WritingMode::Excited => 3,
        },
        None => 255,
    }
}

// ============ Extended Stats ============

/// Get user stats as JSON (caller must free with flowwispr_free_string)
#[unsafe(no_mangle)]
pub extern "C" fn flowwispr_get_stats_json(handle: *mut FlowWhisprHandle) -> *mut c_char {
    let handle = unsafe { &*handle };

    let stats = serde_json::json!({
        "total_transcriptions": handle.storage.get_transcription_count().unwrap_or(0),
        "total_duration_ms": handle.storage.get_total_transcription_time_ms().unwrap_or(0),
        "total_words_dictated": handle.storage.get_total_words_dictated().unwrap_or(0),
        "shortcut_count": handle.shortcuts.count(),
        "correction_count": handle.learning.cache_size(),
    });

    match CString::new(stats.to_string()) {
        Ok(cstr) => cstr.into_raw(),
        Err(_) => ptr::null_mut(),
    }
}

/// Get recent transcriptions as JSON (caller must free with flowwispr_free_string)
#[unsafe(no_mangle)]
pub extern "C" fn flowwispr_get_recent_transcriptions_json(
    handle: *mut FlowWhisprHandle,
    limit: usize,
) -> *mut c_char {
    let handle = unsafe { &*handle };
    let transcriptions = match handle.storage.get_recent_history(limit) {
        Ok(items) => items,
        Err(e) => {
            error!("Failed to load transcriptions: {}", e);
            return ptr::null_mut();
        }
    };

    let summaries: Vec<TranscriptionSummary> = transcriptions
        .into_iter()
        .map(|item| TranscriptionSummary {
            id: item.id.to_string(),
            status: item.status,
            text: item.text,
            raw_text: item.raw_text,
            error: item.error,
            duration_ms: item.duration_ms,
            created_at: item.created_at.to_rfc3339(),
            app_name: item.app_context.map(|ctx| ctx.app_name),
        })
        .collect();

    let json = match serde_json::to_string(&summaries) {
        Ok(value) => value,
        Err(e) => {
            error!("Failed to serialize transcriptions: {}", e);
            return ptr::null_mut();
        }
    };

    match CString::new(json) {
        Ok(cstr) => cstr.into_raw(),
        Err(_) => ptr::null_mut(),
    }
}

/// Get the last error message (caller must free with flowwispr_free_string)
#[unsafe(no_mangle)]
pub extern "C" fn flowwispr_get_last_error(handle: *mut FlowWhisprHandle) -> *mut c_char {
    let handle = unsafe { &*handle };
    let message = handle.last_error.lock().clone();
    match message {
        Some(text) => match CString::new(text) {
            Ok(cstr) => cstr.into_raw(),
            Err(_) => ptr::null_mut(),
        },
        None => ptr::null_mut(),
    }
}

// ============ Provider Configuration ============

/// Switch completion provider (loads API key from database)
/// provider: 0 = OpenAI, 1 = Gemini, 2 = OpenRouter
/// Returns true if provider was switched successfully
#[unsafe(no_mangle)]
pub extern "C" fn flowwispr_switch_completion_provider(
    handle: *mut FlowWhisprHandle,
    provider: u8,
) -> bool {
    let handle = unsafe { &mut *handle };

    let (setting_key, provider_name) = match provider {
        0 => (SETTING_OPENAI_API_KEY, "openai"),
        1 => (SETTING_GEMINI_API_KEY, "gemini"),
        2 => (SETTING_OPENROUTER_API_KEY, "openrouter"),
        _ => {
            set_last_error(handle, "Invalid provider");
            return false;
        }
    };

    // Load the API key from the database
    let api_key = match handle.storage.get_setting(setting_key) {
        Ok(Some(key)) if !key.is_empty() => key,
        Ok(Some(_)) | Ok(None) => {
            let message = format!("No API key configured for {}", provider_name);
            error!("{message}");
            set_last_error(handle, message);
            return false;
        }
        Err(e) => {
            let message = format!("Failed to load API key for {}: {}", provider_name, e);
            error!("{message}");
            set_last_error(handle, message);
            return false;
        }
    };

    // Save the provider preference
    if let Err(e) = handle
        .storage
        .set_setting(SETTING_COMPLETION_PROVIDER, provider_name)
    {
        let message = format!("Failed to save completion provider: {e}");
        error!("{message}");
        set_last_error(handle, message);
        return false;
    }

    // Initialize the provider
    match provider {
        0 => {
            handle.transcription =
                Arc::new(OpenAITranscriptionProvider::new(Some(api_key.clone())));
            handle.completion = Arc::new(OpenAICompletionProvider::new(Some(api_key)));
            debug!("Switched completion provider to OpenAI");
        }
        1 => {
            handle.transcription =
                Arc::new(GeminiTranscriptionProvider::new(Some(api_key.clone())));
            handle.completion = Arc::new(GeminiCompletionProvider::new(Some(api_key)));
            debug!("Switched completion provider to Gemini");
        }
        2 => {
            // OpenRouter only handles completion, keep existing transcription provider
            handle.completion = Arc::new(OpenRouterCompletionProvider::new(Some(api_key)));
            debug!("Switched completion provider to OpenRouter");
        }
        _ => unreachable!(),
    }

    clear_last_error(handle);
    true
}

/// Set completion provider with API key (saves both)
/// provider: 0 = OpenAI, 1 = Gemini, 2 = OpenRouter
/// api_key: The API key for the provider
#[unsafe(no_mangle)]
pub extern "C" fn flowwispr_set_completion_provider(
    handle: *mut FlowWhisprHandle,
    provider: u8,
    api_key: *const c_char,
) -> bool {
    if api_key.is_null() {
        return false;
    }

    let handle = unsafe { &mut *handle };

    let key = match unsafe { CStr::from_ptr(api_key) }.to_str() {
        Ok(s) => s.to_string(),
        Err(_) => return false,
    };

    match provider {
        0 => {
            if let Err(e) = handle.storage.set_setting(SETTING_OPENAI_API_KEY, &key) {
                let message = format!("Failed to save OpenAI API key: {e}");
                error!("{message}");
                set_last_error(handle, message);
                return false;
            }
            if let Err(e) = handle
                .storage
                .set_setting(SETTING_COMPLETION_PROVIDER, "openai")
            {
                let message = format!("Failed to save completion provider: {e}");
                error!("{message}");
                set_last_error(handle, message);
                return false;
            }
            handle.transcription = Arc::new(OpenAITranscriptionProvider::new(Some(key.clone())));
            handle.completion = Arc::new(OpenAICompletionProvider::new(Some(key)));
            debug!("Set completion provider to OpenAI");
        }
        1 => {
            if let Err(e) = handle.storage.set_setting(SETTING_GEMINI_API_KEY, &key) {
                let message = format!("Failed to save Gemini API key: {e}");
                error!("{message}");
                set_last_error(handle, message);
                return false;
            }
            if let Err(e) = handle
                .storage
                .set_setting(SETTING_COMPLETION_PROVIDER, "gemini")
            {
                let message = format!("Failed to save completion provider: {e}");
                error!("{message}");
                set_last_error(handle, message);
                return false;
            }
            handle.transcription = Arc::new(GeminiTranscriptionProvider::new(Some(key.clone())));
            handle.completion = Arc::new(GeminiCompletionProvider::new(Some(key)));
            debug!("Set completion provider to Gemini");
        }
        2 => {
            if let Err(e) = handle.storage.set_setting(SETTING_OPENROUTER_API_KEY, &key) {
                let message = format!("Failed to save OpenRouter API key: {e}");
                error!("{message}");
                set_last_error(handle, message);
                return false;
            }
            if let Err(e) = handle
                .storage
                .set_setting(SETTING_COMPLETION_PROVIDER, "openrouter")
            {
                let message = format!("Failed to save completion provider: {e}");
                error!("{message}");
                set_last_error(handle, message);
                return false;
            }
            // OpenRouter only handles completion, keep transcription provider as-is
            handle.completion = Arc::new(OpenRouterCompletionProvider::new(Some(key)));
            debug!("Set completion provider to OpenRouter");
        }
        _ => return false,
    }

    true
}

/// Get the current completion provider name
/// Returns: 0 = OpenAI, 1 = Gemini, 2 = OpenRouter, 255 = Unknown
#[unsafe(no_mangle)]
pub extern "C" fn flowwispr_get_completion_provider(handle: *mut FlowWhisprHandle) -> u8 {
    let handle = unsafe { &*handle };

    match handle.completion.name() {
        "OpenAI GPT" => 0,
        "Gemini" => 1,
        "OpenRouter" => 2,
        _ => 255,
    }
}

/// Helper function to mask an API key for display
/// Shows the prefix (e.g., "sk-" or "AI") and masks the rest with dots
fn mask_api_key(key: &str) -> String {
    if key.is_empty() {
        return String::new();
    }

    // For OpenAI keys (sk-...)
    if key.starts_with("sk-") {
        return format!("sk-••••••••");
    }
    // For Gemini keys (AI...)
    if key.starts_with("AI") {
        return format!("AI••••••••");
    }
    // For other keys, just show dots
    "••••••••".to_string()
}

/// Get API key for a specific provider in masked form
/// provider: 0 = OpenAI, 1 = Gemini, 2 = OpenRouter
/// Returns null if no key is set, or a masked version like "sk-••••••••"
/// Caller must free the returned string with flowwispr_free_string
#[unsafe(no_mangle)]
pub extern "C" fn flowwispr_get_api_key(
    handle: *mut FlowWhisprHandle,
    provider: u8,
) -> *mut c_char {
    let handle = unsafe { &*handle };

    let setting_key = match provider {
        0 => SETTING_OPENAI_API_KEY,
        1 => SETTING_GEMINI_API_KEY,
        2 => SETTING_OPENROUTER_API_KEY,
        _ => return ptr::null_mut(),
    };

    match handle.storage.get_setting(setting_key) {
        Ok(Some(key)) => {
            let masked = mask_api_key(&key);
            CString::new(masked).unwrap().into_raw()
        }
        _ => ptr::null_mut(),
    }
}

/// Set transcription mode (local or remote)
/// use_local: true for local Whisper, false for cloud provider
/// whisper_model: Model selection (only used when use_local = true)
///   0 = Turbo (~15MB) - quantized, ultra-fast, lowest memory
///   1 = Fast (~39MB) - fast, lower accuracy
///   2 = Balanced (~142MB) - good speed/accuracy balance
///   3 = Quality (~400MB) - great accuracy, still fast [recommended]
///   4 = Best (~750MB) - best quality available
/// Returns true on success, false on failure
#[unsafe(no_mangle)]
pub extern "C" fn flowwispr_set_transcription_mode(
    handle: *mut FlowWhisprHandle,
    use_local: bool,
    whisper_model: u8,
) -> bool {
    let handle = unsafe { &mut *handle };

    // Save setting to database
    if let Err(e) = handle.storage.set_setting(
        SETTING_USE_LOCAL_TRANSCRIPTION,
        if use_local { "true" } else { "false" },
    ) {
        let message = format!("Failed to save transcription mode: {}", e);
        error!("{}", message);
        set_last_error(handle, message);
        return false;
    }

    if use_local {
        // Local Whisper transcription
        let model = match whisper_model {
            0 => WhisperModel::Turbo,
            1 => WhisperModel::Fast,
            2 => WhisperModel::Balanced,
            3 => WhisperModel::Quality,
            4 => WhisperModel::Best,
            _ => {
                set_last_error(handle, "Invalid Whisper model selection (0-4)");
                return false;
            }
        };

        // Save model choice using canonical name
        let model_name = model.as_str();
        if let Err(e) = handle
            .storage
            .set_setting(SETTING_LOCAL_WHISPER_MODEL, model_name)
        {
            let message = format!("Failed to save Whisper model: {}", e);
            error!("{}", message);
            set_last_error(handle, message);
            return false;
        }

        // Get models directory
        let models_dir = match crate::whisper_models::get_models_dir() {
            Ok(dir) => dir,
            Err(e) => {
                let message = format!("Failed to get models directory: {}", e);
                error!("{}", message);
                set_last_error(handle, message);
                return false;
            }
        };

        // Check if model files already exist
        let files_exist = check_model_files_exist(model, &models_dir);

        // Set loading flag if this will require downloading
        if !files_exist {
            handle.is_model_loading.store(true, Ordering::SeqCst);
            debug!(
                "Model files not found, will download (~{}MB)",
                model.size_mb()
            );
        }

        // Create provider
        let provider = Arc::new(LocalWhisperTranscriptionProvider::new(model, models_dir));

        // Trigger model download/load asynchronously
        let provider_clone = Arc::clone(&provider);
        let loading_flag = Arc::clone(&handle.is_model_loading);
        let should_clear_flag = !files_exist;

        handle.runtime.spawn(async move {
            if let Err(e) = provider_clone.load_model().await {
                error!("Failed to load Whisper model: {}", e);
            }
            // Clear loading flag when done (only if we set it)
            if should_clear_flag {
                loading_flag.store(false, Ordering::SeqCst);
                debug!("Model loading completed");
            }
        });

        handle.transcription = provider;
        debug!("Enabled local Whisper transcription with {:?} model", model);
    } else {
        // Remote transcription - use the current completion provider's transcription
        let provider_name = match handle.storage.get_setting(SETTING_COMPLETION_PROVIDER) {
            Ok(Some(name)) => name,
            _ => "openai".to_string(), // default to OpenAI
        };

        match provider_name.as_str() {
            "openai" => {
                if let Ok(Some(key)) = handle.storage.get_setting(SETTING_OPENAI_API_KEY) {
                    handle.transcription = Arc::new(OpenAITranscriptionProvider::new(Some(key)));
                    debug!("Enabled OpenAI remote transcription");
                }
            }
            "gemini" => {
                if let Ok(Some(key)) = handle.storage.get_setting(SETTING_GEMINI_API_KEY) {
                    handle.transcription = Arc::new(GeminiTranscriptionProvider::new(Some(key)));
                    debug!("Enabled Gemini remote transcription");
                }
            }
            _ => {
                // Default to OpenAI
                if let Ok(Some(key)) = handle.storage.get_setting(SETTING_OPENAI_API_KEY) {
                    handle.transcription = Arc::new(OpenAITranscriptionProvider::new(Some(key)));
                    debug!("Enabled OpenAI remote transcription (default)");
                }
            }
        }
    }

    true
}

/// Get current transcription mode settings
/// Returns use_local flag and whisper_model (0-4) via out parameters
/// Returns false on database error, true on success
#[unsafe(no_mangle)]
pub extern "C" fn flowwispr_get_transcription_mode(
    handle: *mut FlowWhisprHandle,
    out_use_local: *mut bool,
    out_whisper_model: *mut u8,
) -> bool {
    let handle = unsafe { &*handle };

    // Read use_local setting from database
    let use_local = match handle.storage.get_setting(SETTING_USE_LOCAL_TRANSCRIPTION) {
        Ok(Some(value)) => value == "true",
        Ok(None) => false, // Default to remote if not set
        Err(e) => {
            error!("Failed to read transcription mode: {}", e);
            return false;
        }
    };

    // Read whisper model setting from database
    let whisper_model = if use_local {
        match handle.storage.get_setting(SETTING_LOCAL_WHISPER_MODEL) {
            Ok(Some(model_str)) => {
                // Convert model name to enum
                let model = WhisperModel::all()
                    .iter()
                    .find(|m| m.as_str() == model_str)
                    .copied()
                    .unwrap_or(WhisperModel::Balanced); // Default to Balanced

                // Convert enum to u8
                match model {
                    WhisperModel::Turbo => 0,
                    WhisperModel::Fast => 1,
                    WhisperModel::Balanced => 2,
                    WhisperModel::Quality => 3,
                    WhisperModel::Best => 4,
                }
            }
            Ok(None) => 1, // Default to Balanced
            Err(e) => {
                error!("Failed to read Whisper model: {}", e);
                return false;
            }
        }
    } else {
        1 // Default value when using remote transcription
    };

    // Write to out parameters
    unsafe {
        *out_use_local = use_local;
        *out_whisper_model = whisper_model;
    }

    true
}

/// Check if a Whisper model is currently being downloaded/initialized
/// Returns true if model download/initialization is in progress
#[unsafe(no_mangle)]
pub extern "C" fn flowwispr_is_model_loading(handle: *mut FlowWhisprHandle) -> bool {
    let handle = unsafe { &*handle };
    handle.is_model_loading.load(Ordering::SeqCst)
}

/// Legacy function - prefer flowwispr_set_transcription_mode
/// Enable local Whisper transcription with Metal + Accelerate acceleration
/// model: 0=Turbo, 1=Fast, 2=Balanced, 3=Quality, 4=Best
/// Returns true on success, false on failure
#[unsafe(no_mangle)]
pub extern "C" fn flowwispr_enable_local_whisper(handle: *mut FlowWhisprHandle, model: u8) -> bool {
    flowwispr_set_transcription_mode(handle, true, model)
}

/// Get available Whisper models as JSON (caller must free with flowwispr_free_string)
/// Returns JSON array with model info including id, name, description, size, and flags
#[unsafe(no_mangle)]
pub extern "C" fn flowwispr_get_whisper_models_json() -> *mut c_char {
    let models: Vec<serde_json::Value> = WhisperModel::all()
        .iter()
        .enumerate()
        .map(|(id, model)| {
            serde_json::json!({
                "id": id,
                "name": model.as_str(),
                "description": model.description(),
                "size_mb": model.size_mb(),
                "is_quantized": model.is_quantized(),
                "is_distilled": model.is_distilled(),
            })
        })
        .collect();

    let json = serde_json::to_string(&models).unwrap_or_else(|_| "[]".to_string());
    CString::new(json).unwrap().into_raw()
}

/// Get all shortcuts as JSON (caller must free with flowwispr_free_string)
#[unsafe(no_mangle)]
pub extern "C" fn flowwispr_get_shortcuts_json(handle: *mut FlowWhisprHandle) -> *mut c_char {
    let handle = unsafe { &*handle };

    let shortcuts: Vec<serde_json::Value> = handle
        .shortcuts
        .get_all()
        .iter()
        .map(|s| {
            serde_json::json!({
                "trigger": s.trigger,
                "replacement": s.replacement,
                "use_count": s.use_count,
                "enabled": s.enabled,
            })
        })
        .collect();

    match CString::new(serde_json::to_string(&shortcuts).unwrap_or_default()) {
        Ok(cstr) => cstr.into_raw(),
        Err(_) => ptr::null_mut(),
    }
}

// ============ Contact Categorization ============

/// Get active contact name from Messages.app window
/// Returns C string with contact name, or null if not available
/// Caller must free with flowwispr_free_string
#[unsafe(no_mangle)]
pub extern "C" fn flowwispr_get_active_messages_contact(
    handle: *mut FlowWhisprHandle,
) -> *mut c_char {
    let handle = unsafe { &*handle };
    clear_last_error(handle);

    match MessagesDetector::get_active_contact() {
        Ok(Some(name)) => match CString::new(name) {
            Ok(cstr) => cstr.into_raw(),
            Err(_) => {
                set_last_error(handle, "Invalid UTF-8 in contact name");
                ptr::null_mut()
            }
        },
        Ok(None) => ptr::null_mut(),
        Err(e) => {
            set_last_error(handle, format!("Failed to get active contact: {}", e));
            ptr::null_mut()
        }
    }
}

/// Classify a contact given name and organization
/// Returns JSON string with category
/// Caller must free with flowwispr_free_string
#[unsafe(no_mangle)]
pub extern "C" fn flowwispr_classify_contact(
    handle: *mut FlowWhisprHandle,
    name: *const c_char,
    organization: *const c_char,
) -> *mut c_char {
    let handle = unsafe { &*handle };
    clear_last_error(handle);

    let name_str = unsafe {
        if name.is_null() {
            set_last_error(handle, "Name cannot be null");
            return ptr::null_mut();
        }
        match CStr::from_ptr(name).to_str() {
            Ok(s) => s,
            Err(_) => {
                set_last_error(handle, "Invalid UTF-8 in name");
                return ptr::null_mut();
            }
        }
    };

    let org_str = unsafe {
        if organization.is_null() {
            String::new()
        } else {
            match CStr::from_ptr(organization).to_str() {
                Ok(s) => s.to_string(),
                Err(_) => String::new(),
            }
        }
    };

    let input = ContactInput {
        name: name_str.to_string(),
        organization: org_str,
    };

    let category = handle.contact_classifier.classify(&input);

    let result = serde_json::json!({
        "name": name_str,
        "category": category,
    });

    match CString::new(result.to_string()) {
        Ok(cstr) => cstr.into_raw(),
        Err(_) => {
            set_last_error(handle, "Failed to serialize result");
            ptr::null_mut()
        }
    }
}

/// Classify multiple contacts from JSON array
/// Input format: [{"name": "...", "organization": "..."}]
/// Output format: {"ContactName": "category", ...}
/// Caller must free with flowwispr_free_string
#[unsafe(no_mangle)]
pub extern "C" fn flowwispr_classify_contacts_batch(
    handle: *mut FlowWhisprHandle,
    contacts_json: *const c_char,
) -> *mut c_char {
    let handle = unsafe { &*handle };
    clear_last_error(handle);

    let json_str = unsafe {
        if contacts_json.is_null() {
            set_last_error(handle, "JSON cannot be null");
            return ptr::null_mut();
        }
        match CStr::from_ptr(contacts_json).to_str() {
            Ok(s) => s,
            Err(_) => {
                set_last_error(handle, "Invalid UTF-8 in JSON");
                return ptr::null_mut();
            }
        }
    };

    let inputs: Vec<ContactInput> = match serde_json::from_str(json_str) {
        Ok(i) => i,
        Err(e) => {
            set_last_error(handle, format!("Invalid JSON: {}", e));
            return ptr::null_mut();
        }
    };

    let result_json = handle.contact_classifier.classify_batch_json(&inputs);

    match CString::new(result_json) {
        Ok(cstr) => cstr.into_raw(),
        Err(_) => {
            set_last_error(handle, "Failed to create result string");
            ptr::null_mut()
        }
    }
}

/// Record interaction with a contact (updates frequency)
#[unsafe(no_mangle)]
pub extern "C" fn flowwispr_record_contact_interaction(
    handle: *mut FlowWhisprHandle,
    name: *const c_char,
) {
    let handle = unsafe { &*handle };
    clear_last_error(handle);

    let name_str = unsafe {
        if name.is_null() {
            return;
        }
        match CStr::from_ptr(name).to_str() {
            Ok(s) => s,
            Err(_) => return,
        }
    };

    handle.contact_classifier.record_interaction(name_str);
}

/// Get frequent contacts as JSON array
/// Returns: [{"name": "...", "category": "...", "frequency": N}, ...]
/// Caller must free with flowwispr_free_string
#[unsafe(no_mangle)]
pub extern "C" fn flowwispr_get_frequent_contacts(
    handle: *mut FlowWhisprHandle,
    limit: u32,
) -> *mut c_char {
    let handle = unsafe { &*handle };
    clear_last_error(handle);

    let contacts = handle
        .contact_classifier
        .get_frequent_contacts(limit as usize);

    let result: Vec<serde_json::Value> = contacts
        .iter()
        .map(|c| {
            serde_json::json!({
                "name": c.name,
                "category": c.category,
                "frequency": c.frequency,
                "organization": c.organization,
            })
        })
        .collect();

    match CString::new(serde_json::to_string(&result).unwrap_or_default()) {
        Ok(cstr) => cstr.into_raw(),
        Err(_) => {
            set_last_error(handle, "Failed to serialize contacts");
            ptr::null_mut()
        }
    }
}

/// Get suggested writing mode for a contact category
/// Returns: 0=Formal, 1=Casual, 2=VeryCasual, 3=Excited
#[unsafe(no_mangle)]
pub extern "C" fn flowwispr_get_writing_mode_for_category(
    handle: *mut FlowWhisprHandle,
    category: u32,
) -> u32 {
    let handle = unsafe { &*handle };
    clear_last_error(handle);

    use crate::types::ContactCategory;

    let contact_category = match category {
        0 => ContactCategory::Professional,
        1 => ContactCategory::CloseFamily,
        2 => ContactCategory::CasualPeer,
        3 => ContactCategory::Partner,
        4 => ContactCategory::FormalNeutral,
        _ => ContactCategory::FormalNeutral,
    };

    let writing_mode = contact_category.suggested_writing_mode();

    match writing_mode {
        WritingMode::Formal => 0,
        WritingMode::Casual => 1,
        WritingMode::VeryCasual => 2,
        WritingMode::Excited => 3,
    }
}
