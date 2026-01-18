//
// FlowWispr.swift
// FlowWispr Swift Wrapper
//
// A Swift-friendly interface to the FlowWispr Rust core.
//

import CFlowWispr
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
        case .openAI: return "OpenAI GPT"
        case .gemini: return "Gemini"
        case .openRouter: return "OpenRouter"
        }
    }
}

/// Whisper model sizes for local transcription
public enum WhisperModel: UInt8, Sendable {
    case tiny = 0
    case base = 1
    case small = 2

    public var displayName: String {
        switch self {
        case .tiny: return "Tiny (75MB)"
        case .base: return "Base (142MB)"
        case .small: return "Small (466MB)"
        }
    }

    public var sizeDescription: String {
        switch self {
        case .tiny: return "Fastest, least accurate"
        case .base: return "Good balance"
        case .small: return "Better accuracy"
        }
    }
}

/// Main interface to the FlowWispr engine
public final class FlowWispr: @unchecked Sendable {
    private let handle: OpaquePointer?

    /// Initialize the FlowWispr engine
    /// - Parameter dbPath: Optional path to the SQLite database. If nil, uses default location.
    public init(dbPath: String? = nil) {
        if let path = dbPath {
            handle = path.withCString { cPath in
                flowwispr_init(cPath)
            }
        } else {
            handle = flowwispr_init(nil)
        }

    }

    deinit {
        if let handle = handle {
            flowwispr_destroy(handle)
        }
    }

    /// Check if the engine is properly initialized
    public var isInitialized: Bool {
        handle != nil
    }

    // MARK: - Audio

    /// Start audio recording
    /// - Returns: true if recording started successfully
    public func startRecording() -> Bool {
        guard let handle = handle else { return false }
        return flowwispr_start_recording(handle)
    }

    /// Stop audio recording
    /// - Returns: Duration of the recording in milliseconds, or 0 on failure
    public func stopRecording() -> UInt64 {
        guard let handle = handle else { return 0 }
        return flowwispr_stop_recording(handle)
    }

    /// Check if currently recording
    public var isRecording: Bool {
        guard let handle = handle else { return false }
        return flowwispr_is_recording(handle)
    }

    // MARK: - Transcription

    /// Transcribe the recorded audio and process it
    /// - Parameter appName: Optional name of the current app for mode selection
    /// - Returns: Processed text, or nil on failure
    public func transcribe(appName: String? = nil) -> String? {
        guard let handle = handle else { return nil }

        let result: UnsafeMutablePointer<CChar>?
        if let app = appName {
            result = app.withCString { cApp in
                flowwispr_transcribe(handle, cApp)
            }
        } else {
            result = flowwispr_transcribe(handle, nil)
        }

        guard let cString = result else { return nil }
        let string = String(cString: cString)
        flowwispr_free_string(cString)
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
                flowwispr_retry_last_transcription(handle, cApp)
            }
        } else {
            result = flowwispr_retry_last_transcription(handle, nil)
        }

        guard let cString = result else { return nil }
        let string = String(cString: cString)
        flowwispr_free_string(cString)
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
                flowwispr_add_shortcut(handle, cTrigger, cReplacement)
            }
        }
    }

    /// Remove a voice shortcut
    /// - Parameter trigger: The trigger phrase to remove
    /// - Returns: true on success
    public func removeShortcut(trigger: String) -> Bool {
        guard let handle = handle else { return false }
        return trigger.withCString { cTrigger in
            flowwispr_remove_shortcut(handle, cTrigger)
        }
    }

    /// Get the number of shortcuts
    public var shortcutCount: Int {
        guard let handle = handle else { return 0 }
        return flowwispr_shortcut_count(handle)
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
            flowwispr_set_app_mode(handle, cApp, mode.rawValue)
        }
    }

    /// Get the writing mode for an app
    /// - Parameter appName: The name of the app
    /// - Returns: The writing mode for the app
    public func getMode(for appName: String) -> WritingMode {
        guard let handle = handle else { return .casual }
        let rawValue = appName.withCString { cApp in
            flowwispr_get_app_mode(handle, cApp)
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
                flowwispr_learn_from_edit(handle, cOriginal, cEdited)
            }
        }
    }

    /// Get the number of learned corrections
    public var correctionCount: Int {
        guard let handle = handle else { return 0 }
        return flowwispr_correction_count(handle)
    }

    // MARK: - Stats

    /// Get total transcription time in minutes
    public var totalTranscriptionMinutes: UInt64 {
        guard let handle = handle else { return 0 }
        return flowwispr_total_transcription_minutes(handle)
    }

    /// Get total transcription count
    public var transcriptionCount: UInt64 {
        guard let handle = handle else { return 0 }
        return flowwispr_transcription_count(handle)
    }

    // MARK: - Configuration

    /// Check if the transcription provider is configured
    public var isConfigured: Bool {
        guard let handle = handle else { return false }
        return flowwispr_is_configured(handle)
    }

    /// Set the OpenAI API key
    /// - Parameter apiKey: The OpenAI API key
    /// - Returns: true on success
    public func setApiKey(_ apiKey: String) -> Bool {
        guard let handle = handle else { return false }
        let trimmedKey = apiKey.trimmingCharacters(in: .whitespacesAndNewlines)
        let isSet = trimmedKey.withCString { cKey in
            flowwispr_set_api_key(handle, cKey)
        }

        return isSet
    }

    /// Set the Gemini API key
    /// - Parameter apiKey: The Gemini API key
    /// - Returns: true on success
    public func setGeminiApiKey(_ apiKey: String) -> Bool {
        guard let handle = handle else { return false }
        let trimmedKey = apiKey.trimmingCharacters(in: .whitespacesAndNewlines)
        let isSet = trimmedKey.withCString { cKey in
            flowwispr_set_gemini_api_key(handle, cKey)
        }

        return isSet
    }

    /// Set the OpenRouter API key
    /// - Parameter apiKey: The OpenRouter API key
    /// - Returns: true on success
    public func setOpenRouterApiKey(_ apiKey: String) -> Bool {
        guard let handle = handle else { return false }
        let trimmedKey = apiKey.trimmingCharacters(in: .whitespacesAndNewlines)
        let isSet = trimmedKey.withCString { cKey in
            flowwispr_set_openrouter_api_key(handle, cKey)
        }

        return isSet
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
                            flowwispr_set_active_app(handle, cName, cBid, cTitle)
                        }
                    } else {
                        return flowwispr_set_active_app(handle, cName, cBid, nil)
                    }
                }
            } else {
                if let title = windowTitle {
                    return title.withCString { cTitle in
                        flowwispr_set_active_app(handle, cName, nil, cTitle)
                    }
                } else {
                    return flowwispr_set_active_app(handle, cName, nil, nil)
                }
            }
        }

        return WritingMode(rawValue: rawMode) ?? .casual
    }

    /// Get the current app's category
    public var currentAppCategory: AppCategory {
        guard let handle = handle else { return .unknown }
        let rawValue = flowwispr_get_app_category(handle)
        return AppCategory(rawValue: rawValue) ?? .unknown
    }

    /// Get the current app name
    public var currentAppName: String? {
        guard let handle = handle else { return nil }
        guard let cString = flowwispr_get_current_app(handle) else { return nil }
        let string = String(cString: cString)
        flowwispr_free_string(cString)
        return string
    }

    // MARK: - Style Learning

    /// Report edited text to learn user's style
    /// - Parameter editedText: The text after user edits
    /// - Returns: true on success
    public func learnStyle(editedText: String) -> Bool {
        guard let handle = handle else { return false }
        return editedText.withCString { cText in
            flowwispr_learn_style(handle, cText)
        }
    }

    /// Get suggested mode based on learned style for current app
    /// - Returns: Suggested mode or nil if not enough data
    public var styleSuggestion: WritingMode? {
        guard let handle = handle else { return nil }
        let rawValue = flowwispr_get_style_suggestion(handle)
        if rawValue == 255 { return nil }
        return WritingMode(rawValue: rawValue)
    }

    // MARK: - Extended Stats

    /// Get stats as a dictionary
    public var stats: [String: Any]? {
        guard let handle = handle else { return nil }
        guard let cString = flowwispr_get_stats_json(handle) else { return nil }
        let jsonString = String(cString: cString)
        flowwispr_free_string(cString)

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
        guard let cString = flowwispr_get_recent_transcriptions_json(handle, limit) else { return [] }
        let jsonString = String(cString: cString)
        flowwispr_free_string(cString)

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
        guard let cString = flowwispr_get_last_error(handle) else { return nil }
        let string = String(cString: cString)
        flowwispr_free_string(cString)
        return string
    }

    /// Get all shortcuts as array of dictionaries
    public var shortcuts: [[String: Any]]? {
        guard let handle = handle else { return nil }
        guard let cString = flowwispr_get_shortcuts_json(handle) else { return nil }
        let jsonString = String(cString: cString)
        flowwispr_free_string(cString)

        guard let data = jsonString.data(using: .utf8),
              let json = try? JSONSerialization.jsonObject(with: data) as? [[String: Any]] else {
            return nil
        }
        return json
    }

    // MARK: - Provider Configuration

    /// Set the completion provider
    /// - Parameters:
    ///   - provider: The provider to use
    ///   - apiKey: The API key for the provider
    /// - Returns: true on success
    public func setCompletionProvider(_ provider: CompletionProvider, apiKey: String) -> Bool {
        guard let handle = handle else { return false }
        let trimmedKey = apiKey.trimmingCharacters(in: .whitespacesAndNewlines)
        let isSet = trimmedKey.withCString { cKey in
            flowwispr_set_completion_provider(handle, provider.rawValue, cKey)
        }

        return isSet
    }

    /// Get the current completion provider
    public var completionProvider: CompletionProvider? {
        guard let handle = handle else { return nil }
        let rawValue = flowwispr_get_completion_provider(handle)
        return CompletionProvider(rawValue: rawValue)
    }

    /// Enable local Whisper transcription with Metal acceleration
    /// - Parameter model: The Whisper model to use
    /// - Returns: true on success
    public func enableLocalWhisper(_ model: WhisperModel) -> Bool {
        guard let handle = handle else { return false }
        return flowwispr_enable_local_whisper(handle, model.rawValue)
    }

    // Configuration persistence is handled in the core database.
}
