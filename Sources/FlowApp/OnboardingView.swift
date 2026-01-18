//
// OnboardingView.swift
// Flow
//
// Guided onboarding flow for first launch.
//

import AppKit
import SwiftUI

struct OnboardingView: View {
    @EnvironmentObject var appState: AppState
    @State private var step: Step = .welcome
    @State private var openAIKey = ""
    @FocusState private var focusedField: Field?

    private enum Field {
        case apiKey
    }

    private enum Step: Int, CaseIterable {
        case welcome
        case apiKey
        case accessibility
        case hotkey

        var title: String {
            switch self {
            case .welcome:
                return "Welcome to Flow"
            case .apiKey:
                return "Connect Your API Key"
            case .accessibility:
                return "Enable Accessibility"
            case .hotkey:
                return "Pick a Hotkey"
            }
        }
    }

    var body: some View {
        VStack(spacing: FW.spacing24) {
            header
            content
            footer
        }
        .padding(FW.spacing32)
        .frame(maxWidth: 520)
        .fwCard()
        .onAppear {
            appState.refreshAccessibilityStatus()
        }
        .onChange(of: step) { _, newStep in
            focusedField = newStep == .apiKey ? .apiKey : nil
            if newStep != .hotkey && appState.isCapturingHotkey {
                appState.endHotkeyCapture()
            }
        }
        .onDisappear {
            if appState.isCapturingHotkey {
                appState.endHotkeyCapture()
            }
        }
    }

    private var header: some View {
        VStack(spacing: FW.spacing8) {
            Image(systemName: "waveform")
                .font(.system(size: 36, weight: .semibold))
                .foregroundStyle(FW.accentGradient)

            Text(step.title)
                .font(.title2.weight(.semibold))

            Text("Step \(stepIndex + 1) of \(Step.allCases.count)")
                .font(FW.fontMonoSmall)
                .foregroundStyle(FW.textTertiary)
        }
    }

    @ViewBuilder
    private var content: some View {
        switch step {
        case .welcome:
            VStack(alignment: .leading, spacing: FW.spacing12) {
                Text("Flow turns quick dictation into clean text, fast.")
                    .font(.subheadline)
                    .foregroundStyle(FW.textSecondary)

                VStack(alignment: .leading, spacing: FW.spacing8) {
                    labelRow(icon: "mic.fill", text: "Record from anywhere with a single hotkey")
                    labelRow(icon: "bolt.fill", text: "Instant transcription and paste")
                    labelRow(icon: "text.badge.checkmark", text: "Custom shortcuts and writing modes")
                }
            }

        case .apiKey:
            VStack(alignment: .leading, spacing: FW.spacing16) {
                Text("Add your OpenAI API key to enable transcription.")
                    .font(.subheadline)
                    .foregroundStyle(FW.textSecondary)

                SecureField("sk-...", text: $openAIKey)
                    .textFieldStyle(.roundedBorder)
                    .font(FW.fontMonoSmall)
                    .focused($focusedField, equals: .apiKey)
                    .onSubmit {
                        if !openAIKey.isEmpty {
                            handleAdvance()
                        }
                    }

                if appState.isConfigured {
                    Text("API key saved")
                        .font(.caption)
                        .foregroundStyle(FW.success)
                } else {
                    Text("You can add this later in Settings, but recording will be disabled until you do.")
                        .font(.caption)
                        .foregroundStyle(FW.textTertiary)
                }
            }

        case .accessibility:
            VStack(alignment: .leading, spacing: FW.spacing16) {
                Text("To listen for your hotkey, Flow needs Accessibility access. You control this anytime in System Settings.")
                    .font(.subheadline)
                    .foregroundStyle(FW.textSecondary)

                HStack(spacing: FW.spacing8) {
                    Circle()
                        .fill(appState.isAccessibilityEnabled ? FW.success : FW.warning)
                        .frame(width: 8, height: 8)

                    Text(appState.isAccessibilityEnabled ? "Accessibility enabled" : "Accessibility not enabled")
                        .font(.caption)
                        .foregroundStyle(appState.isAccessibilityEnabled ? FW.success : FW.warning)
                }

                HStack(spacing: FW.spacing12) {
                    Button("Enable Accessibility") {
                        appState.requestAccessibilityPermission()
                    }
                    .buttonStyle(FWSecondaryButtonStyle())

                    Button("Open System Settings") {
                        if let url = URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility") {
                            NSWorkspace.shared.open(url)
                        }
                    }
                    .buttonStyle(FWSecondaryButtonStyle())
                }

                Button("Check Again") {
                    appState.refreshAccessibilityStatus()
                }
                .buttonStyle(FWGhostButtonStyle())
                .font(.caption)

                Text("You can continue without it, but hotkeys will not work.")
                    .font(.caption)
                    .foregroundStyle(FW.textTertiary)
            }

        case .hotkey:
            VStack(alignment: .leading, spacing: FW.spacing16) {
                Text("Choose a hotkey for starting and stopping recording. Fn defaults to press-and-hold.")
                    .font(.subheadline)
                    .foregroundStyle(FW.textSecondary)

                HStack(spacing: FW.spacing8) {
                    Text("Current:")
                        .font(.caption)
                        .foregroundStyle(FW.textTertiary)
                    Text(appState.hotkey.displayName)
                        .font(FW.fontMonoSmall)
                        .foregroundStyle(FW.textPrimary)
                }

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
    }

    private var footer: some View {
        HStack {
            Button("Back") {
                step = Step(rawValue: max(step.rawValue - 1, 0)) ?? step
            }
            .buttonStyle(FWGhostButtonStyle())
            .disabled(step == .welcome)

            Spacer()

            Button(advanceLabel) {
                handleAdvance()
            }
            .buttonStyle(FWPrimaryButtonStyle())
        }
    }

    private var stepIndex: Int {
        step.rawValue
    }

    private var advanceLabel: String {
        if step == .hotkey { return "Finish" }
        if step == .apiKey && !openAIKey.isEmpty { return "Save and Continue" }
        if step == .apiKey && openAIKey.isEmpty && !appState.isConfigured { return "Skip for now" }
        if step == .accessibility && !appState.isAccessibilityEnabled { return "Continue without" }
        return "Next"
    }

    private func handleAdvance() {
        if step == .apiKey, !openAIKey.isEmpty {
            appState.setApiKey(openAIKey, for: .openAI)
            openAIKey = ""
        }

        if step == .hotkey {
            appState.completeOnboarding()
            return
        }

        step = Step(rawValue: min(step.rawValue + 1, Step.allCases.count - 1)) ?? step
    }

    private func labelRow(icon: String, text: String) -> some View {
        HStack(spacing: FW.spacing8) {
            Image(systemName: icon)
                .font(.caption)
                .foregroundStyle(FW.accent)
            Text(text)
                .font(.subheadline)
                .foregroundStyle(FW.textPrimary)
        }
    }
}

#Preview {
    OnboardingView()
        .environmentObject(AppState())
}
