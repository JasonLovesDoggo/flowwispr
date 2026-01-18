//
// MenuBarView.swift
// Flow
//
// Menu bar dropdown content using standard .menu style.
//

import Flow
import SwiftUI

struct MenuBarView: View {
    @EnvironmentObject var appState: AppState

    var body: some View {
        VStack {
            Button(appState.isRecording ? "Stop Recording (\(appState.hotkey.displayName))" : "Start Recording (\(appState.hotkey.displayName))") {
                appState.toggleRecording()
            }
            .disabled(!appState.isConfigured)

            Divider()

            Text("App: \(appState.targetAppName)")
                .font(.caption)

            Text("Mode: \(appState.currentMode.displayName)")
                .font(.caption)

            Menu("Change Mode") {
                ForEach(WritingMode.allCases, id: \.self) { mode in
                    Button {
                        appState.setMode(mode)
                    } label: {
                        HStack {
                            Text(mode.displayName)
                            if mode == appState.currentMode {
                                Image(systemName: "checkmark")
                            }
                        }
                    }
                }
            }

            Divider()

            Button("Open Flow") {
                WindowManager.openMainWindow()
            }

            Divider()

            Button("Quit") {
                NSApp.terminate(nil)
            }
            .keyboardShortcut("q")
        }
    }
}

#Preview {
    MenuBarView()
        .environmentObject(AppState())
}
