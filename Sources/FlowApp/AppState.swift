//
// AppState.swift
// Flow
//
// Observable state for the Flow app.
//

import AppKit
import Carbon.HIToolbox
import Combine
import Flow
import Foundation

/// Main app state observable
@MainActor
final class AppState: ObservableObject {
    /// The Flow engine
    let engine: Flow

    private func log(_ message: String) {
        let timestamp = ISO8601DateFormatter().string(from: Date())
        print("[\(timestamp)] \(message)")
    }

    /// Current recording state
    @Published var isRecording = false
    @Published var isProcessing = false
    @Published var isInitializingModel = false
    @Published var audioLevel: Float = 0.0
    @Published var smoothedAudioLevel: Float = 0.0

    /// Last transcribed text
    @Published var lastTranscription: String?

    /// Current writing mode
    @Published var currentMode: WritingMode = .casual

    /// Current app name (tracks frontmost app, including Flow itself)
    @Published var currentApp: String = "Unknown"

    /// Current app category
    @Published var currentCategory: AppCategory = .unknown

    /// Target app for mode configuration (the app before Flow became active)
    @Published var targetAppName: String = "Unknown"
    @Published var targetAppBundleId: String?
    @Published var targetAppMode: WritingMode = .casual
    @Published var targetAppCategory: AppCategory = .unknown

    /// Recent transcriptions
    @Published var history: [TranscriptionSummary] = []
    @Published var retryableHistoryId: String?

    /// API key configured
    @Published var isConfigured = false

    /// Current selected tab
    @Published var selectedTab: AppTab = .record

    /// Current recording hotkey
    @Published var hotkey: Hotkey
    @Published var isCapturingHotkey = false
    @Published var isAccessibilityEnabled = false
    @Published var isOnboardingComplete: Bool

    /// Error message to display
    @Published var errorMessage: String?

    /// Recording duration in milliseconds
    @Published var recordingDuration: UInt64 = 0

    /// Workspace observer for app changes
    private var workspaceObserver: NSObjectProtocol?
    private var recordingTimer: Timer?
    private var audioLevelTimer: Timer?
    private var modelLoadingTimer: Timer?
    private var globeKeyHandler: GlobeKeyHandler?
    private var hotkeyCaptureMonitor: Any?
    private var hotkeyFlagsMonitor: Any?
    private var pendingModifierCapture: Hotkey.ModifierKey?
    private var appActiveObserver: NSObjectProtocol?
    private var appInactiveObserver: NSObjectProtocol?
    private var mediaPauseState = MediaPauseState()
    private var recordingIndicator: RecordingIndicatorWindow?
    private var targetApplication: NSRunningApplication?

    private static let onboardingKey = "onboardingComplete"

    init() {
        self.engine = Flow()
        self.isConfigured = engine.isConfigured
        self.hotkey = Hotkey.load()
        self.isOnboardingComplete = UserDefaults.standard.bool(forKey: Self.onboardingKey)
        self.isAccessibilityEnabled = GlobeKeyHandler.isAccessibilityAuthorized()

        setupGlobeKey()
        setupLifecycleObserver()
        setupWorkspaceObserver()
        setupModelLoadingPoller()
        updateCurrentApp()
        refreshHistory()
    }

    func cleanup() {
        if let observer = workspaceObserver {
            NSWorkspace.shared.notificationCenter.removeObserver(observer)
            workspaceObserver = nil
        }
        if let observer = appActiveObserver {
            NotificationCenter.default.removeObserver(observer)
            appActiveObserver = nil
        }
        if let observer = appInactiveObserver {
            NotificationCenter.default.removeObserver(observer)
            appInactiveObserver = nil
        }
        recordingTimer?.invalidate()
        recordingTimer = nil
        audioLevelTimer?.invalidate()
        audioLevelTimer = nil
        modelLoadingTimer?.invalidate()
        modelLoadingTimer = nil
        endHotkeyCapture()
        recordingIndicator?.hide()
    }

    // MARK: - Globe Key

    private func setupGlobeKey() {
        globeKeyHandler = GlobeKeyHandler(hotkey: hotkey) { trigger in
            Task { @MainActor [weak self] in
                self?.handleHotkeyTrigger(trigger)
            }
        }
    }

    private func handleHotkeyTrigger(_ trigger: GlobeKeyHandler.Trigger) {
        log("🎹 [HOTKEY] Trigger detected: \(trigger)")
        switch trigger {
        case .pressed:
            if !isRecording {
                startRecording()
            }
        case .released:
            if isRecording {
                stopRecording()
            }
        case .toggle:
            toggleRecording()
        }
    }

    private func setupLifecycleObserver() {
        appActiveObserver = NotificationCenter.default.addObserver(
            forName: NSApplication.didBecomeActiveNotification,
            object: nil,
            queue: .main
        ) { _ in
            Task { @MainActor [weak self] in
                self?.refreshAccessibilityStatus()
            }
        }

        appInactiveObserver = NotificationCenter.default.addObserver(
            forName: NSApplication.didResignActiveNotification,
            object: nil,
            queue: .main
        ) { _ in
            Task { @MainActor [weak self] in
                self?.updateRecordingIndicatorVisibility()
            }
        }
    }

    private func setupModelLoadingPoller() {
        // Poll model loading state every 0.5 seconds
        modelLoadingTimer = Timer.scheduledTimer(withTimeInterval: 0.5, repeats: true) { [weak self] _ in
            Task { @MainActor [weak self] in
                guard let self else { return }
                let isLoading = self.engine.isModelLoading()
                if self.isInitializingModel != isLoading {
                    self.isInitializingModel = isLoading
                    self.updateRecordingIndicatorVisibility()
                }
            }
        }
    }

    func setHotkey(_ hotkey: Hotkey) {
        self.hotkey = hotkey
        hotkey.save()
        globeKeyHandler?.updateHotkey(hotkey)

        var properties: [String: Any] = [
            "display_name": hotkey.displayName
        ]

        switch hotkey.kind {
        case .globe:
            properties["type"] = "globe"
        case .modifierOnly(let modifier):
            properties["type"] = "modifierOnly"
            properties["modifier"] = modifier.rawValue
        case .custom(let keyCode, let modifiers, let keyLabel):
            properties["type"] = "custom"
            properties["key_code"] = keyCode
            properties["key_label"] = keyLabel
            properties["modifiers"] = modifiers.displayString
        }

        Analytics.shared.track("Hotkey Changed", eventProperties: properties)
    }

    func requestAccessibilityPermission() {
        let started = globeKeyHandler?.startListening(prompt: true) ?? false
        if started {
            isAccessibilityEnabled = true
            Analytics.shared.track("Accessibility Permission Granted")
        } else {
            refreshAccessibilityStatus()
        }
    }

    func refreshAccessibilityStatus() {
        let wasEnabled = isAccessibilityEnabled
        let enabled = GlobeKeyHandler.isAccessibilityAuthorized()
        isAccessibilityEnabled = enabled

        if !wasEnabled && enabled {
            Analytics.shared.track("Accessibility Permission Granted")
        } else if wasEnabled && !enabled {
            Analytics.shared.track("Accessibility Permission Revoked")
        }

        if enabled {
            _ = globeKeyHandler?.startListening(prompt: false)
        }
    }

    func clearError() {
        errorMessage = nil
    }

    func refreshHistory(limit: Int = 50) {
        history = engine.recentTranscriptions(limit: limit)
        if let latest = history.first, latest.status == .failed {
            retryableHistoryId = latest.id
        } else {
            retryableHistoryId = nil
        }
    }

    func completeOnboarding() {
        isOnboardingComplete = true
        UserDefaults.standard.set(true, forKey: Self.onboardingKey)

        Analytics.shared.track("Onboarding Completed")
    }

    func beginHotkeyCapture() {
        guard hotkeyCaptureMonitor == nil, hotkeyFlagsMonitor == nil else { return }
        isCapturingHotkey = true
        pendingModifierCapture = nil

        // Monitor for key+modifier combos
        hotkeyCaptureMonitor = NSEvent.addLocalMonitorForEvents(matching: [.keyDown]) { event in
            Task { @MainActor [weak self] in
                self?.handleHotkeyKeyCapture(event)
            }
            return nil
        }

        // Monitor for modifier-only hotkeys
        hotkeyFlagsMonitor = NSEvent.addLocalMonitorForEvents(matching: [.flagsChanged]) { event in
            Task { @MainActor [weak self] in
                self?.handleHotkeyFlagsCapture(event)
            }
            return event
        }
    }

    func endHotkeyCapture() {
        if let monitor = hotkeyCaptureMonitor {
            NSEvent.removeMonitor(monitor)
            hotkeyCaptureMonitor = nil
        }
        if let monitor = hotkeyFlagsMonitor {
            NSEvent.removeMonitor(monitor)
            hotkeyFlagsMonitor = nil
        }
        isCapturingHotkey = false
        pendingModifierCapture = nil
    }

    private func handleHotkeyKeyCapture(_ event: NSEvent) {
        pendingModifierCapture = nil // Key pressed, cancel any pending modifier capture

        let modifiers = Hotkey.Modifiers.from(nsFlags: event.modifierFlags)
        if event.keyCode == UInt16(kVK_Escape), modifiers.isEmpty {
            endHotkeyCapture()
            return
        }

        setHotkey(Hotkey.from(event: event))
        endHotkeyCapture()
    }

    private func handleHotkeyFlagsCapture(_ event: NSEvent) {
        // Detect single modifier press/release for modifier-only hotkeys
        let flags = event.modifierFlags.intersection(.deviceIndependentFlagsMask)

        // Map NSEvent flags to our ModifierKey
        let modifierMappings: [(NSEvent.ModifierFlags, Hotkey.ModifierKey)] = [
            (.option, .option),
            (.shift, .shift),
            (.control, .control),
            (.command, .command)
        ]

        // Count how many modifiers are currently pressed
        var pressedModifier: Hotkey.ModifierKey?
        var count = 0
        for (flag, key) in modifierMappings {
            if flags.contains(flag) {
                pressedModifier = key
                count += 1
            }
        }

        if count == 1, let modifier = pressedModifier {
            // Single modifier pressed, start pending capture
            pendingModifierCapture = modifier
        } else if count == 0, let pending = pendingModifierCapture {
            // All modifiers released, if we had a pending single modifier, capture it
            setHotkey(Hotkey(kind: .modifierOnly(pending)))
            endHotkeyCapture()
        } else {
            // Multiple modifiers or no modifiers, cancel pending
            pendingModifierCapture = nil
        }
    }

    // MARK: - Workspace Observer

    private func setupWorkspaceObserver() {
        workspaceObserver = NSWorkspace.shared.notificationCenter.addObserver(
            forName: NSWorkspace.didActivateApplicationNotification,
            object: nil,
            queue: .main
        ) { notification in
            guard let app = notification.userInfo?[NSWorkspace.applicationUserInfoKey] as? NSRunningApplication else {
                return
            }
            let appName = app.localizedName ?? "Unknown"
            let bundleId = app.bundleIdentifier

            Task { @MainActor [weak self] in
                self?.handleAppActivation(appName: appName, bundleId: bundleId)
            }
        }
    }

    private func handleAppActivation(appName: String, bundleId: String?) {
        currentApp = appName

        // Check if Flow itself became active
        let isFlow = bundleId == Bundle.main.bundleIdentifier

        if !isFlow {
            // This is an external app - save it as the target for mode configuration
            targetAppName = appName
            targetAppBundleId = bundleId

            let suggestedMode = engine.setActiveApp(name: appName, bundleId: bundleId)
            currentCategory = engine.currentAppCategory
            targetAppCategory = currentCategory

            if let styleSuggestion = engine.styleSuggestion {
                currentMode = styleSuggestion
                targetAppMode = styleSuggestion
            } else {
                currentMode = suggestedMode
                targetAppMode = suggestedMode
            }
        } else {
            // Flow became active - preserve the target app for mode changes
            // Update currentMode to reflect the target app's mode
            currentMode = targetAppMode
            currentCategory = targetAppCategory
        }
    }

    private func updateCurrentApp() {
        if let frontApp = NSWorkspace.shared.frontmostApplication {
            let appName = frontApp.localizedName ?? "Unknown"
            let bundleId = frontApp.bundleIdentifier

            currentApp = appName

            // Initialize target app if this is not Flow
            let isFlow = bundleId == Bundle.main.bundleIdentifier
            if !isFlow {
                targetAppName = appName
                targetAppBundleId = bundleId
            }

            let suggestedMode = engine.setActiveApp(name: appName, bundleId: bundleId)
            currentCategory = engine.currentAppCategory
            currentMode = suggestedMode

            if !isFlow {
                targetAppMode = suggestedMode
                targetAppCategory = currentCategory
            }
        }
    }

    // MARK: - Recording

    func toggleRecording() {
        if isRecording {
            stopRecording()
        } else {
            startRecording()
        }
    }

    func startRecording() {
        guard engine.isConfigured else {
            errorMessage = "Please configure your API key in Settings"
            return
        }

        targetApplication = NSWorkspace.shared.frontmostApplication
        log("🎤 [RECORDING] Starting recording - App: \(currentApp), Mode: \(currentMode.displayName)")
        pauseMediaPlayback()
        if engine.startRecording() {
            isRecording = true
            isProcessing = false
            updateRecordingIndicatorVisibility()
            recordingDuration = 0
            log("✅ [RECORDING] Recording started successfully")

            Analytics.shared.track("Recording Started", eventProperties: [
                "app_name": currentApp,
                "app_category": currentCategory.rawValue,
                "writing_mode": currentMode.rawValue
            ])

            recordingTimer = Timer.scheduledTimer(withTimeInterval: 0.1, repeats: true) { _ in
                Task { @MainActor [weak self] in
                    guard let self, self.isRecording else { return }
                    self.recordingDuration += 100
                }
            }

            audioLevelTimer = Timer.scheduledTimer(withTimeInterval: 1/30, repeats: true) { [weak self] _ in
                Task { @MainActor [weak self] in
                    guard let self, self.isRecording else { return }
                    let newLevel = self.engine.audioLevel
                    self.audioLevel = newLevel
                    // Smooth the audio level with exponential moving average
                    // Higher smoothing factor = smoother but slower response
                    let smoothingFactor: Float = 0.3
                    self.smoothedAudioLevel = self.smoothedAudioLevel * (1 - smoothingFactor) + newLevel * smoothingFactor
                }
            }
        } else {
            errorMessage = engine.lastError ?? "Failed to start recording"
            resumeMediaPlayback()
        }
    }

    func stopRecording() {
        log("⏹️ [RECORDING] Stopping recording - Duration: \(recordingDuration)ms")
        recordingTimer?.invalidate()
        recordingTimer = nil
        audioLevelTimer?.invalidate()
        audioLevelTimer = nil
        audioLevel = 0.0
        smoothedAudioLevel = 0.0

        let duration = engine.stopRecording()
        isRecording = false
        resumeMediaPlayback()

        if duration > 0 {
            log("✅ [RECORDING] Recording stopped successfully - Duration: \(duration)ms")
            Analytics.shared.track("Recording Stopped", eventProperties: [
                "duration_ms": recordingDuration,
                "app_name": currentApp
            ])
            setProcessing(true)
            transcribe()
        } else {
            log("⚠️ [RECORDING] Recording cancelled (too short)")
            Analytics.shared.track("Recording Cancelled", eventProperties: [
                "duration_ms": recordingDuration,
                "app_name": currentApp
            ])
            updateRecordingIndicatorVisibility()
        }
    }

    private func transcribe() {
        let appName = currentApp
        let appCategory = currentCategory
        let mode = currentMode
        let duration = recordingDuration

        log("🔄 [TRANSCRIBE] Starting transcription - App: \(appName), Mode: \(mode.displayName)")

        Task.detached { [weak self] in
            guard let self else { return }
            let result = await Task {
                self.engine.transcribe(appName: appName)
            }.value

            await MainActor.run { [weak self] in
                guard let self else { return }
                if let text = result {
                    self.log("✅ [TRANSCRIBE] Transcription completed - Length: \(text.count) chars")
                    self.log("📝 [TRANSCRIBE] Result: \(text.prefix(100))...")
                    self.lastTranscription = text
                    self.errorMessage = nil
                    NSPasteboard.general.clearContents()
                    NSPasteboard.general.setString(text, forType: .string)
                    self.log("📋 [CLIPBOARD] Text copied to clipboard")

                    Analytics.shared.track("Transcription Completed", eventProperties: [
                        "app_name": appName,
                        "app_category": appCategory.rawValue,
                        "writing_mode": mode.rawValue,
                        "duration_ms": duration,
                        "text_length": text.count
                    ])

                    self.activateTargetAppIfNeeded()
                    DispatchQueue.main.asyncAfter(deadline: .now() + 0.05) { [weak self] in
                        self?.pasteText()
                        self?.finishProcessing()
                    }
                    self.refreshHistory()
                } else {
                    let errorMsg = self.engine.lastError ?? "Transcription failed"
                    self.log("❌ [TRANSCRIBE] Transcription failed: \(errorMsg)")
                    self.errorMessage = errorMsg

                    Analytics.shared.track("Transcription Failed", eventProperties: [
                        "app_name": appName,
                        "error": errorMsg,
                        "duration_ms": duration
                    ])

                    self.refreshHistory()
                    self.finishProcessing()
                }
            }
        }
    }

    func retryLastTranscription() {
        setProcessing(true)
        let appName = currentApp

        Analytics.shared.track("Transcription Retry Attempted", eventProperties: [
            "app_name": appName
        ])

        Task.detached { [weak self] in
            guard let self else { return }
            let result = await Task {
                self.engine.retryLastTranscription(appName: appName)
            }.value

            await MainActor.run { [weak self] in
                guard let self else { return }
                if let text = result {
                    self.lastTranscription = text
                    self.errorMessage = nil
                    NSPasteboard.general.clearContents()
                    NSPasteboard.general.setString(text, forType: .string)

                    Analytics.shared.track("Transcription Retry Succeeded", eventProperties: [
                        "app_name": appName,
                        "text_length": text.count
                    ])

                    self.activateTargetAppIfNeeded()
                    DispatchQueue.main.asyncAfter(deadline: .now() + 0.05) { [weak self] in
                        self?.pasteText()
                        self?.finishProcessing()
                    }
                    self.refreshHistory()
                } else {
                    let errorMsg = self.engine.lastError ?? "Retry failed"
                    self.errorMessage = errorMsg

                    Analytics.shared.track("Transcription Retry Failed", eventProperties: [
                        "app_name": appName,
                        "error": errorMsg
                    ])

                    self.refreshHistory()
                    self.finishProcessing()
                }
            }
        }
    }

    private func pasteText() {
        log("📌 [PASTE] Sending paste command (Cmd+V) to app: \(targetApplication?.localizedName ?? "Unknown")")
        let source = CGEventSource(stateID: .hidSystemState)
        let keyDown = CGEvent(keyboardEventSource: source, virtualKey: 0x09, keyDown: true)
        let keyUp = CGEvent(keyboardEventSource: source, virtualKey: 0x09, keyDown: false)

        keyDown?.flags = .maskCommand
        keyUp?.flags = .maskCommand

        keyDown?.post(tap: .cghidEventTap)
        keyUp?.post(tap: .cghidEventTap)
        log("✅ [PASTE] Paste command sent successfully")

        Analytics.shared.track("Text Pasted", eventProperties: [
            "target_app": targetApplication?.localizedName ?? "Unknown",
            "text_length": NSPasteboard.general.string(forType: .string)?.count ?? 0
        ])
    }

    private func activateTargetAppIfNeeded() {
        guard let app = targetApplication else { return }
        if app.bundleIdentifier == Bundle.main.bundleIdentifier {
            return
        }
        if #available(macOS 14, *) {
            _ = app.activate(options: [.activateAllWindows])
        } else {
            _ = app.activate(options: [.activateAllWindows, .activateIgnoringOtherApps])
        }
    }

    private func ensureRecordingIndicator() {
        if recordingIndicator == nil {
            recordingIndicator = RecordingIndicatorWindow(appState: self)
        }
    }

    private func setProcessing(_ processing: Bool) {
        isProcessing = processing
        updateRecordingIndicatorVisibility()
    }

    private func finishProcessing() {
        setProcessing(false)
    }

    private func updateRecordingIndicatorVisibility() {
        if isRecording || isProcessing || isInitializingModel {
            ensureRecordingIndicator()
            recordingIndicator?.show()
        } else {
            recordingIndicator?.hide()
        }
    }

    // MARK: - Settings

    func setApiKey(_ key: String, for provider: CompletionProvider) {
        let trimmed = key.trimmingCharacters(in: .whitespacesAndNewlines)
        if engine.setCompletionProvider(provider, apiKey: trimmed) {
            isConfigured = engine.isConfigured
            errorMessage = nil
            Analytics.shared.track("\(provider.displayName) API Key Set")
        } else {
            isConfigured = engine.isConfigured
            errorMessage = engine.lastError ?? "Failed to set \(provider.displayName) API key"
        }
    }

    func setProvider(_ provider: CompletionProvider, apiKey: String? = nil) {
        let success: Bool
        if let key = apiKey, !key.isEmpty {
            // Save API key and switch provider
            success = engine.setCompletionProvider(provider, apiKey: key)
        } else {
            // Just switch provider using saved key
            success = engine.switchCompletionProvider(provider)
        }

        if success {
            isConfigured = engine.isConfigured
            errorMessage = nil
            Analytics.shared.track("Provider Changed", eventProperties: ["provider": provider.displayName])
        } else {
            isConfigured = engine.isConfigured
            errorMessage = engine.lastError ?? "Failed to set provider"
        }
    }

    func setMode(_ mode: WritingMode) {
        // Always set the mode for the target app (not Flow itself)
        if engine.setMode(mode, for: targetAppName) {
            currentMode = mode
            targetAppMode = mode
            Analytics.shared.track("Writing Mode Changed", eventProperties: [
                "mode": mode.rawValue,
                "app_name": targetAppName,
                "app_category": targetAppCategory.rawValue
            ])
        }
    }

    // MARK: - Shortcuts

    func addShortcut(trigger: String, replacement: String) -> Bool {
        let result = engine.addShortcut(trigger: trigger, replacement: replacement)
        if result {
            Analytics.shared.track("Shortcut Added", eventProperties: [
                "trigger_length": trigger.count,
                "replacement_length": replacement.count
            ])
        }
        return result
    }

    func removeShortcut(trigger: String) -> Bool {
        let result = engine.removeShortcut(trigger: trigger)
        if result {
            Analytics.shared.track("Shortcut Removed")
        }
        return result
    }

    // MARK: - Stats

    var todayTranscriptions: Int {
        let calendar = Calendar.current
        return history.filter { calendar.isDateInToday($0.createdAt) }.count
    }

    var totalTranscriptions: Int {
        (engine.stats?["total_transcriptions"] as? Int) ?? 0
    }

    var totalMinutes: Int {
        let ms = (engine.stats?["total_duration_ms"] as? Int) ?? 0
        return ms / 60000
    }

    var totalWordsDictated: Int {
        (engine.stats?["total_words_dictated"] as? Int) ?? 0
    }

    private struct MediaPauseState {
        var musicWasPlaying = false
        var spotifyWasPlaying = false
    }

    private func pauseMediaPlayback() {
        mediaPauseState.musicWasPlaying = pauseIfPlaying(app: "Music")
        mediaPauseState.spotifyWasPlaying = pauseIfPlaying(app: "Spotify")
    }

    private func resumeMediaPlayback() {
        if mediaPauseState.musicWasPlaying {
            resumeApp(app: "Music")
        }
        if mediaPauseState.spotifyWasPlaying {
            resumeApp(app: "Spotify")
        }
        mediaPauseState = MediaPauseState()
    }

    private func pauseIfPlaying(app: String) -> Bool {
        let script = """
        tell application \"\(app)\"
            if it is running then
                if player state is playing then
                    pause
                    return \"playing\"
                end if
            end if
        end tell
        return \"\"
        """

        return runAppleScript(script) == "playing"
    }

    private func resumeApp(app: String) {
        let script = """
        tell application \"\(app)\"
            if it is running then
                play
            end if
        end tell
        """

        _ = runAppleScript(script)
    }

    private func runAppleScript(_ script: String) -> String? {
        guard let appleScript = NSAppleScript(source: script) else { return nil }
        var error: NSDictionary?
        let result = appleScript.executeAndReturnError(&error)
        if error != nil {
            return nil
        }
        return result.stringValue
    }
}
