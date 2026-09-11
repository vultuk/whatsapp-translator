import Foundation
@preconcurrency import Intents
@preconcurrency import UserNotifications

struct RecipientQuery: Sendable {
    let handle: String?
    let displayName: String
    let customIdentifier: String?

    init(_ person: INPerson) {
        handle = person.personHandle?.value
        displayName = person.displayName
        customIdentifier = person.customIdentifier
    }

    var candidates: [String] {
        [customIdentifier, handle, displayName].compactMap { value in
            guard let normalized = value?.normalizedSearchValue, !normalized.isEmpty else { return nil }
            return normalized
        }
    }
}

struct MessageSearchFilter: Sendable {
    let conversationIDs: Set<String>
    let personQueries: [String]
    let groupNames: [String]
    let terms: [String]
    let identifiers: Set<String>
    let notificationIdentifiers: Set<String>
    let attributes: INMessageAttributeOptions
    let startDate: Date?
    let endDate: Date?

    init(intent: INSearchForMessagesIntent) {
        conversationIDs = Set(intent.conversationIdentifiers ?? [])
        personQueries = ((intent.senders ?? []) + (intent.recipients ?? []))
            .flatMap { RecipientQuery($0).candidates }
        groupNames = (intent.speakableGroupNames ?? []).map { $0.spokenPhrase.normalizedSearchValue }
        terms = (intent.searchTerms ?? []).map(\.normalizedSearchValue)
        identifiers = Set(intent.identifiers ?? [])
        notificationIdentifiers = Set(intent.notificationIdentifiers ?? [])
        attributes = intent.attributes
        startDate = intent.dateTimeRange?.startDateComponents?.date
        endDate = intent.dateTimeRange?.endDateComponents?.date
    }

    func selectContacts(from contacts: [Contact]) -> [Contact] {
        var contacts = contacts.filter(\.showsInChatList)
        let explicitIDs = conversationIDs.union(identifiers.compactMap { MessagingMessageIdentity.decode($0)?.contactID })
        if !explicitIDs.isEmpty {
            return contacts.filter { explicitIDs.contains($0.id) }
        }
        if attributes.contains(.unread) && !attributes.contains(.read) {
            contacts = contacts.filter { $0.unreadCount > 0 }
        }
        guard !conversationIDs.isEmpty || !personQueries.isEmpty || !groupNames.isEmpty else {
            return contacts
        }
        return contacts.filter { contact in
            conversationIDs.contains(contact.id)
                || personQueries.contains(where: contact.matches)
                || groupNames.contains(where: contact.matches)
        }
    }

    func includes(_ result: IntentMessageResult) -> Bool {
        let message = result.message
        if message.isReaction || message.normalizedContentType == "revoked" { return false }
        if !conversationIDs.isEmpty, !conversationIDs.contains(result.contact.id) { return false }
        if !identifiers.isEmpty,
           !identifiers.contains(message.id),
           !identifiers.contains(MessagingMessageIdentity(contactID: result.contact.id, messageID: message.id).encoded) { return false }
        if attributes.contains(.unread), !attributes.contains(.read), !result.isUnread { return false }
        if attributes.contains(.read), !attributes.contains(.unread), result.isUnread { return false }
        if let startDate, message.date < startDate { return false }
        if let endDate, message.date > endDate { return false }
        if !terms.isEmpty {
            let text = message.displayText.normalizedSearchValue
            guard terms.allSatisfy(text.contains) else { return false }
        }
        return true
    }

    func results(contact: Contact, messages: [ChatMessage]) -> [IntentMessageResult] {
        let incoming = messages.filter { !$0.isFromMe && !$0.isReaction && $0.normalizedContentType != "revoked" }
            .sorted { $0.timestamp == $1.timestamp ? $0.id > $1.id : $0.timestamp > $1.timestamp }
        // The backend exposes a conversation unread count rather than per-message flags.
        let unreadIDs = Set(incoming.prefix(max(0, contact.unreadCount)).map(\.id))
        return messages.map { IntentMessageResult(contact: contact, message: $0, isUnread: unreadIDs.contains($0.id)) }
            .filter(includes)
    }

    func notificationResults(_ notifications: [MessagingIntentNotification]) -> [IntentMessageResult] {
        // An expired or withdrawn alert must never fall back to unrelated messages.
        notifications.filter { notificationIdentifiers.contains($0.identifier) }
            .map(\.result)
            .filter(includes)
    }
}

struct IntentMessageResult: Sendable {
    let contact: Contact
    let message: ChatMessage
    var isUnread = false

    var intentMessage: INMessage {
        let sender = message.isFromMe
            ? INPerson.currentUser
            : INPerson.messagingPerson(
                id: message.senderPhone ?? contact.id,
                name: message.senderName ?? contact.displayName
            )
        return INMessage(
            identifier: MessagingMessageIdentity(contactID: contact.id, messageID: message.id).encoded,
            conversationIdentifier: contact.id,
            content: message.displayText,
            dateSent: message.date,
            sender: sender,
            recipients: message.isFromMe ? [contact.intentPerson] : [.currentUser],
            groupName: contact.isGroup ? INSpeakableString(spokenPhrase: contact.displayName) : nil,
            messageType: message.intentMessageType,
            serviceName: "Babel Bridge"
        )
    }
}

extension Contact {
    var intentPerson: INPerson {
        .messagingPerson(id: id, name: displayName)
    }

    func matches(_ value: String) -> Bool {
        let candidate = value.normalizedSearchValue
        return id.normalizedSearchValue == candidate
            || displayName.normalizedSearchValue.contains(candidate)
            || phone?.normalizedSearchValue.contains(candidate) == true
    }
}

extension Array where Element == Contact {
    func matching(_ query: RecipientQuery) -> [Contact] {
        let contacts = filter(\.showsInChatList)
        // Stable identities take precedence over similar or duplicate display names.
        if let id = query.customIdentifier, !id.isEmpty {
            return contacts.filter { $0.id == id }
        }
        if let handle = query.handle {
            let exact = contacts.filter { $0.id == handle || $0.phone?.normalizedSearchValue == handle.normalizedSearchValue }
            if !exact.isEmpty { return exact }
        }
        return contacts.filter { contact in query.candidates.contains(where: contact.matches) }
    }
}

struct MessagingMessageIdentity: Codable, Equatable, Sendable {
    let contactID: String
    let messageID: String

    var encoded: String {
        let data = try! JSONEncoder().encode([contactID, messageID])
        return "babelbridge.message:" + data.base64EncodedString()
    }

    static func decode(_ value: String) -> Self? {
        let prefix = "babelbridge.message:"
        guard value.hasPrefix(prefix),
              let data = Data(base64Encoded: String(value.dropFirst(prefix.count))),
              let values = try? JSONDecoder().decode([String].self, from: data),
              values.count == 2, !values[0].isEmpty, !values[1].isEmpty else { return nil }
        return Self(contactID: values[0], messageID: values[1])
    }
}

struct MessagingIntentNotification: Sendable {
    let identifier: String
    let result: IntentMessageResult

    init?(identifier: String, content: UNNotificationContent, date: Date) {
        let info = content.userInfo
        guard let contactID = info["contactId"] as? String, !contactID.isEmpty,
              let messageID = info["messageId"] as? String, !messageID.isEmpty else { return nil }
        let group = info["chatType"] as? String == "group" || contactID.hasSuffix("@g.us")
        let sender = (info["senderName"] as? String) ?? content.title
        let name = group ? ((info["conversationName"] as? String) ?? content.subtitle) : sender
        let timestamp = Int64(date.timeIntervalSince1970 * 1_000)
        let contact = Contact(id: contactID, name: name, phone: nil, type: group ? "group" : "private", lastMessageTime: timestamp, unreadCount: 1, pinnedAt: nil, lastMessagePreview: nil)
        guard contact.showsInChatList else { return nil }
        let message = ChatMessage(
            id: messageID, contactId: contactID, timestamp: timestamp,
            isFromMe: false, isForwarded: false, senderName: sender,
            senderPhone: info["senderId"] as? String, contactName: name,
            contactPhone: nil, chatType: group ? "group" : "private", contentType: "text",
            content: nil, originalText: nil, translatedText: content.body,
            sourceLanguage: nil, isTranslated: true
        )
        self.identifier = identifier
        result = IntentMessageResult(contact: contact, message: message, isUnread: true)
    }
}

extension INPerson {
    static var currentUser: INPerson {
        INPerson(
            personHandle: INPersonHandle(value: "current-user", type: .unknown),
            nameComponents: nil,
            displayName: "You",
            image: nil,
            contactIdentifier: nil,
            customIdentifier: "current-user",
            isMe: true
        )
    }

    static func messagingPerson(id: String, name: String) -> INPerson {
        INPerson(
            personHandle: INPersonHandle(value: id, type: .unknown),
            nameComponents: nil,
            displayName: name,
            image: nil,
            contactIdentifier: nil,
            customIdentifier: id,
            isContactSuggestion: true,
            suggestionType: .instantMessageAddress
        )
    }
}

extension ChatMessage {
    var intentMessageType: INMessageType {
        switch normalizedContentType {
        case "audio": .audio
        case "image": .mediaImage
        case "video": .mediaVideo
        case "location": .mediaLocation
        case "contact": .mediaAddressCard
        case "reaction": .reaction
        case "sticker": .sticker
        case "document": .file
        default: .text
        }
    }
}

extension String {
    var normalizedSearchValue: String {
        folding(options: [.caseInsensitive, .diacriticInsensitive], locale: .current)
            .trimmingCharacters(in: .whitespacesAndNewlines)
            .lowercased()
    }
}

extension Array {
    func uniqued<Key: Hashable>(by keyPath: KeyPath<Element, Key>) -> [Element] {
        var seen: Set<Key> = []
        return filter { seen.insert($0[keyPath: keyPath]).inserted }
    }
}
