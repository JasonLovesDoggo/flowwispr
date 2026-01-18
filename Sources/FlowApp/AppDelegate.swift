//
// AppDelegate.swift
// Flow
//
// Handles window lifecycle: ensures window opens on launch and handles reopen.
//

import AppKit

final class AppDelegate: NSObject, NSApplicationDelegate {
    func applicationDidFinishLaunching(_ notification: Notification) {
        DispatchQueue.main.async { @MainActor in
            Analytics.shared.configure(apiKey: "874bf4de55312a14f9b942ab3ab21423")
            Analytics.shared.track("App Launched")

            NSApp.setActivationPolicy(.regular)
            WindowManager.openMainWindow()
        }
    }

    func applicationDidBecomeActive(_ notification: Notification) {
        Task { @MainActor in
            Analytics.shared.track("App Became Active")
        }
    }

    func applicationDidResignActive(_ notification: Notification) {
        Task { @MainActor in
            Analytics.shared.track("App Resigned Active")
        }
    }

    func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows: Bool) -> Bool {
        Task { @MainActor in
            Analytics.shared.track("App Reopened")
        }
        WindowManager.openMainWindow()
        return true
    }
}
