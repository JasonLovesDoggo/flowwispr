//
// WaveformView.swift
// Flow
//
// Animated waveform visualization. The hero visual element.
//

import SwiftUI

struct WaveformView: View {
    let isRecording: Bool
    let barCount: Int
    let audioLevel: Float?

    @State private var sampleBuffer: [Float] = []
    @State private var isDecaying = false

    init(isRecording: Bool, barCount: Int = 32, audioLevel: Float? = nil) {
        self.isRecording = isRecording
        self.barCount = barCount
        self.audioLevel = audioLevel
    }

    var body: some View {
        TimelineView(.animation(minimumInterval: 1/107)) { _ in
            Canvas { context, size in
                let barWidth: CGFloat = 1.5
                let gap: CGFloat = 2.5
                let totalWidth = CGFloat(barCount) * (barWidth + gap) - gap
                let startX = (size.width - totalWidth) / 2
                let maxHeight = size.height * 0.85
                let minHeight = size.height * 0.15

                // Update sample buffer with new audio level or decay
                if isRecording, let level = audioLevel {
                    DispatchQueue.main.async {
                        sampleBuffer.append(level)
                        // Keep buffer size at barCount
                        if sampleBuffer.count > barCount {
                            sampleBuffer.removeFirst()
                        }
                    }
                } else if isDecaying {
                    // Decay samples toward zero with position-based rates (left decays faster than right)
                    DispatchQueue.main.async {
                        let allZero = sampleBuffer.allSatisfy { $0 < 0.01 }
                        if allZero {
                            isDecaying = false
                            sampleBuffer = []
                        } else {
                            sampleBuffer = sampleBuffer.enumerated().map { index, value in
                                // Newer samples (right side) decay slower
                                let position = Float(index) / Float(sampleBuffer.count)
                                let decayRate = 0.92 + position * 0.05 // 0.92 (left) to 0.97 (right)
                                return value * decayRate
                            }
                        }
                    }
                }

                // Fill buffer for display
                let displaySamples: [Float]
                if sampleBuffer.count < barCount {
                    // If buffer isn't full yet, repeat the latest sample to fill space (immediate visual feedback)
                    let fillValue = sampleBuffer.last ?? 0.0
                    displaySamples = Array(repeating: fillValue, count: barCount - sampleBuffer.count) + sampleBuffer
                } else {
                    displaySamples = Array(sampleBuffer.suffix(barCount))
                }

                // Find max in current window for normalization
                let windowMax = displaySamples.max() ?? 0.01
                let normalizationFactor = max(0.3, windowMax) // Use at least 0.3 as baseline
                let bufferFilling = sampleBuffer.count < barCount

                for i in 0..<barCount {
                    let x = startX + CGFloat(i) * (barWidth + gap)

                    // Get sample for this bar and apply log scale normalization
                    var sample = displaySamples[i]

                    // Add positional variation when buffer is filling for immediate visual feedback
                    if bufferFilling && sample > 0.01 {
                        let barPosition = Double(i) / Double(barCount - 1)
                        let positionVariation = sin(barPosition * .pi) // Arc shape
                        sample = sample * Float(0.5 + positionVariation * 0.5)
                    }

                    let normalized = sample / normalizationFactor
                    // Apply gentle log scale to compress dynamic range
                    let amplitude = normalized > 0.01 ? log10(1 + normalized * 9) : 0.0

                    // Always show bars at minHeight, scale up with amplitude
                    let height = minHeight + (maxHeight - minHeight) * CGFloat(amplitude)
                    let y = (size.height - height) / 2

                    let rect = CGRect(x: x, y: y, width: barWidth, height: height)
                    let path = RoundedRectangle(cornerRadius: barWidth / 2)
                        .path(in: rect)

                    // color gradient based on position
                    let progress = CGFloat(i) / CGFloat(barCount - 1)
                    let color = isRecording
                        ? interpolateColor(from: FW.recording, to: FW.recording.opacity(0.6), progress: progress)
                        : interpolateColor(from: FW.accent, to: FW.accentSecondary, progress: progress)

                    context.fill(path, with: .color(color))
                }
            }
        }
        .onChange(of: isRecording) { oldValue, newValue in
            if oldValue && !newValue {
                // Start decay animation when recording stops
                isDecaying = true
            } else if newValue {
                // Clear decay state when recording starts
                isDecaying = false
            }
        }
    }

    private func interpolateColor(from: Color, to: Color, progress: CGFloat) -> Color {
        // simplified linear interpolation
        let nsFrom = NSColor(from)
        let nsTo = NSColor(to)

        var r1: CGFloat = 0, g1: CGFloat = 0, b1: CGFloat = 0, a1: CGFloat = 0
        var r2: CGFloat = 0, g2: CGFloat = 0, b2: CGFloat = 0, a2: CGFloat = 0

        nsFrom.getRed(&r1, green: &g1, blue: &b1, alpha: &a1)
        nsTo.getRed(&r2, green: &g2, blue: &b2, alpha: &a2)

        return Color(
            red: r1 + (r2 - r1) * progress,
            green: g1 + (g2 - g1) * progress,
            blue: b1 + (b2 - b1) * progress,
            opacity: a1 + (a2 - a1) * progress
        )
    }
}

// MARK: - Compact waveform for menu bar

struct CompactWaveformView: View {
    let isRecording: Bool
    let audioLevel: Float?

    init(isRecording: Bool, audioLevel: Float? = nil) {
        self.isRecording = isRecording
        self.audioLevel = audioLevel
    }

    var body: some View {
        WaveformView(isRecording: isRecording, barCount: 9, audioLevel: audioLevel)
            .frame(width: 50, height: 14)
    }
}

// MARK: - Preview

#Preview("Idle") {
    WaveformView(isRecording: false)
        .frame(width: 300, height: 80)
        .padding()
        .background(Color.black.opacity(0.9))
}

#Preview("Recording") {
    WaveformView(isRecording: true)
        .frame(width: 300, height: 80)
        .padding()
        .background(Color.black.opacity(0.9))
}
