import Foundation
@preconcurrency import Intents
@preconcurrency import UserNotifications

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
        let conversationID = intent.conversationIdentifier
        guard !recipients.isEmpty || conversationID?.isEmpty == false else {
            completion([.needsValue()])
            return
        }

        let queries = recipients.map(RecipientQuery.init)
        let completion = IntentCompletionBox(completion)
        Task { [backend] in
            do {
                let contacts = try await backend.contacts()
                if let conversationID, !conversationID.isEmpty {
                    let result: INSendMessageRecipientResolutionResult = contacts.first { $0.id == conversationID }
                        .map { .success(with: $0.intentPerson) } ?? .unsupported()
                    completion.call([result])
                    return
                }
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

    func resolveContent(for intent: INSendMessageIntent, with completion: @escaping (INStringResolutionResult) -> Void) {
        guard let content = intent.content?.trimmingCharacters(in: .whitespacesAndNewlines), !content.isEmpty else {
            completion(.needsValue())
            return
        }
        completion(.success(with: content))
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
                        identifier: MessagingMessageIdentity(contactID: contact.id, messageID: sent.messageId).encoded,
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
    private var configuration: ServerConfiguration?

    func contacts() async throws -> [Contact] {
        try await prepare()
        return try await api.contacts().filter(\.showsInChatList)
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
            let matches = recipientQueries
                .flatMap { contacts.matching($0) }
                .uniqued(by: \.id)
            contact = matches.count == 1 ? matches.first : nil
        }
        guard let contact else { throw BackendError.contactNotFound }
        let response = try await api.send(contactID: contact.id, text: content)
        return (contact, response)
    }

    func search(filter: MessageSearchFilter) async throws -> [IntentMessageResult] {
        if !filter.notificationIdentifiers.isEmpty {
            guard CredentialStore().load() != nil else { throw BackendError.notConfigured }
            let notifications = await UNUserNotificationCenter.current().deliveredNotifications()
            let snapshots = notifications.compactMap {
                MessagingIntentNotification(identifier: $0.request.identifier, content: $0.request.content, date: $0.date)
            }
            return filter.notificationResults(snapshots)
                .sorted { $0.message.timestamp < $1.message.timestamp }
        }
        let contacts = try await contacts()
        let selected = filter.selectContacts(from: contacts).prefix(12)
        let api = self.api
        let conversations = try await withThrowingTaskGroup(
            of: (Contact, [ChatMessage]).self,
            returning: [(Contact, [ChatMessage])].self
        ) { group in
            for contact in selected {
                group.addTask {
                    let response = try await api.messages(contactID: contact.id, limit: max(50, min(contact.unreadCount, 200)))
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
                filter.results(contact: contact, messages: messages)
            }
            .sorted { $0.message.timestamp > $1.message.timestamp }
            .prefix(50)
            .sorted { $0.message.timestamp < $1.message.timestamp }
            .map { $0 }
    }

    func markRead(messageIDs: Set<String>) async throws -> Bool {
        let contacts = try await contacts()
        let identities = messageIDs.compactMap(MessagingMessageIdentity.decode)
        let currentContactIDs = Set(contacts.map(\.id))
        let directContactIDs = Set(identities.map(\.contactID))
        guard directContactIDs.isSubset(of: currentContactIDs) else { return false }
        let legacyIDs = messageIDs.filter { MessagingMessageIdentity.decode($0) == nil }
        if legacyIDs.isEmpty {
            for contactID in directContactIDs { try await api.markRead(contactID: contactID) }
            await removeDeliveredNotifications(for: directContactIDs)
            return !directContactIDs.isEmpty
        }
        let api = self.api
        let matchingContactIDs = try await withThrowingTaskGroup(
            of: String?.self,
            returning: [String].self
        ) { group in
            for contact in contacts.prefix(20) {
                group.addTask {
                    let response = try await api.messages(contactID: contact.id, limit: 50)
                    return response.messages.contains { legacyIDs.contains($0.id) } ? contact.id : nil
                }
            }
            var result: [String] = []
            for try await contactID in group {
                if let contactID { result.append(contactID) }
            }
            return result
        }
        let targets = Set(matchingContactIDs).union(directContactIDs)
        for contactID in targets {
            try await api.markRead(contactID: contactID)
        }
        await removeDeliveredNotifications(for: targets)
        return !targets.isEmpty
    }

    private func prepare() async throws {
        guard let stored = CredentialStore().load() else {
            configuration = nil
            throw BackendError.notConfigured
        }
        guard stored != configuration else { return }
        await api.configure(stored)
        try await api.prepareAuthenticatedRequests()
        configuration = stored
    }

    private func removeDeliveredNotifications(for contactIDs: Set<String>) async {
        let center = UNUserNotificationCenter.current()
        let delivered = await center.deliveredNotifications()
        let identifiers = delivered.compactMap { notification -> String? in
            guard let contactID = notification.request.content.userInfo["contactId"] as? String,
                  contactIDs.contains(contactID) else { return nil }
            return notification.request.identifier
        }
        center.removeDeliveredNotifications(withIdentifiers: identifiers)
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
