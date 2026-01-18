//
// TranscriptionSummary.swift
// Flow
//
// Lightweight transcription summary for history views.
//

import Foundation

public enum TranscriptionStatus: String, Codable {
    case success
    case failed
}

public struct TranscriptionSummary: Identifiable, Codable {
    public let id: String
    public let status: TranscriptionStatus
    public let text: String
    public let rawText: String
    public let error: String?
    public let durationMs: UInt64
    public let createdAt: Date
    public let appName: String?

    public init(
        id: String,
        status: TranscriptionStatus,
        text: String,
        rawText: String = "",
        error: String?,
        durationMs: UInt64,
        createdAt: Date,
        appName: String?
    ) {
        self.id = id
        self.status = status
        self.text = text
        self.rawText = rawText
        self.error = error
        self.durationMs = durationMs
        self.createdAt = createdAt
        self.appName = appName
    }

    enum CodingKeys: String, CodingKey {
        case id
        case status
        case text
        case rawText = "raw_text"
        case error
        case durationMs = "duration_ms"
        case createdAt = "created_at"
        case appName = "app_name"
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        id = try container.decode(String.self, forKey: .id)
        status = try container.decode(TranscriptionStatus.self, forKey: .status)
        text = try container.decode(String.self, forKey: .text)
        rawText = try container.decodeIfPresent(String.self, forKey: .rawText) ?? ""
        error = try container.decodeIfPresent(String.self, forKey: .error)
        durationMs = try container.decode(UInt64.self, forKey: .durationMs)
        createdAt = try container.decode(Date.self, forKey: .createdAt)
        appName = try container.decodeIfPresent(String.self, forKey: .appName)
    }
}
