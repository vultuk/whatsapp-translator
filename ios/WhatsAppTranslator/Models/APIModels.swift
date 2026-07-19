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

    var isUpdates: Bool { id.caseInsensitiveCompare("status@broadcast") == .orderedSame || type == "status" }
    var showsAsUpdatesToolbarItem: Bool { isUpdates }
    var showsInChatList: Bool { !isUpdates }
    var displayName: String { isUpdates ? "Updates" : (name?.nilIfBlank ?? phone?.nilIfBlank ?? "Unknown chat") }
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

enum MessageMediaKind: String, Equatable, Sendable {
    case image
    case video
    case audio
    case document
    case sticker
}

enum MessageActionKind: Equatable, Sendable {
    case reply
    case translate
    case aiReply
    case star
    case react
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
    var deliveryStatus: String? = nil
    var reactions: [String: [String]]? = nil

    var deliveryState: MessageDeliveryState {
        guard isFromMe else { return .none }
        return switch deliveryStatus?.lowercased() {
        case "read", "played": .read
        case "delivered": .delivered
        default: .sent
        }
    }

    var displayText: String {
        let richFallback: String? = switch normalizedContentType {
        case "audio": content?.isVoiceNote == true ? "Voice note" : "Audio"
        case "document": content?.fileName?.nilIfBlank ?? "Document"
        case "sticker": "Sticker"
        case "location": content?.name?.nilIfBlank ?? content?.address?.nilIfBlank ?? "Location"
        case "contact": content?.displayName?.nilIfBlank ?? content?.name?.nilIfBlank ?? "Contact"
        case "poll": content?.question?.nilIfBlank ?? "Poll"
        case "revoked": "This message was deleted"
        case "video": "Video"
        case "image": "Image"
        default: nil
        }
        if isFromMe {
            return content?.body?.nilIfBlank ?? content?.caption?.nilIfBlank ?? originalText?.nilIfBlank ?? translatedText?.nilIfBlank ?? richFallback ?? contentType
        }
        return translatedText?.nilIfBlank ?? content?.body?.nilIfBlank ?? content?.caption?.nilIfBlank ?? originalText?.nilIfBlank ?? richFallback ?? contentType
    }

    var alternateText: String? {
        let candidate = isFromMe ? translatedText : originalText
        guard let candidate = candidate?.nilIfBlank, candidate != displayText else { return nil }
        return candidate
    }

    var date: Date { Date(timeIntervalSince1970: TimeInterval(timestamp) / 1_000) }
    var normalizedContentType: String {
        content?.type?.lowercased().nilIfBlank ?? contentType.lowercased()
    }
    var isReaction: Bool {
        contentType.caseInsensitiveCompare("reaction") == .orderedSame
            || content?.type?.caseInsensitiveCompare("reaction") == .orderedSame
    }
    var mediaKind: MessageMediaKind? { MessageMediaKind(rawValue: normalizedContentType) }
    var isImage: Bool { mediaKind == .image }
    var albumID: String? { content?.albumId?.nilIfBlank }
    var albumIndex: Int? { content?.albumIndex }
    var standaloneEmojiText: String? {
        guard normalizedContentType == "text", content?.replyContext == nil else { return nil }
        let value = displayText.trimmingCharacters(in: .whitespacesAndNewlines)
        let characters = Array(value)
        guard (1...3).contains(characters.count), characters.allSatisfy(\.isEmojiCharacter) else {
            return nil
        }
        return value
    }
    var canTranslate: Bool { !isFromMe && !isTranslated && contentText != nil }
    var canGenerateAIReply: Bool { !isFromMe && contentText != nil }
    var contentText: String? { content?.body?.nilIfBlank ?? content?.caption?.nilIfBlank }
    var availableActions: [MessageActionKind] {
        var actions: [MessageActionKind] = [.reply]
        if canTranslate { actions.append(.translate) }
        if canGenerateAIReply { actions.append(.aiReply) }
        actions.append(contentsOf: [.star, .react])
        return actions
    }
    var locationURL: URL? {
        guard normalizedContentType == "location",
              let latitude = content?.latitude,
              let longitude = content?.longitude else { return nil }
        var components = URLComponents(string: "https://maps.apple.com/")
        let coordinate = "\(latitude),\(longitude)"
        components?.queryItems = [
            URLQueryItem(name: "ll", value: coordinate),
            URLQueryItem(name: "q", value: content?.name?.nilIfBlank ?? content?.address?.nilIfBlank ?? "Location"),
        ]
        return components?.url
    }
    var senderJID: String? {
        guard let senderPhone = senderPhone?.nilIfBlank else { return nil }
        return senderPhone.contains("@") ? senderPhone : "\(senderPhone)@s.whatsapp.net"
    }
    var replyTarget: MessageReplyTarget {
        MessageReplyTarget(
            messageID: id,
            senderJID: senderJID,
            senderName: isFromMe ? "You" : (senderName?.nilIfBlank ?? contactName?.nilIfBlank ?? senderPhone?.nilIfBlank ?? "Contact"),
            text: contentText ?? displayText
        )
    }
    var extractedURLs: [URL] {
        let candidates = displayText.split(whereSeparator: { $0.isWhitespace })
        return candidates.compactMap { value in
            let cleaned = value.trimmingCharacters(in: CharacterSet(charactersIn: "()[]{}<>,.!?;:\"'"))
            guard cleaned.hasPrefix("https://") || cleaned.hasPrefix("http://") else { return nil }
            return URL(string: cleaned)
        }
    }
}

struct PhotoAlbumTimeline: Identifiable {
    let id: String
    let messages: [ChatMessage]

    var primaryMessage: ChatMessage { messages[0] }
    var date: Date { messages.map(\.date).min() ?? primaryMessage.date }
}

enum ConversationTimelineItem: Identifiable {
    case message(ChatMessage)
    case photoAlbum(PhotoAlbumTimeline)

    var id: String {
        switch self {
        case let .message(message): message.id
        case let .photoAlbum(album): "album:\(album.id)"
        }
    }

    var date: Date {
        switch self {
        case let .message(message): message.date
        case let .photoAlbum(album): album.date
        }
    }
}

enum ConversationTimelineBuilder {
    static func items(from messages: [ChatMessage]) -> [ConversationTimelineItem] {
        let grouped = Dictionary(grouping: messages.compactMap { message -> (String, ChatMessage)? in
            guard message.isImage, let albumID = message.albumID else { return nil }
            return (albumID, message)
        }, by: \.0).mapValues { $0.map(\.1) }
        var emittedAlbumIDs = Set<String>()

        return messages.compactMap { message in
            guard let albumID = message.albumID,
                  let albumMessages = grouped[albumID],
                  albumMessages.count > 1 else {
                return .message(message)
            }
            guard emittedAlbumIDs.insert(albumID).inserted else { return nil }
            let ordered = albumMessages.sorted {
                let leftIndex = $0.albumIndex ?? Int.max
                let rightIndex = $1.albumIndex ?? Int.max
                if leftIndex != rightIndex { return leftIndex < rightIndex }
                if $0.timestamp != $1.timestamp { return $0.timestamp < $1.timestamp }
                return $0.id < $1.id
            }
            return .photoAlbum(PhotoAlbumTimeline(id: albumID, messages: ordered))
        }
    }
}

private extension Character {
    var isEmojiCharacter: Bool {
        let scalars = unicodeScalars
        if scalars.count == 1, let scalar = scalars.first, scalar.isASCII,
           (scalar.properties.numericType != nil || scalar.value == 35 || scalar.value == 42) {
            return false
        }
        return scalars.contains { $0.properties.isEmoji }
    }
}

enum MessageDeliveryState: Equatable, Sendable {
    case none
    case sent
    case delivered
    case read

    var accessibilityLabel: String {
        switch self {
        case .none: ""
        case .sent: "Sent"
        case .delivered: "Delivered"
        case .read: "Read"
        }
    }
}

struct MessageContent: Codable, Hashable, Sendable {
    let type: String?
    let body: String?
    let caption: String?
    let mimeType: String?
    let mediaData: String?
    let hasMedia: Bool?
    let fileSize: Int?
    let durationSeconds: Double?
    let isVoiceNote: Bool?
    let fileName: String?
    let isAnimated: Bool?
    let latitude: Double?
    let longitude: Double?
    let name: String?
    let displayName: String?
    let vcard: String?
    let address: String?
    let question: String?
    let options: [String]?
    let rawType: String?
    let emoji: String?
    let targetMessageId: String?
    let albumId: String?
    let albumIndex: Int?
    let showTranslatedPrimary: Bool?
    let replyContext: ReplyContext?

    enum CodingKeys: String, CodingKey {
        case type, body, caption, emoji, showTranslatedPrimary, latitude, longitude, name, address, question, options, vcard
        case mimeType = "mime_type"
        case mediaData = "media_data"
        case hasMedia = "has_media"
        case fileSize = "file_size"
        case durationSeconds = "duration_seconds"
        case isVoiceNote = "is_voice_note"
        case fileName = "file_name"
        case isAnimated = "is_animated"
        case rawType = "raw_type"
        case displayName = "display_name"
        case targetMessageId = "target_message_id"
        case albumId = "album_id"
        case albumIndex = "album_index"
        case replyContext = "reply_context"
    }

    init(
        type: String?,
        body: String?,
        caption: String? = nil,
        mimeType: String? = nil,
        mediaData: String? = nil,
        hasMedia: Bool? = nil,
        fileSize: Int? = nil,
        durationSeconds: Double? = nil,
        isVoiceNote: Bool? = nil,
        fileName: String? = nil,
        isAnimated: Bool? = nil,
        latitude: Double? = nil,
        longitude: Double? = nil,
        name: String? = nil,
        displayName: String? = nil,
        vcard: String? = nil,
        address: String? = nil,
        question: String? = nil,
        options: [String]? = nil,
        rawType: String? = nil,
        emoji: String? = nil,
        targetMessageId: String? = nil,
        albumId: String? = nil,
        albumIndex: Int? = nil,
        showTranslatedPrimary: Bool?,
        replyContext: ReplyContext?
    ) {
        self.type = type
        self.body = body
        self.caption = caption
        self.mimeType = mimeType
        self.mediaData = mediaData
        self.hasMedia = hasMedia
        self.fileSize = fileSize
        self.durationSeconds = durationSeconds
        self.isVoiceNote = isVoiceNote
        self.fileName = fileName
        self.isAnimated = isAnimated
        self.latitude = latitude
        self.longitude = longitude
        self.name = name
        self.displayName = displayName
        self.vcard = vcard
        self.address = address
        self.question = question
        self.options = options
        self.rawType = rawType
        self.emoji = emoji
        self.targetMessageId = targetMessageId
        self.albumId = albumId
        self.albumIndex = albumIndex
        self.showTranslatedPrimary = showTranslatedPrimary
        self.replyContext = replyContext
    }
}

struct ReplyContext: Codable, Hashable, Sendable {
    let messageId: String?
    let senderName: String?
    let text: String?
}

struct MessageReplyTarget: Equatable, Sendable {
    let messageID: String
    let senderJID: String?
    let senderName: String
    let text: String
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

struct SendImageRequest: Encodable, Sendable {
    let contactId: String
    let mediaData: String
    let mimeType: String
    let caption: String?
    let replyTo: String?
    let replyToSender: String?
    let replyToText: String?
    let replyToSenderName: String?
}

struct OutgoingImage: Sendable {
    let data: Data
    let mimeType: String
}

struct SendImageItemRequest: Encodable, Sendable {
    let mediaData: String
    let mimeType: String
}

struct SendImagesRequest: Encodable, Sendable {
    let contactId: String
    let images: [SendImageItemRequest]
    let caption: String?
    let replyTo: String?
    let replyToSender: String?
    let replyToText: String?
    let replyToSenderName: String?
}

struct SendImageResponse: Decodable, Sendable {
    let messageId: String
    let timestamp: Int64
}

struct SendReactionRequest: Encodable, Sendable {
    let contactId: String
    let messageId: String
    let senderJid: String?
    let emoji: String
}

struct TranslateMessageRequest: Encodable, Sendable {
    let text: String
    let messageId: String
    let contactId: String
}

struct TranslateMessageResponse: Decodable, Sendable {
    let success: Bool
    let translatedText: String?
    let sourceLanguage: String?
    let error: String?
}

struct AIReplyRequest: Encodable, Sendable {
    let contactId: String
    let messageId: String
}

struct AIReplyResponse: Decodable, Sendable {
    let success: Bool
    let replyText: String?
    let error: String?
    let costUsd: Double?
}

struct UsageSummary: Decodable, Equatable, Sendable {
    let inputTokens: Int
    let cachedInputTokens: Int
    let outputTokens: Int
    let costUsd: Double
}

struct MediaResponse: Decodable, Sendable {
    let mediaData: String
    let mimeType: String

    enum CodingKeys: String, CodingKey {
        case mediaData = "media_data"
        case mimeType = "mime_type"
    }
}

struct LinkPreview: Decodable, Equatable, Sendable {
    let url: URL
    let title: String?
    let description: String?
    let imageURL: URL?
    let siteName: String?
    let error: String?

    enum CodingKeys: String, CodingKey {
        case url, title, description, siteName, error
        case imageURL = "imageUrl"
    }
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
    let messageIds: [String]?
    let status: String?

    private enum CodingKeys: String, CodingKey {
        case type
        case connected
        case message
        case status
        case chatId = "chat_id"
        case messageIds = "message_ids"
    }
}

private extension String {
    var nilIfBlank: String? { trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ? nil : self }
}
