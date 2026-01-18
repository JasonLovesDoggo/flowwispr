//
//  ContactsBridge.swift
//  FlowWhispr Contact Lookup Bridge
//
//  Provides C-compatible FFI for macOS Contacts framework access from Rust
//

import Foundation
import Contacts

/// C-compatible contact result structure
@frozen
public struct CContactResult {
    public var name: UnsafePointer<CChar>?
    public var organization: UnsafePointer<CChar>?
    public var found: Bool
}

/// Find contact by display name and return organization
/// - Parameter displayName: C string containing the contact's display name
/// - Returns: CContactResult with contact info (caller must free strings)
@_cdecl("contact_lookup_by_name")
public func contactLookupByName(_ displayName: UnsafePointer<CChar>) -> CContactResult {
    let name = String(cString: displayName)

    guard let contact = findContact(by: name) else {
        return CContactResult(name: nil, organization: nil, found: false)
    }

    let contactName = CNContactFormatter.string(from: contact, style: .fullName) ?? name
    let org = contact.organizationName

    // Allocate C strings for return (Rust side must free)
    let namePtr = strdup(contactName)
    let orgPtr = org.isEmpty ? nil : strdup(org)

    return CContactResult(
        name: namePtr,
        organization: orgPtr,
        found: true
    )
}

/// Free a C string allocated by this bridge
/// - Parameter ptr: Pointer to free
@_cdecl("contact_free_string")
public func contactFreeString(_ ptr: UnsafeMutablePointer<CChar>?) {
    guard let ptr = ptr else { return }
    free(ptr)
}

/// Request Contacts permission (async, returns immediately)
/// - Returns: true if permission already granted, false if needs request
@_cdecl("contact_request_permission")
public func contactRequestPermission() -> Bool {
    let store = CNContactStore()
    let status = CNContactStore.authorizationStatus(for: .contacts)

    switch status {
    case .authorized:
        return true
    case .notDetermined:
        // Request permission asynchronously
        store.requestAccess(for: .contacts) { granted, error in
            if let error = error {
                print("Contact permission error: \(error)")
            }
        }
        return false
    case .denied, .restricted:
        return false
    @unknown default:
        return false
    }
}

/// Check if Contacts permission is currently granted
/// - Returns: true if authorized
@_cdecl("contact_is_authorized")
public func contactIsAuthorized() -> Bool {
    return CNContactStore.authorizationStatus(for: .contacts) == .authorized
}

/// Find contact in Contacts.app by matching display name
/// - Parameter displayName: Name to search for
/// - Returns: CNContact if found, nil otherwise
private func findContact(by displayName: String) -> CNContact? {
    let store = CNContactStore()

    // Check permission first
    guard CNContactStore.authorizationStatus(for: .contacts) == .authorized else {
        return nil
    }

    // Keys to fetch from contact
    let keys = [
        CNContactGivenNameKey,
        CNContactFamilyNameKey,
        CNContactOrganizationNameKey,
        CNContactPhoneNumbersKey,
        CNContactFormatter.descriptorForRequiredKeys(for: .fullName)
    ] as [CNKeyDescriptor]

    do {
        // Try exact name match first
        let predicate = CNContact.predicateForContacts(matchingName: displayName)
        let contacts = try store.unifiedContacts(matching: predicate, keysToFetch: keys)

        if let first = contacts.first {
            return first
        }

        // Fallback: Try partial match by iterating all contacts (expensive, only for small datasets)
        // In production, you may want to cache contacts or use a better search strategy
        return nil

    } catch {
        print("Contact lookup error: \(error)")
        return nil
    }
}

/// Get all contacts for batch processing (WARNING: can be slow on large contact lists)
/// - Returns: Array of (name, organization) tuples as JSON string
@_cdecl("contact_get_all_json")
public func contactGetAllJson() -> UnsafePointer<CChar>? {
    guard CNContactStore.authorizationStatus(for: .contacts) == .authorized else {
        return strdup("[]")
    }

    let store = CNContactStore()
    let keys = [
        CNContactGivenNameKey,
        CNContactFamilyNameKey,
        CNContactOrganizationNameKey,
        CNContactFormatter.descriptorForRequiredKeys(for: .fullName)
    ] as [CNKeyDescriptor]

    var results: [[String: String]] = []

    do {
        let request = CNContactFetchRequest(keysToFetch: keys)

        try store.enumerateContacts(with: request) { contact, stop in
            if let fullName = CNContactFormatter.string(from: contact, style: .fullName) {
                let org = contact.organizationName
                results.append([
                    "name": fullName,
                    "organization": org
                ])
            }
        }

        let jsonData = try JSONSerialization.data(withJSONObject: results, options: [])
        if let jsonString = String(data: jsonData, encoding: .utf8) {
            return strdup(jsonString)
        }

    } catch {
        print("Error fetching contacts: \(error)")
    }

    return strdup("[]")
}
