import Foundation
@preconcurrency import Intents

final class IntentHandler: INExtension, INSendMessageIntentHandling, INSearchForMessagesIntentHandling, INSetMessageAttributeIntentHandling, @unchecked Sendable {
    private let backend = MessagingIntentBackend()

    override func handler(for intent: INIntent) -> Any {
        self
    }

    func resolveRecipients(
        for intent: INSendMessageIntent,
        with completion: @escaping ([INSendMessageRecipientResolutionResult]) -> Void
    ) {
        let recipients = intent.recipients ?? []
        guard !recipients.isEmpty else {
            completion([.needsValue()])
            return
        }

        let queries = recipients.map(RecipientQuery.init)
        let completion = IntentCompletionBox(completion)
        Task { [backend] in
            do {
                let contacts = try await backend.contacts()
                let results = queries.map { query -> INSendMessageRecipientResolutionResult in
                    let matches = contacts.matching(query)
                    if matches.count == 1, let contact = matches.first {
                        return .success(with: contact.intentPerson)
                    }
                    if matches.count > 1 {
                        return .disambiguation(with: matches.map(\.intentPerson))
                    }
                    return .unsupported()
                }
                completion.call(results)
            } catch {
                completion.call(queries.map { _ in .unsupported() })
            }
        }
    }

    func handle(
        intent: INSendMessageIntent,
        completion: @escaping (INSendMessageIntentResponse) -> Void
    ) {
        let content = intent.content?.trimmingCharacters(in: .whitespacesAndNewlines)
        let conversationID = intent.conversationIdentifier
        let queries = (intent.recipients ?? []).map(RecipientQuery.init)
        let completion = IntentCompletionBox(completion)

        guard let content, !content.isEmpty else {
            completion.call(INSendMessageIntentResponse(code: .failure, userActivity: nil))
            return
        }

        Task { [backend] in
            do {
                let (contact, sent) = try await backend.send(
                    content: content,
                    conversationID: conversationID,
                    recipientQueries: queries
                )
                let response = INSendMessageIntentResponse(code: .success, userActivity: nil)
                response.sentMessages = [
                    INMessage(
                        identifier: sent.messageId,
                        conversationIdentifier: contact.id,
                        content: sent.translatedText ?? content,
                        dateSent: Date(timeIntervalSince1970: TimeInterval(sent.timestamp) / 1_000),
                        sender: .currentUser,
                        recipients: [contact.intentPerson],
                        groupName: contact.isGroup ? INSpeakableString(spokenPhrase: contact.displayName) : nil,
                        messageType: .text,
                        serviceName: "Babel Bridge"
                    ),
                ]
                completion.call(response)
            } catch MessagingIntentBackend.BackendError.notConfigured {
                completion.call(INSendMessageIntentResponse(
                    code: .failureRequiringInAppAuthentication,
                    userActivity: nil
                ))
            } catch MessagingIntentBackend.BackendError.contactNotFound {
                completion.call(INSendMessageIntentResponse(code: .failure, userActivity: nil))
            } catch {
                completion.call(INSendMessageIntentResponse(
                    code: .failureMessageServiceNotAvailable,
                    userActivity: nil
                ))
            }
        }
    }

    func handle(
        intent: INSearchForMessagesIntent,
        completion: @escaping (INSearchForMessagesIntentResponse) -> Void
    ) {
        let filter = MessageSearchFilter(intent: intent)
        let completion = IntentCompletionBox(completion)
        Task { [backend] in
            do {
                let messages = try await backend.search(filter: filter)
                let response = INSearchForMessagesIntentResponse(code: .success, userActivity: nil)
                response.messages = messages.map { $0.intentMessage }
                completion.call(response)
            } catch MessagingIntentBackend.BackendError.notConfigured {
                completion.call(INSearchForMessagesIntentResponse(
                    code: .failureRequiringInAppAuthentication,
                    userActivity: nil
                ))
            } catch {
                completion.call(INSearchForMessagesIntentResponse(
                    code: .failureMessageServiceNotAvailable,
                    userActivity: nil
                ))
            }
        }
    }

    func handle(
        intent: INSetMessageAttributeIntent,
        completion: @escaping (INSetMessageAttributeIntentResponse) -> Void
    ) {
        let identifiers = Set(intent.identifiers ?? [])
        let attribute = intent.attribute
        let completion = IntentCompletionBox(completion)
        guard !identifiers.isEmpty, attribute == .read || attribute == .played else {
            completion.call(INSetMessageAttributeIntentResponse(
                code: .failureMessageAttributeNotSet,
                userActivity: nil
            ))
            return
        }

        Task { [backend] in
            do {
                let found = try await backend.markRead(messageIDs: identifiers)
                completion.call(INSetMessageAttributeIntentResponse(
                    code: found ? .success : .failureMessageNotFound,
                    userActivity: nil
                ))
            } catch {
                completion.call(INSetMessageAttributeIntentResponse(code: .failure, userActivity: nil))
            }
        }
    }
}

private actor MessagingIntentBackend {
    enum BackendError: Error {
        case notConfigured
        case contactNotFound
    }

    private let api = APIClient()
    private var prepared = false

    func contacts() async throws -> [Contact] {
        try await prepare()
        return try await api.contacts()
    }

    func send(
        content: String,
        conversationID: String?,
        recipientQueries: [RecipientQuery]
    ) async throws -> (Contact, SendMessageResponse) {
        let contacts = try await contacts()
        let contact: Contact?
        if let conversationID, !conversationID.isEmpty {
            contact = contacts.first { $0.id == conversationID }
        } else {
            contact = recipientQueries
                .flatMap { contacts.matching($0) }
                .uniqued(by: \.id)
                .first
        }
        guard let contact else { throw BackendError.contactNotFound }
        let response = try await api.send(contactID: contact.id, text: content)
        return (contact, response)
    }

    func search(filter: MessageSearchFilter) async throws -> [IntentMessageResult] {
        let contacts = try await contacts()
        let selected = filter.selectContacts(from: contacts).prefix(12)
        let api = self.api
        let conversations = try await withThrowingTaskGroup(
            of: (Contact, [ChatMessage]).self,
            returning: [(Contact, [ChatMessage])].self
        ) { group in
            for contact in selected {
                group.addTask {
                    let response = try await api.messages(contactID: contact.id, limit: 20)
                    return (contact, response.messages)
                }
            }
            var results: [(Contact, [ChatMessage])] = []
            for try await result in group {
                results.append(result)
            }
            return results
        }

        return conversations
            .flatMap { contact, messages in
                messages.map { IntentMessageResult(contact: contact, message: $0) }
            }
            .filter(filter.includes)
            .sorted { $0.message.timestamp > $1.message.timestamp }
            .prefix(50)
            .map { $0 }
    }

    func markRead(messageIDs: Set<String>) async throws -> Bool {
        let contacts = try await contacts()
        let api = self.api
        let matchingContactIDs = try await withThrowingTaskGroup(
            of: String?.self,
            returning: [String].self
        ) { group in
            for contact in contacts.prefix(20) {
                group.addTask {
                    let response = try await api.messages(contactID: contact.id, limit: 50)
                    return response.messages.contains { messageIDs.contains($0.id) } ? contact.id : nil
                }
            }
            var result: [String] = []
            for try await contactID in group {
                if let contactID { result.append(contactID) }
            }
            return result
        }
        for contactID in matchingContactIDs {
            try await api.markRead(contactID: contactID)
        }
        return !matchingContactIDs.isEmpty
    }

    private func prepare() async throws {
        guard !prepared else { return }
        guard let configuration = CredentialStore().load() else {
            throw BackendError.notConfigured
        }
        await api.configure(configuration)
        try await api.prepareAuthenticatedRequests()
        prepared = true
    }
}

private struct RecipientQuery: Sendable {
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

private struct MessageSearchFilter: Sendable {
    let conversationIDs: Set<String>
    let personQueries: [String]
    let groupNames: [String]
    let terms: [String]
    let identifiers: Set<String>
    let startDate: Date?
    let endDate: Date?

    init(intent: INSearchForMessagesIntent) {
        conversationIDs = Set(intent.conversationIdentifiers ?? [])
        personQueries = ((intent.senders ?? []) + (intent.recipients ?? []))
            .flatMap { RecipientQuery($0).candidates }
        groupNames = (intent.speakableGroupNames ?? []).map { $0.spokenPhrase.normalizedSearchValue }
        terms = (intent.searchTerms ?? []).map(\.normalizedSearchValue)
        identifiers = Set(intent.identifiers ?? [])
        startDate = intent.dateTimeRange?.startDateComponents?.date
        endDate = intent.dateTimeRange?.endDateComponents?.date
    }

    func selectContacts(from contacts: [Contact]) -> [Contact] {
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
        if !identifiers.isEmpty, !identifiers.contains(message.id) { return false }
        if let startDate, message.date < startDate { return false }
        if let endDate, message.date > endDate { return false }
        if !terms.isEmpty {
            let text = message.displayText.normalizedSearchValue
            guard terms.allSatisfy(text.contains) else { return false }
        }
        return true
    }
}

private struct IntentMessageResult: Sendable {
    let contact: Contact
    let message: ChatMessage

    var intentMessage: INMessage {
        let sender = message.isFromMe
            ? INPerson.currentUser
            : INPerson.messagingPerson(
                id: message.senderPhone ?? contact.id,
                name: message.senderName ?? contact.displayName
            )
        return INMessage(
            identifier: message.id,
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

private final class IntentCompletionBox<Value>: @unchecked Sendable {
    private let completion: (Value) -> Void

    init(_ completion: @escaping (Value) -> Void) {
        self.completion = completion
    }

    func call(_ value: Value) {
        completion(value)
    }
}

private extension Contact {
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

private extension Array where Element == Contact {
    func matching(_ query: RecipientQuery) -> [Contact] {
        filter { contact in query.candidates.contains(where: contact.matches) }
    }
}

private extension INPerson {
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

private extension ChatMessage {
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

private extension String {
    var normalizedSearchValue: String {
        folding(options: [.caseInsensitive, .diacriticInsensitive], locale: .current)
            .trimmingCharacters(in: .whitespacesAndNewlines)
            .lowercased()
    }
}

private extension Array {
    func uniqued<Key: Hashable>(by keyPath: KeyPath<Element, Key>) -> [Element] {
        var seen: Set<Key> = []
        return filter { seen.insert($0[keyPath: keyPath]).inserted }
    }
}
