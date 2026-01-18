//
// HistoryView.swift
// Flow
//
// Transcription history list.
//

import Flow
import SwiftUI

struct HistoryListView: View {
    @EnvironmentObject var appState: AppState

    var body: some View {
        VStack(alignment: .leading, spacing: FW.spacing24) {
            if appState.history.isEmpty {
                emptyState
            } else {
                ForEach(sections) { section in
                    VStack(alignment: .leading, spacing: FW.spacing12) {
                        Text(section.title.uppercased())
                            .font(FW.fontMonoSmall)
                            .foregroundStyle(FW.textTertiary)

                        VStack(spacing: 0) {
                            ForEach(Array(section.items.enumerated()), id: \.element.id) { index, item in
                                historyRow(item)
                                if index < section.items.count - 1 {
                                    Divider()
                                        .padding(.leading, 72)
                                }
                            }
                        }
                        .background {
                            RoundedRectangle(cornerRadius: FW.radiusMedium)
                                .fill(FW.surface)
                                .overlay {
                                    RoundedRectangle(cornerRadius: FW.radiusMedium)
                                        .strokeBorder(FW.border, lineWidth: 1)
                                }
                        }
                    }
                }
            }
        }
        .onAppear {
            appState.refreshHistory()
            Analytics.shared.track("History Viewed", eventProperties: [
                "history_count": appState.history.count
            ])
        }
    }

    private var emptyState: some View {
        VStack(spacing: FW.spacing8) {
            Text("No transcriptions yet")
                .font(.headline)
                .foregroundStyle(FW.textPrimary)

            Text("Your recent dictations will show up here.")
                .font(.caption)
                .foregroundStyle(FW.textTertiary)
        }
        .frame(maxWidth: .infinity)
        .padding(.vertical, FW.spacing32)
        .fwCard()
    }

    private func historyRow(_ item: TranscriptionSummary) -> some View {
        HistoryRowView(item: item, retryableHistoryId: appState.retryableHistoryId) {
            appState.retryLastTranscription()
        }
    }

    private var sections: [HistorySection] {
        let calendar = Calendar.current
        let grouped = Dictionary(grouping: appState.history) { item in
            calendar.startOfDay(for: item.createdAt)
        }

        return grouped
            .map { date, items in
                let title: String
                if calendar.isDateInToday(date) {
                    title = "Today"
                } else if calendar.isDateInYesterday(date) {
                    title = "Yesterday"
                } else {
                    title = dateFormatter.string(from: date)
                }

                return HistorySection(title: title, items: items.sorted { $0.createdAt > $1.createdAt })
            }
            .sorted { $0.sortDate > $1.sortDate }
    }

    private var dateFormatter: DateFormatter {
        let formatter = DateFormatter()
        formatter.dateStyle = .medium
        formatter.timeStyle = .none
        return formatter
    }
}

private struct HistoryRowView: View {
    let item: TranscriptionSummary
    let retryableHistoryId: String?
    let onRetry: () -> Void

    @State private var isHovering = false
    @State private var showCopied = false

    private var timeFormatter: DateFormatter {
        let formatter = DateFormatter()
        formatter.dateFormat = "HH:mm"
        return formatter
    }

    var body: some View {
        HStack(alignment: .top, spacing: FW.spacing12) {
            // Time column - fixed width, top aligned
            Text(timeFormatter.string(from: item.createdAt))
                .font(FW.fontMonoSmall)
                .foregroundStyle(FW.textMuted)
                .frame(width: 48, alignment: .trailing)
                .padding(.top, FW.spacing12)

            // Content column
            VStack(alignment: .leading, spacing: FW.spacing4) {
                if item.status == .success {
                    Text(item.text)
                        .font(.body)
                        .foregroundStyle(FW.textPrimary)
                        .fixedSize(horizontal: false, vertical: true)

//                    #if DEBUG
                    if isHovering && !item.rawText.isEmpty {
                        Text(item.rawText)
                            .font(.caption)
                            .foregroundStyle(FW.textMuted)
                    }
//                    #endif
                } else {
                    HStack(spacing: FW.spacing6) {
                        Text("Failed")
                            .font(.caption.weight(.medium))
                            .foregroundStyle(FW.warning)

                        Text(item.error ?? "Transcription failed")
                            .font(.body)
                            .foregroundStyle(FW.textSecondary)
                    }
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.vertical, FW.spacing12)

            // Actions column
            HStack(spacing: FW.spacing8) {
                if item.status == .success {
                    Button {
                        copyToClipboard()
                    } label: {
                        Image(systemName: showCopied ? "checkmark" : "doc.on.doc")
                            .font(.body)
                            .foregroundStyle(showCopied ? FW.success : FW.textSecondary)
                    }
                    .buttonStyle(.plain)
                } else if item.id == retryableHistoryId {
                    Button {
                        onRetry()
                    } label: {
                        Image(systemName: "arrow.clockwise")
                            .font(.body)
                    }
                    .buttonStyle(FWGhostButtonStyle())
                }
            }
            .padding(.top, FW.spacing12)
            .padding(.trailing, FW.spacing4)
        }
        .padding(.horizontal, FW.spacing12)
        .contentShape(Rectangle())
        .onTapGesture {
            if item.status == .success {
                copyToClipboard()
            }
        }
        .onHover { hovering in
            isHovering = hovering
            if hovering {
                NSCursor.pointingHand.push()
            } else {
                NSCursor.pop()
            }
        }
    }

    @MainActor
    private func copyToClipboard() {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(item.text, forType: .string)
        Analytics.shared.track("History Item Copied", eventProperties: [
            "text_length": item.text.count
        ])

        withAnimation {
            showCopied = true
        }

        DispatchQueue.main.asyncAfter(deadline: .now() + 1) {
            withAnimation {
                showCopied = false
            }
        }
    }
}

private struct HistorySection: Identifiable {
    let id = UUID()
    let title: String
    let items: [TranscriptionSummary]

    var sortDate: Date {
        items.map { $0.createdAt }.max() ?? Date.distantPast
    }
}

#Preview {
    HistoryListView()
        .environmentObject(AppState())
}
