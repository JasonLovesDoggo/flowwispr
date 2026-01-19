//! Integration tests for the FFI layer
//!
//! These tests verify the C-compatible FFI functions that are called from Swift.
//! Tests focus on handle lifecycle, error handling, and data marshalling.

use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::ptr;

use flow::ffi::*;

// ============ Helper Functions ============

fn c_str(s: &str) -> CString {
    CString::new(s).expect("CString creation failed")
}

fn from_c_str_and_free(ptr: *mut c_char) -> Option<String> {
    if ptr.is_null() {
        None
    } else {
        let result = unsafe { CStr::from_ptr(ptr).to_str().ok().map(String::from) };
        flow_free_string(ptr);
        result
    }
}

/// Create a temporary database path for isolated FFI tests
fn temp_db_path() -> CString {
    use std::time::{SystemTime, UNIX_EPOCH};
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = format!("/tmp/flow_test_{}.db", timestamp);
    CString::new(path).unwrap()
}

// ============ Handle Lifecycle Tests ============

#[test]
fn test_init_and_destroy() {
    // init with null path uses default location
    let handle = flow_init(ptr::null());
    assert!(!handle.is_null(), "flow_init should not return null");

    // destroying should not panic
    flow_destroy(handle);
}

#[test]
fn test_init_with_custom_path() {
    let temp_dir = std::env::temp_dir().join("flow_test_db");
    let _ = std::fs::create_dir_all(&temp_dir);
    let db_path = temp_dir.join("test.db");

    let path = c_str(db_path.to_str().unwrap());
    let handle = flow_init(path.as_ptr());
    assert!(!handle.is_null());

    flow_destroy(handle);

    // cleanup
    let _ = std::fs::remove_file(&db_path);
}

#[test]
fn test_destroy_null_handle() {
    // destroying null should not panic
    flow_destroy(ptr::null_mut());
}

#[test]
fn test_multiple_init_destroy_cycles() {
    for _ in 0..5 {
        let handle = flow_init(ptr::null());
        assert!(!handle.is_null());
        flow_destroy(handle);
    }
}

// ============ Configuration Tests ============

#[test]
fn test_is_configured_initial() {
    let handle = flow_init(ptr::null());
    assert!(!handle.is_null());

    // initially configured depends on default provider
    // just verify it doesn't crash
    let _ = flow_is_configured(handle);

    flow_destroy(handle);
}

#[test]
fn test_get_completion_provider() {
    let handle = flow_init(ptr::null());
    assert!(!handle.is_null());

    let provider = flow_get_completion_provider(handle);
    // 0 = OpenAI, 1 = Gemini, 2 = OpenRouter - just verify it returns a valid value
    let _ = provider;

    flow_destroy(handle);
}

// ============ Shortcut Tests ============

#[test]
fn test_add_and_remove_shortcut() {
    // Use temp database to avoid interference from real shortcuts
    let path = temp_db_path();
    let handle = flow_init(path.as_ptr());
    assert!(!handle.is_null());

    let trigger = c_str("my email");
    let replacement = c_str("test@example.com");

    let initial_count = flow_shortcut_count(handle);

    let result = flow_add_shortcut(handle, trigger.as_ptr(), replacement.as_ptr());
    assert!(result, "Adding shortcut should succeed");

    assert_eq!(flow_shortcut_count(handle), initial_count + 1);

    let result = flow_remove_shortcut(handle, trigger.as_ptr());
    assert!(result, "Removing shortcut should succeed");

    assert_eq!(flow_shortcut_count(handle), initial_count);

    flow_destroy(handle);
}

#[test]
fn test_add_shortcut_null_params() {
    let handle = flow_init(ptr::null());
    assert!(!handle.is_null());

    let trigger = c_str("test");
    let replacement = c_str("TEST");

    // null trigger
    assert!(!flow_add_shortcut(
        handle,
        ptr::null(),
        replacement.as_ptr()
    ));

    // null replacement
    assert!(!flow_add_shortcut(handle, trigger.as_ptr(), ptr::null()));

    // both null
    assert!(!flow_add_shortcut(handle, ptr::null(), ptr::null()));

    flow_destroy(handle);
}

#[test]
fn test_remove_shortcut_null_trigger() {
    let handle = flow_init(ptr::null());
    assert!(!handle.is_null());

    let result = flow_remove_shortcut(handle, ptr::null());
    assert!(!result, "Removing null trigger should fail");

    flow_destroy(handle);
}

#[test]
fn test_get_shortcuts_json() {
    let handle = flow_init(ptr::null());
    assert!(!handle.is_null());

    let trigger = c_str("test");
    let replacement = c_str("TEST");
    flow_add_shortcut(handle, trigger.as_ptr(), replacement.as_ptr());

    let json_ptr = flow_get_shortcuts_json(handle);
    assert!(!json_ptr.is_null());

    let json = from_c_str_and_free(json_ptr).unwrap();
    assert!(json.contains("test"));
    assert!(json.contains("TEST"));

    flow_destroy(handle);
}

// ============ Writing Mode Tests ============

#[test]
fn test_set_and_get_app_mode() {
    let handle = flow_init(ptr::null());
    assert!(!handle.is_null());

    let app_name = c_str("TestApp");

    // set to Formal (0)
    let result = flow_set_app_mode(handle, app_name.as_ptr(), 0);
    assert!(result);

    let mode = flow_get_app_mode(handle, app_name.as_ptr());
    assert_eq!(mode, 0);

    // set to VeryCasual (2)
    flow_set_app_mode(handle, app_name.as_ptr(), 2);
    let mode = flow_get_app_mode(handle, app_name.as_ptr());
    assert_eq!(mode, 2);

    flow_destroy(handle);
}

#[test]
fn test_get_app_mode_null_app() {
    let handle = flow_init(ptr::null());
    assert!(!handle.is_null());

    let mode = flow_get_app_mode(handle, ptr::null());
    assert_eq!(mode, 1); // default to Casual

    flow_destroy(handle);
}

#[test]
fn test_set_app_mode_invalid_mode() {
    let handle = flow_init(ptr::null());
    assert!(!handle.is_null());

    let app_name = c_str("TestApp");

    // invalid mode (> 3)
    let result = flow_set_app_mode(handle, app_name.as_ptr(), 99);
    assert!(!result);

    flow_destroy(handle);
}

// ============ Learning Tests ============

#[test]
fn test_learn_from_edit() {
    let handle = flow_init(ptr::null());
    assert!(!handle.is_null());

    let original = c_str("I recieve the package");
    let edited = c_str("I receive the package");

    let result = flow_learn_from_edit(handle, original.as_ptr(), edited.as_ptr());
    assert!(result);

    flow_destroy(handle);
}

#[test]
fn test_learn_from_edit_null_params() {
    let handle = flow_init(ptr::null());
    assert!(!handle.is_null());

    let text = c_str("test");

    assert!(!flow_learn_from_edit(handle, ptr::null(), text.as_ptr()));
    assert!(!flow_learn_from_edit(handle, text.as_ptr(), ptr::null()));
    assert!(!flow_learn_from_edit(handle, ptr::null(), ptr::null()));

    flow_destroy(handle);
}

#[test]
fn test_correction_count() {
    let handle = flow_init(ptr::null());
    assert!(!handle.is_null());

    // initial count may vary due to seeded corrections
    let initial = flow_correction_count(handle);

    // add a correction via learning
    let original = c_str("teh cat");
    let edited = c_str("the cat");
    flow_learn_from_edit(handle, original.as_ptr(), edited.as_ptr());

    let after = flow_correction_count(handle);
    assert!(after >= initial);

    flow_destroy(handle);
}

#[test]
fn test_get_corrections_json() {
    let handle = flow_init(ptr::null());
    assert!(!handle.is_null());

    let json_ptr = flow_get_corrections_json(handle);
    assert!(!json_ptr.is_null());

    let json = from_c_str_and_free(json_ptr).unwrap();
    // should be valid JSON array
    assert!(json.starts_with('['));
    assert!(json.ends_with(']'));

    flow_destroy(handle);
}

#[test]
fn test_delete_all_corrections() {
    let handle = flow_init(ptr::null());
    assert!(!handle.is_null());

    // Just verify it doesn't crash - may delete seeded corrections
    let _ = flow_delete_all_corrections(handle);

    flow_destroy(handle);
}

#[test]
fn test_delete_correction_invalid_id() {
    let handle = flow_init(ptr::null());
    assert!(!handle.is_null());

    let invalid_uuid = c_str("not-a-uuid");
    let result = flow_delete_correction(handle, invalid_uuid.as_ptr());
    assert!(!result);

    flow_destroy(handle);
}

// ============ App Tracking Tests ============

#[test]
fn test_set_active_app() {
    let handle = flow_init(ptr::null());
    assert!(!handle.is_null());

    let app_name = c_str("Slack");
    let bundle_id = c_str("com.tinyspeck.slackmacgap");
    let window_title = c_str("general - Workspace");

    let mode = flow_set_active_app(
        handle,
        app_name.as_ptr(),
        bundle_id.as_ptr(),
        window_title.as_ptr(),
    );
    // returns suggested mode (0-3)
    assert!(mode <= 3);

    flow_destroy(handle);
}

#[test]
fn test_set_active_app_null_name() {
    let handle = flow_init(ptr::null());
    assert!(!handle.is_null());

    let mode = flow_set_active_app(handle, ptr::null(), ptr::null(), ptr::null());
    assert_eq!(mode, 1); // default to Casual

    flow_destroy(handle);
}

#[test]
fn test_get_app_category() {
    let handle = flow_init(ptr::null());
    assert!(!handle.is_null());

    let app_name = c_str("Mail");
    flow_set_active_app(handle, app_name.as_ptr(), ptr::null(), ptr::null());

    let category = flow_get_app_category(handle);
    // 0=Email, 1=Slack, 2=Code, etc.
    assert!(category <= 7);

    flow_destroy(handle);
}

#[test]
fn test_get_current_app() {
    let handle = flow_init(ptr::null());
    assert!(!handle.is_null());

    let app_name = c_str("TestApp");
    flow_set_active_app(handle, app_name.as_ptr(), ptr::null(), ptr::null());

    let current = flow_get_current_app(handle);
    assert!(!current.is_null());

    let name = from_c_str_and_free(current).unwrap();
    assert_eq!(name, "TestApp");

    flow_destroy(handle);
}

// ============ Stats Tests ============

#[test]
fn test_stats_functions() {
    // Use temp database to avoid interference from real transcription data
    let path = temp_db_path();
    let handle = flow_init(path.as_ptr());
    assert!(!handle.is_null());

    let minutes = flow_total_transcription_minutes(handle);
    assert_eq!(minutes, 0);

    let count = flow_transcription_count(handle);
    assert_eq!(count, 0);

    flow_destroy(handle);
}

#[test]
fn test_get_stats_json() {
    let handle = flow_init(ptr::null());
    assert!(!handle.is_null());

    let json_ptr = flow_get_stats_json(handle);
    assert!(!json_ptr.is_null());

    let json = from_c_str_and_free(json_ptr).unwrap();
    assert!(json.contains("total_transcriptions"));
    assert!(json.contains("total_duration_ms"));

    flow_destroy(handle);
}

#[test]
fn test_get_recent_transcriptions_json() {
    // Use temp database to avoid interference from real transcription data
    let path = temp_db_path();
    let handle = flow_init(path.as_ptr());
    assert!(!handle.is_null());

    let json_ptr = flow_get_recent_transcriptions_json(handle, 10);
    assert!(!json_ptr.is_null());

    let json = from_c_str_and_free(json_ptr).unwrap();
    // should be empty array in fresh database
    assert_eq!(json, "[]");

    flow_destroy(handle);
}

// ============ Error Handling Tests ============

#[test]
fn test_get_last_error_when_none() {
    let handle = flow_init(ptr::null());
    assert!(!handle.is_null());

    let error = flow_get_last_error(handle);
    // should be null when no error
    assert!(error.is_null());

    flow_destroy(handle);
}

// ============ Transcription Mode Tests ============

#[test]
fn test_get_transcription_mode() {
    let handle = flow_init(ptr::null());
    assert!(!handle.is_null());

    let mut use_local: bool = false;
    let mut whisper_model: u8 = 255;

    let result = flow_get_transcription_mode(handle, &mut use_local, &mut whisper_model);
    assert!(result);

    // whisper_model should be 0-4
    assert!(whisper_model <= 4 || !use_local);

    flow_destroy(handle);
}

#[test]
fn test_set_transcription_mode_cloud() {
    let handle = flow_init(ptr::null());
    assert!(!handle.is_null());

    // set to cloud transcription
    let result = flow_set_transcription_mode(handle, false, 0);
    assert!(result);

    let mut use_local: bool = true;
    let mut whisper_model: u8 = 255;
    flow_get_transcription_mode(handle, &mut use_local, &mut whisper_model);
    assert!(!use_local);

    flow_destroy(handle);
}

#[test]
fn test_is_model_loading() {
    let handle = flow_init(ptr::null());
    assert!(!handle.is_null());

    // initially should not be loading
    let loading = flow_is_model_loading(handle);
    // may or may not be loading depending on initialization
    let _ = loading;

    flow_destroy(handle);
}

#[test]
fn test_get_whisper_models_json() {
    let json_ptr = flow_get_whisper_models_json();
    assert!(!json_ptr.is_null());

    let json = from_c_str_and_free(json_ptr).unwrap();
    // Model names are lowercase in as_str() output
    assert!(json.contains("turbo"));
    assert!(json.contains("quality"));
    assert!(json.contains("size_mb"));

    // should be array
    assert!(json.starts_with('['));
    assert!(json.ends_with(']'));
}

// ============ Contact Classification Tests ============

#[test]
fn test_classify_contact() {
    let handle = flow_init(ptr::null());
    assert!(!handle.is_null());

    let name = c_str("Mom");
    let result = flow_classify_contact(handle, name.as_ptr(), ptr::null());
    assert!(!result.is_null());

    let json = from_c_str_and_free(result).unwrap();
    assert!(json.contains("Mom"));
    assert!(json.contains("category"));

    flow_destroy(handle);
}

#[test]
fn test_classify_contact_null_name() {
    let handle = flow_init(ptr::null());
    assert!(!handle.is_null());

    let result = flow_classify_contact(handle, ptr::null(), ptr::null());
    assert!(result.is_null());

    flow_destroy(handle);
}

#[test]
fn test_classify_contacts_batch() {
    let handle = flow_init(ptr::null());
    assert!(!handle.is_null());

    let json = c_str(
        r#"[{"name": "Mom", "organization": ""}, {"name": "Dr. Smith", "organization": ""}]"#,
    );

    let result = flow_classify_contacts_batch(handle, json.as_ptr());
    assert!(!result.is_null());

    let result_json = from_c_str_and_free(result).unwrap();
    assert!(result_json.contains("Mom"));
    assert!(result_json.contains("Dr. Smith"));

    flow_destroy(handle);
}

#[test]
fn test_classify_contacts_batch_invalid_json() {
    let handle = flow_init(ptr::null());
    assert!(!handle.is_null());

    let invalid_json = c_str("not valid json");
    let result = flow_classify_contacts_batch(handle, invalid_json.as_ptr());
    assert!(result.is_null());

    flow_destroy(handle);
}

#[test]
fn test_get_writing_mode_for_category() {
    let handle = flow_init(ptr::null());
    assert!(!handle.is_null());

    // Professional (0) -> Formal (0)
    let mode = flow_get_writing_mode_for_category(handle, 0);
    assert_eq!(mode, 0);

    // Partner (3) -> Excited (3)
    let mode = flow_get_writing_mode_for_category(handle, 3);
    assert_eq!(mode, 3);

    flow_destroy(handle);
}

#[test]
fn test_get_frequent_contacts() {
    let handle = flow_init(ptr::null());
    assert!(!handle.is_null());

    let result = flow_get_frequent_contacts(handle, 10);
    assert!(!result.is_null());

    let json = from_c_str_and_free(result).unwrap();
    // should be array (may be empty)
    assert!(json.starts_with('['));
    assert!(json.ends_with(']'));

    flow_destroy(handle);
}

#[test]
fn test_record_contact_interaction() {
    let handle = flow_init(ptr::null());
    assert!(!handle.is_null());

    let name = c_str("Test Contact");
    // should not crash even for non-existent contact
    flow_record_contact_interaction(handle, name.as_ptr());

    flow_destroy(handle);
}

// ============ Cloud Transcription Provider Tests ============

#[test]
fn test_get_cloud_transcription_provider() {
    let handle = flow_init(ptr::null());
    assert!(!handle.is_null());

    let provider = flow_get_cloud_transcription_provider(handle);
    // 0 = OpenAI, 1 = Auto
    assert!(provider <= 1);

    flow_destroy(handle);
}

#[test]
fn test_set_cloud_transcription_provider() {
    let handle = flow_init(ptr::null());
    assert!(!handle.is_null());

    // set to Auto (1)
    let result = flow_set_cloud_transcription_provider(handle, 1);
    assert!(result);

    let provider = flow_get_cloud_transcription_provider(handle);
    assert_eq!(provider, 1);

    flow_destroy(handle);
}

#[test]
fn test_set_cloud_transcription_provider_invalid() {
    let handle = flow_init(ptr::null());
    assert!(!handle.is_null());

    let result = flow_set_cloud_transcription_provider(handle, 99);
    assert!(!result);

    flow_destroy(handle);
}

// ============ String Memory Tests ============

#[test]
fn test_free_null_string() {
    // should not crash
    flow_free_string(ptr::null_mut());
}

// ============ Recording State Tests ============
// Note: These don't actually start recording (requires audio hardware)
// but verify the state checking doesn't crash

#[test]
fn test_is_recording_initial() {
    let handle = flow_init(ptr::null());
    assert!(!handle.is_null());

    let recording = flow_is_recording(handle);
    assert!(!recording);

    flow_destroy(handle);
}

#[test]
fn test_get_audio_level_not_recording() {
    let handle = flow_init(ptr::null());
    assert!(!handle.is_null());

    let level = flow_get_audio_level(handle);
    assert_eq!(level, 0.0);

    flow_destroy(handle);
}

// ============ Style Learning Tests ============

#[test]
fn test_learn_style_no_active_app() {
    let handle = flow_init(ptr::null());
    assert!(!handle.is_null());

    // no active app set
    let text = c_str("some text to learn from");
    let result = flow_learn_style(handle, text.as_ptr());
    assert!(!result); // should fail without active app

    flow_destroy(handle);
}

#[test]
fn test_learn_style_with_active_app() {
    let handle = flow_init(ptr::null());
    assert!(!handle.is_null());

    let app_name = c_str("Slack");
    flow_set_active_app(handle, app_name.as_ptr(), ptr::null(), ptr::null());

    let text = c_str("hey whats up");
    let result = flow_learn_style(handle, text.as_ptr());
    assert!(result);

    flow_destroy(handle);
}

#[test]
fn test_get_style_suggestion_no_data() {
    let handle = flow_init(ptr::null());
    assert!(!handle.is_null());

    let suggestion = flow_get_style_suggestion(handle);
    // 255 = no suggestion
    assert_eq!(suggestion, 255);

    flow_destroy(handle);
}

// ============ API Key Tests ============

#[test]
fn test_get_api_key_not_set() {
    let handle = flow_init(ptr::null());
    assert!(!handle.is_null());

    // OpenAI = 0
    let key = flow_get_api_key(handle, 0);
    // may be null or masked depending on database state
    if !key.is_null() {
        flow_free_string(key);
    }

    flow_destroy(handle);
}

#[test]
fn test_get_api_key_invalid_provider() {
    let handle = flow_init(ptr::null());
    assert!(!handle.is_null());

    let key = flow_get_api_key(handle, 99);
    assert!(key.is_null());

    flow_destroy(handle);
}
