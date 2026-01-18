//
// ContentView.swift
// FlowWispr
//
// Main navigation container with sidebar navigation for Record, Shortcuts, Settings.
//

import AppKit
import SwiftUI

struct ContentView: View {
    @EnvironmentObject var appState: AppState
    @State private var hoveredTab: AppTab?

    var body: some View {
        ZStack {
            HStack(spacing: 0) {
                // Sidebar
                VStack(alignment: .leading, spacing: 0) {
                    // Logo
                    HStack(spacing: 8) {
                        if let iconURL = Bundle.module.url(forResource: "app-icon-old", withExtension: "png"),
                           let nsImage = NSImage(contentsOf: iconURL) {
                            Image(nsImage: nsImage)
                                .resizable()
                                .frame(width: 24, height: 24)
                                .cornerRadius(5)
                        }

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
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.horizontal, 8)
        .padding(.vertical, 10)
        .background {
            if appState.selectedTab == tab {
                RoundedRectangle(cornerRadius: 8)
                    .fill(FW.accentGradient.opacity(0.1))
            } else if hoveredTab == tab {
                RoundedRectangle(cornerRadius: 8)
                    .fill(FW.surfaceElevated.opacity(0.6))
            }
        }
        .contentShape(Rectangle())
        .if(appState.selectedTab == tab) { view in
            view.foregroundStyle(FW.accentGradient)
        }
        .if(appState.selectedTab != tab) { view in
            view.foregroundStyle(FW.textSecondary)
        }
        .buttonStyle(.plain)
        .onHover { hovering in
            if hovering {
                NSCursor.pointingHand.push()
            } else {
                NSCursor.pop()
            }
            withAnimation(.easeOut(duration: 0.12)) {
                hoveredTab = hovering ? tab : nil
            }
        }
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
