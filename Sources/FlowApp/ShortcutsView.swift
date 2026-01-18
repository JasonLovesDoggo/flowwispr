//
// ShortcutsView.swift
// Flow
//
// Voice shortcuts management interface.
//

import SwiftUI

struct ShortcutsContentView: View {
    @EnvironmentObject var appState: AppState
    @State private var newTrigger = ""
    @State private var newReplacement = ""
    @State private var shortcuts: [ShortcutItem] = []
    @State private var showingAddSheet = false

    var body: some View {
        VStack(spacing: 0) {
            // header
            HStack {
                VStack(alignment: .leading, spacing: FW.spacing2) {
                    Text("Voice Shortcuts")
                        .font(.title3.weight(.semibold))

                    Text("Expand phrases while dictating")
                        .font(.caption)
                        .foregroundStyle(FW.textTertiary)
                }

                Spacer()

                Button {
                    showingAddSheet = true
                } label: {
                    HStack(spacing: FW.spacing4) {
                        Image(systemName: "plus")
                        Text("Add")
                    }
                }
                .buttonStyle(FWSecondaryButtonStyle())
            }
            .padding(FW.spacing24)

            Divider()

            // list
            if shortcuts.isEmpty {
                emptyState
            } else {
                ScrollView {
                    LazyVStack(spacing: FW.spacing8) {
                        ForEach(shortcuts) { shortcut in
                            shortcutRow(shortcut)
                        }
                    }
                    .padding(FW.spacing24)
                }
            }
        }
        .sheet(isPresented: $showingAddSheet) {
            AddShortcutSheet(
                trigger: $newTrigger,
                replacement: $newReplacement,
                onAdd: addShortcut,
                onCancel: { showingAddSheet = false }
            )
        }
        .onAppear {
            refreshShortcuts()
        }
    }

    private var emptyState: some View {
        VStack(spacing: FW.spacing16) {
            Image(systemName: "text.badge.plus")
                .font(.system(size: 48))
                .foregroundStyle(FW.textTertiary)

            VStack(spacing: FW.spacing4) {
                Text("No shortcuts yet")
                    .font(.headline)

                Text("Add shortcuts to quickly expand phrases")
                    .font(.subheadline)
                    .foregroundStyle(FW.textSecondary)
            }

            Button("Add Shortcut") {
                showingAddSheet = true
            }
            .buttonStyle(FWPrimaryButtonStyle())
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private func shortcutRow(_ shortcut: ShortcutItem) -> some View {
        HStack(spacing: FW.spacing12) {
            VStack(alignment: .leading, spacing: FW.spacing4) {
                Text(shortcut.trigger)
                    .font(.headline)
                    .foregroundStyle(FW.accent)

                Text(shortcut.replacement)
                    .font(.subheadline)
                    .foregroundStyle(FW.textSecondary)
                    .lineLimit(2)
            }

            Spacer()

            if shortcut.useCount > 0 {
                Text("\(shortcut.useCount)")
                    .font(FW.fontMonoSmall)
                    .foregroundStyle(FW.textTertiary)
                    .padding(.horizontal, FW.spacing8)
                    .padding(.vertical, FW.spacing4)
                    .background {
                        Capsule()
                            .fill(FW.surfaceElevated)
                    }
            }

            Button {
                deleteShortcut(shortcut)
            } label: {
                Image(systemName: "trash")
                    .foregroundStyle(FW.recording.opacity(0.8))
            }
            .buttonStyle(.plain)
        }
        .padding(FW.spacing12)
        .background {
            RoundedRectangle(cornerRadius: FW.radiusSmall)
                .fill(FW.surfaceElevated.opacity(0.5))
        }
    }

    private func refreshShortcuts() {
        if let raw = appState.engine.shortcuts {
            shortcuts = raw.compactMap { dict in
                guard let trigger = dict["trigger"] as? String,
                      let replacement = dict["replacement"] as? String else {
                    return nil
                }
                let useCount = dict["use_count"] as? Int ?? 0
                return ShortcutItem(trigger: trigger, replacement: replacement, useCount: useCount)
            }
        }
    }

    private func addShortcut() {
        guard !newTrigger.isEmpty, !newReplacement.isEmpty else { return }

        if appState.addShortcut(trigger: newTrigger, replacement: newReplacement) {
            refreshShortcuts()
            newTrigger = ""
            newReplacement = ""
            showingAddSheet = false
        }
    }

    private func deleteShortcut(_ shortcut: ShortcutItem) {
        if appState.removeShortcut(trigger: shortcut.trigger) {
            refreshShortcuts()
        }
    }
}

// MARK: - Supporting Types

struct ShortcutItem: Identifiable {
    let id = UUID()
    let trigger: String
    let replacement: String
    let useCount: Int
}

struct AddShortcutSheet: View {
    @Binding var trigger: String
    @Binding var replacement: String
    let onAdd: () -> Void
    let onCancel: () -> Void

    var body: some View {
        VStack(spacing: FW.spacing24) {
            // header
            VStack(spacing: FW.spacing4) {
                Text("Add Shortcut")
                    .font(.title3.weight(.semibold))

                Text("Say the trigger phrase to expand it")
                    .font(.caption)
                    .foregroundStyle(FW.textTertiary)
            }

            // form
            VStack(spacing: FW.spacing16) {
                VStack(alignment: .leading, spacing: FW.spacing4) {
                    Text("Trigger")
                        .font(.caption.weight(.medium))
                        .foregroundStyle(FW.textSecondary)

                    TextField("e.g., 'my email'", text: $trigger)
                        .textFieldStyle(.roundedBorder)
                }

                VStack(alignment: .leading, spacing: FW.spacing4) {
                    Text("Replacement")
                        .font(.caption.weight(.medium))
                        .foregroundStyle(FW.textSecondary)

                    TextField("e.g., 'hello@example.com'", text: $replacement)
                        .textFieldStyle(.roundedBorder)
                }
            }

            // buttons
            HStack {
                Button("Cancel") {
                    onCancel()
                }
                .keyboardShortcut(.cancelAction)
                .buttonStyle(FWGhostButtonStyle())

                Spacer()

                Button("Add") {
                    onAdd()
                }
                .keyboardShortcut(.defaultAction)
                .buttonStyle(FWPrimaryButtonStyle())
                .disabled(trigger.isEmpty || replacement.isEmpty)
            }
        }
        .padding(FW.spacing24)
        .frame(width: 360)
    }
}

#Preview {
    ShortcutsContentView()
        .environmentObject(AppState())
}
