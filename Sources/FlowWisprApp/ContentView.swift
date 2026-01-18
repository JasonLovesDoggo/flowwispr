//
// ContentView.swift
// FlowWispr
//
// Main navigation container with sidebar navigation for Record, Shortcuts, Settings.
//

import SwiftUI

struct ContentView: View {
    @EnvironmentObject var appState: AppState

    var body: some View {
        ZStack {
            HStack(spacing: 0) {
                // Sidebar
                VStack(alignment: .leading, spacing: 0) {
                    // Logo
                    HStack(spacing: 8) {
                        Image(systemName: "waveform")
                            .font(.title2)
                            .foregroundStyle(FW.accentGradient)
                        
                        Text("Flow")
                            .font(.title2.weight(.semibold))
                    }
                    .frame(height: 50)
                    .padding(.horizontal, 12)

                    Divider()

                    // Navigation items
                    VStack(alignment: .leading, spacing: 4) {
                        ForEach(AppTab.allCases, id: \.self) { tab in
                            navigationItem(tab)
                        }
                    }
                    .padding(.vertical, 8)
                    .padding(.horizontal, 8)

                    Spacer()

                    // Status indicator at bottom
                    statusIndicator
                        .padding(.horizontal, 12)
                        .padding(.vertical, 12)

                    Divider()
                }
                .frame(width: 200)
                .background(FW.surfacePrimary)

                // Content area
                VStack(spacing: 0) {
                    Group {
                        switch appState.selectedTab {
                        case .record:
                            RecordView()
                        case .shortcuts:
                            ShortcutsContentView()
                        case .settings:
                            SettingsContentView()
                        }
                    }
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                }
                .frame(minWidth: WindowSize.minWidth - 200, minHeight: WindowSize.minHeight)
                .background(FW.surfacePrimary)
            }
            .frame(minWidth: WindowSize.minWidth, minHeight: WindowSize.minHeight)

            if !appState.isOnboardingComplete {
                Color.black.opacity(0.3)
                    .ignoresSafeArea()

                OnboardingView()
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .center)
                    .padding(FW.spacing24)
            }
        }
        .onAppear {
            if !appState.isOnboardingComplete {
                WindowManager.openMainWindow()
            }
        }
    }

    // MARK: - Navigation Item

    private func navigationItem(_ tab: AppTab) -> some View {
        Button(action: {
            appState.selectedTab = tab
            Analytics.shared.track("Tab Changed", eventProperties: [
                "tab": tab.rawValue
            ])
        }) {
            HStack(spacing: 12) {
                Image(systemName: tab.icon)
                    .font(.body)
                    .frame(width: 20)
                
                Text(tab.rawValue)
                    .font(.body)

                Spacer()
            }
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 10)
        .background(
            appState.selectedTab == tab ?
            RoundedRectangle(cornerRadius: 8)
                .fill(FW.accentGradient.opacity(0.1))
            : nil
        )
        .if(appState.selectedTab == tab) { view in
            view.foregroundStyle(FW.accentGradient)
        }
        .if(appState.selectedTab != tab) { view in
            view.foregroundStyle(FW.textSecondary)
        }
        .buttonStyle(.plain)
    }

    private var statusIndicator: some View {
        HStack(spacing: 8) {
            Circle()
                .fill(appState.isConfigured ? FW.success : FW.warning)
                .frame(width: 8, height: 8)

            Text(appState.isConfigured ? "Ready" : "Setup required")
                .font(.caption)
                .foregroundStyle(FW.textSecondary)

            Spacer()
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 8)
        .background {
            RoundedRectangle(cornerRadius: 6)
                .fill((appState.isConfigured ? FW.success : FW.warning).opacity(0.1))
        }
    }
}

#Preview {
    ContentView()
        .environmentObject(AppState())
}

// MARK: - View Extensions

extension View {
    @ViewBuilder
    func `if`<Content: View>(_ condition: Bool, transform: (Self) -> Content) -> some View {
        if condition {
            transform(self)
        } else {
            self
        }
    }
}
