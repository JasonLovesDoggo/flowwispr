//
//  AXEditMonitorService.swift
//  Flow
//
//  AX-based monitoring of text field edits after transcription paste.
//  Uses macOS Accessibility notifications for event-driven detection.
//

import AppKit
import ApplicationServices

/// Event-driven service that monitors text field changes using AX notifications
final class AXEditMonitorService {
    // MARK: - Properties

    private var axObserver: AXObserver?
    private var monitoredElement: AXUIElement?
    private var originalText: String = ""
    private var stabilityTimer: Timer?
    private var lastText: String = ""
    private var lastTextChangeTime: Date?

    /// How long text must be stable before we consider edits complete
    private let stabilityDelay: TimeInterval = 1.5

    /// Maximum time to monitor before giving up
    private let maxDuration: TimeInterval = 60.0

    private var startTime: Date?

    /// Callback when edit is detected
    var onEditDetected: ((String, String) -> Void)?

    // MARK: - Public Methods

    /// Start monitoring a text element for edits
    /// - Parameters:
    ///   - element: The AXUIElement to monitor
    ///   - originalText: The text that was originally pasted
    func startMonitoring(element: AXUIElement, originalText: String) {
        stopMonitoring()

        self.monitoredElement = element
        self.originalText = originalText
        self.lastText = originalText
        self.startTime = Date()
        self.lastTextChangeTime = Date()

        // Get app PID from element
        var pid: pid_t = 0
        guard AXUIElementGetPid(element, &pid) == .success else {
            log("Failed to get PID from element")
            return
        }

        // Create AX observer
        var observer: AXObserver?
        let callback: AXObserverCallback = { _, element, notification, refcon in
            guard let refcon = refcon else { return }
            let service = Unmanaged<AXEditMonitorService>.fromOpaque(refcon).takeUnretainedValue()
            service.handleNotification(element: element, notification: notification as String)
        }

        guard AXObserverCreate(pid, callback, &observer) == .success,
              let observer = observer else {
            log("Failed to create AX observer")
            return
        }

        self.axObserver = observer

        // Add notifications
        let refcon = Unmanaged.passUnretained(self).toOpaque()
        AXObserverAddNotification(observer, element, kAXValueChangedNotification as CFString, refcon)
        AXObserverAddNotification(observer, element, kAXSelectedTextChangedNotification as CFString, refcon)

        // Add to run loop
        CFRunLoopAddSource(CFRunLoopGetMain(), AXObserverGetRunLoopSource(observer), .defaultMode)

        log("Started monitoring text element")

        // Set up timeout
        DispatchQueue.main.asyncAfter(deadline: .now() + maxDuration) { [weak self] in
            self?.finishMonitoring()
        }
    }

    /// Stop monitoring
    func stopMonitoring() {
        stabilityTimer?.invalidate()
        stabilityTimer = nil

        if let observer = axObserver, let element = monitoredElement {
            AXObserverRemoveNotification(observer, element, kAXValueChangedNotification as CFString)
            AXObserverRemoveNotification(observer, element, kAXSelectedTextChangedNotification as CFString)
            CFRunLoopRemoveSource(CFRunLoopGetMain(), AXObserverGetRunLoopSource(observer), .defaultMode)
        }

        axObserver = nil
        monitoredElement = nil
        startTime = nil
    }

    // MARK: - Private Methods

    private func handleNotification(element: AXUIElement, notification: String) {
        // Reset stability timer on any change
        stabilityTimer?.invalidate()

        // Read current text
        var value: AnyObject?
        guard AXUIElementCopyAttributeValue(element, kAXValueAttribute as CFString, &value) == .success,
              let currentText = value as? String else {
            return
        }

        // Check if text actually changed
        if currentText != lastText {
            lastText = currentText
            lastTextChangeTime = Date()
            log("Text changed, resetting stability timer")
        }

        // Start new stability timer
        stabilityTimer = Timer.scheduledTimer(withTimeInterval: stabilityDelay, repeats: false) { [weak self] _ in
            self?.textStabilized()
        }
    }

    private func textStabilized() {
        guard lastText != originalText else {
            log("Text unchanged from original, no edits detected")
            return
        }

        log("Text stabilized with edits")
        onEditDetected?(originalText, lastText)
        stopMonitoring()
    }

    private func finishMonitoring() {
        guard monitoredElement != nil else { return }

        if lastText != originalText {
            log("Timeout reached with edits, triggering callback")
            onEditDetected?(originalText, lastText)
        } else {
            log("Timeout reached, no edits detected")
        }

        stopMonitoring()
    }

    private func log(_ message: String) {
        #if DEBUG
        let timestamp = ISO8601DateFormatter().string(from: Date())
        print("[\(timestamp)] [AXMonitor] \(message)")
        #endif
    }
}

// MARK: - Focused Element Helper

extension AXEditMonitorService {
    /// Get the currently focused text element from an app
    /// - Parameter pid: Process ID of the target app
    /// - Returns: The focused AXUIElement if it's a text element
    static func getFocusedTextElement(pid: pid_t) -> AXUIElement? {
        let appElement = AXUIElementCreateApplication(pid)

        var focusedElement: CFTypeRef?
        guard AXUIElementCopyAttributeValue(appElement, kAXFocusedUIElementAttribute as CFString, &focusedElement) == .success,
              let focused = focusedElement else {
            return nil
        }

        let axElement = focused as! AXUIElement

        // Verify it's a text element
        var roleRef: CFTypeRef?
        AXUIElementCopyAttributeValue(axElement, kAXRoleAttribute as CFString, &roleRef)
        let role = (roleRef as? String) ?? ""

        let validRoles = ["AXTextArea", "AXTextField", "AXTextView", "AXWebArea"]
        guard validRoles.contains(where: { role.contains($0) }) else {
            return nil
        }

        return axElement
    }

    /// Get the current text value from an AXUIElement
    static func getTextValue(from element: AXUIElement) -> String? {
        var value: AnyObject?
        guard AXUIElementCopyAttributeValue(element, kAXValueAttribute as CFString, &value) == .success,
              let text = value as? String, !text.isEmpty else {
            return nil
        }
        return text
    }
}
