//
// RecordingIndicatorWindow.swift
// Flow
//
// Lightweight, non-activating recording indicator shown while recording or processing.
//

import AppKit
import SwiftUI

@MainActor
final class RecordingIndicatorWindow {
    private let window: NSPanel

    init(appState: AppState) {
        let view = RecordingIndicatorView()
            .environmentObject(appState)
        let hosting = NSHostingController(rootView: view)

        let panel = NSPanel(contentViewController: hosting)
        panel.styleMask = [.borderless, .nonactivatingPanel]
        panel.isFloatingPanel = true
        panel.level = .statusBar
        panel.backgroundColor = .clear
        panel.isOpaque = false
        panel.hasShadow = false
        panel.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary]
        panel.hidesOnDeactivate = false
        panel.ignoresMouseEvents = true
        panel.setFrame(NSRect(x: 0, y: 0, width: 400, height: 32), display: false)

        self.window = panel
        positionWindow()
    }

    func show() {
        window.alphaValue = 0
        positionWindow()
        window.orderFrontRegardless()

        // Small delay to ensure layout is settled before animating
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.01) { [weak self] in
            guard let self else { return }
            NSAnimationContext.runAnimationGroup { context in
                context.duration = 0.35
                context.timingFunction = CAMediaTimingFunction(name: .easeOut)
                self.window.animator().alphaValue = 1
            }
        }
    }

    func hide() {
        NSAnimationContext.runAnimationGroup({ context in
            context.duration = 0.4
            context.timingFunction = CAMediaTimingFunction(name: .easeIn)
            window.animator().alphaValue = 0

            // Slide down slightly
            var frame = window.frame
            frame.origin.y -= 15
            window.animator().setFrame(frame, display: true)
        }, completionHandler: {
            self.window.orderOut(nil)
            self.window.alphaValue = 1
            Task { @MainActor in
                self.positionWindow() // Reset position for next show
            }
        })
    }

    private func positionWindow() {
        guard let screen = NSScreen.main else { return }
        let screenFrame = screen.visibleFrame
        let size = window.frame.size
        let padding: CGFloat = 12
        let origin = CGPoint(
            x: screenFrame.midX - size.width / 2,
            y: screenFrame.minY + padding
        )
        window.setFrameOrigin(origin)
    }
}

private struct RecordingIndicatorView: View {
    @EnvironmentObject var appState: AppState
    @State private var pulse = false

    var showPill: Bool {
        appState.isRecording || appState.isProcessing || appState.isInitializingModel
    }

    var body: some View {
        HStack(spacing: FW.spacing6) {
            // Left side: Circle or Spinner (fixed 14x14 to prevent shifts)
            ZStack {
                // Show pulsing dot when recording or idle (always present to avoid disappearing)
                Circle()
                    .fill(appState.isRecording ? FW.recording : FW.accent)
                    .frame(width: 8, height: 8)
                    .opacity((appState.isProcessing && !appState.isRecording) ? 0 : (pulse ? 0.6 : 1.0))
                    .animation(.linear(duration: 0.15), value: appState.isRecording) // Smooth color transition

                // Show spinner when processing (overlaid)
                if appState.isProcessing && !appState.isRecording {
                    ProgressView()
                        .progressViewStyle(.circular)
                        .controlSize(.small)
                        .tint(.white.opacity(0.9))
                }
            }
            .frame(width: 14, height: 14)

            // Right side: Waveform or text (fixed width to prevent shifts)
            if appState.isInitializingModel {
                Text("Initializing Whisper model...")
                    .font(.caption)
                    .foregroundStyle(.white.opacity(0.9))
                    .lineLimit(1)
                    .frame(height: 14)
            } else {
                // Always show waveform, let it decay naturally
                CompactWaveformView(isRecording: appState.isRecording, audioLevel: appState.smoothedAudioLevel)
            }
        }
        .padding(.horizontal, 10)
        .padding(.vertical, FW.spacing6)
        .background(
            Capsule()
                .fill(Color.black.opacity(0.55))
        )
        .compositingGroup()
        .onAppear {
            withAnimation(.easeInOut(duration: 0.8).repeatForever(autoreverses: true)) {
                pulse = true
            }
        }
    }
}
