#if os(iOS)
import CarPlay
import UIKit

@MainActor
final class CarPlaySceneDelegate: UIResponder, CPTemplateApplicationSceneDelegate {
    private let api = APIClient()
    private var configuration: ServerConfiguration?
    private var listTemplate: CPListTemplate?
    private var messagesTemplate: CPListTemplate?
    private var refreshTask: Task<Void, Never>?

    func templateApplicationScene(_ templateApplicationScene: CPTemplateApplicationScene, didConnect interfaceController: CPInterfaceController) {
        let template = CPListTemplate(title: "Chats", sections: [])
        template.tabTitle = "Chats"
        template.tabImage = UIImage(systemName: "person.2.fill")
        let messages = CPListTemplate(title: "Messages", sections: [])
        messages.tabTitle = "Messages"
        messages.tabImage = UIImage(systemName: "text.bubble.fill")
        messages.emptyViewTitleVariants = ["Loading messages…"]
        template.emptyViewTitleVariants = ["Loading conversations…"]
        let refresh = CPBarButton(title: "Refresh") { [weak self] _ in self?.startRefreshing() }
        template.trailingNavigationBarButtons = [refresh]
        messages.trailingNavigationBarButtons = [CPBarButton(title: "Refresh") { [weak self] _ in self?.startRefreshing() }]
        listTemplate = template
        messagesTemplate = messages
        interfaceController.setRootTemplate(CPTabBarTemplate(templates: [messages, template]), animated: false, completion: nil)
        startRefreshing()
    }

    func templateApplicationScene(_ templateApplicationScene: CPTemplateApplicationScene, didDisconnectInterfaceController interfaceController: CPInterfaceController) {
        refreshTask?.cancel()
        refreshTask = nil
        listTemplate = nil
        messagesTemplate = nil
        configuration = nil
    }

    func sceneDidBecomeActive(_ scene: UIScene) { startRefreshing() }

    func sceneWillResignActive(_ scene: UIScene) {
        refreshTask?.cancel()
        refreshTask = nil
    }

    private func startRefreshing() {
        guard listTemplate != nil else { return }
        refreshTask?.cancel()
        refreshTask = Task { [weak self] in
            while !Task.isCancelled {
                await self?.refresh()
                do { try await Task.sleep(for: .seconds(30)) }
                catch { return }
            }
        }
    }

    private func refresh() async {
        do {
            let contacts: [Contact]
            let messages: [ChatMessage]
            if ProcessInfo.processInfo.arguments.contains("-demo") {
                contacts = CarPlayConversations.demoContacts
                messages = CarPlayConversations.demoMessages
            } else {
                guard let stored = CredentialStore().load() else {
                    configuration = nil
                    showEmpty(title: "Set up Babel Bridge", subtitle: "Open the app on your iPhone when parked to connect your account.")
                    return
                }
                if stored != configuration {
                    await api.configure(stored)
                    try await api.prepareAuthenticatedRequests()
                    guard !Task.isCancelled else { return }
                    configuration = stored
                }
                async let loadedContacts = api.contacts()
                async let loadedFeed = api.feed()
                contacts = try await loadedContacts
                messages = try await loadedFeed.messages
            }
            guard !Task.isCancelled else { return }
            listTemplate?.emptyViewTitleVariants = ["No conversations yet"]
            listTemplate?.emptyViewSubtitleVariants = ["Your WhatsApp conversations will appear here."]
            listTemplate?.updateSections(CarPlayConversations.sections(contacts, limit: min(12, CPListTemplate.maximumItemCount)))
            messagesTemplate?.emptyViewTitleVariants = ["No messages yet"]
            messagesTemplate?.emptyViewSubtitleVariants = ["Messages from all your chats will appear here."]
            messagesTemplate?.updateSections(CarPlayConversations.messageSections(messages, contacts: contacts, limit: min(12, CPListTemplate.maximumItemCount)))
        } catch {
            guard !Task.isCancelled else { return }
            showEmpty(title: "Couldn’t load conversations", subtitle: "Check your connection and try Refresh.")
        }
    }

    private func showEmpty(title: String, subtitle: String) {
        for template in [listTemplate, messagesTemplate].compactMap({ $0 }) {
            template.emptyViewTitleVariants = [title]
            template.emptyViewSubtitleVariants = [subtitle]
            template.updateSections([])
        }
    }
}

@MainActor
enum CarPlayConversations {
    static func messageSections(_ messages: [ChatMessage], contacts: [Contact], limit: Int) -> [CPListSection] {
        let visible = Dictionary(contacts.filter(\.showsInChatList).map { ($0.id, $0) }, uniquingKeysWith: { first, _ in first })
        let ordered = messages.filter { visible[$0.contactId] != nil && !$0.isReaction && $0.normalizedContentType != "revoked" }
            .sorted { $0.timestamp == $1.timestamp ? $0.id > $1.id : $0.timestamp > $1.timestamp }
        var incomingCounts: [String: Int] = [:]
        let items = ordered.prefix(max(0, limit)).map { message in
            let contact = visible[message.contactId]!
            if !message.isFromMe { incomingCounts[contact.id, default: 0] += 1 }
            let unread = !message.isFromMe && incomingCounts[contact.id, default: 0] <= contact.unreadCount
            // CarPlay displays sender/context only; Siri handles message bodies and dictation.
            return CPMessageListItem(
                conversationIdentifier: MessagingMessageIdentity(contactID: contact.id, messageID: message.id).encoded,
                text: contact.displayName,
                leadingConfiguration: CPMessageListItemLeadingConfiguration(leadingItem: .none,
                    leadingImage: UIImage(systemName: contact.isGroup ? "person.2.fill" : "person.fill"), unread: unread),
                trailingConfiguration: nil,
                detailText: message.isFromMe ? "You · Sent message" : (contact.isGroup ? (message.senderName ?? "Group member") + " · Message" : "Received message"),
                trailingText: message.date.formatted(date: .omitted, time: .shortened)
            )
        }
        return items.isEmpty ? [] : [CPListSection(items: items)]
    }

    static func sections(_ contacts: [Contact], limit: Int) -> [CPListSection] {
        let ordered = AppSession.orderedContacts(contacts.filter(\.showsInChatList))
        let unread = ordered.filter { $0.unreadCount > 0 }
        let read = ordered.filter { $0.unreadCount <= 0 }
        let visible = Array((unread + read).prefix(max(0, limit)))
        return [("Unread", visible.filter { $0.unreadCount > 0 }), ("Recent", visible.filter { $0.unreadCount <= 0 })]
            .compactMap { title, contacts in
                guard !contacts.isEmpty else { return nil }
                return CPListSection(items: contacts.map(item), header: title, sectionIndexTitle: nil)
            }
    }

    static func item(_ contact: Contact) -> CPMessageListItem {
        CPMessageListItem(
            conversationIdentifier: contact.id,
            text: contact.displayName,
            leadingConfiguration: CPMessageListItemLeadingConfiguration(
                leadingItem: contact.pinnedAt == nil ? .none : .pin,
                leadingImage: UIImage(systemName: contact.isGroup ? "person.2.fill" : "person.fill"),
                unread: contact.unreadCount > 0
            ),
            trailingConfiguration: nil,
            detailText: contact.unreadCount > 0
                ? "\(contact.unreadCount) unread \(contact.unreadCount == 1 ? "message" : "messages")" : "Send a message",
            trailingText: nil
        )
    }

    static let demoContacts = [
        Contact(id: "carplay-alex@s.whatsapp.net", name: "Alex", phone: nil, type: "private", lastMessageTime: 3, unreadCount: 2, pinnedAt: nil, lastMessagePreview: nil),
        Contact(id: "carplay-family@g.us", name: "Family", phone: nil, type: "group", lastMessageTime: 2, unreadCount: 1, pinnedAt: nil, lastMessagePreview: nil),
        Contact(id: "carplay-sam@s.whatsapp.net", name: "Sam", phone: nil, type: "private", lastMessageTime: 1, unreadCount: 0, pinnedAt: nil, lastMessagePreview: nil),
    ]

    static var demoMessages: [ChatMessage] {
        let now = Int64(Date().timeIntervalSince1970 * 1_000)
        return [demoContacts[0], demoContacts[1], demoContacts[0]].enumerated().map { index, contact in
            ChatMessage(id: "carplay-demo-\(index)", contactId: contact.id, timestamp: now - Int64(index * 60_000),
                        isFromMe: false, isForwarded: false, senderName: contact.isGroup ? "Jordan" : contact.displayName,
                        senderPhone: nil, contactName: contact.displayName, contactPhone: nil, chatType: contact.type ?? "private",
                        contentType: "text", content: nil, originalText: "See you soon.", translatedText: nil,
                        sourceLanguage: nil, isTranslated: false)
        }
    }
}
#endif
