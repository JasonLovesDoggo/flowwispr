//
// RecordView.swift
// Flow
//
// Main recording view with waveform visualization and transcription output.
//

import Flow
import SwiftUI

struct RecordView: View {
    @EnvironmentObject var appState: AppState

    var body: some View {
        ScrollView {
            VStack(spacing: 0) {
                // Header section
                headerSection
                    .padding(.horizontal, FW.spacing32)
                    .padding(.top, FW.spacing32)
                    .padding(.bottom, FW.spacing24)

                // Stats bars
                statsSection
                    .padding(.horizontal, FW.spacing32)
                    .padding(.bottom, FW.spacing24)

                if let errorMessage = appState.errorMessage {
                    errorBanner(text: errorMessage)
                        .padding(.horizontal, FW.spacing32)
                        .padding(.bottom, FW.spacing16)
                }

                if !appState.isConfigured {
                    banner(text: "Add your API key to start recording.", actionTitle: "Open Settings") {
                        appState.selectedTab = .settings
                    }
                    .padding(.horizontal, FW.spacing32)
                    .padding(.bottom, FW.spacing16)
                }

                if !appState.isAccessibilityEnabled {
                    banner(text: "Enable Accessibility to use the hotkey.", actionTitle: "Enable") {
                        appState.requestAccessibilityPermission()
                    }
                    .padding(.horizontal, FW.spacing32)
                    .padding(.bottom, FW.spacing16)
                }

                // Hero waveform + record button
                heroSection
                    .padding(.horizontal, FW.spacing32)
                    .padding(.top, FW.spacing8)

                // Context bar
                contextBar
                    .padding(.horizontal, FW.spacing32)
                    .padding(.top, FW.spacing24)

                // Output area
                if let text = appState.lastTranscription {
                    outputSection(text)
                        .padding(.horizontal, FW.spacing32)
                        .padding(.top, FW.spacing16)
                }

                // History
                HistoryListView()
                    .padding(.horizontal, FW.spacing32)
                    .padding(.top, FW.spacing24)

                Spacer(minLength: FW.spacing32)
            }
        }
        .background(FW.background)
    }

    // MARK: - Header Section

    private var headerSection: some View {
        HStack {
            VStack(alignment: .leading, spacing: FW.spacing4) {
                Text(greeting)
                    .font(.system(size: 28, weight: .bold))
                    .foregroundStyle(FW.textPrimary)

                Text("Ready to capture your thoughts")
                    .font(.body)
                    .foregroundStyle(FW.textSecondary)
            }

            Spacer()
        }
    }

    private var greeting: String {
        let hour = Calendar.current.component(.hour, from: Date())
        switch hour {
        case 5 ..< 12: return "Good morning"
        case 12 ..< 17: return "Good afternoon"
        default: return "Good evening"
        }
    }

    // MARK: - Stats Section

    private var statsSection: some View {
        ViewThatFits(in: .horizontal) {
            // Try 3 across
            HStack(spacing: FW.spacing12) {
                statCard(icon: "mic.fill", iconColor: FW.accent, value: "\(appState.todayTranscriptions)", label: "Today")
                    .frame(minWidth: 130)
                statCard(icon: "textformat", iconColor: FW.accent, value: "\(appState.totalWordsDictated)", label: "Words")
                    .frame(minWidth: 130)
                statCard(icon: "clock.fill", iconColor: FW.textMuted, value: "\(appState.totalMinutes)", label: "Minutes")
                    .frame(minWidth: 130)
            }

            // Try 2 + 1
            VStack(spacing: FW.spacing12) {
                HStack(spacing: FW.spacing12) {
                    statCard(icon: "mic.fill", iconColor: FW.accent, value: "\(appState.todayTranscriptions)", label: "Today")
                        .frame(minWidth: 130)
                    statCard(icon: "textformat", iconColor: FW.accent, value: "\(appState.totalWordsDictated)", label: "Words")
                        .frame(minWidth: 130)
                }
                statCard(icon: "clock.fill", iconColor: FW.textMuted, value: "\(appState.totalMinutes)", label: "Minutes")
            }

            // Stack all
            VStack(spacing: FW.spacing12) {
                statCard(icon: "mic.fill", iconColor: FW.accent, value: "\(appState.todayTranscriptions)", label: "Today")
                statCard(icon: "textformat", iconColor: FW.accent, value: "\(appState.totalWordsDictated)", label: "Words")
                statCard(icon: "clock.fill", iconColor: FW.textMuted, value: "\(appState.totalMinutes)", label: "Minutes")
            }
        }
    }

    private func statCard(icon: String, iconColor: Color, value: String, label: String) -> some View {
        HStack(spacing: FW.spacing12) {
            Image(systemName: icon)
                .font(.body)
                .foregroundStyle(iconColor)
                .frame(width: 36, height: 36)
                .background {
                    Circle()
                        .fill(iconColor.opacity(0.1))
                }

            VStack(alignment: .leading, spacing: FW.spacing2) {
                Text(value)
                    .font(.title3.weight(.semibold))
                    .foregroundStyle(FW.textPrimary)

                Text(label)
                    .font(.caption)
                    .foregroundStyle(FW.textSecondary)
                    .lineLimit(1)
            }

            Spacer(minLength: 0)
        }
        .padding(FW.spacing16)
        .frame(maxWidth: .infinity)
        .fwSection()
    }

    // MARK: - Hero Section

    private var heroSection: some View {
        VStack(spacing: FW.spacing24) {
            // Waveform visualization
            WaveformView(isRecording: appState.isRecording, audioLevel: appState.smoothedAudioLevel)
                .frame(height: 80)
                .padding(.horizontal, FW.spacing16)

            // Big record button
            ZStack {
                if appState.isRecording {
                    HStack(spacing: FW.spacing12) {
                        Image(systemName: "stop.fill")
                            .font(.system(size: 18, weight: .semibold))

                        Text(formatDuration(appState.recordingDuration))
                            .font(FW.fontMonoLarge)
                    }
                } else {
                    Text("Record")
                        .font(.system(size: 18, weight: .semibold))
                }
            }
            .frame(width: 200, height: 52)
            .foregroundStyle(appState.isRecording ? .white : FW.textPrimary)
            .background {
                RoundedRectangle(cornerRadius: FW.radiusLarge)
                    .fill(appState.isRecording ? FW.danger : FW.surface)
            }
            .overlay {
                RoundedRectangle(cornerRadius: FW.radiusLarge)
                    .strokeBorder(appState.isRecording ? FW.danger : FW.accent, lineWidth: 2)
            }
            .onTapGesture {
                appState.toggleRecording()
            }

            // Shortcut hint
            if case .globe = appState.hotkey.kind {
                Text("Hold \(appState.hotkey.displayName) to record")
                    .font(FW.fontMonoSmall)
                    .foregroundStyle(FW.textMuted)
            } else {
                Text("Hotkey: \(appState.hotkey.displayName)")
                    .font(FW.fontMonoSmall)
                    .foregroundStyle(FW.textMuted)
            }
        }
        .padding(FW.spacing24)
        .fwSection()
    }

    // MARK: - Context Bar

    private var contextBar: some View {
        HStack(spacing: FW.spacing16) {
            // Target app
            HStack(spacing: FW.spacing8) {
                Image(systemName: "app.fill")
                    .font(.caption)
                    .foregroundStyle(FW.accent)

                Text(appState.targetAppName)
                    .font(.subheadline)
                    .foregroundStyle(FW.textPrimary)
                    .lineLimit(1)
            }

            Spacer()

            // Mode picker
            Menu {
                ForEach(WritingMode.allCases, id: \.self) { mode in
                    Button {
                        appState.setMode(mode)
                    } label: {
                        HStack {
                            Text(mode.displayName)
                            if mode == appState.currentMode {
                                Image(systemName: "checkmark")
                            }
                        }
                    }
                }
            } label: {
                HStack(spacing: FW.spacing4) {
                    Text(appState.currentMode.displayName)
                        .font(.subheadline.weight(.medium))

                    Image(systemName: "chevron.down")
                        .font(.caption2.weight(.semibold))
                }
                .foregroundStyle(FW.accent)
                .padding(.horizontal, FW.spacing12)
                .padding(.vertical, FW.spacing6)
                .background {
                    RoundedRectangle(cornerRadius: FW.radiusSmall)
                        .fill(FW.accent.opacity(0.1))
                }
            }
            .buttonStyle(.plain)
        }
        .padding(FW.spacing12)
        .background {
            RoundedRectangle(cornerRadius: FW.radiusSmall)
                .fill(FW.surface)
                .overlay {
                    RoundedRectangle(cornerRadius: FW.radiusSmall)
                        .strokeBorder(FW.border, lineWidth: 1)
                }
        }
    }

    // MARK: - Output Section

    private func outputSection(_ text: String) -> some View {
        VStack(alignment: .leading, spacing: FW.spacing8) {
            HStack {
                Text("Output")
                    .font(.caption.weight(.medium))
                    .foregroundStyle(FW.textMuted)

                Spacer()

                Button {
                    NSPasteboard.general.clearContents()
                    NSPasteboard.general.setString(text, forType: .string)
                } label: {
                    HStack(spacing: FW.spacing4) {
                        Image(systemName: "doc.on.doc")
                        Text("Copy")
                    }
                    .font(.caption)
                }
                .buttonStyle(FWSecondaryButtonStyle())
            }

            Text(text)
                .font(.body)
                .foregroundStyle(FW.textPrimary)
                .textSelection(.enabled)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(FW.spacing12)
                .background {
                    RoundedRectangle(cornerRadius: FW.radiusSmall)
                        .fill(FW.surface)
                        .overlay {
                            RoundedRectangle(cornerRadius: FW.radiusSmall)
                                .strokeBorder(FW.border, lineWidth: 1)
                        }
                }
        }
    }

    // MARK: - Banners

    private func banner(text: String, actionTitle: String? = nil, action: (() -> Void)? = nil) -> some View {
        HStack(spacing: FW.spacing12) {
            Text(text)
                .font(.subheadline)
                .foregroundStyle(FW.textSecondary)
                .frame(maxWidth: .infinity, alignment: .leading)

            if let actionTitle, let action {
                Button(actionTitle) {
                    action()
                }
                .buttonStyle(FWSecondaryButtonStyle())
            }
        }
        .padding(FW.spacing16)
        .background {
            RoundedRectangle(cornerRadius: FW.radiusSmall)
                .fill(FW.surface)
                .overlay {
                    RoundedRectangle(cornerRadius: FW.radiusSmall)
                        .strokeBorder(FW.warning.opacity(0.3), lineWidth: 1)
                }
        }
    }

    private func errorBanner(text: String) -> some View {
        HStack(spacing: FW.spacing12) {
            Image(systemName: "exclamationmark.triangle.fill")
                .foregroundStyle(FW.danger)

            Text(text)
                .font(.subheadline)
                .foregroundStyle(FW.textSecondary)
                .frame(maxWidth: .infinity, alignment: .leading)

            Button("Retry") {
                appState.retryLastTranscription()
            }
            .buttonStyle(FWSecondaryButtonStyle())

            Button("Dismiss") {
                appState.clearError()
            }
            .buttonStyle(FWGhostButtonStyle())
        }
        .padding(FW.spacing16)
        .background {
            RoundedRectangle(cornerRadius: FW.radiusSmall)
                .fill(FW.surface)
                .overlay {
                    RoundedRectangle(cornerRadius: FW.radiusSmall)
                        .strokeBorder(FW.danger.opacity(0.3), lineWidth: 1)
                }
        }
    }

    // MARK: - Helpers

    private func formatDuration(_ ms: UInt64) -> String {
        let seconds = ms / 1000
        let minutes = seconds / 60
        let secs = seconds % 60
        return String(format: "%d:%02d", minutes, secs)
    }
}

#Preview {
    RecordView()
        .environmentObject(AppState())
}
