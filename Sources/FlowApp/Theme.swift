//
// Theme.swift
// Flow
//
// Design system. Indigo-forward, clean, with subtle depth.
//

import AppKit
import SwiftUI

// MARK: - Window Size

enum WindowSize {
    static var screen: CGRect { NSScreen.main?.visibleFrame ?? CGRect(x: 0, y: 0, width: 1440, height: 900) }
    static var width: CGFloat { screen.width * 0.7 }
    static var height: CGFloat { screen.height * 0.7 }
    static let minWidth: CGFloat = 700
    static let minHeight: CGFloat = 500
}

// MARK: - Design System

enum FW {
    // MARK: - Colors

    /// Primary brand color - a rich indigo
    static let accent = Color(red: 0.38, green: 0.35, blue: 0.95)

    /// Secondary accent for gradients
    static let accentSecondary = Color(red: 0.55, green: 0.35, blue: 0.95)

    /// Recording state red
    static let recording = Color(red: 0.95, green: 0.25, blue: 0.3)

    /// Success green
    static let success = Color(red: 0.2, green: 0.78, blue: 0.55)

    /// Warning amber
    static let warning = Color(red: 0.95, green: 0.65, blue: 0.15)

    /// Surface colors
    static let surfacePrimary = Color(nsColor: .windowBackgroundColor)
    static let surfaceElevated = Color(nsColor: .controlBackgroundColor)

    /// Text colors
    static let textPrimary = Color(nsColor: .labelColor)
    static let textSecondary = Color(nsColor: .secondaryLabelColor)
    static let textTertiary = Color(nsColor: .tertiaryLabelColor)

    // MARK: - Gradients

    static let accentGradient = LinearGradient(
        colors: [accent, accentSecondary],
        startPoint: .topLeading,
        endPoint: .bottomTrailing
    )

    static let recordingGradient = LinearGradient(
        colors: [recording, recording.opacity(0.8)],
        startPoint: .top,
        endPoint: .bottom
    )

    static let subtleGradient = LinearGradient(
        colors: [Color.white.opacity(0.05), Color.clear],
        startPoint: .top,
        endPoint: .bottom
    )

    // MARK: - Spacing

    static let spacing2: CGFloat = 2
    static let spacing4: CGFloat = 4
    static let spacing6: CGFloat = 6
    static let spacing8: CGFloat = 8
    static let spacing12: CGFloat = 12
    static let spacing16: CGFloat = 16
    static let spacing24: CGFloat = 24
    static let spacing32: CGFloat = 32

    // MARK: - Radii

    static let radiusSmall: CGFloat = 6
    static let radiusMedium: CGFloat = 12
    static let radiusLarge: CGFloat = 16
    static let radiusXL: CGFloat = 24

    // MARK: - Typography

    static let fontMono = Font.system(.body, design: .monospaced)
    static let fontMonoSmall = Font.system(.caption, design: .monospaced)
    static let fontMonoLarge = Font.system(.title3, design: .monospaced).weight(.medium)
}

// MARK: - View Extensions

extension View {
    /// Elevated card style with subtle border and shadow
    func fwCard() -> some View {
        self
            .background {
                RoundedRectangle(cornerRadius: FW.radiusMedium)
                    .fill(.ultraThinMaterial)
                    .overlay {
                        RoundedRectangle(cornerRadius: FW.radiusMedium)
                            .strokeBorder(Color.white.opacity(0.1), lineWidth: 1)
                    }
                    .shadow(color: .black.opacity(0.1), radius: 8, y: 4)
            }
    }

    /// Subtle hover effect
    func fwHover() -> some View {
        self.modifier(HoverEffect())
    }
}

struct HoverEffect: ViewModifier {
    @State private var isHovered = false

    func body(content: Content) -> some View {
        content
            .scaleEffect(isHovered ? 1.02 : 1.0)
            .animation(.easeOut(duration: 0.15), value: isHovered)
            .onHover { hovering in
                isHovered = hovering
            }
    }
}

// MARK: - Button Styles

struct FWPrimaryButtonStyle: ButtonStyle {
    var isRecording: Bool = false

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.headline)
            .foregroundStyle(.white)
            .padding(.horizontal, FW.spacing24)
            .padding(.vertical, FW.spacing16)
            .background {
                RoundedRectangle(cornerRadius: FW.radiusMedium)
                    .fill(isRecording ? FW.recordingGradient : FW.accentGradient)
                    .overlay {
                        RoundedRectangle(cornerRadius: FW.radiusMedium)
                            .fill(configuration.isPressed ? Color.black.opacity(0.2) : Color.clear)
                    }
            }
            .scaleEffect(configuration.isPressed ? 0.97 : 1.0)
            .animation(.easeOut(duration: 0.1), value: configuration.isPressed)
    }
}

struct FWSecondaryButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.subheadline.weight(.medium))
            .foregroundStyle(FW.accent)
            .padding(.horizontal, FW.spacing16)
            .padding(.vertical, FW.spacing8)
            .background {
                RoundedRectangle(cornerRadius: FW.radiusSmall)
                    .fill(FW.accent.opacity(configuration.isPressed ? 0.15 : 0.1))
            }
            .animation(.easeOut(duration: 0.1), value: configuration.isPressed)
    }
}

struct FWGhostButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .foregroundStyle(configuration.isPressed ? FW.textSecondary : FW.textPrimary)
            .animation(.easeOut(duration: 0.1), value: configuration.isPressed)
    }
}
