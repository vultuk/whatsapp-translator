import Foundation

struct WatchMessage: Codable, Identifiable, Equatable, Sendable {
    let messageID: String
    let contactID: String
    let conversation: String
    let sender: String
    let text: String
    let timestamp: Int64
    let isFromMe: Bool
    let isGroup: Bool

    var id: String { contactID + ":" + messageID }
    var date: Date { Date(timeIntervalSince1970: Double(timestamp) / 1_000) }
}

struct WatchFeedSnapshot: Codable, Equatable, Sendable {
    let accountID: String
    var messages: [WatchMessage]
    let updatedAt: Date

    // Watch Connectivity limits each message. Keep a contiguous, recent window,
    // measuring encoded bytes so multibyte text cannot exceed the transfer budget.
    func encoded() throws -> Data {
        var snapshot = self
        var data = try JSONEncoder().encode(snapshot)
        while data.count > 48_000, !snapshot.messages.isEmpty {
            snapshot.messages.removeFirst()
            data = try JSONEncoder().encode(snapshot)
        }
        return data
    }
}

struct WatchReply: Codable, Equatable, Sendable {
    let requestID: UUID
    let accountID: String
    let target: WatchMessage
    let text: String
}

struct WatchRequest: Codable, Sendable {
    enum Action: String, Codable { case feed, reply }
    let action: Action
    var reply: WatchReply? = nil
}

struct WatchResponse: Codable, Sendable {
    enum Status: String, Codable { case feed, sent, rejected, uncertain }
    let status: Status
    var snapshot: Data? = nil
    var message: String? = nil

    func encoded() throws -> Data {
        let encoder = JSONEncoder()
        // Snapshot data is base64 inside this envelope. Avoid slash escaping so
        // a 48 KB snapshot stays below Watch Connectivity's 65,536-byte limit.
        encoder.outputFormatting = [.withoutEscapingSlashes]
        return try encoder.encode(self)
    }
}

struct WatchReplyDraft: Codable, Equatable, Sendable {
    let target: WatchMessage
    let accountID: String
    var text = ""
    var submitted: WatchReply?

    mutating func submission() -> WatchReply {
        if let submitted { return submitted }
        let reply = WatchReply(requestID: UUID(), accountID: accountID, target: target,
                               text: text.trimmingCharacters(in: .whitespacesAndNewlines))
        submitted = reply
        return reply
    }
}

extension WatchFeedSnapshot {
    static var demo: Self {
        let now = Int64(Date().timeIntervalSince1970 * 1_000)
        return Self(accountID: "demo", messages: [
            WatchMessage(messageID: "one", contactID: "jordan", conversation: "Jordan", sender: "Jordan", text: "Shall we meet by the café at six?", timestamp: now - 180_000, isFromMe: false, isGroup: false),
            WatchMessage(messageID: "two", contactID: "studio", conversation: "Studio team", sender: "Alex", text: "The new sketches look great. Thank you!", timestamp: now - 120_000, isFromMe: false, isGroup: true),
            WatchMessage(messageID: "three", contactID: "jordan", conversation: "Jordan", sender: "You", text: "Six works for me.", timestamp: now - 60_000, isFromMe: true, isGroup: false),
            WatchMessage(messageID: "four", contactID: "studio", conversation: "Studio team", sender: "Sam", text: "I’ll bring the prints tomorrow morning.", timestamp: now, isFromMe: false, isGroup: true),
        ], updatedAt: Date())
    }
}
