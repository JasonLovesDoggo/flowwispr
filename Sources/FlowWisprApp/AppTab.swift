//
// AppTab.swift
// FlowWispr
//
// Shared tab identifiers for app navigation.
//

import Foundation

enum AppTab: String, CaseIterable {
    case record = "Record"
    case shortcuts = "Shortcuts"
    case settings = "Settings"

    var icon: String {
        switch self {
        case .record:
            return "waveform.circle"
        case .shortcuts:
            return "bolt.fill"
        case .settings:
            return "gear"
        }
    }
}
