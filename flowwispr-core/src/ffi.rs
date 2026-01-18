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

use parking_lot::Mutex;
use serde::Serialize;
use tokio::runtime::Runtime;
use tracing::{debug, error};

use crate::apps::AppTracker;
use crate::audio::{AudioCapture, CaptureState};
use crate::learning::LearningEngine;
use crate::modes::{StyleLearner, WritingMode, WritingModeEngine};
use crate::providers::{
    CompletionProvider, CompletionRequest, GeminiCompletionProvider, GeminiTranscriptionProvider,
    LocalWhisperTranscriptionProvider, OpenAICompletionProvider, OpenAITranscriptionProvider,
    OpenRouterCompletionProvider, TranscriptionProvider, TranscriptionRequest, WhisperModel,
};
use crate::shortcuts::ShortcutsEngine;
use crate::storage::{
    SETTING_COMPLETION_PROVIDER, SETTING_GEMINI_API_KEY, SETTING_OPENAI_API_KEY,
    SETTING_OPENROUTER_API_KEY, Storage,
};
use crate::types::{Shortcut, Transcription, TranscriptionHistoryEntry, TranscriptionStatus};

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
}

#[derive(Serialize)]
struct TranscriptionSummary {
    id: String,
    status: TranscriptionStatus,
    text: String,
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

fn clear_last_error(handle: &FlowWhisprHandle) {
    *handle.last_error.lock() = None;
}

fn estimate_duration_ms(bytes: usize, sample_rate: u32) -> u64 {
    let samples = bytes / 2;
    (samples as u64 * 1000) / sample_rate as u64
}

fn load_persisted_configuration(handle: &mut FlowWhisprHandle) {
    let openai_key = match handle.storage.get_setting(SETTING_OPENAI_API_KEY) {
        Ok(value) => value,
        Err(e) => {
            error!("Failed to load OpenAI key: {}", e);
            None
        }
    };

    handle.transcription = Arc::new(OpenAITranscriptionProvider::new(openai_key.clone()));
    handle.completion = Arc::new(OpenAICompletionProvider::new(openai_key));
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
    };

    load_persisted_configuration(&mut handle);

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

// ============ Transcription ============

fn transcribe_with_audio(
    handle: &FlowWhisprHandle,
    audio_data: crate::AudioData,
    sample_rate: u32,
    app_name: Option<String>,
) -> crate::error::Result<String> {
    let mode = if let Some(ref name) = app_name {
        let mut modes = handle.modes.lock();
        modes.get_mode_with_storage(name, &handle.storage)
    } else {
        WritingMode::Casual
    };

    let transcription_provider = Arc::clone(&handle.transcription);
    let completion_provider = Arc::clone(&handle.completion);
    let app_context = handle.app_tracker.current_app();

    let transcription = handle.runtime.block_on(async {
        let request = TranscriptionRequest::new(audio_data, sample_rate);
        transcription_provider.transcribe(request).await
    })?;

    let (text_with_shortcuts, _triggered) = handle.shortcuts.process(&transcription.text);
    let (text_with_corrections, _applied) = handle.learning.apply_corrections(&text_with_shortcuts);

    let completion_result = handle.runtime.block_on(async {
        let completion_request = if let Some(name) = app_name.clone() {
            CompletionRequest::new(text_with_corrections.clone(), mode).with_app_context(name)
        } else {
            CompletionRequest::new(text_with_corrections.clone(), mode)
        };
        completion_provider.complete(completion_request).await
    });

    let processed_text = match completion_result {
        Ok(completion) => completion.text,
        Err(err) => {
            error!("Completion failed, using corrected text: {}", err);
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

/// Set the OpenAI API key
#[unsafe(no_mangle)]
pub extern "C" fn flowwispr_set_api_key(
    handle: *mut FlowWhisprHandle,
    api_key: *const c_char,
) -> bool {
    if api_key.is_null() {
        return false;
    }

    let handle = unsafe { &mut *handle };

    let key = match unsafe { CStr::from_ptr(api_key) }.to_str() {
        Ok(s) => s.trim().to_string(),
        Err(_) => {
            set_last_error(handle, "Invalid API key");
            return false;
        }
    };

    if key.is_empty() {
        set_last_error(handle, "API key is empty");
        return false;
    }

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

    // reinitialize providers with new key
    handle.transcription = Arc::new(OpenAITranscriptionProvider::new(Some(key.clone())));
    handle.completion = Arc::new(OpenAICompletionProvider::new(Some(key)));

    clear_last_error(handle);
    true
}

/// Set the Gemini API key
#[unsafe(no_mangle)]
pub extern "C" fn flowwispr_set_gemini_api_key(
    handle: *mut FlowWhisprHandle,
    api_key: *const c_char,
) -> bool {
    if api_key.is_null() {
        return false;
    }

    let handle = unsafe { &mut *handle };

    let key = match unsafe { CStr::from_ptr(api_key) }.to_str() {
        Ok(s) => s.trim().to_string(),
        Err(_) => {
            set_last_error(handle, "Invalid API key");
            return false;
        }
    };

    if key.is_empty() {
        set_last_error(handle, "API key is empty");
        return false;
    }

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

    // reinitialize providers with new key
    handle.transcription = Arc::new(GeminiTranscriptionProvider::new(Some(key.clone())));
    handle.completion = Arc::new(GeminiCompletionProvider::new(Some(key)));

    clear_last_error(handle);
    true
}

/// Set the OpenRouter API key
#[unsafe(no_mangle)]
pub extern "C" fn flowwispr_set_openrouter_api_key(
    handle: *mut FlowWhisprHandle,
    api_key: *const c_char,
) -> bool {
    if api_key.is_null() {
        return false;
    }

    let handle = unsafe { &mut *handle };

    let key = match unsafe { CStr::from_ptr(api_key) }.to_str() {
        Ok(s) => s.trim().to_string(),
        Err(_) => {
            set_last_error(handle, "Invalid API key");
            return false;
        }
    };

    if key.is_empty() {
        set_last_error(handle, "API key is empty");
        return false;
    }

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

    clear_last_error(handle);
    true
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

/// Set completion provider
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

/// Enable local Whisper transcription with Metal acceleration
/// model: 0 = Tiny, 1 = Base, 2 = Small
/// Returns true on success, false on failure
#[unsafe(no_mangle)]
pub extern "C" fn flowwispr_enable_local_whisper(
    handle: *mut FlowWhisprHandle,
    model: u8,
) -> bool {
    let handle = unsafe { &mut *handle };

    let whisper_model = match model {
        0 => WhisperModel::Tiny,
        1 => WhisperModel::Base,
        2 => WhisperModel::Small,
        _ => {
            set_last_error(handle, "Invalid Whisper model selection");
            return false;
        }
    };

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

    // Create provider
    let provider = Arc::new(LocalWhisperTranscriptionProvider::new(whisper_model, models_dir));

    // Trigger model download/load asynchronously
    let provider_clone = Arc::clone(&provider);
    handle.runtime.spawn(async move {
        if let Err(e) = provider_clone.load_model().await {
            error!("Failed to load Whisper model: {}", e);
        }
    });

    handle.transcription = provider;
    debug!("Enabled local Whisper transcription with {:?} model", whisper_model);

    true
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
