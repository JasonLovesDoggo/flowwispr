//
// GlobeKeyHandler.swift
// Flow
//
// Captures the recording hotkey (Fn key or custom) using a CGEvent tap.
// Fn key and modifier-only use press-and-hold for recording.
// Custom hotkeys (key + modifiers) use toggle mode.
// All hotkeys are captured via CGEventTap (no Carbon dependency).
// Requires "Accessibility" permission in System Settings > Privacy & Security.
//

import ApplicationServices
import Foundation

final class GlobeKeyHandler {
    enum Trigger {
        case pressed
        case released
        case toggle
    }

    private var eventTap: CFMachPort?
    private var runLoopSource: CFRunLoopSource?
    private var onHotkeyTriggered: (@Sendable (Trigger) -> Void)?
    private var hotkey: Hotkey

    private var isFunctionDown = false
    private var functionUsedAsModifier = false
    private var hasFiredFnPressed = false

    private var isModifierDown = false
    private var modifierUsedAsModifier = false
    private var hasFiredModifierPressed = false

    // Resilience: track tap restarts to avoid infinite loops
    private var tapRestartCount = 0
    private let maxTapRestarts = 5
    private var lastTapRestartTime: Date?

    init(hotkey: Hotkey, onHotkeyTriggered: @escaping @Sendable (Trigger) -> Void) {
        self.hotkey = hotkey
        self.onHotkeyTriggered = onHotkeyTriggered
        startListening(prompt: false)
    }

    deinit {
        if let eventTap {
            CGEvent.tapEnable(tap: eventTap, enable: false)
        }
        if let runLoopSource {
            CFRunLoopRemoveSource(CFRunLoopGetMain(), runLoopSource, .commonModes)
        }
    }

    func updateHotkey(_ hotkey: Hotkey) {
        self.hotkey = hotkey

        // Reset state for Fn/modifier-only modes
        isFunctionDown = false
        functionUsedAsModifier = false
        hasFiredFnPressed = false
        isModifierDown = false
        modifierUsedAsModifier = false
        hasFiredModifierPressed = false
    }

    @discardableResult
    func startListening(prompt: Bool) -> Bool {
        guard accessibilityTrusted(prompt: prompt) else { return false }
        guard eventTap == nil else { return true }

        // Event tap for all hotkey types: Fn key, modifier-only, and custom key combos
        // Listen to flagsChanged (modifiers) and keyDown (for custom key+modifier combos)
        let eventMask = (1 << CGEventType.flagsChanged.rawValue) | (1 << CGEventType.keyDown.rawValue)
        guard let eventTap = CGEvent.tapCreate(
            tap: .cgSessionEventTap,
            place: .headInsertEventTap,
            options: .listenOnly,
            eventsOfInterest: CGEventMask(eventMask),
            callback: globeKeyEventTapCallback,
            userInfo: Unmanaged.passUnretained(self).toOpaque()
        ) else {
            return false
        }

        self.eventTap = eventTap
        let runLoopSource = CFMachPortCreateRunLoopSource(kCFAllocatorDefault, eventTap, 0)
        self.runLoopSource = runLoopSource
        CFRunLoopAddSource(CFRunLoopGetMain(), runLoopSource, .commonModes)
        CGEvent.tapEnable(tap: eventTap, enable: true)
        tapRestartCount = 0
        return true
    }

    static func isAccessibilityAuthorized() -> Bool {
        accessibilityTrusted(prompt: false)
    }

    private static func accessibilityTrusted(prompt: Bool) -> Bool {
        let promptKey = "AXTrustedCheckOptionPrompt" as CFString
        let options = [promptKey: prompt] as CFDictionary
        return AXIsProcessTrustedWithOptions(options)
    }

    private func accessibilityTrusted(prompt: Bool) -> Bool {
        Self.accessibilityTrusted(prompt: prompt)
    }

    fileprivate func handleEvent(type: CGEventType, event: CGEvent) {
        // Handle tap being disabled by system (timeout or user input flood)
        if type == .tapDisabledByTimeout || type == .tapDisabledByUserInput {
            restartTapIfNeeded()
            return
        }

        switch hotkey.kind {
        case .globe:
            switch type {
            case .flagsChanged:
                handleFunctionFlagChange(event)
            case .keyDown:
                if isFunctionDown {
                    let keycode = event.getIntegerValueField(.keyboardEventKeycode)
                    // kVK_Function = 63
                    if keycode != 63 {
                        functionUsedAsModifier = true
                    }
                }
            default:
                break
            }
        case .modifierOnly(let modifier):
            switch type {
            case .flagsChanged:
                handleModifierFlagChange(event, modifier: modifier)
            case .keyDown:
                if isModifierDown {
                    modifierUsedAsModifier = true
                }
            default:
                break
            }
        case .custom(let keyCode, let modifiers, _):
            // Handle custom key+modifier combos via CGEventTap (no Carbon needed)
            if type == .keyDown {
                handleCustomKeyDown(event, expectedKeyCode: keyCode, expectedModifiers: modifiers)
            }
        }
    }

    private func handleCustomKeyDown(_ event: CGEvent, expectedKeyCode: Int, expectedModifiers: Hotkey.Modifiers) {
        let pressedKeyCode = Int(event.getIntegerValueField(.keyboardEventKeycode))
        let pressedModifiers = Hotkey.Modifiers.from(cgFlags: event.flags)

        if pressedKeyCode == expectedKeyCode && pressedModifiers == expectedModifiers {
            fireHotkey(.toggle)
        }
    }

    private func restartTapIfNeeded() {
        guard let eventTap else { return }

        // Rate limit restarts to avoid infinite loops
        let now = Date()
        if let lastRestart = lastTapRestartTime, now.timeIntervalSince(lastRestart) < 1.0 {
            tapRestartCount += 1
            if tapRestartCount >= maxTapRestarts {
                // Too many restarts, give up (user may need to check accessibility permissions)
                return
            }
        } else {
            tapRestartCount = 0
        }
        lastTapRestartTime = now

        CGEvent.tapEnable(tap: eventTap, enable: true)
    }

    private func handleFunctionFlagChange(_ event: CGEvent) {
        let hasFn = event.flags.contains(.maskSecondaryFn)
        guard hasFn != isFunctionDown else { return }

        if hasFn {
            isFunctionDown = true
            functionUsedAsModifier = false
            hasFiredFnPressed = true
            // Fire immediately - no delay for instant response
            fireHotkey(.pressed)
            return
        }

        guard isFunctionDown else { return }
        isFunctionDown = false

        if hasFiredFnPressed && !functionUsedAsModifier {
            fireHotkey(.released)
        }
        hasFiredFnPressed = false
    }

    private func handleModifierFlagChange(_ event: CGEvent, modifier: Hotkey.ModifierKey) {
        let hasModifier = event.flags.contains(modifier.cgFlag)

        // Check if other modifiers are also pressed (means it's being used as a combo)
        let otherModifiersPressed = hasOtherModifiers(event.flags, excluding: modifier)

        guard hasModifier != isModifierDown else {
            // If the modifier is still down but other modifiers changed, mark as used
            if isModifierDown && otherModifiersPressed {
                modifierUsedAsModifier = true
            }
            return
        }

        if hasModifier {
            // Modifier just pressed
            if otherModifiersPressed {
                // Already in a combo, don't trigger
                return
            }
            isModifierDown = true
            modifierUsedAsModifier = false
            hasFiredModifierPressed = true
            // Fire immediately - no delay for instant response
            fireHotkey(.pressed)
            return
        }

        // Modifier released
        guard isModifierDown else { return }
        isModifierDown = false

        if hasFiredModifierPressed && !modifierUsedAsModifier {
            fireHotkey(.released)
        }
        hasFiredModifierPressed = false
    }

    private func hasOtherModifiers(_ flags: CGEventFlags, excluding: Hotkey.ModifierKey) -> Bool {
        let allModifiers: [(CGEventFlags, Hotkey.ModifierKey)] = [
            (.maskAlternate, .option),
            (.maskShift, .shift),
            (.maskControl, .control),
            (.maskCommand, .command)
        ]
        for (flag, key) in allModifiers {
            if key != excluding && flags.contains(flag) {
                return true
            }
        }
        return false
    }

    private func fireHotkey(_ trigger: Trigger) {
        onHotkeyTriggered?(trigger)
    }
}

private func globeKeyEventTapCallback(
    proxy: CGEventTapProxy,
    type: CGEventType,
    event: CGEvent,
    refcon: UnsafeMutableRawPointer?
) -> Unmanaged<CGEvent>? {
    guard let refcon else {
        return Unmanaged.passUnretained(event)
    }

    let handler = Unmanaged<GlobeKeyHandler>.fromOpaque(refcon).takeUnretainedValue()
    handler.handleEvent(type: type, event: event)
    return Unmanaged.passUnretained(event)
}
