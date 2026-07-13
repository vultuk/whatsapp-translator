import Foundation

struct AuthCheckResponse: Decodable, Sendable { let required: Bool }

struct AuthResponse: Decodable, Sendable {
    let success: Bool
    let token: String?
    let error: String?
}

struct BackendStatus: Decodable, Sendable {
    let connected: Bool
    let phone: String?
    let name: String?
}

struct AvatarResponse: Decodable, Sendable { let url: URL? }

struct Contact: Codable, Identifiable, Hashable, Sendable {
    let id: String
    let name: String?
    let phone: String?
    let type: String?
    let lastMessageTime: Int64
    var unreadCount: Int
    let pinnedAt: Int64?
    let lastMessagePreview: String?

    var displayName: String { name?.nilIfBlank ?? phone?.nilIfBlank ?? "Unknown chat" }
    var isGroup: Bool { type == "group" || id.contains("@g.us") }
    var initials: String {
        let parts = displayName.split(separator: " ").prefix(2)
        return parts.compactMap(\.first).map(String.init).joined().uppercased()
    }
}

struct MessagesResponse: Decodable, Sendable {
    let messages: [ChatMessage]
    let hasMore: Bool
}

struct ChatMessage: Codable, Identifiable, Hashable, Sendable {
    let id: String
    let contactId: String
    let timestamp: Int64
    let isFromMe: Bool
    let isForwarded: Bool
    let senderName: String?
    let senderPhone: String?
    let contactName: String?
    let contactPhone: String?
    let chatType: String
    let contentType: String
    let content: MessageContent?
    let originalText: String?
    let translatedText: String?
    let sourceLanguage: String?
    let isTranslated: Bool

    var displayText: String {
        if isFromMe {
            return content?.body?.nilIfBlank ?? originalText?.nilIfBlank ?? translatedText?.nilIfBlank ?? contentType
        }
        return translatedText?.nilIfBlank ?? content?.body?.nilIfBlank ?? originalText?.nilIfBlank ?? contentType
    }

    var alternateText: String? {
        let candidate = isFromMe ? translatedText : originalText
        guard let candidate = candidate?.nilIfBlank, candidate != displayText else { return nil }
        return candidate
    }

    var date: Date { Date(timeIntervalSince1970: TimeInterval(timestamp) / 1_000) }
}

struct MessageContent: Codable, Hashable, Sendable {
    let type: String?
    let body: String?
    let showTranslatedPrimary: Bool?
    let replyContext: ReplyContext?

    enum CodingKeys: String, CodingKey {
        case type, body, showTranslatedPrimary
        case replyContext = "reply_context"
    }
}

struct ReplyContext: Codable, Hashable, Sendable {
    let messageId: String?
    let senderName: String?
    let text: String?
}

struct SendMessageRequest: Encodable, Sendable {
    let contactId: String
    let text: String
    let replyTo: String?
    let replyToSender: String?
    let replyToText: String?
    let replyToSenderName: String?
}

struct SendMessageResponse: Decodable, Sendable {
    let messageId: String
    let timestamp: Int64
    let isTranslated: Bool
    let translatedText: String?
    let sourceLanguage: String?
    let originalFollowUpSent: Bool
    let originalMessageId: String?
    let originalTimestamp: Int64?
    let originalFollowUpError: String?
}

struct ConversationSettings: Codable, Equatable, Sendable {
    var languageOverride: String?
    var translationStyle: String?
    var sendOriginalFollowUp: Bool
}

struct OpenAISettings: Codable, Equatable, Sendable {
    var model: String?
    var reasoningEffort: String?
    var available: Bool?
}

struct PushDeviceRegistration: Encodable, Sendable {
    let installationId: String
    let token: String
    let environment: String
}

struct LiveEvent: Decodable, Sendable {
    let type: String
    let connected: Bool?
    let chatId: String?
    let message: ChatMessage?
}

private extension String {
    var nilIfBlank: String? { trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ? nil : self }
}
