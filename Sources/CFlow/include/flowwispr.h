//
// flowwispr.h
// Flow C Interface
//
// Auto-generated header for the Flow Rust FFI layer.
// This header provides C-compatible function declarations for Swift interop.
//

#ifndef FLOWWHISPR_H
#define FLOWWHISPR_H

#include <stddef.h>
#include <stdint.h>
#include <stdbool.h>

#ifdef __cplusplus
extern "C" {
#endif

/// Opaque handle to the Flow engine
typedef struct FlowHandle FlowHandle;

// ============ Lifecycle ============

/// Initialize the Flow engine
/// @param db_path Path to the SQLite database file, or NULL for default location
/// @return Opaque handle to the engine, or NULL on failure
FlowHandle* flow_init(const char* db_path);

/// Destroy the Flow engine and free resources
/// @param handle Handle returned by flow_init
void flow_destroy(FlowHandle* handle);

// ============ Audio ============

/// Start audio recording
/// @param handle Engine handle
/// @return true on success
bool flow_start_recording(FlowHandle* handle);

/// Stop audio recording and get the duration
/// @param handle Engine handle
/// @return Duration in milliseconds, or 0 on failure
uint64_t flow_stop_recording(FlowHandle* handle);

/// Check if currently recording
/// @param handle Engine handle
/// @return true if recording
bool flow_is_recording(FlowHandle* handle);

/// Get current audio level (RMS amplitude) from the recording
/// @param handle Engine handle
/// @return Value between 0.0 and 1.0, or 0.0 if not recording
float flow_get_audio_level(FlowHandle* handle);

// ============ Transcription ============

/// Transcribe the recorded audio and process it
/// @param handle Engine handle
/// @param app_name Name of the current app (for mode selection), or NULL
/// @return Processed text (caller must free with flow_free_string), or NULL on failure
char* flow_transcribe(FlowHandle* handle, const char* app_name);

/// Retry the last transcription using cached audio
/// @param handle Engine handle
/// @param app_name Name of the current app (for mode selection), or NULL
/// @return Processed text (caller must free with flow_free_string), or NULL on failure
char* flow_retry_last_transcription(FlowHandle* handle, const char* app_name);

// ============ Shortcuts ============

/// Add a voice shortcut
/// @param handle Engine handle
/// @param trigger Trigger phrase
/// @param replacement Replacement text
/// @return true on success
bool flow_add_shortcut(FlowHandle* handle, const char* trigger, const char* replacement);

/// Remove a voice shortcut
/// @param handle Engine handle
/// @param trigger Trigger phrase to remove
/// @return true on success
bool flow_remove_shortcut(FlowHandle* handle, const char* trigger);

/// Get the number of shortcuts
/// @param handle Engine handle
/// @return Number of shortcuts
size_t flow_shortcut_count(FlowHandle* handle);

// ============ Writing Modes ============

/// Writing mode constants
/// 0 = Formal, 1 = Casual, 2 = VeryCasual, 3 = Excited

/// Set the writing mode for an app
/// @param handle Engine handle
/// @param app_name Name of the app
/// @param mode Writing mode (0-3)
/// @return true on success
bool flow_set_app_mode(FlowHandle* handle, const char* app_name, uint8_t mode);

/// Get the writing mode for an app
/// @param handle Engine handle
/// @param app_name Name of the app
/// @return Writing mode (0-3)
uint8_t flow_get_app_mode(FlowHandle* handle, const char* app_name);

// ============ Learning ============

/// Report a user edit to learn from
/// @param handle Engine handle
/// @param original Original transcribed text
/// @param edited Text after user edits
/// @return true on success
bool flow_learn_from_edit(FlowHandle* handle, const char* original, const char* edited);

/// Get the number of learned corrections
/// @param handle Engine handle
/// @return Number of corrections
size_t flow_correction_count(FlowHandle* handle);

// ============ Stats ============

/// Get total transcription time in minutes
/// @param handle Engine handle
/// @return Total minutes
uint64_t flow_total_transcription_minutes(FlowHandle* handle);

/// Get total transcription count
/// @param handle Engine handle
/// @return Total count
uint64_t flow_transcription_count(FlowHandle* handle);

// ============ Utilities ============

/// Free a string returned by flowwispr functions
/// @param s String to free
void flow_free_string(char* s);

/// Check if the transcription provider is configured
/// @param handle Engine handle
/// @return true if configured
bool flow_is_configured(FlowHandle* handle);

// ============ App Tracking ============

/// Set the currently active app
/// @param handle Engine handle
/// @param app_name Name of the app
/// @param bundle_id Bundle ID (can be NULL)
/// @param window_title Window title (can be NULL)
/// @return Suggested writing mode (0=Formal, 1=Casual, 2=VeryCasual, 3=Excited)
uint8_t flow_set_active_app(FlowHandle* handle, const char* app_name, const char* bundle_id, const char* window_title);

/// Get the current app's category
/// @param handle Engine handle
/// @return Category (0=Email, 1=Slack, 2=Code, 3=Documents, 4=Social, 5=Browser, 6=Terminal, 7=Unknown)
uint8_t flow_get_app_category(FlowHandle* handle);

/// Get current app name
/// @param handle Engine handle
/// @return App name (caller must free with flow_free_string)
char* flow_get_current_app(FlowHandle* handle);

// ============ Style Learning ============

/// Report edited text to learn user's style
/// @param handle Engine handle
/// @param edited_text The edited text
/// @return true on success
bool flow_learn_style(FlowHandle* handle, const char* edited_text);

/// Get suggested mode based on learned style
/// @param handle Engine handle
/// @return Mode (0-3) or 255 if no suggestion
uint8_t flow_get_style_suggestion(FlowHandle* handle);

// ============ Extended Stats ============

/// Get user stats as JSON
/// @param handle Engine handle
/// @return JSON string (caller must free with flow_free_string)
char* flow_get_stats_json(FlowHandle* handle);

/// Get recent transcriptions as JSON
/// @param handle Engine handle
/// @param limit Maximum number of transcriptions to return
/// @return JSON string (caller must free with flow_free_string)
char* flow_get_recent_transcriptions_json(FlowHandle* handle, size_t limit);

/// Get all shortcuts as JSON
/// @param handle Engine handle
/// @return JSON string (caller must free with flow_free_string)
char* flow_get_shortcuts_json(FlowHandle* handle);

// ============ Provider Configuration ============

/// Switch completion provider (loads API key from database)
/// @param handle Engine handle
/// @param provider 0 = OpenAI, 1 = Gemini, 2 = OpenRouter
/// @return true on success
bool flow_switch_completion_provider(FlowHandle* handle, uint8_t provider);

/// Set completion provider with API key (saves both)
/// @param handle Engine handle
/// @param provider 0 = OpenAI, 1 = Gemini, 2 = OpenRouter
/// @param api_key API key for the provider
/// @return true on success
bool flow_set_completion_provider(FlowHandle* handle, uint8_t provider, const char* api_key);

/// Get current completion provider
/// @param handle Engine handle
/// @return 0 = OpenAI, 1 = Gemini, 2 = OpenRouter, 255 = Unknown
uint8_t flow_get_completion_provider(FlowHandle* handle);

/// Get API key for a specific provider in masked form (e.g., "sk-••••••••")
/// @param handle Engine handle
/// @param provider 0 = OpenAI, 1 = Gemini, 2 = OpenRouter
/// @return Masked API key string (caller must free with flow_free_string) or NULL if not set
char* flow_get_api_key(FlowHandle* handle, uint8_t provider);

/// Set transcription mode (local or remote)
/// @param handle Engine handle
/// @param use_local true for local Whisper, false for cloud provider
/// @param whisper_model Whisper model: 0 = Tiny (39MB), 1 = Base (142MB), 2 = Small (466MB)
/// @return true on success, false on failure
bool flow_set_transcription_mode(FlowHandle* handle, bool use_local, uint8_t whisper_model);

/// Get current transcription mode settings
/// @param handle Engine handle
/// @param out_use_local Output parameter for use_local flag
/// @param out_whisper_model Output parameter for whisper_model (0-4)
/// @return true on success, false on database error
bool flow_get_transcription_mode(FlowHandle* handle, bool* out_use_local, uint8_t* out_whisper_model);

/// Check if a Whisper model is currently being downloaded/initialized
/// @param handle Engine handle
/// @return true if model download/initialization is in progress
bool flow_is_model_loading(FlowHandle* handle);

/// Legacy: Enable local Whisper transcription with Metal acceleration
/// @param handle Engine handle
/// @param model Whisper model: 0 = Tiny (39MB), 1 = Base (142MB), 2 = Small (466MB)
/// @return true on success, false on failure
bool flow_enable_local_whisper(FlowHandle* handle, uint8_t model);

// ============ Error Handling ============

/// Get the last error message
/// @param handle Engine handle
/// @return Error string (caller must free with flow_free_string) or NULL if none
char* flow_get_last_error(FlowHandle* handle);

#ifdef __cplusplus
}
#endif

#endif // FLOWWHISPR_H
