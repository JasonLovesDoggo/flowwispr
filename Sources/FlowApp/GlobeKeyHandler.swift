//
// GlobeKeyHandler.swift
// Flow
//
// Captures the recording hotkey (Fn key or custom) using a CGEvent tap.
// Fn defaults to press-and-hold for recording.
// Custom hotkeys use Carbon's RegisterEventHotKey for global capture.
// Requires "Accessibility" permission in System Settings > Privacy & Security.
//

import ApplicationServices
import Carbon.HIToolbox
import Foundation

// Unique signature for our hotkey (arbitrary 4-char code)
private let kHotkeySignature: FourCharCode = 0x464C_5752 // "FLWR"
private let kHotkeyID: UInt32 = 1

final class GlobeKeyHandler {
    enum Trigger {
        case pressed
        case released
        case toggle
    }

    private let fnHoldDelaySeconds: TimeInterval = 0.06
    private var eventTap: CFMachPort?
    private var runLoopSource: CFRunLoopSource?
    private var onHotkeyTriggered: (@Sendable (Trigger) -> Void)?
    private var hotkey: Hotkey

    private var isFunctionDown = false
    private var functionUsedAsModifier = false
    private var pendingFnTrigger: DispatchWorkItem?

    private var isModifierDown = false
    private var modifierUsedAsModifier = false
    private var pendingModifierTrigger: DispatchWorkItem?

    // Carbon hotkey for custom key combos (works globally)
    private var carbonHotKeyRef: EventHotKeyRef?
    private var carbonEventHandler: EventHandlerRef?

    init(hotkey: Hotkey, onHotkeyTriggered: @escaping @Sendable (Trigger) -> Void) {
        self.hotkey = hotkey
        self.onHotkeyTriggered = onHotkeyTriggered
        startListening(prompt: false)
    }

    deinit {
        unregisterCarbonHotkey()
        if let eventTap {
            CGEvent.tapEnable(tap: eventTap, enable: false)
        }
        if let runLoopSource {
            CFRunLoopRemoveSource(CFRunLoopGetMain(), runLoopSource, .commonModes)
        }
    }

    func updateHotkey(_ hotkey: Hotkey) {
        let oldKind = self.hotkey.kind
        self.hotkey = hotkey

        // Reset state for Fn/modifier-only modes
        isFunctionDown = false
        functionUsedAsModifier = false
        pendingFnTrigger?.cancel()
        pendingFnTrigger = nil
        isModifierDown = false
        modifierUsedAsModifier = false
        pendingModifierTrigger?.cancel()
        pendingModifierTrigger = nil

        // Update Carbon hotkey registration if switching to/from custom
        if case .custom = oldKind {
            unregisterCarbonHotkey()
        }
        if case .custom(let keyCode, let modifiers, _) = hotkey.kind {
            registerCarbonHotkey(keyCode: keyCode, modifiers: modifiers)
        }
    }

    @discardableResult
    func startListening(prompt: Bool) -> Bool {
        guard accessibilityTrusted(prompt: prompt) else { return false }

        // Register Carbon hotkey if using custom hotkey
        if case .custom(let keyCode, let modifiers, _) = hotkey.kind {
            registerCarbonHotkey(keyCode: keyCode, modifiers: modifiers)
        }

        guard eventTap == nil else { return true }

        // Event tap for Fn key and modifier-only hotkeys (flagsChanged events)
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
        if type == .tapDisabledByTimeout || type == .tapDisabledByUserInput {
            if let eventTap {
                CGEvent.tapEnable(tap: eventTap, enable: true)
            }
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
                    if keycode != Int64(kVK_Function) {
                        functionUsedAsModifier = true
                        pendingFnTrigger?.cancel()
                        pendingFnTrigger = nil
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
                    pendingModifierTrigger?.cancel()
                    pendingModifierTrigger = nil
                }
            default:
                break
            }
        case .custom:
            // Custom hotkeys are handled by Carbon RegisterEventHotKey (global)
            break
        }
    }

    private func handleFunctionFlagChange(_ event: CGEvent) {
        let hasFn = event.flags.contains(.maskSecondaryFn)
        guard hasFn != isFunctionDown else { return }

        if hasFn {
            isFunctionDown = true
            functionUsedAsModifier = false
            pendingFnTrigger?.cancel()
            let workItem = DispatchWorkItem { [weak self] in
                guard let self, self.isFunctionDown, !self.functionUsedAsModifier else { return }
                self.fireHotkey(.pressed)
            }
            pendingFnTrigger = workItem
            DispatchQueue.main.asyncAfter(deadline: .now() + fnHoldDelaySeconds, execute: workItem)
            return
        }

        guard isFunctionDown else { return }
        isFunctionDown = false
        pendingFnTrigger?.cancel()
        pendingFnTrigger = nil

        if !functionUsedAsModifier {
            fireHotkey(.released)
        }
    }

    private func handleModifierFlagChange(_ event: CGEvent, modifier: Hotkey.ModifierKey) {
        let hasModifier = event.flags.contains(modifier.cgFlag)

        // Check if other modifiers are also pressed (means it's being used as a combo)
        let otherModifiersPressed = hasOtherModifiers(event.flags, excluding: modifier)

        guard hasModifier != isModifierDown else {
            // If the modifier is still down but other modifiers changed, mark as used
            if isModifierDown && otherModifiersPressed {
                modifierUsedAsModifier = true
                pendingModifierTrigger?.cancel()
                pendingModifierTrigger = nil
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
            pendingModifierTrigger?.cancel()
            let workItem = DispatchWorkItem { [weak self] in
                guard let self, self.isModifierDown, !self.modifierUsedAsModifier else { return }
                self.fireHotkey(.pressed)
            }
            pendingModifierTrigger = workItem
            DispatchQueue.main.asyncAfter(deadline: .now() + fnHoldDelaySeconds, execute: workItem)
            return
        }

        // Modifier released
        guard isModifierDown else { return }
        isModifierDown = false
        pendingModifierTrigger?.cancel()
        pendingModifierTrigger = nil

        if !modifierUsedAsModifier {
            fireHotkey(.released)
        }
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

    // MARK: - Carbon Hotkey Registration (for global custom hotkeys)

    private func registerCarbonHotkey(keyCode: Int, modifiers: Hotkey.Modifiers) {
        unregisterCarbonHotkey()

        // Install event handler if not already installed
        if carbonEventHandler == nil {
            var eventType = EventTypeSpec(
                eventClass: OSType(kEventClassKeyboard),
                eventKind: UInt32(kEventHotKeyPressed)
            )

            let handlerRef = Unmanaged.passUnretained(self).toOpaque()
            let status = InstallEventHandler(
                GetApplicationEventTarget(),
                carbonHotkeyCallback,
                1,
                &eventType,
                handlerRef,
                &carbonEventHandler
            )

            if status != noErr {
                return
            }
        }

        // Convert our modifiers to Carbon modifiers
        var carbonModifiers: UInt32 = 0
        if modifiers.contains(.command) { carbonModifiers |= UInt32(cmdKey) }
        if modifiers.contains(.option) { carbonModifiers |= UInt32(optionKey) }
        if modifiers.contains(.control) { carbonModifiers |= UInt32(controlKey) }
        if modifiers.contains(.shift) { carbonModifiers |= UInt32(shiftKey) }

        let hotkeyID = EventHotKeyID(signature: kHotkeySignature, id: kHotkeyID)
        var hotKeyRef: EventHotKeyRef?

        let status = RegisterEventHotKey(
            UInt32(keyCode),
            carbonModifiers,
            hotkeyID,
            GetApplicationEventTarget(),
            0,
            &hotKeyRef
        )

        if status == noErr {
            carbonHotKeyRef = hotKeyRef
        }
    }

    private func unregisterCarbonHotkey() {
        if let hotKeyRef = carbonHotKeyRef {
            UnregisterEventHotKey(hotKeyRef)
            carbonHotKeyRef = nil
        }
    }

    fileprivate func handleCarbonHotkey() {
        fireHotkey(.toggle)
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

private func carbonHotkeyCallback(
    nextHandler: EventHandlerCallRef?,
    event: EventRef?,
    userData: UnsafeMutableRawPointer?
) -> OSStatus {
    guard let userData, let event else {
        return OSStatus(eventNotHandledErr)
    }

    var hotkeyID = EventHotKeyID()
    let status = GetEventParameter(
        event,
        EventParamName(kEventParamDirectObject),
        EventParamType(typeEventHotKeyID),
        nil,
        MemoryLayout<EventHotKeyID>.size,
        nil,
        &hotkeyID
    )

    guard status == noErr,
          hotkeyID.signature == kHotkeySignature,
          hotkeyID.id == kHotkeyID else {
        return OSStatus(eventNotHandledErr)
    }

    let handler = Unmanaged<GlobeKeyHandler>.fromOpaque(userData).takeUnretainedValue()
    DispatchQueue.main.async {
        handler.handleCarbonHotkey()
    }

    return noErr
}
