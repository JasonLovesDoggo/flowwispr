//
// flowwispr.h
// FlowWhispr C Interface
//
// Auto-generated header for the FlowWhispr Rust FFI layer.
// This header provides C-compatible function declarations for Swift interop.
//

#ifndef FLOWWHISPR_H
#define FLOWWHISPR_H

#include <stdint.h>
#include <stdbool.h>

#ifdef __cplusplus
extern "C" {
#endif

/// Opaque handle to the FlowWhispr engine
typedef struct FlowWhisprHandle FlowWhisprHandle;

// ============ Lifecycle ============

/// Initialize the FlowWhispr engine
/// @param db_path Path to the SQLite database file, or NULL for default location
/// @return Opaque handle to the engine, or NULL on failure
FlowWhisprHandle* flowwispr_init(const char* db_path);

/// Destroy the FlowWhispr engine and free resources
/// @param handle Handle returned by flowwispr_init
void flowwispr_destroy(FlowWhisprHandle* handle);

// ============ Audio ============

/// Start audio recording
/// @param handle Engine handle
/// @return true on success
bool flowwispr_start_recording(FlowWhisprHandle* handle);

/// Stop audio recording and get the duration
/// @param handle Engine handle
/// @return Duration in milliseconds, or 0 on failure
uint64_t flowwispr_stop_recording(FlowWhisprHandle* handle);

/// Check if currently recording
/// @param handle Engine handle
/// @return true if recording
bool flowwispr_is_recording(FlowWhisprHandle* handle);

// ============ Transcription ============

/// Transcribe the recorded audio and process it
/// @param handle Engine handle
/// @param app_name Name of the current app (for mode selection), or NULL
/// @return Processed text (caller must free with flowwispr_free_string), or NULL on failure
char* flowwispr_transcribe(FlowWhisprHandle* handle, const char* app_name);

/// Retry the last transcription using cached audio
/// @param handle Engine handle
/// @param app_name Name of the current app (for mode selection), or NULL
/// @return Processed text (caller must free with flowwispr_free_string), or NULL on failure
char* flowwispr_retry_last_transcription(FlowWhisprHandle* handle, const char* app_name);

// ============ Shortcuts ============

/// Add a voice shortcut
/// @param handle Engine handle
/// @param trigger Trigger phrase
/// @param replacement Replacement text
/// @return true on success
bool flowwispr_add_shortcut(FlowWhisprHandle* handle, const char* trigger, const char* replacement);

/// Remove a voice shortcut
/// @param handle Engine handle
/// @param trigger Trigger phrase to remove
/// @return true on success
bool flowwispr_remove_shortcut(FlowWhisprHandle* handle, const char* trigger);

/// Get the number of shortcuts
/// @param handle Engine handle
/// @return Number of shortcuts
size_t flowwispr_shortcut_count(FlowWhisprHandle* handle);

// ============ Writing Modes ============

/// Writing mode constants
/// 0 = Formal, 1 = Casual, 2 = VeryCasual, 3 = Excited

/// Set the writing mode for an app
/// @param handle Engine handle
/// @param app_name Name of the app
/// @param mode Writing mode (0-3)
/// @return true on success
bool flowwispr_set_app_mode(FlowWhisprHandle* handle, const char* app_name, uint8_t mode);

/// Get the writing mode for an app
/// @param handle Engine handle
/// @param app_name Name of the app
/// @return Writing mode (0-3)
uint8_t flowwispr_get_app_mode(FlowWhisprHandle* handle, const char* app_name);

// ============ Learning ============

/// Report a user edit to learn from
/// @param handle Engine handle
/// @param original Original transcribed text
/// @param edited Text after user edits
/// @return true on success
bool flowwispr_learn_from_edit(FlowWhisprHandle* handle, const char* original, const char* edited);

/// Get the number of learned corrections
/// @param handle Engine handle
/// @return Number of corrections
size_t flowwispr_correction_count(FlowWhisprHandle* handle);

// ============ Stats ============

/// Get total transcription time in minutes
/// @param handle Engine handle
/// @return Total minutes
uint64_t flowwispr_total_transcription_minutes(FlowWhisprHandle* handle);

/// Get total transcription count
/// @param handle Engine handle
/// @return Total count
uint64_t flowwispr_transcription_count(FlowWhisprHandle* handle);

// ============ Utilities ============

/// Free a string returned by flowwispr functions
/// @param s String to free
void flowwispr_free_string(char* s);

/// Check if the transcription provider is configured
/// @param handle Engine handle
/// @return true if configured
bool flowwispr_is_configured(FlowWhisprHandle* handle);

/// Set the OpenAI API key
/// @param handle Engine handle
/// @param api_key OpenAI API key
/// @return true on success
bool flowwispr_set_api_key(FlowWhisprHandle* handle, const char* api_key);

/// Set the Gemini API key
/// @param handle Engine handle
/// @param api_key Gemini API key
/// @return true on success
bool flowwispr_set_gemini_api_key(FlowWhisprHandle* handle, const char* api_key);

/// Set the OpenRouter API key
/// @param handle Engine handle
/// @param api_key OpenRouter API key
/// @return true on success
bool flowwispr_set_openrouter_api_key(FlowWhisprHandle* handle, const char* api_key);

// ============ App Tracking ============

/// Set the currently active app
/// @param handle Engine handle
/// @param app_name Name of the app
/// @param bundle_id Bundle ID (can be NULL)
/// @param window_title Window title (can be NULL)
/// @return Suggested writing mode (0=Formal, 1=Casual, 2=VeryCasual, 3=Excited)
uint8_t flowwispr_set_active_app(FlowWhisprHandle* handle, const char* app_name, const char* bundle_id, const char* window_title);

/// Get the current app's category
/// @param handle Engine handle
/// @return Category (0=Email, 1=Slack, 2=Code, 3=Documents, 4=Social, 5=Browser, 6=Terminal, 7=Unknown)
uint8_t flowwispr_get_app_category(FlowWhisprHandle* handle);

/// Get current app name
/// @param handle Engine handle
/// @return App name (caller must free with flowwispr_free_string)
char* flowwispr_get_current_app(FlowWhisprHandle* handle);

// ============ Style Learning ============

/// Report edited text to learn user's style
/// @param handle Engine handle
/// @param edited_text The edited text
/// @return true on success
bool flowwispr_learn_style(FlowWhisprHandle* handle, const char* edited_text);

/// Get suggested mode based on learned style
/// @param handle Engine handle
/// @return Mode (0-3) or 255 if no suggestion
uint8_t flowwispr_get_style_suggestion(FlowWhisprHandle* handle);

// ============ Extended Stats ============

/// Get user stats as JSON
/// @param handle Engine handle
/// @return JSON string (caller must free with flowwispr_free_string)
char* flowwispr_get_stats_json(FlowWhisprHandle* handle);

/// Get recent transcriptions as JSON
/// @param handle Engine handle
/// @param limit Maximum number of transcriptions to return
/// @return JSON string (caller must free with flowwispr_free_string)
char* flowwispr_get_recent_transcriptions_json(FlowWhisprHandle* handle, size_t limit);

/// Get all shortcuts as JSON
/// @param handle Engine handle
/// @return JSON string (caller must free with flowwispr_free_string)
char* flowwispr_get_shortcuts_json(FlowWhisprHandle* handle);

// ============ Provider Configuration ============

/// Set completion provider
/// @param handle Engine handle
/// @param provider 0 = OpenAI, 1 = Gemini, 2 = OpenRouter
/// @param api_key API key for the provider
/// @return true on success
bool flowwispr_set_completion_provider(FlowWhisprHandle* handle, uint8_t provider, const char* api_key);

/// Get current completion provider
/// @param handle Engine handle
/// @return 0 = OpenAI, 1 = Gemini, 2 = OpenRouter, 255 = Unknown
uint8_t flowwispr_get_completion_provider(FlowWhisprHandle* handle);

/// Enable local Whisper transcription with Metal acceleration
/// @param handle Engine handle
/// @param model Whisper model: 0 = Tiny (75MB), 1 = Base (142MB), 2 = Small (466MB)
/// @return true on success, false on failure
bool flowwispr_enable_local_whisper(FlowWhisprHandle* handle, uint8_t model);

// ============ Error Handling ============

/// Get the last error message
/// @param handle Engine handle
/// @return Error string (caller must free with flowwispr_free_string) or NULL if none
char* flowwispr_get_last_error(FlowWhisprHandle* handle);

#ifdef __cplusplus
}
#endif

#endif // FLOWWHISPR_H
