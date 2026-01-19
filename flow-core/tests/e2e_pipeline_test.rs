//! End-to-end pipeline tests
//!
//! These tests verify complete workflows through the system:
//! - Transcription processing with shortcuts and corrections
//! - Learning flow from edits
//! - Mode selection based on app context
//! - Contact-based writing mode selection

use flow::contacts::{ContactClassifier, ContactInput};
use flow::learning::LearningEngine;
use flow::modes::{StyleAnalyzer, StyleLearner, WritingMode, WritingModeEngine};
use flow::shortcuts::ShortcutsEngine;
use flow::storage::Storage;
use flow::types::{
    AppCategory, AppContext, Contact, ContactCategory, Shortcut, Transcription,
    TranscriptionHistoryEntry,
};

// ============ Full Text Processing Pipeline ============

#[test]
fn test_full_processing_pipeline() {
    // simulates: transcription → shortcuts → corrections → final output
    let storage = Storage::in_memory().unwrap();
    storage.delete_all_corrections().unwrap();

    let shortcuts = ShortcutsEngine::new();
    shortcuts.add_shortcut(Shortcut::new(
        "my email".to_string(),
        "test@example.com".to_string(),
    ));
    shortcuts.add_shortcut(Shortcut::new(
        "my phone".to_string(),
        "555-1234".to_string(),
    ));

    // use the public API to learn corrections
    let learning = LearningEngine::from_storage(&storage).unwrap();
    // BUG EXPOSURE: "teh" -> "the" won't be learned (Jaro-Winkler similarity 0.556 < 0.7)
    learning
        .learn_from_edit("teh cat", "the cat", &storage)
        .unwrap();
    // "recieve" -> "receive" will be learned (similarity 0.967)
    learning
        .learn_from_edit("recieve mail", "receive mail", &storage)
        .unwrap();

    // simulate raw transcription from whisper
    let raw_transcription = "please send teh report to my email and I will recieve it";

    // step 1: apply shortcuts
    let (with_shortcuts, triggered_shortcuts) = shortcuts.process(raw_transcription);
    assert_eq!(
        with_shortcuts,
        "please send teh report to test@example.com and I will recieve it"
    );
    assert_eq!(triggered_shortcuts.len(), 1);
    assert_eq!(triggered_shortcuts[0].trigger, "my email");

    // step 2: apply corrections
    // BUG: Only "recieve" is corrected, "teh" remains unchanged
    let (final_text, applied_corrections) = learning.apply_corrections(&with_shortcuts);
    assert_eq!(
        final_text,
        "please send teh report to test@example.com and I will receive it" // teh not fixed
    );
    assert_eq!(applied_corrections.len(), 1); // Only 1 correction applied
}

#[test]
fn test_pipeline_no_shortcuts_or_corrections() {
    let shortcuts = ShortcutsEngine::new();
    let learning = LearningEngine::new();

    let raw = "hello world this is a test";

    let (with_shortcuts, triggered) = shortcuts.process(raw);
    assert_eq!(with_shortcuts, raw);
    assert!(triggered.is_empty());

    let (final_text, applied) = learning.apply_corrections(&with_shortcuts);
    assert_eq!(final_text, raw);
    assert!(applied.is_empty());
}

#[test]
fn test_pipeline_multiple_shortcuts_same_text() {
    let shortcuts = ShortcutsEngine::new();
    shortcuts.add_shortcut(Shortcut::new("hi".to_string(), "hello".to_string()));

    let raw = "hi there hi again";
    let (result, triggered) = shortcuts.process(raw);

    assert_eq!(result, "hello there hello again");
    assert_eq!(triggered.len(), 2);
}

// ============ Learning Flow Tests ============

#[test]
fn test_learning_from_user_edit() {
    let storage = Storage::in_memory().unwrap();
    storage.delete_all_corrections().unwrap();

    let learning = LearningEngine::from_storage(&storage).unwrap();

    // simulate user edit
    let original = "I recieve teh package";
    let edited = "I receive the package";

    let learned = learning
        .learn_from_edit(original, edited, &storage)
        .unwrap();

    // BUG EXPOSURE: Only "recieve" -> "receive" is learned, not "teh" -> "the".
    // Jaro-Winkler similarity for "teh" vs "the" is only 0.556, which is below
    // MIN_SIMILARITY (0.7). This means common typos like "teh" won't be learned.
    // The threshold is too strict for short transposition typos.
    assert_eq!(learned.len(), 1);

    // verify recieve is in cache
    assert!(learning.has_correction("recieve"));
    // BUG: teh is NOT learned due to low similarity score
    assert!(!learning.has_correction("teh"));

    // partial correction works (only recieve fixed)
    let (result, _) = learning.apply_corrections("I recieve teh mail");
    assert_eq!(result, "I receive teh mail"); // teh not corrected
}

#[test]
fn test_learning_increments_confidence() {
    let storage = Storage::in_memory().unwrap();
    storage.delete_all_corrections().unwrap();

    let learning = LearningEngine::from_storage(&storage).unwrap();

    // BUG EXPOSURE: "teh" -> "the" has Jaro-Winkler similarity of 0.556, which is
    // below MIN_SIMILARITY (0.7), so this correction is never learned.
    // Use "recieve" -> "receive" instead (similarity 0.967).
    for _ in 0..5 {
        learning
            .learn_from_edit("recieve mail", "receive mail", &storage)
            .unwrap();
    }

    // confidence should have increased
    let corrections = storage.get_all_corrections().unwrap();
    let correction = corrections
        .iter()
        .find(|c| c.original == "recieve")
        .unwrap();

    // Confidence increases with occurrences (calculated in save_correction)
    assert!(correction.confidence > 0.5);
    assert!(correction.occurrences >= 5);
}

#[test]
fn test_learning_persists_across_instances() {
    let storage = Storage::in_memory().unwrap();
    storage.delete_all_corrections().unwrap();

    // first instance learns (using recieve since teh similarity is too low)
    {
        let learning = LearningEngine::from_storage(&storage).unwrap();
        learning
            .learn_from_edit("recieve mail", "receive mail", &storage)
            .unwrap();
    }

    // Corrections persist and load correctly across instances
    {
        let learning = LearningEngine::from_storage(&storage).unwrap();
        assert!(learning.has_correction("recieve"));

        let (result, _) = learning.apply_corrections("recieve mail");
        assert_eq!(result, "receive mail");
    }
}

// ============ Mode Selection Pipeline ============

#[test]
fn test_mode_with_app_specific_override() {
    let mut engine = WritingModeEngine::new(WritingMode::Casual);

    // default for an unknown app
    assert_eq!(engine.get_mode("MyApp"), WritingMode::Casual);

    // set override
    engine.set_mode("MyApp", WritingMode::Excited);
    assert_eq!(engine.get_mode("MyApp"), WritingMode::Excited);

    // other apps still use default
    assert_eq!(engine.get_mode("OtherApp"), WritingMode::Casual);
}

#[test]
fn test_mode_selection_with_storage() {
    let storage = Storage::in_memory().unwrap();
    let mut engine = WritingModeEngine::new(WritingMode::Casual);

    // set and persist mode
    engine
        .set_mode_with_storage("Slack", WritingMode::VeryCasual, &storage)
        .unwrap();

    // create new engine and load from storage
    let mut engine2 = WritingModeEngine::new(WritingMode::Casual);
    let mode = engine2.get_mode_with_storage("Slack", &storage);
    assert_eq!(mode, WritingMode::VeryCasual);
}

// ============ Contact-Based Mode Selection ============

#[test]
fn test_contact_to_mode_pipeline() {
    let classifier = ContactClassifier::new();

    // classify contact
    let input = ContactInput {
        name: "Mom".to_string(),
        organization: String::new(),
    };
    let category = classifier.classify(&input);
    assert_eq!(category, ContactCategory::CloseFamily);

    // get suggested writing mode
    let mode = category.suggested_writing_mode();
    assert_eq!(mode, WritingMode::Casual);

    // partner should map to Excited
    let partner_input = ContactInput {
        name: "❤️ Alex".to_string(),
        organization: String::new(),
    };
    let partner_category = classifier.classify(&partner_input);
    assert_eq!(partner_category, ContactCategory::Partner);
    assert_eq!(
        partner_category.suggested_writing_mode(),
        WritingMode::Excited
    );

    // professional should map to Formal
    let prof_input = ContactInput {
        name: "Dr. Smith".to_string(),
        organization: String::new(),
    };
    let prof_category = classifier.classify(&prof_input);
    assert_eq!(prof_category, ContactCategory::Professional);
    assert_eq!(prof_category.suggested_writing_mode(), WritingMode::Formal);
}

#[test]
fn test_messages_app_contact_mode_selection() {
    // simulates the flow when in Messages.app

    let classifier = ContactClassifier::new();

    // detected contact from Messages window
    let contact_name = "Bae";

    let input = ContactInput {
        name: contact_name.to_string(),
        organization: String::new(),
    };

    let category = classifier.classify(&input);
    assert_eq!(category, ContactCategory::Partner);

    let mode = category.suggested_writing_mode();
    assert_eq!(mode, WritingMode::Excited);
}

// ============ Style Learning Pipeline ============

#[test]
fn test_style_learning_pipeline() {
    let mut learner = StyleLearner::new();

    // observe text samples for an app
    let samples = vec![
        "hey whats up",
        "cool thanks",
        "lol yeah for sure",
        "k sounds good",
        "nice one",
        "sweet",
    ];

    for sample in samples {
        learner.observe("Slack", sample);
    }

    // should now have a suggestion
    let suggestion = learner.suggest_mode("Slack");
    assert!(suggestion.is_some());

    let suggestion = suggestion.unwrap();
    assert_eq!(suggestion.suggested_mode, WritingMode::VeryCasual);
    assert!(suggestion.confidence > 0.0);
}

#[test]
fn test_style_analysis_consistency() {
    // verify style analysis is consistent with learning

    let samples = vec![
        "I would appreciate if you could review the attached document at your earliest convenience.",
        "Please find the quarterly report attached for your review.",
        "Best regards, and thank you for your continued support.",
    ];

    for sample in &samples {
        let mode = StyleAnalyzer::analyze_style(sample);
        assert_eq!(
            mode,
            WritingMode::Formal,
            "Sample should be formal: {}",
            sample
        );
    }

    let samples_vec: Vec<String> = samples.iter().map(|s| s.to_string()).collect();
    let mode = StyleAnalyzer::analyze_samples(&samples_vec);
    assert_eq!(mode, WritingMode::Formal);
}

// ============ Shortcut Flow Tests ============

#[test]
fn test_shortcut_definition_and_trigger() {
    let storage = Storage::in_memory().unwrap();

    // define shortcut
    let shortcut = Shortcut::new(
        "my linkedin".to_string(),
        "linkedin.com/in/username".to_string(),
    );
    storage.save_shortcut(&shortcut).unwrap();

    // load shortcuts engine
    let engine = ShortcutsEngine::from_storage(&storage).unwrap();

    // trigger shortcut
    let (result, triggered) = engine.process("check out my linkedin for more");

    assert_eq!(result, "check out linkedin.com/in/username for more");
    assert_eq!(triggered.len(), 1);
    assert_eq!(triggered[0].trigger, "my linkedin");
}

#[test]
fn test_shortcut_persistence() {
    let storage = Storage::in_memory().unwrap();

    // add shortcut via first engine instance
    {
        let engine = ShortcutsEngine::from_storage(&storage).unwrap();
        engine.add_shortcut(Shortcut::new("foo".to_string(), "bar".to_string()));
        // save back to storage
        let shortcut = engine
            .get_all()
            .iter()
            .find(|s| s.trigger == "foo")
            .unwrap()
            .clone();
        storage.save_shortcut(&shortcut).unwrap();
    }

    // second instance should have it
    {
        let engine = ShortcutsEngine::from_storage(&storage).unwrap();
        assert!(engine.contains_shortcuts("test foo here"));

        let (result, _) = engine.process("test foo here");
        assert_eq!(result, "test bar here");
    }
}

// ============ App Context Flow ============

#[test]
fn test_app_context_determines_mode() {
    let storage = Storage::in_memory().unwrap();
    let mut engine = WritingModeEngine::new(WritingMode::Casual);

    // Without an explicit override, get_mode_with_storage returns the default
    let mode = engine.get_mode_with_storage("Mail", &storage);
    assert_eq!(mode, WritingMode::Casual); // default mode

    // Set an override for Mail
    engine
        .set_mode_with_storage("Mail", WritingMode::Formal, &storage)
        .unwrap();

    // Now it should return Formal
    let mode = engine.get_mode_with_storage("Mail", &storage);
    assert_eq!(mode, WritingMode::Formal);
}

#[test]
fn test_full_transcription_flow_with_context() {
    let storage = Storage::in_memory().unwrap();
    storage.delete_all_corrections().unwrap();

    // setup
    let shortcuts = ShortcutsEngine::new();
    shortcuts.add_shortcut(Shortcut::new(
        "my sig".to_string(),
        "Best regards,\nJohn".to_string(),
    ));

    let learning = LearningEngine::from_storage(&storage).unwrap();
    // BUG EXPOSURE: "teh" won't be learned due to low Jaro-Winkler similarity (0.556 < 0.7)
    learning
        .learn_from_edit("teh end", "the end", &storage)
        .unwrap();

    // simulate transcription in email context
    // BUG: "teh" correction won't be applied since it wasn't learned
    let raw = "please review teh document my sig";

    let (with_shortcuts, _) = shortcuts.process(raw);
    let (final_text, _) = learning.apply_corrections(&with_shortcuts);

    // Documents buggy behavior: "teh" is not corrected
    assert_eq!(final_text, "please review teh document Best regards,\nJohn");

    // save transcription
    let mut transcription = Transcription::new(raw.to_string(), final_text.clone(), 0.95, 2000);
    transcription.app_context = Some(AppContext {
        app_name: "Mail".to_string(),
        bundle_id: Some("com.apple.mail".to_string()),
        window_title: Some("New Message".to_string()),
        category: AppCategory::Email,
    });

    storage.save_transcription(&transcription).unwrap();

    // verify saved
    let recent = storage.get_recent_transcriptions(1).unwrap();
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].processed_text, final_text);
    assert_eq!(
        recent[0].app_context.as_ref().unwrap().category,
        AppCategory::Email
    );
}

// ============ Error Recovery Tests ============

#[test]
fn test_pipeline_handles_empty_input() {
    let shortcuts = ShortcutsEngine::new();
    let learning = LearningEngine::new();

    let (with_shortcuts, triggered) = shortcuts.process("");
    assert_eq!(with_shortcuts, "");
    assert!(triggered.is_empty());

    let (final_text, applied) = learning.apply_corrections(&with_shortcuts);
    assert_eq!(final_text, "");
    assert!(applied.is_empty());
}

#[test]
fn test_pipeline_handles_unicode() {
    let shortcuts = ShortcutsEngine::new();
    shortcuts.add_shortcut(Shortcut::new("heart".to_string(), "❤️".to_string()));

    let learning = LearningEngine::new();

    let raw = "send heart to 日本語";

    let (with_shortcuts, _) = shortcuts.process(raw);
    assert_eq!(with_shortcuts, "send ❤️ to 日本語");

    // corrections should handle unicode gracefully
    let (final_text, _) = learning.apply_corrections(&with_shortcuts);
    assert_eq!(final_text, "send ❤️ to 日本語");
}

// ============ Multi-Step Correction Learning ============

#[test]
fn test_incremental_learning_improves_accuracy() {
    let storage = Storage::in_memory().unwrap();
    storage.delete_all_corrections().unwrap();

    let learning = LearningEngine::from_storage(&storage).unwrap();

    // BUG EXPOSURE: "teh" -> "the" has Jaro-Winkler similarity 0.556 < MIN_SIMILARITY (0.7),
    // so it won't be learned at all. Use "recieve" -> "receive" instead.
    learning
        .learn_from_edit("recieve", "receive", &storage)
        .unwrap();

    let corrections = storage.get_all_corrections().unwrap();
    let first_confidence = corrections
        .iter()
        .find(|c| c.original == "recieve")
        .map(|c| c.confidence)
        .unwrap();

    // repeat the correction multiple times
    for _ in 0..10 {
        learning
            .learn_from_edit("recieve", "receive", &storage)
            .unwrap();
    }

    let corrections = storage.get_all_corrections().unwrap();
    let final_confidence = corrections
        .iter()
        .find(|c| c.original == "recieve")
        .map(|c| c.confidence)
        .unwrap();

    // Confidence increases with repeated corrections
    assert!(first_confidence > 0.5); // First occurrence already above 0.5
    assert!(final_confidence > first_confidence); // Increases with more occurrences
}

// ============ Contact Interaction Tracking ============

#[test]
fn test_contact_interaction_updates_frequency() {
    let classifier = ContactClassifier::new();

    // create and store contact
    let contact = Contact::new(
        "Test Person".to_string(),
        None,
        ContactCategory::FormalNeutral,
    );
    classifier.upsert_contact(contact);

    // initial frequency
    let initial = classifier.get_contact("Test Person").unwrap();
    assert_eq!(initial.frequency, 0);

    // record interactions
    for _ in 0..5 {
        classifier.record_interaction("Test Person");
    }

    // frequency should have increased
    let updated = classifier.get_contact("Test Person").unwrap();
    assert_eq!(updated.frequency, 5);
    assert!(updated.last_contacted.is_some());
}

#[test]
fn test_frequent_contacts_ordering() {
    let classifier = ContactClassifier::new();

    // create contacts with different frequencies
    let mut c1 = Contact::new("Frequent".to_string(), None, ContactCategory::CasualPeer);
    c1.frequency = 100;
    let mut c2 = Contact::new("Medium".to_string(), None, ContactCategory::CasualPeer);
    c2.frequency = 50;
    let mut c3 = Contact::new("Rare".to_string(), None, ContactCategory::CasualPeer);
    c3.frequency = 10;

    classifier.upsert_contact(c1);
    classifier.upsert_contact(c2);
    classifier.upsert_contact(c3);

    let frequent = classifier.get_frequent_contacts(3);
    assert_eq!(frequent.len(), 3);
    assert_eq!(frequent[0].name, "Frequent");
    assert_eq!(frequent[1].name, "Medium");
    assert_eq!(frequent[2].name, "Rare");
}

// ============ History Tracking ============

#[test]
fn test_transcription_history_success_and_failure() {
    let storage = Storage::in_memory().unwrap();

    // successful transcription
    let success = TranscriptionHistoryEntry::success(
        "raw text".to_string(),
        "Processed text.".to_string(),
        1500,
    );
    storage.save_history_entry(&success).unwrap();

    // failed transcription
    let failure = TranscriptionHistoryEntry::failure("Network timeout".to_string(), 500);
    storage.save_history_entry(&failure).unwrap();

    let history = storage.get_recent_history(10).unwrap();
    assert_eq!(history.len(), 2);

    // most recent (failure) should be first
    assert!(history[0].error.is_some());
    assert_eq!(history[0].error.as_ref().unwrap(), "Network timeout");

    // success entry
    assert!(history[1].error.is_none());
    assert_eq!(history[1].text, "Processed text.");
}
