//
// AccessibilityContext.swift
// Flow
//
// Extracts context from the currently focused text field via macOS Accessibility APIs.
// Provides surrounding text context to improve transcription accuracy.
// Based on Wispr Flow's FocusChangeDetector + AX API pattern.
//
// Requires "Accessibility" permission in System Settings > Privacy & Security.
//

import ApplicationServices
import AppKit
import Foundation

// MARK: - IDE Context

/// Context extracted from IDEs like Cursor and VSCode
struct IDEContext {
    /// File names from open tabs
    let openFiles: [String]

    /// Function/class/variable names extracted from visible code
    let codeSymbols: [String]

    /// Combined vocabulary words for transcription hints
    var vocabularyWords: [String] {
        var words: [String] = []

        // Add file names without extensions as vocabulary
        for file in openFiles {
            if let name = file.split(separator: ".").first {
                words.append(String(name))
            }
        }

        // Add code symbols
        words.append(contentsOf: codeSymbols)

        return Array(Set(words)) // Dedupe
    }

    var isEmpty: Bool {
        openFiles.isEmpty && codeSymbols.isEmpty
    }

    /// Human-readable summary for logging
    var summary: String {
        var parts: [String] = []
        if !openFiles.isEmpty {
            parts.append("Files: \(openFiles.joined(separator: ", "))")
        }
        if !codeSymbols.isEmpty {
            let symbolSample = codeSymbols.prefix(10).joined(separator: ", ")
            let suffix = codeSymbols.count > 10 ? " (+\(codeSymbols.count - 10) more)" : ""
            parts.append("Symbols: \(symbolSample)\(suffix)")
        }
        return parts.isEmpty ? "No IDE context" : parts.joined(separator: "\n")
    }
}

// MARK: - Text Field Context

/// Context extracted from the focused text element
struct TextFieldContext {
    /// Text currently selected (highlighted) in the field
    let selectedText: String?

    /// Text before the cursor/selection
    let beforeText: String?

    /// Text after the cursor/selection
    let afterText: String?

    /// The full value of the text field
    let fullText: String?

    /// Placeholder/label of the field if available
    let placeholder: String?

    /// Role description (e.g., "text field", "text area")
    let roleDescription: String?

    /// Bundle ID of the app containing this field
    let appBundleId: String?

    /// Human-readable context summary for transcription prompt
    var contextSummary: String? {
        var parts: [String] = []

        if let before = beforeText, !before.isEmpty {
            // Take last ~100 chars of context before cursor
            let trimmed = before.count > 100 ? "..." + String(before.suffix(100)) : before
            parts.append("Text before cursor: \"\(trimmed)\"")
        }

        if let selected = selectedText, !selected.isEmpty {
            parts.append("Selected text: \"\(selected)\"")
        }

        guard !parts.isEmpty else { return nil }
        return parts.joined(separator: "\n")
    }

    static let empty = TextFieldContext(
        selectedText: nil,
        beforeText: nil,
        afterText: nil,
        fullText: nil,
        placeholder: nil,
        roleDescription: nil,
        appBundleId: nil
    )
}

final class AccessibilityContext {
    /// Extract context from the currently focused text element
    static func extractFocusedTextContext() -> TextFieldContext {
        guard let focusedElement = getFocusedElement() else {
            return .empty
        }

        let role = getStringAttribute(focusedElement, kAXRoleAttribute as CFString)

        // Only extract from text-input elements
        let textRoles = [
            kAXTextFieldRole as String,
            kAXTextAreaRole as String,
            kAXComboBoxRole as String
        ]

        guard let role, textRoles.contains(role) else {
            return .empty
        }

        let fullText = getStringAttribute(focusedElement, kAXValueAttribute as CFString)
        let selectedText = getSelectedText(focusedElement)
        let placeholder = getStringAttribute(focusedElement, kAXPlaceholderValueAttribute as CFString)
        let roleDescription = getStringAttribute(focusedElement, kAXRoleDescriptionAttribute as CFString)

        // Get text before and after selection
        var beforeText: String?
        var afterText: String?

        if let fullText, let range = getSelectedTextRange(focusedElement) {
            let startIndex = range.location
            let endIndex = range.location + range.length

            if startIndex > 0 && startIndex <= fullText.count {
                let idx = fullText.index(fullText.startIndex, offsetBy: min(startIndex, fullText.count))
                beforeText = String(fullText[..<idx])
            }

            if endIndex < fullText.count {
                let idx = fullText.index(fullText.startIndex, offsetBy: min(endIndex, fullText.count))
                afterText = String(fullText[idx...])
            }
        }

        // Get the app bundle ID
        var appBundleId: String?
        if let app = NSWorkspace.shared.frontmostApplication {
            appBundleId = app.bundleIdentifier
        }

        return TextFieldContext(
            selectedText: selectedText,
            beforeText: beforeText,
            afterText: afterText,
            fullText: fullText,
            placeholder: placeholder,
            roleDescription: roleDescription,
            appBundleId: appBundleId
        )
    }

    // MARK: - Private Helpers

    private static func getFocusedElement() -> AXUIElement? {
        // Get the frontmost application
        guard let app = NSWorkspace.shared.frontmostApplication else { return nil }

        let appElement = AXUIElementCreateApplication(app.processIdentifier)

        // Get the focused UI element
        var focusedElement: CFTypeRef?
        let result = AXUIElementCopyAttributeValue(
            appElement,
            kAXFocusedUIElementAttribute as CFString,
            &focusedElement
        )

        guard result == .success, let element = focusedElement else { return nil }
        return (element as! AXUIElement)
    }

    private static func getStringAttribute(_ element: AXUIElement, _ attribute: CFString) -> String? {
        var value: CFTypeRef?
        let result = AXUIElementCopyAttributeValue(element, attribute, &value)
        guard result == .success, let stringValue = value as? String else { return nil }
        return stringValue
    }

    private static func getSelectedText(_ element: AXUIElement) -> String? {
        var value: CFTypeRef?
        let result = AXUIElementCopyAttributeValue(
            element,
            kAXSelectedTextAttribute as CFString,
            &value
        )
        guard result == .success, let text = value as? String else { return nil }
        return text
    }

    private static func getSelectedTextRange(_ element: AXUIElement) -> NSRange? {
        var value: CFTypeRef?
        let result = AXUIElementCopyAttributeValue(
            element,
            kAXSelectedTextRangeAttribute as CFString,
            &value
        )
        guard result == .success, let rangeValue = value else { return nil }

        // AXValue contains a CFRange
        var range = CFRange()
        guard AXValueGetValue(rangeValue as! AXValue, .cfRange, &range) else { return nil }

        return NSRange(location: range.location, length: range.length)
    }

    // MARK: - IDE Context Extraction

    /// Bundle IDs for supported IDEs
    private static let ideBundleIDs = [
        "com.todesktop.230313mzl4w4u92",  // Cursor
        "com.microsoft.VSCode",            // VSCode
        "com.microsoft.VSCodeInsiders",    // VSCode Insiders
        "com.jetbrains.intellij",          // IntelliJ IDEA
        "com.jetbrains.WebStorm",          // WebStorm
        "com.jetbrains.pycharm",           // PyCharm
        "com.sublimetext.4",               // Sublime Text 4
        "com.sublimetext.3"                // Sublime Text 3
    ]

    /// Check if the frontmost app is a supported IDE
    static func isIDEActive() -> Bool {
        guard let app = NSWorkspace.shared.frontmostApplication,
              let bundleId = app.bundleIdentifier else {
            return false
        }
        return ideBundleIDs.contains(bundleId)
    }

    /// Extract context from IDE (file names from tabs, code symbols from visible editors)
    static func extractIDEContext() -> IDEContext? {
        guard let app = NSWorkspace.shared.frontmostApplication,
              let bundleId = app.bundleIdentifier,
              ideBundleIDs.contains(bundleId) else {
            return nil
        }

        let appElement = AXUIElementCreateApplication(app.processIdentifier)

        // Extract tab names (file names)
        let tabNames = extractTabNames(from: appElement)

        // Extract code symbols from visible editors
        let symbols = extractCodeSymbols(from: appElement)

        let context = IDEContext(openFiles: tabNames, codeSymbols: symbols)
        return context.isEmpty ? nil : context
    }

    /// Extract file names from IDE tabs
    private static func extractTabNames(from app: AXUIElement) -> [String] {
        var names: [String] = []

        // Get all windows
        guard let windows = getChildren(of: app) else {
            return names
        }

        for window in windows {
            // Look for tab groups and tabs within windows
            extractTabNamesRecursively(from: window, into: &names, depth: 0)
        }

        return Array(Set(names)) // Dedupe
    }

    /// Recursively search for tab elements and extract their titles
    private static func extractTabNamesRecursively(from element: AXUIElement, into names: inout [String], depth: Int) {
        // Limit recursion depth to avoid getting lost in the AX tree
        guard depth < 8 else { return }

        let role = getStringAttribute(element, kAXRoleAttribute as CFString)

        // Check if this is a tab or tab-like element
        // Note: Tab buttons don't have a constant in ApplicationServices, use string literal
        if role == kAXTabGroupRole as String || role == "AXTabButton" ||
           role == "AXRadioButton" { // VSCode uses radio buttons for tabs
            if let title = getStringAttribute(element, kAXTitleAttribute as CFString),
               isValidFileName(title) {
                names.append(title)
            }
        }

        // Also check window title which often contains current file
        if role == kAXWindowRole as String {
            if let title = getStringAttribute(element, kAXTitleAttribute as CFString) {
                // Window titles often have format "filename — Project" or "filename - VSCode"
                let parts = title.split(separator: "—").first ?? title.split(separator: " - ").first
                if let name = parts.map({ String($0).trimmingCharacters(in: .whitespaces) }),
                   isValidFileName(name) {
                    names.append(name)
                }
            }
        }

        // Recurse into children
        if let children = getChildren(of: element) {
            for child in children {
                extractTabNamesRecursively(from: child, into: &names, depth: depth + 1)
            }
        }
    }

    /// Extract code symbols (function/class/variable names) from visible code
    private static func extractCodeSymbols(from app: AXUIElement) -> [String] {
        var symbols: [String] = []

        // Find text areas (code editors)
        var textAreas: [AXUIElement] = []
        findTextAreasRecursively(from: app, into: &textAreas, depth: 0)

        for textArea in textAreas {
            if let text = getStringAttribute(textArea, kAXValueAttribute as CFString) {
                symbols.append(contentsOf: parseCodeSymbols(from: text))
            }
        }

        return Array(Set(symbols)) // Dedupe
    }

    /// Recursively find text areas in the AX tree
    private static func findTextAreasRecursively(from element: AXUIElement, into areas: inout [AXUIElement], depth: Int) {
        guard depth < 10 else { return }

        let role = getStringAttribute(element, kAXRoleAttribute as CFString)

        if role == kAXTextAreaRole as String {
            areas.append(element)
        }

        if let children = getChildren(of: element) {
            for child in children {
                findTextAreasRecursively(from: child, into: &areas, depth: depth + 1)
            }
        }
    }

    /// Get children of an AX element
    private static func getChildren(of element: AXUIElement) -> [AXUIElement]? {
        var value: CFTypeRef?
        let result = AXUIElementCopyAttributeValue(
            element,
            kAXChildrenAttribute as CFString,
            &value
        )
        guard result == .success, let children = value as? [AXUIElement] else { return nil }
        return children
    }

    /// Check if a string looks like a valid file name
    private static func isValidFileName(_ name: String) -> Bool {
        // Must have an extension
        guard name.contains(".") else { return false }

        // Must not be too long or too short
        guard name.count >= 3 && name.count <= 100 else { return false }

        // Should start with a letter, number, or underscore
        guard let first = name.first, first.isLetter || first.isNumber || first == "_" else {
            return false
        }

        // Common code file extensions
        let codeExtensions = [
            "swift", "rs", "go", "py", "js", "ts", "jsx", "tsx", "java", "kt",
            "c", "cpp", "h", "hpp", "m", "mm", "rb", "php", "cs", "fs",
            "json", "yaml", "yml", "toml", "xml", "html", "css", "scss", "less",
            "md", "txt", "sh", "bash", "zsh", "fish", "ps1",
            "sql", "graphql", "proto", "ex", "exs", "erl", "hs", "ml", "clj"
        ]

        let ext = name.split(separator: ".").last.map(String.init)?.lowercased() ?? ""
        return codeExtensions.contains(ext)
    }

    /// Parse code symbols (function/class/variable names) from source code
    private static func parseCodeSymbols(from code: String) -> [String] {
        var symbols: [String] = []

        // Limit how much code we process to avoid performance issues
        let codeToProcess = String(code.prefix(10000))

        // Patterns for common languages
        let patterns = [
            // Functions
            "func\\s+(\\w+)",               // Swift
            "fn\\s+(\\w+)",                 // Rust
            "function\\s+(\\w+)",           // JS/TS
            "def\\s+(\\w+)",                // Python/Ruby
            "async\\s+def\\s+(\\w+)",       // Python async
            "pub\\s+fn\\s+(\\w+)",          // Rust public
            "private\\s+func\\s+(\\w+)",    // Swift private

            // Classes/Types
            "class\\s+(\\w+)",              // Most languages
            "struct\\s+(\\w+)",             // Swift/Rust/Go/C
            "enum\\s+(\\w+)",               // Most languages
            "interface\\s+(\\w+)",          // TS/Java/Go
            "type\\s+(\\w+)",               // TS/Go
            "trait\\s+(\\w+)",              // Rust
            "protocol\\s+(\\w+)",           // Swift

            // Variables (be conservative to avoid noise)
            "const\\s+(\\w+)\\s*=",         // JS/TS
            "let\\s+(\\w+)\\s*[=:]",        // Swift/JS
            "var\\s+(\\w+)\\s*[=:]"         // Swift/JS/Go
        ]

        for pattern in patterns {
            if let regex = try? NSRegularExpression(pattern: pattern, options: []) {
                let range = NSRange(codeToProcess.startIndex..., in: codeToProcess)
                let matches = regex.matches(in: codeToProcess, range: range)
                for match in matches {
                    if match.numberOfRanges > 1,
                       let range = Range(match.range(at: 1), in: codeToProcess) {
                        let symbol = String(codeToProcess[range])
                        // Filter out common keywords and short names
                        if symbol.count >= 3 && !isCommonKeyword(symbol) {
                            symbols.append(symbol)
                        }
                    }
                }
            }
        }

        return symbols
    }

    /// Check if a word is a common keyword (not worth adding to vocabulary)
    private static func isCommonKeyword(_ word: String) -> Bool {
        let keywords = [
            "self", "this", "super", "init", "new", "null", "nil", "true", "false",
            "let", "var", "const", "func", "function", "def", "class", "struct",
            "enum", "interface", "type", "return", "if", "else", "for", "while",
            "switch", "case", "break", "continue", "try", "catch", "throw",
            "async", "await", "import", "export", "from", "package", "module"
        ]
        return keywords.contains(word.lowercased())
    }
}
