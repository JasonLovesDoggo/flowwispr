//
// AudioFeedback.swift
// Flow
//
// Provides audio feedback sounds for recording start/stop events.
// Uses system sounds for immediate, non-jarring feedback.
// Disabled by default - can be enabled in Settings.
//

import AppKit
import SwiftUI

/// Plays audio feedback for recording events
final class AudioFeedback {
    static let shared = AudioFeedback()

    private var startSound: NSSound?
    private var stopSound: NSSound?
    private var errorSound: NSSound?

    /// Key for storing the audio feedback setting
    private static let enabledKey = "audioFeedbackEnabled"

    /// Whether audio feedback is enabled (defaults to OFF - user found clicking sounds annoying)
    static var isEnabled: Bool {
        get { UserDefaults.standard.bool(forKey: enabledKey) }
        set { UserDefaults.standard.set(newValue, forKey: enabledKey) }
    }

    private init() {
        // Use softer system sounds - Blow/Glass are gentler than Tink/Pop clicking sounds
        startSound = NSSound(named: "Blow")
        stopSound = NSSound(named: "Glass")
        errorSound = NSSound(named: "Basso")
    }

    /// Play the recording start sound
    func playStart() {
        guard Self.isEnabled else { return }
        startSound?.play()
    }

    /// Play the recording stop sound
    func playStop() {
        guard Self.isEnabled else { return }
        stopSound?.play()
    }

    /// Play error sound (e.g., paste failed, transcription failed)
    func playError() {
        guard Self.isEnabled else { return }
        errorSound?.play()
    }
}
