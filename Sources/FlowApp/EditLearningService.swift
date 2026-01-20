//
//  EditLearningService.swift
//  Flow
//
//  Monitors text field edits after transcription paste to learn user corrections.
//  Uses AX-based monitoring with Needleman-Wunsch alignment for edit detection.
//

import AppKit
import ApplicationServices
import Flow

/// Service that detects when users edit pasted transcription text and triggers learning.
///
/// After a transcription is pasted:
/// 1. Uses AX notifications to monitor text changes (event-driven, not polling)
/// 2. When text is stable for 1.5+ seconds, runs alignment via Rust
/// 3. Filters corrections through proper noun detection API
/// 4. Shows toast notification with undo option
final class EditLearningService {
    static let shared = EditLearningService()

    // MARK: - Configuration

    /// How often to poll the text field as fallback (seconds)
    private let pollInterval: TimeInterval = 1.0

    /// How long text must be unchanged before we consider it "stable" (seconds)
    private let stabilityThreshold: TimeInterval = 2.0

    /// Maximum time to monitor before giving up (seconds)
    private let maxMonitoringDuration: TimeInterval = 15.0

    /// Minimum text length to bother learning from
    private let minimumTextLength = 10

    /// Minimum word overlap ratio required (edited text should share words with original)
    private let minimumWordOverlap: Double = 0.3

    // MARK: - State

    /// Reference to the Flow engine for calling Rust functions
    private var engine: Flow?

    /// AX-based monitor (preferred method)
    private let axMonitor = AXEditMonitorService()

    /// Timer for polling (fallback)
    private var pollTimer: Timer?

    /// Original text that was pasted
    private var originalText: String?

    /// Target app's PID for the current monitoring session
    private var targetAppPID: pid_t?

    /// When monitoring started
    private var monitoringStartTime: Date?

    /// Last text we read from the field
    private var lastReadText: String?

    /// Last change time
    private var lastChangeTime: Date?

    /// Known bad values that indicate we read the wrong element
    private let invalidPatterns = [
        "untitled",
        "new document",
        "new tab",
        "loading",
        "about:blank"
    ]

    /// Worker URL for proper noun extraction
    private let workerBaseURL = "https://flow-transcribe.flow-voice.workers.dev"

    private init() {}

    // MARK: - Public Methods

    /// Configure the service with the Flow engine
    func configure(engine: Flow) {
        self.engine = engine
    }

    /// Start monitoring for edits after a paste operation
    /// - Parameters:
    ///   - originalText: The text that was pasted
    ///   - targetApp: The application where text was pasted
    func startMonitoring(originalText: String, targetApp: NSRunningApplication?) {
        // Cancel any existing monitoring
        cancelMonitoring()

        // Don't bother with very short text
        guard originalText.count >= minimumTextLength else {
            log("Skipping: text too short (\(originalText.count) chars)")
            return
        }

        self.originalText = originalText
        self.targetAppPID = targetApp?.processIdentifier
        self.monitoringStartTime = Date()
        self.lastReadText = nil
        self.lastChangeTime = Date()

        log("Starting edit monitoring for \(originalText.count) chars in \(targetApp?.localizedName ?? "Unknown")")

        // Try AX-based monitoring first (preferred)
        if let pid = targetAppPID,
           let element = AXEditMonitorService.getFocusedTextElement(pid: pid) {

            axMonitor.onEditDetected = { [weak self] original, edited in
                self?.processEdit(original: original, edited: edited)
            }
            axMonitor.startMonitoring(element: element, originalText: originalText)
            log("Using AX notification-based monitoring")
            return
        }

        // Fall back to polling-based monitoring
        log("Falling back to polling-based monitoring")
        pollTimer = Timer.scheduledTimer(withTimeInterval: pollInterval, repeats: true) { [weak self] _ in
            self?.pollTextElement()
        }
    }

    /// Cancel any pending monitoring
    func cancelMonitoring() {
        pollTimer?.invalidate()
        pollTimer = nil
        axMonitor.stopMonitoring()
        cleanup()
    }

    /// Undo the most recent learned words
    func undoLastLearnedWords() {
        guard let engine = engine else { return }

        if engine.undoLearnedWords() {
            log("Successfully undid last learned words")
        } else {
            log("No learned words to undo")
        }
    }

    // MARK: - Private Methods

    private func pollTextElement() {
        guard let original = originalText,
              let pid = targetAppPID,
              let startTime = monitoringStartTime else {
            cancelMonitoring()
            return
        }

        // Check if we've exceeded max monitoring time
        let elapsed = Date().timeIntervalSince(startTime)
        if elapsed > maxMonitoringDuration {
            log("Monitoring timeout after \(Int(elapsed))s")
            // Try to learn from whatever we have
            if let lastText = lastReadText, lastText != original {
                log("Using last captured text as final edit")
                processEdit(original: original, edited: lastText)
            }
            cancelMonitoring()
            return
        }

        // Try to read the focused text element
        guard let (currentText, role) = readFocusedTextElement(pid: pid) else {
            // Lost focus - treat this as "done editing" signal
            if let lastText = lastReadText, lastText != original {
                log("Lost focus, treating last text as final edit")
                processEdit(original: original, edited: lastText)
            } else {
                log("Lost focus on text element, no edits detected")
            }
            cancelMonitoring()
            return
        }

        // Validate the role is a text input
        let validRoles = ["AXTextArea", "AXTextField", "AXTextView", "AXWebArea", "AXStaticText"]
        let roleIsValid = validRoles.contains { role.contains($0) }
        if !roleIsValid {
            return
        }

        // Skip if text looks like a title/placeholder
        if isInvalidText(currentText) {
            return
        }

        // Check if text changed since last poll
        if currentText != lastReadText {
            lastReadText = currentText
            lastChangeTime = Date()
            log("Text changed, resetting stability timer")
            return
        }

        // Text is the same as last poll, check if stable long enough
        guard let lastChange = lastChangeTime else { return }
        let stableFor = Date().timeIntervalSince(lastChange)

        if stableFor >= stabilityThreshold {
            log("Text stable for \(Int(stableFor))s, processing edits")
            processEdit(original: original, edited: currentText)
            cancelMonitoring()
        }
    }

    /// Process detected edit using alignment algorithm
    private func processEdit(original: String, edited: String) {
        guard let engine = engine else { return }

        // Skip if texts are identical (no edits made)
        if edited == original {
            log("No edits detected, text unchanged")
            return
        }

        // Validate there's meaningful word overlap
        let overlap = wordOverlapRatio(original: original, edited: edited)
        if overlap < minimumWordOverlap {
            log("Insufficient word overlap (\(Int(overlap * 100))%), probably wrong element")
            return
        }

        // Get alignment result from Rust
        guard let alignmentJSON = engine.alignAndExtractCorrections(original: original, edited: edited),
              let alignmentData = alignmentJSON.data(using: .utf8),
              let alignment = try? JSONDecoder().decode(AlignmentResult.self, from: alignmentData) else {
            log("Failed to get alignment from Rust")
            // Fall back to legacy learning
            let _ = engine.learnFromEdit(original: original, edited: edited)
            return
        }

        log("Alignment: \(alignment.wordEditVector)")
        log("Found \(alignment.corrections.count) potential correction(s)")

        guard !alignment.corrections.isEmpty else {
            log("No corrections detected")
            return
        }

        // Get the corrected words for proper noun filtering
        let correctedWords = alignment.corrections.map { $0.corrected }.joined(separator: " ")

        // Filter through proper noun API
        Task {
            let properNouns = await filterProperNouns(words: correctedWords)

            guard !properNouns.isEmpty else {
                log("No proper nouns detected, skipping learning")
                return
            }

            log("Detected proper nouns: \(properNouns.joined(separator: ", "))")

            // Filter corrections to only proper nouns
            let filteredCorrections = alignment.corrections.filter { correction in
                properNouns.contains { $0.lowercased() == correction.corrected.lowercased() }
            }

            guard !filteredCorrections.isEmpty else {
                log("No proper noun corrections to learn")
                return
            }

            // Learn each correction
            var learnedWords: [String] = []
            for correction in filteredCorrections {
                log("Learning: '\(correction.original)' -> '\(correction.corrected)'")

                // Use the existing learn mechanism which will save to DB
                let _ = engine.learnFromEdit(original: correction.original, edited: correction.corrected)
                learnedWords.append(correction.corrected)
            }

            // Save learned words session for undo
            if !learnedWords.isEmpty {
                engine.saveLearnedWordsSession(words: learnedWords)

                // Save edit analytics
                engine.saveEditAnalytics(
                    wordEditVector: alignment.wordEditVector,
                    punctEditVector: alignment.punctEditVector,
                    original: original,
                    edited: edited
                )

                // Show toast notification on main thread
                let wordsToShow = learnedWords
                await MainActor.run {
                    showLearnedWordsToast(words: wordsToShow)
                }
            }
        }
    }

    /// Filter words through proper noun detection API
    private func filterProperNouns(words: String) async -> [String] {
        guard !words.isEmpty else { return [] }

        // Build request
        guard let url = URL(string: "\(workerBaseURL)/extract-proper-nouns") else {
            return []
        }

        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")

        let body: [String: String] = ["potential_words": words]
        request.httpBody = try? JSONEncoder().encode(body)

        do {
            let (data, response) = try await URLSession.shared.data(for: request)

            guard let httpResponse = response as? HTTPURLResponse,
                  httpResponse.statusCode == 200 else {
                log("Proper noun API returned non-200")
                return []
            }

            struct ProperNounResponse: Decodable {
                let words: String
            }

            let result = try JSONDecoder().decode(ProperNounResponse.self, from: data)

            // Parse comma-separated list
            return result.words
                .split(separator: ",")
                .map { $0.trimmingCharacters(in: .whitespaces) }
                .filter { !$0.isEmpty }

        } catch {
            log("Proper noun API error: \(error)")
            return []
        }
    }

    /// Show toast notification for learned words
    private func showLearnedWordsToast(words: [String]) {
        LearnedWordsToastController.shared.show(words: words) { [weak self] in
            self?.undoLastLearnedWords()
        }
    }

    private func cleanup() {
        originalText = nil
        targetAppPID = nil
        monitoringStartTime = nil
        lastReadText = nil
        lastChangeTime = nil
    }

    /// Check if text looks like a title/placeholder that indicates we read the wrong element
    private func isInvalidText(_ text: String) -> Bool {
        let lower = text.lowercased().trimmingCharacters(in: .whitespacesAndNewlines)

        if lower.count < 5 {
            return true
        }

        for pattern in invalidPatterns {
            if lower.hasPrefix(pattern) || lower == pattern {
                return true
            }
        }

        return false
    }

    /// Calculate how many words overlap between original and edited text
    private func wordOverlapRatio(original: String, edited: String) -> Double {
        let originalWords = Set(original.lowercased().components(separatedBy: .whitespacesAndNewlines).filter { !$0.isEmpty })
        let editedWords = Set(edited.lowercased().components(separatedBy: .whitespacesAndNewlines).filter { !$0.isEmpty })

        guard !originalWords.isEmpty else { return 0 }

        let intersection = originalWords.intersection(editedWords)
        return Double(intersection.count) / Double(originalWords.count)
    }

    /// Read the current text from the focused UI element in the target app
    private func readFocusedTextElement(pid: pid_t) -> (String, String)? {
        let appElement = AXUIElementCreateApplication(pid)

        var focusedElement: CFTypeRef?
        let focusResult = AXUIElementCopyAttributeValue(
            appElement,
            kAXFocusedUIElementAttribute as CFString,
            &focusedElement
        )

        guard focusResult == .success, let focused = focusedElement else {
            return nil
        }

        let axElement = focused as! AXUIElement

        var roleRef: CFTypeRef?
        AXUIElementCopyAttributeValue(axElement, kAXRoleAttribute as CFString, &roleRef)
        let role = (roleRef as? String) ?? "Unknown"

        var value: CFTypeRef?
        let valueResult = AXUIElementCopyAttributeValue(
            axElement,
            kAXValueAttribute as CFString,
            &value
        )

        if valueResult == .success, let textValue = value as? String, !textValue.isEmpty {
            return (textValue, role)
        }

        // Try selected text as fallback
        var selectedText: CFTypeRef?
        let selectedResult = AXUIElementCopyAttributeValue(
            axElement,
            kAXSelectedTextAttribute as CFString,
            &selectedText
        )

        if selectedResult == .success, let selected = selectedText as? String, !selected.isEmpty {
            return (selected, role)
        }

        // For web areas, try to find text in children
        if role == "AXWebArea" {
            if let childText = findTextInWebArea(axElement) {
                return (childText, role)
            }
        }

        return nil
    }

    private func findTextInWebArea(_ webArea: AXUIElement) -> String? {
        var childrenRef: CFTypeRef?
        let result = AXUIElementCopyAttributeValue(webArea, kAXChildrenAttribute as CFString, &childrenRef)

        guard result == .success, let children = childrenRef as? [AXUIElement] else {
            return nil
        }

        for child in children.prefix(20) {
            var roleRef: CFTypeRef?
            AXUIElementCopyAttributeValue(child, kAXRoleAttribute as CFString, &roleRef)
            let childRole = (roleRef as? String) ?? ""

            if childRole == "AXTextArea" || childRole == "AXTextField" {
                var valueRef: CFTypeRef?
                if AXUIElementCopyAttributeValue(child, kAXValueAttribute as CFString, &valueRef) == .success,
                   let text = valueRef as? String, !text.isEmpty {
                    return text
                }
            }

            // Check grandchildren
            var grandchildrenRef: CFTypeRef?
            if AXUIElementCopyAttributeValue(child, kAXChildrenAttribute as CFString, &grandchildrenRef) == .success,
               let grandchildren = grandchildrenRef as? [AXUIElement] {
                for grandchild in grandchildren.prefix(10) {
                    var gcRoleRef: CFTypeRef?
                    AXUIElementCopyAttributeValue(grandchild, kAXRoleAttribute as CFString, &gcRoleRef)
                    let gcRole = (gcRoleRef as? String) ?? ""

                    if gcRole == "AXTextArea" || gcRole == "AXTextField" {
                        var valueRef: CFTypeRef?
                        if AXUIElementCopyAttributeValue(grandchild, kAXValueAttribute as CFString, &valueRef) == .success,
                           let text = valueRef as? String, !text.isEmpty {
                            return text
                        }
                    }
                }
            }
        }

        return nil
    }

    private func log(_ message: String) {
        #if DEBUG
        let timestamp = ISO8601DateFormatter().string(from: Date())
        print("[\(timestamp)] [EditLearning] \(message)")
        #endif
    }
}

// MARK: - Alignment Result Model

/// Decoded alignment result from Rust
private struct AlignmentResult: Decodable {
    let steps: [AlignmentStep]
    let wordEditVector: String
    let punctEditVector: String
    let corrections: [Correction]

    enum CodingKeys: String, CodingKey {
        case steps
        case wordEditVector = "word_edit_vector"
        case punctEditVector = "punct_edit_vector"
        case corrections
    }

    struct AlignmentStep: Decodable {
        let wordLabel: String
        let punctLabel: String
        let originalWord: String
        let editedWord: String

        enum CodingKeys: String, CodingKey {
            case wordLabel = "word_label"
            case punctLabel = "punct_label"
            case originalWord = "original_word"
            case editedWord = "edited_word"
        }
    }

    struct Correction: Decodable {
        let original: String
        let corrected: String

        init(from decoder: Decoder) throws {
            var container = try decoder.unkeyedContainer()
            original = try container.decode(String.self)
            corrected = try container.decode(String.self)
        }
    }
}
