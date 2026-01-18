//
// FlowApp.swift
// Flow
//
// Main app entry point with single-window architecture.
//

import SwiftUI

@main
struct FlowApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) var appDelegate
    @StateObject private var appState = AppState()
    
    private var menuBarIcon: NSImage? {
        guard let iconURL = Bundle.main.url(forResource: "menubar", withExtension: "svg"),
              let icon = NSImage(contentsOf: iconURL) else {
            return nil
        }
        icon.isTemplate = true
        // Scale to 75%
        let scaledSize = NSSize(width: icon.size.width * 0.75, height: icon.size.height * 0.75)
        icon.size = scaledSize
        return icon
    }

    var body: some Scene {
        // main window
        WindowGroup {
            ContentView()
                .environmentObject(appState)
        }
        .windowResizability(.contentMinSize)
        .defaultSize(width: WindowSize.width, height: WindowSize.height)
        .commands {
            CommandGroup(replacing: .newItem) {}
        }

        // menu bar
        MenuBarExtra {
            MenuBarView()
                .environmentObject(appState)
        } label: {
            if let icon = menuBarIcon {
                Image(nsImage: icon)
                    .foregroundStyle(appState.isRecording ? .red : .primary)
            }
        }
        .menuBarExtraStyle(.menu)
    }
}
