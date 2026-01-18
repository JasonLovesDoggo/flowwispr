//
// Analytics.swift
// Flow
//
// Analytics service wrapper for Amplitude SDK.
//

import Amplitude
import Foundation

@MainActor
final class Analytics {
    static let shared = Analytics()

    private var isConfigured = false

    private init() {}

    /// Initialize the analytics SDK with your API key.
    /// - Parameter apiKey: Your Amplitude project API key
    func configure(apiKey: String) {
        guard !apiKey.isEmpty else {
            print("Analytics: API key is empty, analytics disabled")
            return
        }

        Amplitude.instance().initializeApiKey(apiKey)
        isConfigured = true
    }

    /// Track an event.
    /// - Parameters:
    ///   - eventType: Event name
    ///   - eventProperties: Optional event properties
    func track(_ eventType: String, eventProperties: [String: Any]? = nil) {
        guard isConfigured else {
            print("Analytics: SDK not configured, skipping event: \(eventType)")
            return
        }

        Amplitude.instance().logEvent(eventType, withEventProperties: eventProperties)
    }

    /// Set user ID.
    /// - Parameter userId: User identifier
    func setUserId(_ userId: String) {
        Amplitude.instance().setUserId(userId)
    }

    /// Set user properties.
    /// - Parameter properties: User properties to set
    func setUserProperties(_ properties: [String: Any]) {
        guard isConfigured else { return }

        let identify = AMPIdentify()
        for (key, value) in properties {
            if let nsValue = value as? NSObject {
                identify.set(key, value: nsValue)
            }
        }
        Amplitude.instance().identify(identify)
    }

    /// Reset user identity (clears user ID and generates new device ID).
    func reset() {
        Amplitude.instance().regenerateDeviceId()
    }
}
