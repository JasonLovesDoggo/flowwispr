//
// SettingsView.swift
// Flow
//
// Settings content view with sections for API, general settings, and about.
//

import AppKit
import Flow
import SwiftUI

struct SettingsContentView: View {
    var body: some View {
        ScrollView {
            VStack(spacing: FW.spacing24) {
                APISettingsSection()
                Divider()
                GeneralSettingsSection()
                Divider()
                AccessibilitySection()
                Divider()
                LearningStatsSection()
                Divider()
                AboutSection()
            }
            .padding(FW.spacing24)
        }
    }
}

// MARK: - API Settings

struct APISettingsSection: View {
    @EnvironmentObject var appState: AppState
    @State private var openAIKey = ""
    @State private var showOpenAIKey = false
    @State private var geminiKey = ""
    @State private var showGeminiKey = false
    @State private var openRouterKey = ""
    @State private var showOpenRouterKey = false
    @State private var selectedProvider: CompletionProvider = .openAI
    @State private var useLocalTranscription = false
    @State private var selectedWhisperModel: WhisperModel = .quality
    @State private var existingOpenAIKey: String?
    @State private var existingGeminiKey: String?
    @State private var existingOpenRouterKey: String?

    var body: some View {
        VStack(alignment: .leading, spacing: FW.spacing16) {
            Label("API Keys", systemImage: "key")
                .font(.headline)

            // Provider Selection
            VStack(alignment: .leading, spacing: FW.spacing8) {
                Text("Active Provider")
                    .font(.subheadline.weight(.medium))
                    .foregroundStyle(FW.textPrimary)

                Picker("", selection: $selectedProvider) {
                    ForEach([CompletionProvider.openAI, CompletionProvider.gemini, CompletionProvider.openRouter], id: \.rawValue) { provider in
                        Text(provider.displayName).tag(provider)
                    }
                }
                .pickerStyle(.segmented)
                .labelsHidden()
                .onChange(of: selectedProvider) { _, newProvider in
                    // Switch provider using saved API key from database
                    if !appState.engine.switchCompletionProvider(newProvider) {
                        // Switch failed, revert selection
                        if let current = appState.engine.completionProvider {
                            selectedProvider = current
                        }
                        appState.errorMessage = appState.engine.lastError ?? "Failed to switch provider. Make sure you've saved an API key for \(newProvider.displayName)."
                    } else {
                        appState.isConfigured = appState.engine.isConfigured
                        appState.errorMessage = nil
                    }
                }
                .onAppear {
                    // Load current provider
                    if let current = appState.engine.completionProvider {
                        selectedProvider = current
                    }
                }

                if let current = appState.engine.completionProvider {
                    Text("Currently using: \(current.displayName)")
                        .font(.caption)
                        .foregroundStyle(FW.textTertiary)
                }
            }

            Divider()

            // OpenAI
            VStack(alignment: .leading, spacing: FW.spacing8) {
                Text("OpenAI (Whisper)")
                    .font(.subheadline.weight(.medium))
                    .foregroundStyle(FW.textPrimary)

                HStack {
                    Group {
                        if showOpenAIKey {
                            TextField("sk-...", text: $openAIKey)
                        } else {
                            SecureField("sk-...", text: $openAIKey)
                        }
                    }
                    .textFieldStyle(.roundedBorder)
                    .font(FW.fontMonoSmall)

                    Button {
                        showOpenAIKey.toggle()
                    } label: {
                        Image(systemName: showOpenAIKey ? "eye.slash" : "eye")
                    }
                    .buttonStyle(.borderless)

                    Button("Save") {
                        appState.setApiKey(openAIKey, for: .openAI)
                        // Refresh the masked key display
                        existingOpenAIKey = appState.engine.maskedOpenAIKey
                        openAIKey = ""
                    }
                    .buttonStyle(FWSecondaryButtonStyle())
                    .disabled(openAIKey.isEmpty)
                }

                if let existing = existingOpenAIKey {
                    Text("Currently configured: \(existing)")
                        .font(.caption)
                        .foregroundStyle(FW.success)
                } else {
                    Text("Required for transcription")
                        .font(.caption)
                        .foregroundStyle(FW.textTertiary)
                }
            }

            // Gemini
            VStack(alignment: .leading, spacing: FW.spacing8) {
                Text("Gemini")
                    .font(.subheadline.weight(.medium))
                    .foregroundStyle(FW.textPrimary)

                HStack {
                    Group {
                        if showGeminiKey {
                            TextField("AI...", text: $geminiKey)
                        } else {
                            SecureField("AI...", text: $geminiKey)
                        }
                    }
                    .textFieldStyle(.roundedBorder)
                    .font(FW.fontMonoSmall)

                    Button {
                        showGeminiKey.toggle()
                    } label: {
                        Image(systemName: showGeminiKey ? "eye.slash" : "eye")
                    }
                    .buttonStyle(.borderless)

                    Button("Save") {
                        appState.setApiKey(geminiKey, for: .gemini)
                        // Refresh the masked key display
                        existingGeminiKey = appState.engine.maskedGeminiKey
                        geminiKey = ""
                    }
                    .buttonStyle(FWSecondaryButtonStyle())
                    .disabled(geminiKey.isEmpty)
                }

                if let existing = existingGeminiKey {
                    Text("Currently configured: \(existing)")
                        .font(.caption)
                        .foregroundStyle(FW.success)
                } else {
                    Text("Alternative provider for transcription and completion")
                        .font(.caption)
                        .foregroundStyle(FW.textTertiary)
                }
            }

            // OpenRouter
            VStack(alignment: .leading, spacing: FW.spacing8) {
                Text("OpenRouter")
                    .font(.subheadline.weight(.medium))
                    .foregroundStyle(FW.textPrimary)

                HStack {
                    Group {
                        if showOpenRouterKey {
                            TextField("sk-or-v1-...", text: $openRouterKey)
                        } else {
                            SecureField("sk-or-v1-...", text: $openRouterKey)
                        }
                    }
                    .textFieldStyle(.roundedBorder)
                    .font(FW.fontMonoSmall)

                    Button {
                        showOpenRouterKey.toggle()
                    } label: {
                        Image(systemName: showOpenRouterKey ? "eye.slash" : "eye")
                    }
                    .buttonStyle(.borderless)

                    Button("Save") {
                        appState.setApiKey(openRouterKey, for: .openRouter)
                        // Refresh the masked key display
                        existingOpenRouterKey = appState.engine.maskedOpenRouterKey
                        openRouterKey = ""
                    }
                    .buttonStyle(FWSecondaryButtonStyle())
                    .disabled(openRouterKey.isEmpty)
                }

                if let existing = existingOpenRouterKey {
                    Text("Currently configured: \(existing)")
                        .font(.caption)
                        .foregroundStyle(FW.success)
                } else {
                    Text("Access multiple LLM providers (Llama, Claude, GPT, etc.)")
                        .font(.caption)
                        .foregroundStyle(FW.textTertiary)
                }
            }

            Divider()

            // Transcription Mode
            VStack(alignment: .leading, spacing: FW.spacing8) {
                Text("Transcription")
                    .font(.subheadline.weight(.medium))
                    .foregroundStyle(FW.textPrimary)

                Toggle("Use local Whisper (privacy, no API costs)", isOn: $useLocalTranscription)
                    .onChange(of: useLocalTranscription) { _, newValue in
                        if newValue {
                            _ = appState.engine.setTranscriptionMode(.local(model: selectedWhisperModel))
                        } else {
                            _ = appState.engine.setTranscriptionMode(.remote)
                        }
                    }

                if useLocalTranscription {
                    VStack(alignment: .leading, spacing: FW.spacing8) {
                        Text("Model Size")
                            .font(.caption)
                            .foregroundStyle(FW.textSecondary)

                        Picker("", selection: $selectedWhisperModel) {
                            ForEach([WhisperModel.turbo, WhisperModel.fast, WhisperModel.balanced, WhisperModel.quality, WhisperModel.best], id: \.rawValue) { model in
                                HStack {
                                    VStack(alignment: .leading) {
                                        HStack(spacing: 4) {
                                            Text(model.displayName)
                                            if model == .quality {
                                                Text("Recommended")
                                                    .font(.caption2)
                                                    .padding(.horizontal, 4)
                                                    .padding(.vertical, 1)
                                                    .background(FW.accent.opacity(0.2))
                                                    .foregroundStyle(FW.accent)
                                                    .cornerRadius(4)
                                            }
                                        }
                                        Text(model.sizeDescription)
                                            .font(.caption2)
                                            .foregroundStyle(FW.textTertiary)
                                    }
                                }
                                .tag(model)
                            }
                        }
                        .pickerStyle(.radioGroup)
                        .onChange(of: selectedWhisperModel) { _, newModel in
                            _ = appState.engine.setTranscriptionMode(.local(model: newModel))
                        }

                        Text("Model will be downloaded when selected")
                            .font(.caption2)
                            .foregroundStyle(FW.textTertiary)
                    }
                    .padding(.leading, 20)
                } else {
                    Text("Using cloud provider (\(selectedProvider.displayName))")
                        .font(.caption)
                        .foregroundStyle(FW.textTertiary)
                }
            }

            // Status
            HStack(spacing: FW.spacing8) {
                Circle()
                    .fill(appState.isConfigured ? FW.success : FW.warning)
                    .frame(width: 10, height: 10)

                Text(appState.isConfigured ? "API configured" : "API key required")
                    .foregroundStyle(appState.isConfigured ? FW.success : FW.warning)
            }
            .padding(.top, FW.spacing8)
        }
        .onAppear {
            // Load current provider
            if let current = appState.engine.completionProvider {
                selectedProvider = current
            }

            // Load masked API keys to show they're configured
            existingOpenAIKey = appState.engine.maskedOpenAIKey
            existingGeminiKey = appState.engine.maskedGeminiKey
            existingOpenRouterKey = appState.engine.maskedOpenRouterKey

            // Load transcription mode settings from database
            if let mode = appState.engine.getTranscriptionMode() {
                switch mode {
                case .local(let model):
                    useLocalTranscription = true
                    selectedWhisperModel = model
                case .remote:
                    useLocalTranscription = false
                }
            }
        }
    }
}

// MARK: - General Settings

struct GeneralSettingsSection: View {
    @EnvironmentObject var appState: AppState
    @AppStorage("launchAtLogin") private var launchAtLogin = false
    @AppStorage("playSounds") private var playSounds = true
    @AppStorage("defaultMode") private var defaultMode = 1

    var body: some View {
        VStack(alignment: .leading, spacing: FW.spacing16) {
            Label("General", systemImage: "gear")
                .font(.headline)

            VStack(alignment: .leading, spacing: FW.spacing12) {
                Toggle("Launch at login", isOn: $launchAtLogin)
                Toggle("Play sounds", isOn: $playSounds)
            }

            VStack(alignment: .leading, spacing: FW.spacing8) {
                Text("Default writing mode")
                    .font(.subheadline.weight(.medium))

                Picker("", selection: $defaultMode) {
                    ForEach(WritingMode.allCases, id: \.rawValue) { mode in
                        Text(mode.displayName).tag(Int(mode.rawValue))
                    }
                }
                .pickerStyle(.segmented)
                .labelsHidden()

                if let mode = WritingMode(rawValue: UInt8(defaultMode)) {
                    Text(mode.description)
                        .font(.caption)
                        .foregroundStyle(FW.textTertiary)
                }
            }
        }
    }
}

// MARK: - Accessibility
struct AccessibilitySection: View {
    @EnvironmentObject var appState: AppState

    var body: some View {
        VStack(alignment: .leading, spacing: FW.spacing16) {
            Label("Keyboard", systemImage: "keyboard")
                .font(.headline)

            VStack(alignment: .leading, spacing: FW.spacing8) {
                HStack(spacing: FW.spacing8) {
                    Text("🌐")
                        .font(.title2)
                    Text("Fn key is the default hotkey")
                        .font(.subheadline.weight(.medium))
                }

                Text("Hold the Fn key to record, release to stop. Custom hotkeys toggle recording. This requires Accessibility permission.")
                    .font(.caption)
                    .foregroundStyle(FW.textSecondary)

                Button("Open Privacy Settings") {
                    if let url = URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility") {
                        NSWorkspace.shared.open(url)
                    }
                }
                .buttonStyle(FWSecondaryButtonStyle())
                .padding(.top, FW.spacing4)
            }

            Divider()

            VStack(alignment: .leading, spacing: FW.spacing8) {
                Text("Recording hotkey")
                    .font(.subheadline.weight(.medium))

                Text("Current: \(appState.hotkey.displayName)")
                    .font(.caption)
                    .foregroundStyle(FW.textSecondary)

                HStack(spacing: FW.spacing12) {
                    Button(appState.isCapturingHotkey ? "Press keys..." : "Change Hotkey") {
                        if appState.isCapturingHotkey {
                            appState.endHotkeyCapture()
                        } else {
                            appState.beginHotkeyCapture()
                        }
                    }
                    .buttonStyle(FWSecondaryButtonStyle())

                    Button("Use Fn Key") {
                        appState.setHotkey(Hotkey.defaultHotkey)
                    }
                    .buttonStyle(FWSecondaryButtonStyle())
                }

                if appState.isCapturingHotkey {
                    Text("Press a key combination, or Esc to cancel.")
                        .font(.caption)
                        .foregroundStyle(FW.textTertiary)
                }
            }
        }
        .onDisappear {
            if appState.isCapturingHotkey {
                appState.endHotkeyCapture()
            }
        }
    }
}

// MARK: - Learning Stats

struct LearningStatsSection: View {
    @EnvironmentObject var appState: AppState

    var body: some View {
        VStack(alignment: .leading, spacing: FW.spacing16) {
            Label("Learning System", systemImage: "brain")
                .font(.headline)

            VStack(alignment: .leading, spacing: FW.spacing12) {
                // Correction count
                HStack {
                    VStack(alignment: .leading, spacing: 4) {
                        Text("Learned Corrections")
                            .font(.subheadline.weight(.medium))
                        Text("Auto-applies high-confidence fixes")
                            .font(.caption)
                            .foregroundStyle(FW.textTertiary)
                    }
                    Spacer()
                    Text("\(appState.engine.correctionCount)")
                        .font(.title2.weight(.semibold))
                        .foregroundStyle(FW.accent)
                }
                .padding(FW.spacing12)
                .background(FW.accent.opacity(0.1))
                .cornerRadius(8)

                // Shortcut count
                HStack {
                    VStack(alignment: .leading, spacing: 4) {
                        Text("Voice Shortcuts")
                            .font(.subheadline.weight(.medium))
                        Text("Custom expansions you've created")
                            .font(.caption)
                            .foregroundStyle(FW.textTertiary)
                    }
                    Spacer()
                    Text("\(appState.engine.shortcutCount)")
                        .font(.title2.weight(.semibold))
                        .foregroundStyle(FW.accent)
                }
                .padding(FW.spacing12)
                .background(FW.accent.opacity(0.1))
                .cornerRadius(8)

                // Style suggestion
                if let suggestion = appState.engine.styleSuggestion {
                    HStack {
                        VStack(alignment: .leading, spacing: 4) {
                            Text("Suggested Style for \(appState.currentApp)")
                                .font(.subheadline.weight(.medium))
                            Text("Based on your writing patterns")
                                .font(.caption)
                                .foregroundStyle(FW.textTertiary)
                        }
                        Spacer()
                        Text(suggestion.displayName)
                            .font(.title3.weight(.semibold))
                            .foregroundStyle(FW.success)
                    }
                    .padding(FW.spacing12)
                    .background(FW.success.opacity(0.1))
                    .cornerRadius(8)
                } else {
                    VStack(alignment: .leading, spacing: 4) {
                        Text("Style Learning")
                            .font(.subheadline.weight(.medium))
                        Text("Need 3+ samples in \(appState.currentApp) to suggest a style")
                            .font(.caption)
                            .foregroundStyle(FW.textTertiary)
                    }
                    .padding(FW.spacing12)
                    .background(FW.textTertiary.opacity(0.05))
                    .cornerRadius(8)
                }

                // Total stats
                VStack(alignment: .leading, spacing: 8) {
                    Text("All-Time Stats")
                        .font(.caption.weight(.medium))
                        .foregroundStyle(FW.textSecondary)

                    HStack(spacing: FW.spacing24) {
                        VStack(spacing: 4) {
                            Text("\(appState.totalTranscriptions)")
                                .font(.title3.weight(.semibold))
                            Text("Transcriptions")
                                .font(.caption2)
                                .foregroundStyle(FW.textTertiary)
                        }

                        VStack(spacing: 4) {
                            Text("\(appState.totalMinutes)")
                                .font(.title3.weight(.semibold))
                            Text("Minutes")
                                .font(.caption2)
                                .foregroundStyle(FW.textTertiary)
                        }
                    }
                }
                .padding(.top, FW.spacing8)
            }
        }
    }
}

// MARK: - About

struct AboutSection: View {
    var body: some View {
        VStack(spacing: FW.spacing16) {
            // logo
            ZStack {
                Circle()
                    .fill(FW.accentGradient)
                    .frame(width: 60, height: 60)

                Image(systemName: "waveform")
                    .font(.system(size: 24, weight: .medium))
                    .foregroundStyle(.white)
            }

            VStack(spacing: FW.spacing4) {
                Text("Flow")
                    .font(.title3.weight(.semibold))

                Text("v0.1.7")
                    .font(FW.fontMonoSmall)
                    .foregroundStyle(FW.textTertiary)
            }

            Text("Voice dictation powered by AI")
                .font(.subheadline)
                .foregroundStyle(FW.textSecondary)

            HStack(spacing: FW.spacing24) {
                Link(destination: URL(string: "https://gowithflow.tech")!) {
                    HStack(spacing: FW.spacing4) {
                        Image(systemName: "globe")
                        Text("Website")
                    }
                    .font(.caption)
                    .foregroundStyle(FW.accent)
                }

                Link(destination: URL(string: "https://github.com/jasonlovesdoggo/flow")!) {
                    HStack(spacing: FW.spacing4) {
                        Image(systemName: "chevron.left.forwardslash.chevron.right")
                        Text("GitHub")
                    }
                    .font(.caption)
                    .foregroundStyle(FW.accent)
                }
            }
        }
        .frame(maxWidth: .infinity)
        .padding(.vertical, FW.spacing16)
    }
}

#Preview {
    SettingsContentView()
        .environmentObject(AppState())
}
