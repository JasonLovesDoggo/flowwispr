//
// WindowManager.swift
// Flow
//
// Centralized helpers for finding and showing the main app window.
//

import AppKit

@MainActor
enum WindowManager {
    static func openMainWindow() {
        if NSApp.activationPolicy() != .regular {
            NSApp.setActivationPolicy(.regular)
        }

        guard let window = primaryWindow() else {
            NSApp.activate(ignoringOtherApps: true)
            return
        }

        if window.isMiniaturized {
            window.deminiaturize(nil)
        }

        window.makeKeyAndOrderFront(nil)
        window.orderFrontRegardless()
        NSApp.activate(ignoringOtherApps: true)
    }

    private static func primaryWindow() -> NSWindow? {
        let normalWindows = NSApp.windows.filter { $0.level == .normal }
        if let window = normalWindows.first(where: { $0.canBecomeKey }) {
            return window
        }

        if let window = normalWindows.first {
            return window
        }

        return NSApp.windows.first(where: { $0.canBecomeKey })
    }
}
