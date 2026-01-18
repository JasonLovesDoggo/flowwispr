//
// Flow.swift
// Flow Swift Wrapper
//
// A Swift-friendly interface to the Flow Rust core.
//

import CFlow
import Foundation

/// Writing modes for text style adjustment
public enum WritingMode: UInt8, Sendable, CaseIterable {
    case formal = 0
    case casual = 1
    case veryCasual = 2
    case excited = 3

    public var displayName: String {
        switch self {
        case .formal: return "Formal"
        case .casual: return "Casual"
        case .veryCasual: return "Very Casual"
        case .excited: return "Excited"
        }
    }

    public var description: String {
        switch self {
        case .formal: return "Professional with full punctuation"
        case .casual: return "Conversational but clear"
        case .veryCasual: return "Lowercase, minimal punctuation"
        case .excited: return "Energetic with exclamation!"
        }
    }
}

/// App categories for mode suggestions
public enum AppCategory: UInt8, Sendable {
    case email = 0
    case slack = 1
    case code = 2
    case documents = 3
    case social = 4
    case browser = 5
    case terminal = 6
    case unknown = 7

    public var displayName: String {
        switch self {
        case .email: return "Email"
        case .slack: return "Chat"
        case .code: return "Code"
        case .documents: return "Documents"
        case .social: return "Social"
        case .browser: return "Browser"
        case .terminal: return "Terminal"
        case .unknown: return "Other"
        }
    }
}

/// Completion provider options
public enum CompletionProvider: UInt8, Sendable {
    case openAI = 0
    case gemini = 1
    case openRouter = 2

    public var displayName: String {
        switch self {
        case .openAI: return "OpenAI"
        case .gemini: return "Gemini"
        case .openRouter: return "OpenRouter"
        }
    }
}

/// Cloud transcription provider options (for remote transcription)
public enum CloudTranscriptionProvider: UInt8, Sendable, CaseIterable {
    case openAI = 0
    case auto = 1

    public var displayName: String {
        switch self {
        case .openAI: return "OpenAI"
        case .auto: return "Auto"
        }
    }

    public var description: String {
        switch self {
        case .openAI: return "OpenAI Whisper API"
        case .auto: return "Fastest available (recommended)"
        }
    }
}

/// Whisper model sizes for local transcription
public enum WhisperModel: UInt8, Sendable {
    case turbo = 0     // Quantized tiny (~15MB) - blazing fast
    case fast = 1      // Tiny (~39MB)
    case balanced = 2  // Base (~142MB)
    case quality = 3   // Distil-medium (~400MB) - recommended
    case best = 4      // Distil-large-v3 (~750MB)

    public var displayName: String {
        switch self {
        case .turbo: return "Turbo"
        case .fast: return "Fast"
        case .balanced: return "Balanced"
        case .quality: return "Quality"
        case .best: return "Best"
        }
    }

    public var sizeDescription: String {
        switch self {
        case .turbo: return "~15MB, quantized, blazing fast"
        case .fast: return "~39MB, good speed"
        case .balanced: return "~142MB, balanced"
        case .quality: return "~400MB, recommended"
        case .best: return "~750MB, highest accuracy"
        }
    }

}

/// Transcription mode: local or remote
public enum TranscriptionMode: Sendable {
    case local(model: WhisperModel)
    case remote

    public var displayName: String {
        switch self {
        case .local(let model): return "Local (\(model.displayName))"
        case .remote: return "Cloud API"
        }
    }
}

/// A learned correction from user edits
public struct Correction: Identifiable, Sendable {
    public let id: String
    public let original: String
    public let corrected: String
    public let occurrences: Int
    public let confidence: Double
    public let source: String
    public let createdAt: Date
    public let updatedAt: Date
}

/// Validation result for a correction pair
public struct CorrectionValidation: Sendable {
    public let original: String
    public let corrected: String
    public let valid: Bool
    public let reason: String?
}

/// Main interface to the Flow engine
public final class Flow: @unchecked Sendable {
    private let handle: OpaquePointer?

    /// Initialize the Flow engine
    /// - Parameter dbPath: Optional path to the SQLite database. If nil, uses default location.
    public init(dbPath: String? = nil) {
        let path = dbPath ?? {
            let fm = FileManager.default
            let appSupport = fm.urls(for: .applicationSupportDirectory, in: .userDomainMask).first!
            let flowDir = appSupport.appendingPathComponent("flow")

            // Create directory if needed
            try? fm.createDirectory(at: flowDir, withIntermediateDirectories: true)

            return flowDir.appendingPathComponent("flow.db").path
        }()

        handle = path.withCString { cPath in
            flow_init(cPath)
        }
    }

    deinit {
        if let handle = handle {
            flow_destroy(handle)
        }
    }

    /// Check if the engine is properly initialized
    public var isInitialized: Bool {
        handle != nil
    }

    // MARK: - Configuration

    /// Check if the transcription provider is configured
    public var isConfigured: Bool {
        guard let handle = handle else { return false }
        return flow_is_configured(handle)
    }

    // MARK: - Audio

    /// Start audio recording
    /// - Returns: true if recording started successfully
    public func startRecording() -> Bool {
        guard let handle = handle else { return false }
        return flow_start_recording(handle)
    }

    /// Stop audio recording
    /// - Returns: Duration of the recording in milliseconds, or 0 on failure
    public func stopRecording() -> UInt64 {
        guard let handle = handle else { return 0 }
        return flow_stop_recording(handle)
    }

    /// Check if currently recording
    public var isRecording: Bool {
        guard let handle = handle else { return false }
        return flow_is_recording(handle)
    }

    /// Get current audio level (RMS amplitude) from the recording
    /// - Returns: A value between 0.0 and 1.0, or 0.0 if not recording
    public var audioLevel: Float {
        guard let handle = handle else { return 0.0 }
        return flow_get_audio_level(handle)
    }

    // MARK: - Transcription

    /// Transcribe the recorded audio and process it
    /// - Parameter appName: Optional name of the current app for mode selection
    /// - Returns: Processed text, or nil on failure
    public func transcribe(appName: String? = nil) -> String? {
        guard let handle = handle else { return nil }

        let startTime = Date()
        print("[\(ISO8601DateFormatter().string(from: startTime))] 🦀 [RUST/FFI] Entering Rust transcription")

        let result: UnsafeMutablePointer<CChar>?
        if let app = appName {
            result = app.withCString { cApp in
                flow_transcribe(handle, cApp)
            }
        } else {
            result = flow_transcribe(handle, nil)
        }

        let endTime = Date()
        let duration = endTime.timeIntervalSince(startTime)
        print("[\(ISO8601DateFormatter().string(from: endTime))] 🦀 [RUST/FFI] Exited Rust transcription - Duration: \(String(format: "%.2f", duration))s")

        guard let cString = result else { return nil }
        let string = String(cString: cString)
        flow_free_string(cString)
        return string
    }

    /// Retry the last transcription using cached audio
    /// - Parameter appName: Optional name of the current app for mode selection
    /// - Returns: Processed text, or nil on failure
    public func retryLastTranscription(appName: String? = nil) -> String? {
        guard let handle = handle else { return nil }

        let result: UnsafeMutablePointer<CChar>?
        if let app = appName {
            result = app.withCString { cApp in
                flow_retry_last_transcription(handle, cApp)
            }
        } else {
            result = flow_retry_last_transcription(handle, nil)
        }

        guard let cString = result else { return nil }
        let string = String(cString: cString)
        flow_free_string(cString)
        return string
    }

    // MARK: - Shortcuts

    /// Add a voice shortcut
    /// - Parameters:
    ///   - trigger: The trigger phrase
    ///   - replacement: The replacement text
    /// - Returns: true on success
    public func addShortcut(trigger: String, replacement: String) -> Bool {
        guard let handle = handle else { return false }
        return trigger.withCString { cTrigger in
            replacement.withCString { cReplacement in
                flow_add_shortcut(handle, cTrigger, cReplacement)
            }
        }
    }

    /// Remove a voice shortcut
    /// - Parameter trigger: The trigger phrase to remove
    /// - Returns: true on success
    public func removeShortcut(trigger: String) -> Bool {
        guard let handle = handle else { return false }
        return trigger.withCString { cTrigger in
            flow_remove_shortcut(handle, cTrigger)
        }
    }

    /// Get the number of shortcuts
    public var shortcutCount: Int {
        guard let handle = handle else { return 0 }
        return flow_shortcut_count(handle)
    }

    // MARK: - Writing Modes

    /// Set the writing mode for an app
    /// - Parameters:
    ///   - mode: The writing mode to set
    ///   - appName: The name of the app
    /// - Returns: true on success
    public func setMode(_ mode: WritingMode, for appName: String) -> Bool {
        guard let handle = handle else { return false }
        return appName.withCString { cApp in
            flow_set_app_mode(handle, cApp, mode.rawValue)
        }
    }

    /// Get the writing mode for an app
    /// - Parameter appName: The name of the app
    /// - Returns: The writing mode for the app
    public func getMode(for appName: String) -> WritingMode {
        guard let handle = handle else { return .casual }
        let rawValue = appName.withCString { cApp in
            flow_get_app_mode(handle, cApp)
        }
        return WritingMode(rawValue: rawValue) ?? .casual
    }

    // MARK: - Learning

    /// Report a user edit to learn from
    /// - Parameters:
    ///   - original: The original transcribed text
    ///   - edited: The text after user edits
    /// - Returns: true on success
    public func learnFromEdit(original: String, edited: String) -> Bool {
        guard let handle = handle else { return false }
        return original.withCString { cOriginal in
            edited.withCString { cEdited in
                flow_learn_from_edit(handle, cOriginal, cEdited)
            }
        }
    }

    /// Get the number of learned corrections
    public var correctionCount: Int {
        guard let handle = handle else { return 0 }
        return flow_correction_count(handle)
    }

    /// Get all learned corrections
    public var corrections: [Correction] {
        guard let handle = handle else { return [] }
        guard let cString = flow_get_corrections_json(handle) else { return [] }
        let jsonString = String(cString: cString)
        flow_free_string(cString)

        guard let data = jsonString.data(using: .utf8) else { return [] }

        struct RawCorrection: Decodable {
            let id: String
            let original: String
            let corrected: String
            let occurrences: Int
            let confidence: Double
            let source: String
            let created_at: String
            let updated_at: String
        }

        let decoder = JSONDecoder()
        guard let rawCorrections = try? decoder.decode([RawCorrection].self, from: data) else {
            return []
        }

        let dateFormatter = ISO8601DateFormatter()
        dateFormatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]

        let standardFormatter = ISO8601DateFormatter()
        standardFormatter.formatOptions = [.withInternetDateTime]

        return rawCorrections.map { raw in
            let createdAt = dateFormatter.date(from: raw.created_at)
                ?? standardFormatter.date(from: raw.created_at)
                ?? Date()
            let updatedAt = dateFormatter.date(from: raw.updated_at)
                ?? standardFormatter.date(from: raw.updated_at)
                ?? Date()

            return Correction(
                id: raw.id,
                original: raw.original,
                corrected: raw.corrected,
                occurrences: raw.occurrences,
                confidence: raw.confidence,
                source: raw.source,
                createdAt: createdAt,
                updatedAt: updatedAt
            )
        }
    }

    /// Delete a correction by ID
    /// - Parameter id: The UUID string of the correction to delete
    /// - Returns: true if the correction was deleted
    public func deleteCorrection(id: String) -> Bool {
        guard let handle = handle else { return false }
        return id.withCString { cId in
            flow_delete_correction(handle, cId)
        }
    }

    /// Delete all corrections
    /// - Returns: The number of corrections deleted
    @discardableResult
    public func deleteAllCorrections() -> Int {
        guard let handle = handle else { return 0 }
        return flow_delete_all_corrections(handle)
    }

    /// Validate corrections using AI before learning
    /// - Parameter corrections: Array of (original, corrected) pairs to validate
    /// - Returns: Array of validation results, or nil on error
    public func validateCorrections(_ corrections: [(original: String, corrected: String)]) -> [CorrectionValidation]? {
        guard let handle = handle else { return nil }

        // Build JSON array
        let jsonArray: [[String: String]] = corrections.map { pair in
            ["original": pair.original, "corrected": pair.corrected]
        }

        guard let jsonData = try? JSONSerialization.data(withJSONObject: jsonArray),
              let jsonString = String(data: jsonData, encoding: .utf8) else {
            return nil
        }

        let cResult = jsonString.withCString { cJson in
            flow_validate_corrections(handle, cJson)
        }

        guard let cString = cResult else { return nil }
        let resultString = String(cString: cString)
        flow_free_string(cString)

        guard let resultData = resultString.data(using: .utf8) else { return nil }

        struct RawValidation: Decodable {
            let original: String
            let corrected: String
            let valid: Bool
            let reason: String?
        }

        guard let rawResults = try? JSONDecoder().decode([RawValidation].self, from: resultData) else {
            return nil
        }

        return rawResults.map { raw in
            CorrectionValidation(
                original: raw.original,
                corrected: raw.corrected,
                valid: raw.valid,
                reason: raw.reason
            )
        }
    }

    // MARK: - Stats

    /// Get total transcription time in minutes
    public var totalTranscriptionMinutes: UInt64 {
        guard let handle = handle else { return 0 }
        return flow_total_transcription_minutes(handle)
    }

    /// Get total transcription count
    public var transcriptionCount: UInt64 {
        guard let handle = handle else { return 0 }
        return flow_transcription_count(handle)
    }

    // MARK: - App Tracking

    /// Set the currently active app
    /// - Parameters:
    ///   - appName: Name of the app
    ///   - bundleId: Optional bundle identifier
    ///   - windowTitle: Optional window title
    /// - Returns: Suggested writing mode for the app
    @discardableResult
    public func setActiveApp(name appName: String, bundleId: String? = nil, windowTitle: String? = nil) -> WritingMode {
        guard let handle = handle else { return .casual }

        let rawMode = appName.withCString { cName in
            if let bid = bundleId {
                return bid.withCString { cBid in
                    if let title = windowTitle {
                        return title.withCString { cTitle in
                            flow_set_active_app(handle, cName, cBid, cTitle)
                        }
                    } else {
                        return flow_set_active_app(handle, cName, cBid, nil)
                    }
                }
            } else {
                if let title = windowTitle {
                    return title.withCString { cTitle in
                        flow_set_active_app(handle, cName, nil, cTitle)
                    }
                } else {
                    return flow_set_active_app(handle, cName, nil, nil)
                }
            }
        }

        return WritingMode(rawValue: rawMode) ?? .casual
    }

    /// Get the current app's category
    public var currentAppCategory: AppCategory {
        guard let handle = handle else { return .unknown }
        let rawValue = flow_get_app_category(handle)
        return AppCategory(rawValue: rawValue) ?? .unknown
    }

    /// Get the current app name
    public var currentAppName: String? {
        guard let handle = handle else { return nil }
        guard let cString = flow_get_current_app(handle) else { return nil }
        let string = String(cString: cString)
        flow_free_string(cString)
        return string
    }

    // MARK: - Style Learning

    /// Report edited text to learn user's style
    /// - Parameter editedText: The text after user edits
    /// - Returns: true on success
    public func learnStyle(editedText: String) -> Bool {
        guard let handle = handle else { return false }
        return editedText.withCString { cText in
            flow_learn_style(handle, cText)
        }
    }

    /// Get suggested mode based on learned style for current app
    /// - Returns: Suggested mode or nil if not enough data
    public var styleSuggestion: WritingMode? {
        guard let handle = handle else { return nil }
        let rawValue = flow_get_style_suggestion(handle)
        if rawValue == 255 { return nil }
        return WritingMode(rawValue: rawValue)
    }

    // MARK: - Extended Stats

    /// Get stats as a dictionary
    public var stats: [String: Any]? {
        guard let handle = handle else { return nil }
        guard let cString = flow_get_stats_json(handle) else { return nil }
        let jsonString = String(cString: cString)
        flow_free_string(cString)

        guard let data = jsonString.data(using: .utf8),
              let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            return nil
        }
        return json
    }

    /// Get recent transcriptions
    /// - Parameter limit: Maximum number of items to return
    public func recentTranscriptions(limit: Int = 50) -> [TranscriptionSummary] {
        guard let handle = handle else { return [] }
        guard let cString = flow_get_recent_transcriptions_json(handle, limit) else { return [] }
        let jsonString = String(cString: cString)
        flow_free_string(cString)

        guard let data = jsonString.data(using: .utf8) else { return [] }
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .custom { decoder in
            let container = try decoder.singleValueContainer()
            let dateString = try container.decode(String.self)

            let fractionalFormatter = ISO8601DateFormatter()
            fractionalFormatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
            if let date = fractionalFormatter.date(from: dateString) {
                return date
            }

            let standardFormatter = ISO8601DateFormatter()
            standardFormatter.formatOptions = [.withInternetDateTime]
            if let date = standardFormatter.date(from: dateString) {
                return date
            }

            throw DecodingError.dataCorruptedError(
                in: container,
                debugDescription: "Invalid date: \(dateString)"
            )
        }
        return (try? decoder.decode([TranscriptionSummary].self, from: data)) ?? []
    }

    /// Get the most recent error from the engine
    public var lastError: String? {
        guard let handle = handle else { return nil }
        guard let cString = flow_get_last_error(handle) else { return nil }
        let string = String(cString: cString)
        flow_free_string(cString)
        return string
    }

    /// Get all shortcuts as array of dictionaries
    public var shortcuts: [[String: Any]]? {
        guard let handle = handle else { return nil }
        guard let cString = flow_get_shortcuts_json(handle) else { return nil }
        let jsonString = String(cString: cString)
        flow_free_string(cString)

        guard let data = jsonString.data(using: .utf8),
              let json = try? JSONSerialization.jsonObject(with: data) as? [[String: Any]] else {
            return nil
        }
        return json
    }

    // MARK: - Provider Configuration

    /// Switch the completion provider (loads API key from database)
    /// - Parameter provider: The provider to use
    /// - Returns: true on success
    public func switchCompletionProvider(_ provider: CompletionProvider) -> Bool {
        guard let handle = handle else { return false }
        return flow_switch_completion_provider(handle, provider.rawValue)
    }

    /// Set the completion provider with API key (saves both)
    /// - Parameters:
    ///   - provider: The provider to use
    ///   - apiKey: The API key for the provider
    /// - Returns: true on success
    public func setCompletionProvider(_ provider: CompletionProvider, apiKey: String) -> Bool {
        guard let handle = handle else { return false }
        let trimmedKey = apiKey.trimmingCharacters(in: .whitespacesAndNewlines)
        let isSet = trimmedKey.withCString { cKey in
            flow_set_completion_provider(handle, provider.rawValue, cKey)
        }

        return isSet
    }

    /// Get the current completion provider
    public var completionProvider: CompletionProvider? {
        guard let handle = handle else { return nil }
        let rawValue = flow_get_completion_provider(handle)
        return CompletionProvider(rawValue: rawValue)
    }

    /// Get API key for a specific provider in masked form
    /// - Parameter provider: The provider to get the key for
    /// - Returns: Masked API key (e.g., "sk-••••••••") or nil if not set
    public func getMaskedApiKey(for provider: CompletionProvider) -> String? {
        guard let handle = handle else { return nil }
        guard let cString = flow_get_api_key(handle, provider.rawValue) else { return nil }
        let string = String(cString: cString)
        flow_free_string(cString)
        return string
    }

    /// Get the OpenAI API key in masked form (e.g., "sk-••••••••")
    /// - Returns: Masked API key or nil if not set
    public var maskedOpenAIKey: String? {
        getMaskedApiKey(for: .openAI)
    }

    /// Get the Gemini API key in masked form (e.g., "AI••••••••")
    /// - Returns: Masked API key or nil if not set
    public var maskedGeminiKey: String? {
        getMaskedApiKey(for: .gemini)
    }

    /// Get the OpenRouter API key in masked form
    /// - Returns: Masked API key or nil if not set
    public var maskedOpenRouterKey: String? {
        getMaskedApiKey(for: .openRouter)
    }

    /// Set transcription mode (local or remote)
    /// - Parameter mode: The transcription mode to use
    /// - Returns: true on success
    public func setTranscriptionMode(_ mode: TranscriptionMode) -> Bool {
        guard let handle = handle else { return false }

        switch mode {
        case .local(let model):
            return flow_set_transcription_mode(handle, true, model.rawValue)
        case .remote:
            return flow_set_transcription_mode(handle, false, 0) // model doesn't matter for remote
        }
    }

    /// Legacy: Enable local Whisper transcription with Metal acceleration
    /// - Parameter model: The Whisper model to use
    /// - Returns: true on success
    @available(*, deprecated, message: "Use setTranscriptionMode(.local(model:)) instead")
    public func enableLocalWhisper(_ model: WhisperModel) -> Bool {
        return setTranscriptionMode(.local(model: model))
    }

    /// Get current transcription mode settings from database
    /// - Returns: TranscriptionMode (local with model or remote), or nil on error
    public func getTranscriptionMode() -> TranscriptionMode? {
        guard let handle = handle else { return nil }

        var useLocal: Bool = false
        var whisperModel: UInt8 = 3 // default to quality

        guard flow_get_transcription_mode(handle, &useLocal, &whisperModel) else {
            return nil
        }

        if useLocal {
            let model = WhisperModel(rawValue: whisperModel) ?? .quality
            return .local(model: model)
        } else {
            return .remote
        }
    }

    /// Check if a Whisper model is currently being downloaded/initialized
    /// - Returns: true if model download is in progress
    public func isModelLoading() -> Bool {
        guard let handle = handle else { return false }
        return flow_is_model_loading(handle)
    }

    // MARK: - Cloud Transcription Provider

    /// Set the cloud transcription provider
    /// - Parameter provider: The cloud transcription provider to use
    /// - Returns: true on success
    public func setCloudTranscriptionProvider(_ provider: CloudTranscriptionProvider) -> Bool {
        guard let handle = handle else { return false }
        return flow_set_cloud_transcription_provider(handle, provider.rawValue)
    }

    /// Get the current cloud transcription provider
    public var cloudTranscriptionProvider: CloudTranscriptionProvider? {
        guard let handle = handle else { return nil }
        let rawValue = flow_get_cloud_transcription_provider(handle)
        return CloudTranscriptionProvider(rawValue: rawValue)
    }

    // Configuration persistence is handled in the core database.
}
