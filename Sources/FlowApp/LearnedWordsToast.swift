//
//  LearnedWordsToast.swift
//  Flow
//
//  Toast notification for displaying newly learned words with undo functionality.
//

import SwiftUI

/// Toast view shown when words are automatically learned
struct LearnedWordsToast: View {
    let words: [String]
    let onUndo: () -> Void
    let onDismiss: () -> Void

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: "text.book.closed.fill")
                .font(.system(size: 20))
                .foregroundColor(.accentColor)

            VStack(alignment: .leading, spacing: 2) {
                Text("Learned \(words.count) word\(words.count == 1 ? "" : "s")")
                    .font(.headline)
                Text(words.joined(separator: ", "))
                    .font(.caption)
                    .foregroundColor(.secondary)
                    .lineLimit(1)
                    .truncationMode(.tail)
            }

            Spacer()

            Button("Undo") {
                onUndo()
            }
            .buttonStyle(.bordered)
            .controlSize(.small)

            Button {
                onDismiss()
            } label: {
                Image(systemName: "xmark")
                    .font(.system(size: 12, weight: .bold))
                    .foregroundColor(.secondary)
            }
            .buttonStyle(.plain)
            .padding(4)
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 12)
        .background(.ultraThinMaterial)
        .cornerRadius(12)
        .shadow(color: .black.opacity(0.15), radius: 8, x: 0, y: 4)
        .frame(maxWidth: 400)
    }
}

/// Window controller for displaying toast notifications
final class LearnedWordsToastController {
    private var window: NSWindow?
    private var dismissWorkItem: DispatchWorkItem?

    static let shared = LearnedWordsToastController()
    private init() {}

    /// Show toast with learned words
    /// - Parameters:
    ///   - words: The words that were learned
    ///   - onUndo: Callback when user taps undo
    func show(words: [String], onUndo: @escaping () -> Void) {
        // Cancel any existing toast
        dismiss()

        guard !words.isEmpty else { return }

        // Create the hosting view
        let toastView = LearnedWordsToast(
            words: words,
            onUndo: { [weak self] in
                onUndo()
                self?.dismiss()
            },
            onDismiss: { [weak self] in
                self?.dismiss()
            }
        )

        let hostingView = NSHostingView(rootView: toastView)
        hostingView.frame = CGRect(x: 0, y: 0, width: 380, height: 60)

        // Create window
        let window = NSWindow(
            contentRect: hostingView.frame,
            styleMask: [.borderless],
            backing: .buffered,
            defer: false
        )

        window.contentView = hostingView
        window.backgroundColor = .clear
        window.isOpaque = false
        window.level = .floating
        window.collectionBehavior = [.canJoinAllSpaces, .stationary]
        window.isMovableByWindowBackground = false
        window.hasShadow = false

        // Position at top-right of screen
        if let screen = NSScreen.main {
            let screenFrame = screen.visibleFrame
            let windowFrame = window.frame
            let x = screenFrame.maxX - windowFrame.width - 20
            let y = screenFrame.maxY - windowFrame.height - 20
            window.setFrameOrigin(CGPoint(x: x, y: y))
        }

        self.window = window

        // Show with animation
        window.alphaValue = 0
        window.orderFront(nil)
        NSAnimationContext.runAnimationGroup { context in
            context.duration = 0.2
            window.animator().alphaValue = 1
        }

        // Play sound if enabled
        if UserDefaults.standard.bool(forKey: "autoAddToDictSound") {
            NSSound(named: "Glass")?.play()
        }

        // Auto-dismiss after 5 seconds
        let workItem = DispatchWorkItem { [weak self] in
            self?.dismiss()
        }
        dismissWorkItem = workItem
        DispatchQueue.main.asyncAfter(deadline: .now() + 5, execute: workItem)
    }

    /// Dismiss the current toast
    func dismiss() {
        dismissWorkItem?.cancel()
        dismissWorkItem = nil

        guard let window = window else { return }

        NSAnimationContext.runAnimationGroup({ context in
            context.duration = 0.2
            window.animator().alphaValue = 0
        }, completionHandler: { [weak self] in
            window.orderOut(nil)
            self?.window = nil
        })
    }
}

// MARK: - Preview

#if DEBUG
struct LearnedWordsToast_Previews: PreviewProvider {
    static var previews: some View {
        VStack(spacing: 20) {
            LearnedWordsToast(
                words: ["Anthropic"],
                onUndo: {},
                onDismiss: {}
            )

            LearnedWordsToast(
                words: ["Anthropic", "Claude", "OpenAI"],
                onUndo: {},
                onDismiss: {}
            )

            LearnedWordsToast(
                words: ["Anthropic", "Claude", "OpenAI", "ChatGPT", "Gemini"],
                onUndo: {},
                onDismiss: {}
            )
        }
        .padding()
        .background(Color.gray.opacity(0.3))
    }
}
#endif
