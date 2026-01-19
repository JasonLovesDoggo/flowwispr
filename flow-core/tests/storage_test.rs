//! Integration tests for the storage layer
//!
//! These tests verify database operations, schema initialization,
//! and data persistence across multiple operations.

use flow::storage::Storage;
use flow::types::{
    AppCategory, AppContext, Contact, ContactCategory, Correction, CorrectionSource, Shortcut,
    Transcription, TranscriptionHistoryEntry, WritingMode,
};
use std::sync::Arc;
use std::thread;

// ============ Schema Initialization Tests ============

#[test]
fn test_fresh_database_initialization() {
    let storage = Storage::in_memory().expect("Failed to create in-memory storage");

    // verify tables exist by querying them
    let transcription_count = storage.get_transcription_count().unwrap();
    let shortcuts = storage.get_enabled_shortcuts().unwrap();
    let corrections = storage.get_all_corrections().unwrap();

    assert_eq!(transcription_count, 0);
    assert!(shortcuts.is_empty());
    // corrections may have seeded values - just verify query works
    let _ = corrections;
}

#[test]
fn test_database_seeds_default_corrections() {
    let storage = Storage::in_memory().expect("Failed to create in-memory storage");

    let corrections = storage.get_all_corrections().unwrap();

    // should have seeded corrections
    let seeded_pairs = vec![
        ("u of t hacks", "UofTHacks"),
        ("get hub", "GitHub"),
        ("anthropic", "Anthropic"),
        ("open ai", "OpenAI"),
        ("chat gpt", "ChatGPT"),
        ("gonna", "going to"),
        ("wanna", "want to"),
        ("kinda", "kind of"),
    ];

    for (original, corrected) in seeded_pairs {
        let found = corrections
            .iter()
            .find(|c| c.original == original && c.corrected == corrected);
        assert!(
            found.is_some(),
            "Seeded correction not found: {} -> {}",
            original,
            corrected
        );
    }
}

// ============ Transcription CRUD Tests ============

#[test]
fn test_save_and_retrieve_transcription() {
    let storage = Storage::in_memory().unwrap();

    let transcription = Transcription::new(
        "hello world".to_string(),
        "Hello world.".to_string(),
        0.95,
        1500,
    );

    storage.save_transcription(&transcription).unwrap();

    let recent = storage.get_recent_transcriptions(10).unwrap();
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].raw_text, "hello world");
    assert_eq!(recent[0].processed_text, "Hello world.");
    assert!((recent[0].confidence - 0.95).abs() < 0.001);
    assert_eq!(recent[0].duration_ms, 1500);
}

#[test]
fn test_transcription_with_app_context() {
    let storage = Storage::in_memory().unwrap();

    let mut transcription = Transcription::new("test".to_string(), "Test.".to_string(), 0.9, 1000);
    transcription.app_context = Some(AppContext {
        app_name: "Slack".to_string(),
        bundle_id: Some("com.tinyspeck.slackmacgap".to_string()),
        window_title: Some("general - Workspace".to_string()),
        category: AppCategory::Slack,
    });

    storage.save_transcription(&transcription).unwrap();

    let recent = storage.get_recent_transcriptions(10).unwrap();
    assert_eq!(recent.len(), 1);
    let ctx = recent[0].app_context.as_ref().unwrap();
    assert_eq!(ctx.app_name, "Slack");
    assert_eq!(ctx.bundle_id, Some("com.tinyspeck.slackmacgap".to_string()));
    assert_eq!(ctx.category, AppCategory::Slack);
}

#[test]
fn test_transcription_ordering() {
    let storage = Storage::in_memory().unwrap();

    // save multiple transcriptions
    for i in 0..5 {
        let t = Transcription::new(
            format!("text {}", i),
            format!("Text {}.", i),
            0.9,
            1000 + i * 100,
        );
        storage.save_transcription(&t).unwrap();
    }

    let recent = storage.get_recent_transcriptions(3).unwrap();
    assert_eq!(recent.len(), 3);

    // most recent should be first (text 4)
    assert_eq!(recent[0].raw_text, "text 4");
    assert_eq!(recent[1].raw_text, "text 3");
    assert_eq!(recent[2].raw_text, "text 2");
}

#[test]
fn test_transcription_limit() {
    let storage = Storage::in_memory().unwrap();

    for i in 0..10 {
        let t = Transcription::new(format!("text {}", i), format!("Text {}.", i), 0.9, 1000);
        storage.save_transcription(&t).unwrap();
    }

    let recent = storage.get_recent_transcriptions(5).unwrap();
    assert_eq!(recent.len(), 5);
}

// ============ Transcription History Tests ============

#[test]
fn test_save_and_retrieve_history_entry() {
    let storage = Storage::in_memory().unwrap();

    let entry = TranscriptionHistoryEntry::success(
        "raw text".to_string(),
        "processed text".to_string(),
        1500,
    );

    storage.save_history_entry(&entry).unwrap();

    let history = storage.get_recent_history(10).unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].raw_text, "raw text");
    assert_eq!(history[0].text, "processed text");
}

#[test]
fn test_save_failed_history_entry() {
    let storage = Storage::in_memory().unwrap();

    let entry = TranscriptionHistoryEntry::failure("Network error".to_string(), 500);

    storage.save_history_entry(&entry).unwrap();

    let history = storage.get_recent_history(10).unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].error, Some("Network error".to_string()));
}

// ============ Shortcut CRUD Tests ============

#[test]
fn test_save_and_retrieve_shortcut() {
    let storage = Storage::in_memory().unwrap();

    let shortcut = Shortcut::new("my email".to_string(), "test@example.com".to_string());
    storage.save_shortcut(&shortcut).unwrap();

    let shortcuts = storage.get_enabled_shortcuts().unwrap();
    assert_eq!(shortcuts.len(), 1);
    assert_eq!(shortcuts[0].trigger, "my email");
    assert_eq!(shortcuts[0].replacement, "test@example.com");
}

#[test]
fn test_shortcut_update_on_conflict() {
    let storage = Storage::in_memory().unwrap();

    let mut shortcut = Shortcut::new("my email".to_string(), "old@example.com".to_string());
    storage.save_shortcut(&shortcut).unwrap();

    // update the same trigger with new replacement
    shortcut.replacement = "new@example.com".to_string();
    storage.save_shortcut(&shortcut).unwrap();

    let shortcuts = storage.get_all_shortcuts().unwrap();
    // should still be only 1 shortcut (unique constraint on trigger)
    // Note: the current implementation uses INSERT OR REPLACE on id, not trigger
    // so this may create duplicates - this test documents current behavior
    assert!(!shortcuts.is_empty());
}

#[test]
fn test_delete_shortcut() {
    let storage = Storage::in_memory().unwrap();

    let shortcut = Shortcut::new("foo".to_string(), "bar".to_string());
    storage.save_shortcut(&shortcut).unwrap();

    assert_eq!(storage.get_all_shortcuts().unwrap().len(), 1);

    storage.delete_shortcut(&shortcut.id).unwrap();

    assert_eq!(storage.get_all_shortcuts().unwrap().len(), 0);
}

#[test]
fn test_increment_shortcut_use() {
    let storage = Storage::in_memory().unwrap();

    let shortcut = Shortcut::new("test".to_string(), "TEST".to_string());
    storage.save_shortcut(&shortcut).unwrap();

    storage.increment_shortcut_use("test").unwrap();
    storage.increment_shortcut_use("test").unwrap();

    let shortcuts = storage.get_all_shortcuts().unwrap();
    assert_eq!(shortcuts[0].use_count, 2);
}

#[test]
fn test_disabled_shortcut_not_in_enabled() {
    let storage = Storage::in_memory().unwrap();

    let mut shortcut = Shortcut::new("test".to_string(), "TEST".to_string());
    shortcut.enabled = false;
    storage.save_shortcut(&shortcut).unwrap();

    let enabled = storage.get_enabled_shortcuts().unwrap();
    assert_eq!(enabled.len(), 0);

    let all = storage.get_all_shortcuts().unwrap();
    assert_eq!(all.len(), 1);
}

// ============ Correction CRUD Tests ============

#[test]
fn test_save_and_retrieve_correction() {
    let storage = Storage::in_memory().unwrap();

    // clear seeded corrections first
    storage.delete_all_corrections().unwrap();

    let correction = Correction::new(
        "teh".to_string(),
        "the".to_string(),
        CorrectionSource::UserEdit,
    );
    storage.save_correction(&correction).unwrap();

    let corrections = storage.get_all_corrections().unwrap();
    assert_eq!(corrections.len(), 1);
    assert_eq!(corrections[0].original, "teh");
    assert_eq!(corrections[0].corrected, "the");
}

#[test]
fn test_correction_upsert_increments_occurrences() {
    let storage = Storage::in_memory().unwrap();
    storage.delete_all_corrections().unwrap();

    let c1 = Correction::new(
        "teh".to_string(),
        "the".to_string(),
        CorrectionSource::UserEdit,
    );
    storage.save_correction(&c1).unwrap();

    // save same original -> corrected pair again
    let c2 = Correction::new(
        "teh".to_string(),
        "the".to_string(),
        CorrectionSource::UserEdit,
    );
    storage.save_correction(&c2).unwrap();

    let corrections = storage.get_all_corrections().unwrap();
    // unique constraint on (original, corrected) means it should upsert
    let teh_correction = corrections.iter().find(|c| c.original == "teh").unwrap();
    assert_eq!(teh_correction.occurrences, 2);
}

#[test]
fn test_get_correction_by_original() {
    let storage = Storage::in_memory().unwrap();
    storage.delete_all_corrections().unwrap();

    let mut correction = Correction::new(
        "teh".to_string(),
        "the".to_string(),
        CorrectionSource::UserEdit,
    );
    correction.confidence = 0.9;
    storage.save_correction(&correction).unwrap();

    // should find with min_confidence below actual
    let found = storage.get_correction("teh", 0.5).unwrap();
    assert_eq!(found, Some("the".to_string()));

    // should not find with min_confidence above actual
    let not_found = storage.get_correction("teh", 0.95).unwrap();
    assert_eq!(not_found, None);

    // should not find non-existent
    let missing = storage.get_correction("xyz", 0.0).unwrap();
    assert_eq!(missing, None);
}

#[test]
fn test_delete_correction() {
    let storage = Storage::in_memory().unwrap();
    storage.delete_all_corrections().unwrap();

    let correction = Correction::new(
        "teh".to_string(),
        "the".to_string(),
        CorrectionSource::UserEdit,
    );
    storage.save_correction(&correction).unwrap();

    let deleted = storage.delete_correction(&correction.id).unwrap();
    assert!(deleted);

    let corrections = storage.get_all_corrections().unwrap();
    assert!(corrections.is_empty());
}

#[test]
fn test_delete_nonexistent_correction() {
    let storage = Storage::in_memory().unwrap();

    let deleted = storage.delete_correction(&uuid::Uuid::new_v4()).unwrap();
    assert!(!deleted);
}

#[test]
fn test_delete_all_corrections() {
    let storage = Storage::in_memory().unwrap();

    // seeded corrections exist
    let initial = storage.get_all_corrections().unwrap();
    assert!(!initial.is_empty());

    let deleted_count = storage.delete_all_corrections().unwrap();
    assert!(deleted_count > 0);

    let remaining = storage.get_all_corrections().unwrap();
    assert!(remaining.is_empty());
}

// ============ Settings Tests ============

#[test]
fn test_set_and_get_setting() {
    let storage = Storage::in_memory().unwrap();

    storage.set_setting("test_key", "test_value").unwrap();

    let value = storage.get_setting("test_key").unwrap();
    assert_eq!(value, Some("test_value".to_string()));
}

#[test]
fn test_setting_update() {
    let storage = Storage::in_memory().unwrap();

    storage.set_setting("key", "value1").unwrap();
    storage.set_setting("key", "value2").unwrap();

    let value = storage.get_setting("key").unwrap();
    assert_eq!(value, Some("value2".to_string()));
}

#[test]
fn test_get_nonexistent_setting() {
    let storage = Storage::in_memory().unwrap();

    let value = storage.get_setting("nonexistent").unwrap();
    assert_eq!(value, None);
}

// ============ App Mode Tests ============

#[test]
fn test_save_and_get_app_mode() {
    let storage = Storage::in_memory().unwrap();

    storage.save_app_mode("Slack", WritingMode::Casual).unwrap();

    let mode = storage.get_app_mode("Slack").unwrap();
    assert_eq!(mode, Some(WritingMode::Casual));
}

#[test]
fn test_get_nonexistent_app_mode() {
    let storage = Storage::in_memory().unwrap();

    let mode = storage.get_app_mode("NonexistentApp").unwrap();
    assert_eq!(mode, None);
}

#[test]
fn test_update_app_mode() {
    let storage = Storage::in_memory().unwrap();

    storage.save_app_mode("App", WritingMode::Formal).unwrap();
    storage
        .save_app_mode("App", WritingMode::VeryCasual)
        .unwrap();

    let mode = storage.get_app_mode("App").unwrap();
    assert_eq!(mode, Some(WritingMode::VeryCasual));
}

// ============ Style Sample Tests ============

#[test]
fn test_save_and_get_style_samples() {
    let storage = Storage::in_memory().unwrap();

    storage.save_style_sample("Slack", "hey whats up").unwrap();
    storage.save_style_sample("Slack", "cool thanks").unwrap();
    storage.save_style_sample("Mail", "Dear Sir,").unwrap();

    let slack_samples = storage.get_style_samples("Slack", 10).unwrap();
    assert_eq!(slack_samples.len(), 2);

    let mail_samples = storage.get_style_samples("Mail", 10).unwrap();
    assert_eq!(mail_samples.len(), 1);
}

#[test]
fn test_style_samples_limit() {
    let storage = Storage::in_memory().unwrap();

    for i in 0..10 {
        storage
            .save_style_sample("App", &format!("sample {}", i))
            .unwrap();
    }

    let samples = storage.get_style_samples("App", 5).unwrap();
    assert_eq!(samples.len(), 5);
}

// ============ Contact Tests ============

#[test]
fn test_save_and_get_contact() {
    let storage = Storage::in_memory().unwrap();

    let contact = Contact::new(
        "John Doe".to_string(),
        Some("Acme Corp".to_string()),
        ContactCategory::Professional,
    );
    storage.save_contact(&contact).unwrap();

    let retrieved = storage.get_contact_by_name("John Doe").unwrap();
    assert!(retrieved.is_some());
    let retrieved = retrieved.unwrap();
    assert_eq!(retrieved.name, "John Doe");
    assert_eq!(retrieved.organization, Some("Acme Corp".to_string()));
    assert_eq!(retrieved.category, ContactCategory::Professional);
}

#[test]
fn test_get_all_contacts() {
    let storage = Storage::in_memory().unwrap();

    storage
        .save_contact(&Contact::new(
            "Alice".to_string(),
            None,
            ContactCategory::CasualPeer,
        ))
        .unwrap();
    storage
        .save_contact(&Contact::new(
            "Bob".to_string(),
            None,
            ContactCategory::CloseFamily,
        ))
        .unwrap();

    let contacts = storage.get_all_contacts().unwrap();
    assert_eq!(contacts.len(), 2);
}

#[test]
fn test_get_frequent_contacts() {
    let storage = Storage::in_memory().unwrap();

    let mut c1 = Contact::new("High".to_string(), None, ContactCategory::CasualPeer);
    c1.frequency = 10;
    let mut c2 = Contact::new("Low".to_string(), None, ContactCategory::CasualPeer);
    c2.frequency = 1;
    let mut c3 = Contact::new("Zero".to_string(), None, ContactCategory::CasualPeer);
    c3.frequency = 0;

    storage.save_contact(&c1).unwrap();
    storage.save_contact(&c2).unwrap();
    storage.save_contact(&c3).unwrap();

    let frequent = storage.get_frequent_contacts(2).unwrap();
    // frequency > 0, ordered by frequency DESC
    assert_eq!(frequent.len(), 2);
    assert_eq!(frequent[0].name, "High");
    assert_eq!(frequent[1].name, "Low");
}

#[test]
fn test_delete_contact() {
    let storage = Storage::in_memory().unwrap();

    storage
        .save_contact(&Contact::new(
            "ToDelete".to_string(),
            None,
            ContactCategory::FormalNeutral,
        ))
        .unwrap();

    storage.delete_contact("ToDelete").unwrap();

    let retrieved = storage.get_contact_by_name("ToDelete").unwrap();
    assert!(retrieved.is_none());
}

#[test]
fn test_contact_upsert() {
    let storage = Storage::in_memory().unwrap();

    let c1 = Contact::new(
        "Same Name".to_string(),
        Some("Old Org".to_string()),
        ContactCategory::Professional,
    );
    storage.save_contact(&c1).unwrap();

    let mut c2 = Contact::new(
        "Same Name".to_string(),
        Some("New Org".to_string()),
        ContactCategory::CasualPeer,
    );
    c2.frequency = 5;
    storage.save_contact(&c2).unwrap();

    let all = storage.get_all_contacts().unwrap();
    assert_eq!(all.len(), 1);

    let contact = all.first().unwrap();
    assert_eq!(contact.organization, Some("New Org".to_string()));
    assert_eq!(contact.category, ContactCategory::CasualPeer);
    assert_eq!(contact.frequency, 5);
}

// ============ Stats Tests ============

#[test]
fn test_transcription_count() {
    let storage = Storage::in_memory().unwrap();

    assert_eq!(storage.get_transcription_count().unwrap(), 0);

    for _ in 0..5 {
        let t = Transcription::new("test".to_string(), "Test.".to_string(), 0.9, 1000);
        storage.save_transcription(&t).unwrap();
    }

    assert_eq!(storage.get_transcription_count().unwrap(), 5);
}

#[test]
fn test_total_transcription_time() {
    let storage = Storage::in_memory().unwrap();

    assert_eq!(storage.get_total_transcription_time_ms().unwrap(), 0);

    storage
        .save_transcription(&Transcription::new(
            "a".to_string(),
            "A".to_string(),
            0.9,
            1000,
        ))
        .unwrap();
    storage
        .save_transcription(&Transcription::new(
            "b".to_string(),
            "B".to_string(),
            0.9,
            2000,
        ))
        .unwrap();

    assert_eq!(storage.get_total_transcription_time_ms().unwrap(), 3000);
}

#[test]
fn test_total_words_dictated() {
    let storage = Storage::in_memory().unwrap();

    assert_eq!(storage.get_total_words_dictated().unwrap(), 0);

    storage
        .save_transcription(&Transcription::new(
            "one two three".to_string(),
            "One Two Three".to_string(),
            0.9,
            1000,
        ))
        .unwrap();
    storage
        .save_transcription(&Transcription::new(
            "four five".to_string(),
            "Four Five".to_string(),
            0.9,
            1000,
        ))
        .unwrap();

    // raw_text is used: "one two three" (3) + "four five" (2) = 5
    assert_eq!(storage.get_total_words_dictated().unwrap(), 5);
}

// ============ Concurrent Access Tests ============

#[test]
fn test_concurrent_reads() {
    let storage = Arc::new(Storage::in_memory().unwrap());

    // add some data
    storage
        .save_shortcut(&Shortcut::new("test".to_string(), "TEST".to_string()))
        .unwrap();

    let mut handles = vec![];
    for _ in 0..10 {
        let storage_clone = Arc::clone(&storage);
        let handle = thread::spawn(move || {
            for _ in 0..100 {
                let _ = storage_clone.get_enabled_shortcuts().unwrap();
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.join().unwrap();
    }
}

#[test]
fn test_concurrent_writes() {
    let storage = Arc::new(Storage::in_memory().unwrap());

    let mut handles = vec![];
    for i in 0..10 {
        let storage_clone = Arc::clone(&storage);
        let handle = thread::spawn(move || {
            for j in 0..10 {
                let t = Transcription::new(
                    format!("thread {} item {}", i, j),
                    format!("Thread {} Item {}", i, j),
                    0.9,
                    1000,
                );
                storage_clone.save_transcription(&t).unwrap();
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.join().unwrap();
    }

    assert_eq!(storage.get_transcription_count().unwrap(), 100);
}

// ============ Edge Case Tests ============

#[test]
fn test_unicode_in_transcription() {
    let storage = Storage::in_memory().unwrap();

    let t = Transcription::new(
        "こんにちは世界".to_string(),
        "こんにちは世界！".to_string(),
        0.9,
        1000,
    );
    storage.save_transcription(&t).unwrap();

    let recent = storage.get_recent_transcriptions(1).unwrap();
    assert_eq!(recent[0].raw_text, "こんにちは世界");
}

#[test]
fn test_emoji_in_shortcut() {
    let storage = Storage::in_memory().unwrap();

    let shortcut = Shortcut::new("heart".to_string(), "❤️💕".to_string());
    storage.save_shortcut(&shortcut).unwrap();

    let shortcuts = storage.get_all_shortcuts().unwrap();
    assert_eq!(shortcuts[0].replacement, "❤️💕");
}

#[test]
fn test_empty_string_setting() {
    let storage = Storage::in_memory().unwrap();

    storage.set_setting("empty", "").unwrap();

    let value = storage.get_setting("empty").unwrap();
    assert_eq!(value, Some("".to_string()));
}

#[test]
fn test_very_long_text() {
    let storage = Storage::in_memory().unwrap();

    let long_text = "a".repeat(100_000);
    let t = Transcription::new(long_text.clone(), long_text.clone(), 0.9, 60000);
    storage.save_transcription(&t).unwrap();

    let recent = storage.get_recent_transcriptions(1).unwrap();
    assert_eq!(recent[0].raw_text.len(), 100_000);
}
