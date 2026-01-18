//
// AppTab.swift
// Flow
//
// Shared tab identifiers for app navigation.
//

import Foundation

enum AppTab: String, CaseIterable {
    case record = "Record"
    case shortcuts = "Shortcuts"
    case corrections = "Learnings"
    case settings = "Settings"

    var icon: String {
        switch self {
        case .record:
            return "waveform.circle"
        case .shortcuts:
            return "bolt.fill"
        case .corrections:
            return "brain.head.profile"
        case .settings:
            return "gear"
        }
    }
}
