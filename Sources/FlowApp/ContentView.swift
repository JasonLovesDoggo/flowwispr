//
// ContentView.swift
// Flow
//
// Main navigation container with sidebar navigation for Record, Shortcuts, Settings.
//

import AppKit
import SwiftUI

struct ContentView: View {
    @EnvironmentObject var appState: AppState
    @Environment(\.colorScheme) private var colorScheme
    @State private var hoveredTab: AppTab?

    var body: some View {
        ZStack {
            HStack(spacing: 0) {
                // Sidebar
                VStack(alignment: .leading, spacing: 0) {
                    // Logo
                    HStack(spacing: FW.spacing8) {
                        if let iconURL = Bundle.module.url(forResource: "app-icon-old", withExtension: "png"),
                           let nsImage = NSImage(contentsOf: iconURL) {
                            Image(nsImage: nsImage)
                                .resizable()
                                .frame(width: 24, height: 24)
                                .cornerRadius(5)
                                .if(colorScheme == .dark) { $0.colorInvert() }
                        }

                        Text("Flow")
                            .font(.title3.weight(.semibold))
                            .foregroundStyle(FW.textPrimary)
                    }
                    .frame(height: 50)
                    .padding(.horizontal, FW.spacing16)

                    // Navigation items
                    VStack(alignment: .leading, spacing: FW.spacing4) {
                        ForEach(AppTab.allCases, id: \.self) { tab in
                            navigationItem(tab)
                        }
                    }
                    .padding(.top, FW.spacing12)
                    .padding(.horizontal, FW.spacing12)

                    Spacer()

                    // Status indicator at bottom
                    statusIndicator
                        .padding(.horizontal, FW.spacing12)
                        .padding(.bottom, FW.spacing16)
                }
                .frame(width: 200)
                .background(FW.surface)

                // Subtle separator
                Rectangle()
                    .fill(FW.border)
                    .frame(width: 1)

                // Content area
                VStack(spacing: 0) {
                    Group {
                        switch appState.selectedTab {
                        case .record:
                            RecordView()
                        case .shortcuts:
                            ShortcutsContentView()
                        case .corrections:
                            CorrectionsContentView()
                        case .settings:
                            SettingsContentView()
                        }
                    }
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                }
                .frame(minWidth: WindowSize.minWidth - 200, minHeight: WindowSize.minHeight)
                .background(FW.background)
            }
            .frame(minWidth: WindowSize.minWidth, minHeight: WindowSize.minHeight)

            if !appState.isOnboardingComplete {
                Color.black.opacity(0.5)
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
            HStack(spacing: FW.spacing12) {
                Image(systemName: tab.icon)
                    .font(.body)
                    .frame(width: 20)

                Text(tab.rawValue)
                    .font(.body)

                Spacer()
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, FW.spacing12)
            .padding(.vertical, 10)
            .background {
                if appState.selectedTab == tab {
                    RoundedRectangle(cornerRadius: FW.radiusSmall)
                        .fill(FW.accent.opacity(0.15))
                } else if hoveredTab == tab {
                    RoundedRectangle(cornerRadius: FW.radiusSmall)
                        .fill(FW.border.opacity(0.5))
                }
            }
            .contentShape(Rectangle())
        }
        .foregroundStyle(appState.selectedTab == tab ? FW.accent : FW.textSecondary)
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
        HStack(spacing: FW.spacing8) {
            Circle()
                .fill(appState.isConfigured ? FW.success : FW.warning)
                .frame(width: 8, height: 8)

            Text(appState.isConfigured ? "Ready" : "Setup required")
                .font(.caption)
                .foregroundStyle(FW.textSecondary)

            Spacer()
        }
        .padding(.horizontal, FW.spacing12)
        .padding(.vertical, FW.spacing8)
        .background {
            RoundedRectangle(cornerRadius: FW.radiusSmall)
                .fill(FW.background)
                .overlay {
                    RoundedRectangle(cornerRadius: FW.radiusSmall)
                        .strokeBorder(FW.border, lineWidth: 1)
                }
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
