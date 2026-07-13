import Foundation
import Observation

@MainActor
@Observable
final class AppSession {
    enum Phase: Equatable {
        case restoring
        case needsConfiguration
        case connecting
        case ready
    }

    var phase: Phase = .restoring
    var configuration: ServerConfiguration?
    var backendStatus = BackendStatus(connected: false, phone: nil, name: nil)
    var contacts: [Contact] = []
    var avatarURLs: [String: URL] = [:]
    var messages: [String: [ChatMessage]] = [:]
    var messageHistoryHasMore: [String: Bool] = [:]
    var selectedContactID: String?
    var searchText = ""
    var errorMessage: String?
    var isRefreshing = false
    var sendingContactIDs: Set<String> = []

    let draftStore: DraftStore
    private let api: APIClient
    private let credentials: CredentialStore
    private let cache: ChatCacheStore
    private let demoMode: Bool
    private let demoConversationMode: Bool
    private var avatarRequests: Set<String> = []
    private var isConnecting = false

    init(
        api: APIClient = APIClient(),
        credentials: CredentialStore = CredentialStore(),
        cache: ChatCacheStore = .shared,
        draftStore: DraftStore = DraftStore(),
        demoMode: Bool = ProcessInfo.processInfo.arguments.contains("-demo")
    ) {
        self.api = api
        self.credentials = credentials
        self.cache = cache
        self.draftStore = draftStore
        self.demoMode = demoMode
        self.demoConversationMode = ProcessInfo.processInfo.arguments.contains("-demoConversation")
    }

    var filteredContacts: [Contact] {
        let query = searchText.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !query.isEmpty else { return contacts }
        return contacts.filter {
            $0.displayName.localizedCaseInsensitiveContains(query)
                || ($0.lastMessagePreview?.localizedCaseInsensitiveContains(query) ?? false)
        }
    }

    func start() async {
        if demoMode {
            loadDemoData()
            return
        }
        guard let stored = credentials.load() else {
            phase = .needsConfiguration
            return
        }
        await connect(configuration: stored, remember: false, restoreCached: true)
    }

    func connect(address: String, password: String) async {
        do {
            let configuration = try ServerConfiguration.make(address: address, password: password)
            await connect(configuration: configuration, remember: true)
        } catch {
            errorMessage = error.localizedDescription
            phase = .needsConfiguration
        }
    }

    func refresh() async {
        guard !demoMode else { return }
        isRefreshing = true
        defer { isRefreshing = false }
        do {
            async let status = api.status()
            async let contacts = api.contacts()
            backendStatus = try await status
            self.contacts = try await contacts
            await persistCache()
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func loadMessages(for contactID: String, older: Bool = false) async {
        if demoMode { return }
        do {
            let current = messages[contactID] ?? []
            let oldest = older ? current.first : nil
            let response = try await api.messages(
                contactID: contactID,
                before: oldest?.timestamp,
                beforeID: oldest?.id
            )
            messages[contactID] = older ? merge(response.messages + current) : response.messages
            messageHistoryHasMore[contactID] = response.hasMore
            if let index = contacts.firstIndex(where: { $0.id == contactID }) {
                contacts[index].unreadCount = 0
            }
            try? await api.markRead(contactID: contactID)
            await persistCache()
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func loadAvatar(for contactID: String) async {
        guard !demoMode,
              avatarURLs[contactID] == nil,
              !avatarRequests.contains(contactID) else { return }
        avatarRequests.insert(contactID)
        defer { avatarRequests.remove(contactID) }
        if let url = try? await api.avatar(contactID: contactID) {
            avatarURLs[contactID] = url
        }
    }

    func send(text: String, to contactID: String) async -> Bool {
        let clean = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !clean.isEmpty else { return false }
        sendingContactIDs.insert(contactID)
        defer { sendingContactIDs.remove(contactID) }

        if demoMode {
            let message = ChatMessage.demoOutgoing(contactID: contactID, text: clean)
            messages[contactID, default: []].append(message)
            updateContactPreview(contactID: contactID, preview: "You: \(clean)", timestamp: message.timestamp)
            return true
        }

        do {
            _ = try await api.send(contactID: contactID, text: clean)
            await loadMessages(for: contactID)
            await refresh()
            return true
        } catch {
            errorMessage = error.localizedDescription
            return false
        }
    }

    func conversationSettings(for contactID: String) async throws -> ConversationSettings {
        if demoMode {
            return ConversationSettings(languageOverride: "Spanish", translationStyle: "Friendly", sendOriginalFollowUp: true)
        }
        return try await api.conversationSettings(contactID: contactID)
    }

    func saveConversationSettings(_ settings: ConversationSettings, for contactID: String) async throws {
        guard !demoMode else { return }
        try await api.updateConversationSettings(contactID: contactID, settings: settings)
    }

    func openAISettings() async throws -> OpenAISettings {
        if demoMode { return OpenAISettings(model: "gpt-5.6-terra", reasoningEffort: "medium", available: true) }
        return try await api.openAISettings()
    }

    func saveOpenAISettings(_ settings: OpenAISettings) async throws {
        guard !demoMode else { return }
        try await api.updateOpenAISettings(settings)
    }

    func forgetServer() {
        Task {
            await api.disconnectLiveEvents()
            await cache.clear()
        }
        credentials.clear()
        configuration = nil
        contacts = []
        avatarURLs = [:]
        avatarRequests = []
        messages = [:]
        selectedContactID = nil
        phase = .needsConfiguration
    }

    private func connect(
        configuration: ServerConfiguration,
        remember: Bool,
        restoreCached: Bool = false
    ) async {
        isConnecting = true
        defer { isConnecting = false }
        errorMessage = nil
        self.configuration = configuration
        await api.configure(configuration)
        var restoredCache = false
        if restoreCached {
            restoredCache = await restoreCachedState()
        }
        phase = restoredCache ? .ready : .connecting
        do {
            backendStatus = try await api.authenticate()
            if remember { try credentials.save(configuration) }
            contacts = try await api.contacts()
            phase = .ready
            await persistCache()
            await PushNotificationCoordinator.shared.activate(for: self)
            try await api.connectLiveEvents { [weak self] event in
                self?.handle(event)
            }
        } catch {
            errorMessage = error.localizedDescription
            phase = restoredCache ? .ready : .needsConfiguration
        }
    }

    func restoreCachedState() async -> Bool {
        guard !demoMode,
              let configuration,
              let snapshot = await cache.load(for: configuration) else {
            return false
        }
        contacts = snapshot.contacts
        messages = snapshot.messages
        return true
    }

    func becameActive() async {
        guard phase == .ready, !demoMode, !isConnecting else { return }
        _ = await restoreCachedState()
        await refresh()
    }

    func registerPushDevice(token: String, installationID: String, environment: String) async {
        guard phase == .ready, !demoMode else { return }
        do {
            try await api.registerPushDevice(
                PushDeviceRegistration(
                    installationId: installationID,
                    token: token,
                    environment: environment
                )
            )
        } catch {
            #if DEBUG
            print("Push notification registration failed: \(error.localizedDescription)")
            #endif
        }
    }

    func openConversation(fromNotification contactID: String) {
        selectedContactID = contactID
        Task {
            await refresh()
            selectedContactID = contactID
            await loadMessages(for: contactID)
        }
    }

    private func handle(_ event: LiveEvent) {
        switch event.type {
        case "message":
            guard let message = event.message else { return }
            messages[message.contactId] = merge((messages[message.contactId] ?? []) + [message])
            updateContactPreview(
                contactID: message.contactId,
                preview: message.displayText,
                timestamp: message.timestamp
            )
            persistCacheSoon()
        case "status":
            if let connected = event.connected {
                backendStatus = BackendStatus(connected: connected, phone: backendStatus.phone, name: backendStatus.name)
            }
        case "mark_as_read":
            if let id = event.chatId, let index = contacts.firstIndex(where: { $0.id == id }) {
                contacts[index].unreadCount = 0
                persistCacheSoon()
            }
        default:
            break
        }
    }

    private func merge(_ values: [ChatMessage]) -> [ChatMessage] {
        Array(Dictionary(grouping: values, by: \ChatMessage.id).compactMap { $0.value.last })
            .sorted { ($0.timestamp, $0.id) < ($1.timestamp, $1.id) }
    }

    private func updateContactPreview(contactID: String, preview: String, timestamp: Int64) {
        guard let index = contacts.firstIndex(where: { $0.id == contactID }) else { return }
        let old = contacts[index]
        contacts[index] = Contact(
            id: old.id,
            name: old.name,
            phone: old.phone,
            type: old.type,
            lastMessageTime: timestamp,
            unreadCount: old.unreadCount,
            pinnedAt: old.pinnedAt,
            lastMessagePreview: preview
        )
        contacts.sort {
            if ($0.pinnedAt != nil) != ($1.pinnedAt != nil) {
                return $0.pinnedAt != nil
            }
            return $0.lastMessageTime > $1.lastMessageTime
        }
    }

    private func persistCache() async {
        guard let snapshot = cacheSnapshot() else { return }
        await cache.save(snapshot)
    }

    private func persistCacheSoon() {
        guard let snapshot = cacheSnapshot() else { return }
        Task { await cache.save(snapshot) }
    }

    private func cacheSnapshot() -> ChatCacheSnapshot? {
        guard let configuration else { return nil }
        let trimmedMessages = messages.mapValues {
            Array($0.suffix(ChatCacheSnapshot.maximumMessagesPerChat))
        }
        return ChatCacheSnapshot(
            serverBaseURL: configuration.baseURL.absoluteString,
            contacts: contacts,
            messages: trimmedMessages,
            updatedAt: Date()
        )
    }

    private func loadDemoData() {
        configuration = try? ServerConfiguration.make(address: "https://translator.example.com", password: "demo")
        backendStatus = BackendStatus(connected: true, phone: "447853803055", name: "Simon Skinner")
        contacts = Contact.demoContacts
        messages = ChatMessage.demoMessages
        if demoConversationMode {
            selectedContactID = contacts.first?.id
            if ProcessInfo.processInfo.arguments.contains("-demoSending"),
               let selectedContactID {
                sendingContactIDs.insert(selectedContactID)
            }
        }
        phase = .ready
    }
}

private extension Contact {
    static let demoContacts = [
        Contact(id: "virag@s.whatsapp.net", name: "Virág Skinner", phone: "+36 30 555 0142", type: "private", lastMessageTime: 1_783_940_400_000, unreadCount: 2, pinnedAt: 1, lastMessagePreview: "Perfect, I’ll be there just after six."),
        Contact(id: "family@g.us", name: "The Skinners", phone: nil, type: "group", lastMessageTime: 1_783_936_860_000, unreadCount: 0, pinnedAt: nil, lastMessagePreview: "Eileen: Photo"),
        Contact(id: "anyu@s.whatsapp.net", name: "Anyu", phone: "+36 20 555 0181", type: "private", lastMessageTime: 1_783_850_000_000, unreadCount: 1, pinnedAt: nil, lastMessagePreview: "Nagyon köszönöm ❤️"),
        Contact(id: "work@g.us", name: "AFX / RCX Hedge Funds", phone: nil, type: "group", lastMessageTime: 1_783_760_000_000, unreadCount: 0, pinnedAt: nil, lastMessagePreview: "Will check it this afternoon."),
    ]
}

private extension ChatMessage {
    static let demoMessages: [String: [ChatMessage]] = [
        "virag@s.whatsapp.net": [
            demo(id: "1", contactID: "virag@s.whatsapp.net", timestamp: 1_783_939_620_000, fromMe: false, body: "Mikor érkezel?", translated: "What time will you arrive?", sender: "Virág"),
            demo(id: "2", contactID: "virag@s.whatsapp.net", timestamp: 1_783_939_740_000, fromMe: true, body: "Probably just after six — I’ll message when I leave.", translated: "Valószínűleg nem sokkal hat után — írok, amikor elindulok.", sender: nil),
            demo(id: "3", contactID: "virag@s.whatsapp.net", timestamp: 1_783_940_400_000, fromMe: false, body: "Tökéletes, akkor hat után várlak.", translated: "Perfect, I’ll be there just after six.", sender: "Virág"),
        ]
    ]

    static func demoOutgoing(contactID: String, text: String) -> ChatMessage {
        demo(id: UUID().uuidString, contactID: contactID, timestamp: Int64(Date().timeIntervalSince1970 * 1_000), fromMe: true, body: text, translated: nil, sender: nil)
    }

    static func demo(id: String, contactID: String, timestamp: Int64, fromMe: Bool, body: String, translated: String?, sender: String?) -> ChatMessage {
        ChatMessage(
            id: id,
            contactId: contactID,
            timestamp: timestamp,
            isFromMe: fromMe,
            isForwarded: false,
            senderName: sender,
            senderPhone: nil,
            contactName: nil,
            contactPhone: nil,
            chatType: "private",
            contentType: "Text",
            content: MessageContent(type: "text", body: body, showTranslatedPrimary: nil, replyContext: nil),
            originalText: translated == nil ? nil : body,
            translatedText: translated,
            sourceLanguage: translated == nil ? nil : "Hungarian",
            isTranslated: translated != nil
        )
    }
}
