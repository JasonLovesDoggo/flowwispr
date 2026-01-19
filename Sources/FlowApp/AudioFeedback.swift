//
// AudioFeedback.swift
// Flow
//
// Provides audio feedback sounds for recording start/stop events.
// Uses system sounds for immediate, non-jarring feedback.
//

import AppKit

/// Plays audio feedback for recording events
final class AudioFeedback {
    static let shared = AudioFeedback()

    private var startSound: NSSound?
    private var stopSound: NSSound?
    private var errorSound: NSSound?

    /// Whether audio feedback is enabled (can be user-configurable later)
    var isEnabled = true

    private init() {
        // Use system sounds - "Tink" for start (subtle), "Pop" for stop (slightly more noticeable)
        // These are reliable system sounds that don't require bundling audio files
        startSound = NSSound(named: "Tink")
        stopSound = NSSound(named: "Pop")
        errorSound = NSSound(named: "Basso")
    }

    /// Play the recording start sound
    func playStart() {
        guard isEnabled else { return }
        startSound?.play()
    }

    /// Play the recording stop sound
    func playStop() {
        guard isEnabled else { return }
        stopSound?.play()
    }

    /// Play error sound (e.g., paste failed, transcription failed)
    func playError() {
        guard isEnabled else { return }
        errorSound?.play()
    }
}
