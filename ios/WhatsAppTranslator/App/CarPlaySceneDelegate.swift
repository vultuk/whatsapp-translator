#if os(iOS)
import CarPlay
import UIKit

@MainActor
final class CarPlaySceneDelegate: UIResponder, CPTemplateApplicationSceneDelegate {
    private let api = APIClient()
    private var configuration: ServerConfiguration?
    private var listTemplate: CPListTemplate?
    private var refreshTask: Task<Void, Never>?

    func templateApplicationScene(_ templateApplicationScene: CPTemplateApplicationScene, didConnect interfaceController: CPInterfaceController) {
        let template = CPListTemplate(title: "Babel Bridge", sections: [])
        template.emptyViewTitleVariants = ["Loading conversations…"]
        let refresh = CPBarButton(title: "Refresh") { [weak self] _ in self?.startRefreshing() }
        template.trailingNavigationBarButtons = [refresh]
        listTemplate = template
        interfaceController.setRootTemplate(template, animated: false, completion: nil)
        startRefreshing()
    }

    func templateApplicationScene(_ templateApplicationScene: CPTemplateApplicationScene, didDisconnectInterfaceController interfaceController: CPInterfaceController) {
        refreshTask?.cancel()
        refreshTask = nil
        listTemplate = nil
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
            if ProcessInfo.processInfo.arguments.contains("-demo") {
                contacts = CarPlayConversations.demoContacts
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
                contacts = try await api.contacts()
            }
            guard !Task.isCancelled else { return }
            listTemplate?.emptyViewTitleVariants = ["No conversations yet"]
            listTemplate?.emptyViewSubtitleVariants = ["Your WhatsApp conversations will appear here."]
            listTemplate?.updateSections(CarPlayConversations.sections(contacts, limit: min(12, CPListTemplate.maximumItemCount)))
        } catch {
            guard !Task.isCancelled else { return }
            showEmpty(title: "Couldn’t load conversations", subtitle: "Check your connection and try Refresh.")
        }
    }

    private func showEmpty(title: String, subtitle: String) {
        listTemplate?.emptyViewTitleVariants = [title]
        listTemplate?.emptyViewSubtitleVariants = [subtitle]
        listTemplate?.updateSections([])
    }
}

@MainActor
enum CarPlayConversations {
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
}
#endif
