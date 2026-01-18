//
// EditLearningService.swift
// Flow
//
// Monitors text field edits after transcription paste to learn user corrections.
// Uses macOS Accessibility API to read focused text elements.
//

import AppKit
import ApplicationServices
import Flow

/// Service that detects when users edit pasted transcription text and triggers learning.
///
/// After a transcription is pasted:
/// 1. Polls the focused text element every second
/// 2. Tracks when text last changed
/// 3. When text is stable for 5+ seconds, triggers learning
/// 4. Gives up after 30 seconds max
final class EditLearningService {
    static let shared = EditLearningService()

    /// How often to poll the text field (seconds)
    private let pollInterval: TimeInterval = 1.0

    /// How long text must be unchanged before we consider it "stable" (seconds)
    private let stabilityThreshold: TimeInterval = 5.0

    /// Maximum time to monitor before giving up (seconds)
    private let maxMonitoringDuration: TimeInterval = 30.0

    /// Minimum text length to bother learning from
    private let minimumTextLength = 10

    /// Minimum word overlap ratio required (edited text should share words with original)
    private let minimumWordOverlap: Double = 0.3

    /// Reference to the Flow engine for calling learnFromEdit
    private var engine: Flow?

    /// Timer for polling
    private var pollTimer: Timer?

    /// Original text that was pasted
    private var originalText: String?

    /// Target app's PID for the current monitoring session
    private var targetAppPID: pid_t?

    /// When monitoring started
    private var monitoringStartTime: Date?

    /// Last text we read from the field
    private var lastReadText: String?

    /// When the text last changed
    private var lastChangeTime: Date?

    /// Known bad values that indicate we read the wrong element
    private let invalidPatterns = [
        "untitled",
        "new document",
        "new tab",
        "loading",
        "about:blank"
    ]

    private init() {}

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

        // Start polling
        pollTimer = Timer.scheduledTimer(withTimeInterval: pollInterval, repeats: true) { [weak self] _ in
            self?.pollTextElement()
        }
    }

    /// Cancel any pending monitoring
    func cancelMonitoring() {
        pollTimer?.invalidate()
        pollTimer = nil
        cleanup()
    }

    // MARK: - Private

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
            log("Monitoring timeout after \(Int(elapsed))s, giving up")
            cancelMonitoring()
            return
        }

        // Try to read the focused text element
        guard let (currentText, role) = readFocusedTextElement(pid: pid) else {
            // Can't read, might have switched apps, give up
            log("Lost focus on text element, stopping")
            cancelMonitoring()
            return
        }

        // Validate the role is a text input
        let validRoles = ["AXTextArea", "AXTextField", "AXTextView", "AXWebArea", "AXStaticText"]
        let roleIsValid = validRoles.contains { role.contains($0) }
        if !roleIsValid {
            // Wrong element type, keep waiting
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
            // Text has been stable, time to learn
            log("Text stable for \(Int(stableFor))s, checking for edits")
            checkAndLearn(original: original, current: currentText)
            cancelMonitoring()
        }
    }

    private func checkAndLearn(original: String, current: String) {
        guard let engine = engine else { return }

        // Skip if texts are identical (no edits made)
        if current == original {
            log("No edits detected, text unchanged")
            return
        }

        // Validate there's meaningful word overlap (user edited, didn't completely replace)
        let overlap = wordOverlapRatio(original: original, edited: current)
        if overlap < minimumWordOverlap {
            log("Insufficient word overlap (\(Int(overlap * 100))%), probably wrong element")
            return
        }

        // Extract word-level corrections
        let corrections = extractWordCorrections(original: original, edited: current)

        if corrections.isEmpty {
            log("No word-level corrections detected")
            return
        }

        log("Detected \(corrections.count) potential correction(s)")
        for (orig, corr) in corrections {
            log("  '\(orig)' -> '\(corr)'")
        }

        // Validate corrections via AI
        if let validations = engine.validateCorrections(corrections) {
            let validCount = validations.filter { $0.valid }.count
            log("AI validation: \(validCount)/\(validations.count) corrections valid")

            for validation in validations {
                if validation.valid {
                    log("  ✓ '\(validation.original)' -> '\(validation.corrected)'")
                } else {
                    log("  ✗ '\(validation.original)' -> '\(validation.corrected)': \(validation.reason ?? "unknown")")
                }
            }

            // Only proceed if we have at least one valid correction
            if validCount == 0 {
                log("No valid corrections, skipping learning")
                return
            }
        } else {
            log("AI validation unavailable, proceeding with heuristic check")
        }

        // Learn from edit (Rust will do its own Jaro-Winkler matching)
        if engine.learnFromEdit(original: original, edited: current) {
            log("Learned from edit successfully")
        }
    }

    /// Extract word-level corrections by comparing original and edited text
    private func extractWordCorrections(original: String, edited: String) -> [(original: String, corrected: String)] {
        let originalWords = original.components(separatedBy: .whitespacesAndNewlines).filter { !$0.isEmpty }
        let editedWords = edited.components(separatedBy: .whitespacesAndNewlines).filter { !$0.isEmpty }

        var corrections: [(original: String, corrected: String)] = []

        // Simple position-based comparison (similar to Rust's learn_from_edit)
        let minLen = min(originalWords.count, editedWords.count)

        for i in 0..<minLen {
            let orig = originalWords[i].lowercased()
            let edit = editedWords[i].lowercased()

            // Skip if identical
            if orig == edit { continue }

            // Skip very short words
            if orig.count < 2 || edit.count < 2 { continue }

            // Skip if length difference is too large (probably not a typo fix)
            if abs(orig.count - edit.count) > 2 { continue }

            // Strip punctuation for comparison
            let origClean = orig.trimmingCharacters(in: .punctuationCharacters)
            let editClean = edit.trimmingCharacters(in: .punctuationCharacters)

            // Skip if only punctuation differs
            if origClean == editClean { continue }

            corrections.append((original: origClean, corrected: editClean))
        }

        return corrections
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

        // Very short text is suspicious
        if lower.count < 5 {
            return true
        }

        // Check against known bad patterns
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
    /// Returns tuple of (text, role) for validation
    private func readFocusedTextElement(pid: pid_t) -> (String, String)? {
        let appElement = AXUIElementCreateApplication(pid)

        // Get the focused UI element
        var focusedElement: CFTypeRef?
        let focusResult = AXUIElementCopyAttributeValue(
            appElement,
            kAXFocusedUIElementAttribute as CFString,
            &focusedElement
        )

        guard focusResult == .success, let focused = focusedElement else {
            log("Could not get focused element (error: \(focusResult.rawValue))")
            return nil
        }

        let axElement = focused as! AXUIElement

        // Get the role for validation
        var roleRef: CFTypeRef?
        AXUIElementCopyAttributeValue(axElement, kAXRoleAttribute as CFString, &roleRef)
        let role = (roleRef as? String) ?? "Unknown"

        // Get role description for debugging
        var roleDescRef: CFTypeRef?
        AXUIElementCopyAttributeValue(axElement, kAXRoleDescriptionAttribute as CFString, &roleDescRef)
        let roleDesc = (roleDescRef as? String) ?? ""

        // Get title for debugging
        var titleRef: CFTypeRef?
        AXUIElementCopyAttributeValue(axElement, kAXTitleAttribute as CFString, &titleRef)
        let title = (titleRef as? String) ?? ""

        // Get description for debugging
        var descRef: CFTypeRef?
        AXUIElementCopyAttributeValue(axElement, kAXDescriptionAttribute as CFString, &descRef)
        let desc = (descRef as? String) ?? ""

        log("Focused element: role=\(role), roleDesc=\(roleDesc), title='\(title.prefix(30))', desc='\(desc.prefix(30))'")

        // Try to get the value attribute (text content)
        var value: CFTypeRef?
        let valueResult = AXUIElementCopyAttributeValue(
            axElement,
            kAXValueAttribute as CFString,
            &value
        )

        if valueResult == .success, let textValue = value as? String, !textValue.isEmpty {
            log("Got value: '\(textValue.prefix(50))...' (\(textValue.count) chars)")
            return (textValue, role)
        }

        // Some elements use kAXSelectedTextAttribute for text fields with selection
        var selectedText: CFTypeRef?
        let selectedResult = AXUIElementCopyAttributeValue(
            axElement,
            kAXSelectedTextAttribute as CFString,
            &selectedText
        )

        if selectedResult == .success, let selected = selectedText as? String, !selected.isEmpty {
            log("Got selected text: '\(selected.prefix(50))...'")
            return (selected, role)
        }

        // For web areas, try to get the focused element within it
        if role == "AXWebArea" {
            log("WebArea detected, looking for focused child...")
            if let childText = findTextInWebArea(axElement) {
                return (childText, role)
            }
        }

        log("Could not read text value (error: \(valueResult.rawValue))")
        return nil
    }

    /// Try to find editable text within a web area by looking for text fields
    private func findTextInWebArea(_ webArea: AXUIElement) -> String? {
        // Get children
        var childrenRef: CFTypeRef?
        let result = AXUIElementCopyAttributeValue(webArea, kAXChildrenAttribute as CFString, &childrenRef)

        guard result == .success, let children = childrenRef as? [AXUIElement] else {
            return nil
        }

        // Look for text areas or text fields in children (limited depth)
        for child in children.prefix(20) {
            var roleRef: CFTypeRef?
            AXUIElementCopyAttributeValue(child, kAXRoleAttribute as CFString, &roleRef)
            let childRole = (roleRef as? String) ?? ""

            if childRole == "AXTextArea" || childRole == "AXTextField" {
                var valueRef: CFTypeRef?
                if AXUIElementCopyAttributeValue(child, kAXValueAttribute as CFString, &valueRef) == .success,
                   let text = valueRef as? String, !text.isEmpty {
                    log("Found text in child \(childRole): '\(text.prefix(50))...'")
                    return text
                }
            }

            // Check one level deeper
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
                            log("Found text in grandchild \(gcRole): '\(text.prefix(50))...'")
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
