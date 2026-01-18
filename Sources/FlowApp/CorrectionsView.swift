//
// CorrectionsView.swift
// Flow
//
// Learned corrections management interface.
// Users can view, search, and delete auto-learned typo corrections.
//

import Flow
import SwiftUI

struct CorrectionsContentView: View {
    @EnvironmentObject var appState: AppState
    @State private var corrections: [Correction] = []
    @State private var searchText = ""
    @State private var showingClearConfirmation = false
    @State private var sortOrder: CorrectionSortOrder = .confidence

    private var filteredCorrections: [Correction] {
        let filtered = searchText.isEmpty
            ? corrections
            : corrections.filter {
                $0.original.localizedCaseInsensitiveContains(searchText) ||
                $0.corrected.localizedCaseInsensitiveContains(searchText)
            }

        switch sortOrder {
        case .confidence:
            return filtered.sorted { $0.confidence > $1.confidence }
        case .occurrences:
            return filtered.sorted { $0.occurrences > $1.occurrences }
        case .alphabetical:
            return filtered.sorted { $0.original.lowercased() < $1.original.lowercased() }
        case .recent:
            return filtered.sorted { $0.updatedAt > $1.updatedAt }
        }
    }

    /// Stats for the header
    private var activeCount: Int {
        corrections.filter { $0.confidence >= 0.55 }.count
    }

    var body: some View {
        VStack(spacing: 0) {
            // Header
            VStack(spacing: FW.spacing16) {
                HStack {
                    VStack(alignment: .leading, spacing: FW.spacing4) {
                        Text("Learnings")
                            .font(.title.weight(.bold))
                            .foregroundStyle(FW.textPrimary)

                        if corrections.isEmpty {
                            Text("Flow learns from your edits after transcription")
                                .font(.body)
                                .foregroundStyle(FW.textSecondary)
                        } else {
                            Text("\(activeCount) active corrections (55%+ confidence)")
                                .font(.body)
                                .foregroundStyle(FW.textSecondary)
                        }
                    }

                    Spacer()

                    if !corrections.isEmpty {
                        Button(role: .destructive) {
                            showingClearConfirmation = true
                        } label: {
                            HStack(spacing: FW.spacing6) {
                                Image(systemName: "trash")
                                Text("Clear All")
                            }
                        }
                        .buttonStyle(FWGhostButtonStyle())
                    }
                }

                if !corrections.isEmpty {
                    HStack(spacing: FW.spacing12) {
                        // Search
                        HStack(spacing: FW.spacing8) {
                            Image(systemName: "magnifyingglass")
                                .foregroundStyle(FW.textMuted)

                            TextField("Search corrections...", text: $searchText)
                                .textFieldStyle(.plain)
                                .font(.body)
                        }
                        .padding(.horizontal, FW.spacing12)
                        .padding(.vertical, FW.spacing8)
                        .background {
                            RoundedRectangle(cornerRadius: FW.radiusSmall)
                                .fill(FW.surface)
                                .overlay {
                                    RoundedRectangle(cornerRadius: FW.radiusSmall)
                                        .strokeBorder(FW.border, lineWidth: 1)
                                }
                        }

                        // Sort picker
                        Menu {
                            ForEach(CorrectionSortOrder.allCases, id: \.self) { order in
                                Button {
                                    sortOrder = order
                                } label: {
                                    HStack {
                                        Text(order.label)
                                        if sortOrder == order {
                                            Image(systemName: "checkmark")
                                        }
                                    }
                                }
                            }
                        } label: {
                            HStack(spacing: FW.spacing4) {
                                Image(systemName: "arrow.up.arrow.down")
                                Text(sortOrder.label)
                            }
                            .font(.subheadline)
                            .foregroundStyle(FW.textSecondary)
                            .padding(.horizontal, FW.spacing12)
                            .padding(.vertical, FW.spacing8)
                            .background {
                                RoundedRectangle(cornerRadius: FW.radiusSmall)
                                    .fill(FW.surface)
                                    .overlay {
                                        RoundedRectangle(cornerRadius: FW.radiusSmall)
                                            .strokeBorder(FW.border, lineWidth: 1)
                                    }
                            }
                        }
                        .menuStyle(.borderlessButton)
                    }
                }
            }
            .padding(FW.spacing32)

            // Separator
            Rectangle()
                .fill(FW.border)
                .frame(height: 1)

            // List
            if corrections.isEmpty {
                emptyState
            } else if filteredCorrections.isEmpty {
                noResultsState
            } else {
                ScrollView {
                    LazyVStack(spacing: FW.spacing12) {
                        ForEach(filteredCorrections) { correction in
                            correctionRow(correction)
                        }
                    }
                    .padding(FW.spacing32)
                }
            }
        }
        .background(FW.background)
        .confirmationDialog(
            "Clear All Corrections?",
            isPresented: $showingClearConfirmation,
            titleVisibility: .visible
        ) {
            Button("Clear All", role: .destructive) {
                clearAllCorrections()
            }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("This will delete all \(corrections.count) learned corrections. This cannot be undone.")
        }
        .onAppear {
            refreshCorrections()
        }
    }

    private var emptyState: some View {
        VStack(spacing: FW.spacing20) {
            Image(systemName: "brain.head.profile")
                .font(.system(size: 48))
                .foregroundStyle(FW.textMuted)

            VStack(spacing: FW.spacing12) {
                Text("No learnings yet")
                    .font(.headline)
                    .foregroundStyle(FW.textPrimary)

                VStack(spacing: FW.spacing4) {
                    Text("Flow watches for edits after you paste transcribed text.")
                    Text("If you fix a typo, it learns the correction automatically.")
                }
                .font(.body)
                .foregroundStyle(FW.textSecondary)
                .multilineTextAlignment(.center)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private var noResultsState: some View {
        VStack(spacing: FW.spacing20) {
            Image(systemName: "magnifyingglass")
                .font(.system(size: 48))
                .foregroundStyle(FW.textMuted)

            VStack(spacing: FW.spacing8) {
                Text("No matches found")
                    .font(.headline)
                    .foregroundStyle(FW.textPrimary)

                Text("Try a different search term")
                    .font(.body)
                    .foregroundStyle(FW.textSecondary)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private func correctionRow(_ correction: Correction) -> some View {
        HStack(spacing: FW.spacing16) {
            // Original -> Corrected
            HStack(spacing: FW.spacing8) {
                Text(correction.original)
                    .font(.headline)
                    .foregroundStyle(FW.danger.opacity(0.8))
                    .strikethrough(true, color: FW.danger.opacity(0.5))

                Image(systemName: "arrow.right")
                    .font(.caption)
                    .foregroundStyle(FW.textMuted)

                Text(correction.corrected)
                    .font(.headline)
                    .foregroundStyle(FW.success)
            }

            Spacer()

            // Stats
            HStack(spacing: FW.spacing12) {
                // Confidence badge
                confidenceBadge(correction.confidence)

                // Occurrences
                HStack(spacing: FW.spacing4) {
                    Image(systemName: "repeat")
                        .font(.caption2)
                    Text("\(correction.occurrences)")
                }
                .font(FW.fontMonoSmall)
                .foregroundStyle(FW.textMuted)
                .padding(.horizontal, FW.spacing8)
                .padding(.vertical, FW.spacing4)
                .background {
                    Capsule()
                        .fill(FW.background)
                }
            }

            // Delete button
            Button {
                deleteCorrection(correction)
            } label: {
                Image(systemName: "xmark.circle.fill")
                    .foregroundStyle(FW.textMuted)
            }
            .buttonStyle(.plain)
            .help("Delete this correction")
        }
        .padding(FW.spacing16)
        .fwSection()
    }

    private func confidenceBadge(_ confidence: Double) -> some View {
        let percentage = Int(confidence * 100)
        let color: Color = confidence >= 0.8 ? FW.success :
                           confidence >= 0.55 ? FW.warning : FW.textMuted

        return Text("\(percentage)%")
            .font(FW.fontMonoSmall)
            .foregroundStyle(color)
            .padding(.horizontal, FW.spacing8)
            .padding(.vertical, FW.spacing4)
            .background {
                Capsule()
                    .fill(color.opacity(0.15))
            }
    }

    private func refreshCorrections() {
        corrections = appState.engine.corrections
    }

    private func deleteCorrection(_ correction: Correction) {
        if appState.engine.deleteCorrection(id: correction.id) {
            withAnimation(.easeOut(duration: 0.2)) {
                corrections.removeAll { $0.id == correction.id }
            }
        }
    }

    private func clearAllCorrections() {
        let _ = appState.engine.deleteAllCorrections()
        withAnimation(.easeOut(duration: 0.2)) {
            corrections = []
        }
    }
}

// MARK: - Sort Order

private enum CorrectionSortOrder: CaseIterable {
    case confidence
    case occurrences
    case alphabetical
    case recent

    var label: String {
        switch self {
        case .confidence: return "Confidence"
        case .occurrences: return "Frequency"
        case .alphabetical: return "A-Z"
        case .recent: return "Recent"
        }
    }
}

#Preview {
    CorrectionsContentView()
        .environmentObject(AppState())
}
