import Foundation
import Observation
import UniformTypeIdentifiers

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
    var errorTitle = "Something went wrong"
    var errorMessage: String?
    var isRefreshing = false
    var sendingContactIDs: Set<String> = []
    var activeMessageActionIDs: Set<String> = []
    var messageImages: [String: PlatformImage] = [:]
    var messageMediaURLs: [String: URL] = [:]
    var mediaLoadingIDs: Set<String> = []
    var mediaErrorIDs: Set<String> = []
    var loadingCompleteHistoryContactIDs: Set<String> = []
    var linkPreviews: [URL: LinkPreview] = [:]

    let draftStore: DraftStore
    let preferences: AppPreferencesStore
    private let api: APIClient
    private let credentials: CredentialStore
    private let cache: ChatCacheStore
    private let demoMode: Bool
    private let demoConversationMode: Bool
    private var avatarRequests: Set<String> = []
    private var mediaRequests: Set<String> = []
    private var linkPreviewRequests: Set<URL> = []
    private var isConnecting = false

    init(
        api: APIClient = APIClient(),
        credentials: CredentialStore = CredentialStore(),
        cache: ChatCacheStore = .shared,
        draftStore: DraftStore = DraftStore(),
        preferences: AppPreferencesStore = AppPreferencesStore(),
        demoMode: Bool = ProcessInfo.processInfo.arguments.contains("-demo")
    ) {
        self.api = api
        self.credentials = credentials
        self.cache = cache
        self.draftStore = draftStore
        self.preferences = preferences
        self.demoMode = demoMode
        self.demoConversationMode = ProcessInfo.processInfo.arguments.contains("-demoConversation")
    }

    var filteredContacts: [Contact] {
        let query = searchText.trimmingCharacters(in: .whitespacesAndNewlines)
        let ordered = Self.orderedContacts(contacts)
        guard !query.isEmpty else { return ordered }
        return ordered.filter {
            displayName(for: $0).localizedCaseInsensitiveContains(query)
                || ($0.lastMessagePreview?.localizedCaseInsensitiveContains(query) ?? false)
        }
    }

    static func orderedContacts(_ contacts: [Contact]) -> [Contact] {
        contacts.sorted { first, second in
            switch (first.pinnedAt, second.pinnedAt) {
            case let (firstPinned?, secondPinned?):
                if firstPinned != secondPinned { return firstPinned < secondPinned }
            case (.some, .none):
                return true
            case (.none, .some):
                return false
            case (.none, .none):
                break
            }
            if first.lastMessageTime != second.lastMessageTime {
                return first.lastMessageTime > second.lastMessageTime
            }
            return first.id < second.id
        }
    }

    func displayName(for contact: Contact) -> String {
        preferences.nickname(for: contact.id) ?? contact.displayName
    }

    func localTimeDescription(for contactID: String) -> String? {
        guard let identifier = preferences.conversationPreferences(for: contactID).timezoneIdentifier,
              let timezone = TimeZone(identifier: identifier) else { return nil }
        var format = Date.FormatStyle(date: .omitted, time: .shortened)
        format.timeZone = timezone
        let city = identifier.split(separator: "/").last?.replacingOccurrences(of: "_", with: " ") ?? identifier
        return "\(Date().formatted(format)) in \(city)"
    }

    func replyTarget(for message: ChatMessage) -> MessageReplyTarget {
        let target = message.replyTarget
        guard message.isFromMe, let senderJID = ownSenderJID else { return target }
        return MessageReplyTarget(
            messageID: target.messageID,
            senderJID: senderJID,
            senderName: target.senderName,
            text: target.text
        )
    }

    func start() async {
        if ProcessInfo.processInfo.arguments.contains("-demoOnboarding") {
            phase = .needsConfiguration
            return
        }
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
            presentError("Couldn’t connect", error)
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
            presentError("Couldn’t refresh chats", error)
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
            messages[contactID] = older ? normalizeMessages(response.messages + current) : normalizeMessages(response.messages)
            messageHistoryHasMore[contactID] = response.hasMore
            if let index = contacts.firstIndex(where: { $0.id == contactID }) {
                contacts[index].unreadCount = 0
            }
            try? await api.markRead(contactID: contactID)
            await persistCache()
        } catch {
            presentError("Couldn’t load messages", error)
        }
    }

    func loadAllMessages(for contactID: String) async {
        guard !demoMode, !loadingCompleteHistoryContactIDs.contains(contactID) else { return }
        loadingCompleteHistoryContactIDs.insert(contactID)
        defer { loadingCompleteHistoryContactIDs.remove(contactID) }
        do {
            let response = try await api.messages(contactID: contactID, limit: 0)
            messages[contactID] = normalizeMessages(response.messages + (messages[contactID] ?? []))
            messageHistoryHasMore[contactID] = false
            await persistCache()
        } catch {
            presentError("Couldn’t load the full conversation", error)
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

    func togglePin(_ contact: Contact) async {
        do {
            let pinned: Bool
            if demoMode {
                pinned = contact.pinnedAt == nil
            } else {
                pinned = try await api.togglePin(contactID: contact.id)
            }
            guard let index = contacts.firstIndex(where: { $0.id == contact.id }) else { return }
            let current = contacts[index]
            contacts[index] = Contact(
                id: current.id,
                name: current.name,
                phone: current.phone,
                type: current.type,
                lastMessageTime: current.lastMessageTime,
                unreadCount: current.unreadCount,
                pinnedAt: pinned ? Int64(Date().timeIntervalSince1970 * 1_000) : nil,
                lastMessagePreview: current.lastMessagePreview
            )
            contacts = Self.orderedContacts(contacts)
            await persistCache()
        } catch {
            presentError(
                contact.pinnedAt == nil ? "Couldn’t pin conversation" : "Couldn’t unpin conversation",
                error
            )
        }
    }

    func send(text: String, to contactID: String, reply: MessageReplyTarget? = nil) async -> Bool {
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
            let response = try await api.send(contactID: contactID, text: clean, reply: reply)
            if let contact = contacts.first(where: { $0.id == contactID }) {
                MessagingIntentDonor.donateOutgoing(
                    originalText: clean,
                    response: response,
                    contact: contact,
                    displayName: displayName(for: contact)
                )
            }
            await loadMessages(for: contactID)
            await refresh()
            return true
        } catch {
            presentError("Couldn’t send message", error)
            return false
        }
    }

    func sendImage(
        data: Data,
        mimeType: String,
        caption: String? = nil,
        to contactID: String,
        reply: MessageReplyTarget? = nil
    ) async -> Bool {
        guard !data.isEmpty, data.count <= 16 * 1_024 * 1_024 else {
            presentError("Couldn’t send image", "Images must be smaller than 16 MB.")
            return false
        }
        sendingContactIDs.insert(contactID)
        defer { sendingContactIDs.remove(contactID) }

        if demoMode {
            let message = ChatMessage.demoImage(contactID: contactID)
            messages[contactID, default: []].append(message)
            messageImages[message.id] = PlatformImage(data: data)
            return true
        }

        do {
            _ = try await api.sendImage(contactID: contactID, data: data, mimeType: mimeType, caption: caption, reply: reply)
            await loadMessages(for: contactID)
            await refresh()
            return true
        } catch {
            presentError("Couldn’t send image", error)
            return false
        }
    }

    func translate(_ message: ChatMessage) async {
        guard message.canTranslate else { return }
        activeMessageActionIDs.insert(message.id)
        defer { activeMessageActionIDs.remove(message.id) }
        if demoMode {
            replaceMessage(message.id, in: message.contactId) { current in
                ChatMessage.demoTranslated(from: current)
            }
            return
        }
        do {
            let result = try await api.translate(message)
            guard result.success else { throw APIError.server(result.error ?? "Translation failed.") }
            await loadMessages(for: message.contactId)
        } catch {
            presentError("Couldn’t translate message", error)
        }
    }

    func generateAIReply(to message: ChatMessage) async -> String? {
        guard message.canGenerateAIReply else { return nil }
        activeMessageActionIDs.insert(message.id)
        defer { activeMessageActionIDs.remove(message.id) }
        if demoMode { return "Sounds good — I’ll let you know when I’m leaving." }
        do {
            let result = try await api.generateAIReply(to: message)
            guard result.success, let reply = result.replyText?.trimmingCharacters(in: .whitespacesAndNewlines), !reply.isEmpty else {
                throw APIError.server(result.error ?? "AI reply was empty.")
            }
            return reply
        } catch {
            presentError("Couldn’t generate an AI reply", error)
            return nil
        }
    }

    func react(to message: ChatMessage, emoji: String) async {
        activeMessageActionIDs.insert(message.id)
        defer { activeMessageActionIDs.remove(message.id) }
        if demoMode {
            replaceMessage(message.id, in: message.contactId) { current in
                var updated = current
                updated.reactions = [emoji: ["me"]]
                return updated
            }
            return
        }
        do {
            try await api.react(to: message, emoji: emoji, senderJID: message.isFromMe ? ownSenderJID : message.senderJID)
            await loadMessages(for: message.contactId)
        } catch {
            presentError("Couldn’t react to message", error)
        }
    }

    func loadMedia(for message: ChatMessage) async {
        guard message.mediaKind != nil,
              messageImages[message.id] == nil,
              messageMediaURLs[message.id] == nil,
              !mediaRequests.contains(message.id) else { return }
        mediaErrorIDs.remove(message.id)
        if let base64 = message.content?.mediaData, let data = Data(base64Encoded: base64) {
            storeMedia(data, mimeType: message.content?.mimeType, for: message)
            return
        }
        guard message.content?.hasMedia == true, !demoMode else { return }
        mediaRequests.insert(message.id)
        mediaLoadingIDs.insert(message.id)
        defer {
            mediaRequests.remove(message.id)
            mediaLoadingIDs.remove(message.id)
        }
        do {
            let response = try await api.media(messageID: message.id)
            guard let data = Data(base64Encoded: response.mediaData) else {
                throw APIError.decoding("The media payload was invalid.")
            }
            storeMedia(data, mimeType: response.mimeType, for: message)
        } catch {
            mediaErrorIDs.insert(message.id)
        }
    }

    func retryMedia(for message: ChatMessage) async {
        messageImages.removeValue(forKey: message.id)
        messageMediaURLs.removeValue(forKey: message.id)
        mediaErrorIDs.remove(message.id)
        await loadMedia(for: message)
    }

    func loadLinkPreview(for url: URL) async {
        guard linkPreviews[url] == nil, !linkPreviewRequests.contains(url) else { return }
        linkPreviewRequests.insert(url)
        defer { linkPreviewRequests.remove(url) }
        if demoMode {
            linkPreviews[url] = LinkPreview(
                url: url,
                title: "A place worth visiting this weekend",
                description: "A quick guide with opening times, directions, and the best things to see.",
                imageURL: nil,
                siteName: url.host(),
                error: nil
            )
            return
        }
        if let preview = try? await api.linkPreview(for: url), preview.error == nil {
            linkPreviews[url] = preview
        }
    }

    func conversationUsage(for contactID: String) async throws -> UsageSummary {
        if demoMode { return UsageSummary(inputTokens: 12_840, cachedInputTokens: 8_120, outputTokens: 2_430, costUsd: 0.0842) }
        return try await api.conversationUsage(contactID: contactID)
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
        messageImages = [:]
        messageMediaURLs = [:]
        mediaLoadingIDs = []
        mediaErrorIDs = []
        linkPreviews = [:]
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
            presentError("Couldn’t connect", error)
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
            messages[message.contactId] = normalizeMessages((messages[message.contactId] ?? []) + [message])
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

    private func storeMedia(_ data: Data, mimeType: String?, for message: ChatMessage) {
        switch message.mediaKind {
        case .image, .sticker:
            guard let image = PlatformImage(data: data) else {
                mediaErrorIDs.insert(message.id)
                return
            }
            messageImages[message.id] = image
        case .video, .audio, .document:
            let fileManager = FileManager.default
            let directory = fileManager.urls(for: .cachesDirectory, in: .userDomainMask)[0]
                .appending(path: "MessageMedia", directoryHint: .isDirectory)
            do {
                try fileManager.createDirectory(at: directory, withIntermediateDirectories: true)
                let preferredExtension = message.content?.fileName
                    .flatMap { URL(fileURLWithPath: $0).pathExtension.nilIfBlank }
                    ?? mimeType.flatMap { UTType(mimeType: $0)?.preferredFilenameExtension }
                    ?? "bin"
                let url = directory.appending(path: "\(message.id.filenameSafe).\(preferredExtension.filenameSafe)")
                try data.write(to: url, options: .atomic)
                messageMediaURLs[message.id] = url
            } catch {
                mediaErrorIDs.insert(message.id)
            }
        case nil:
            break
        }
    }

    private func presentError(_ title: String, _ error: Error) {
        presentError(title, error.localizedDescription)
    }

    private func presentError(_ title: String, _ message: String) {
        errorTitle = title
        errorMessage = message
    }

    func normalizeMessages(_ values: [ChatMessage]) -> [ChatMessage] {
        let sorted = Array(Dictionary(grouping: values, by: \ChatMessage.id).compactMap { $0.value.last })
            .sorted { ($0.timestamp, $0.id) < ($1.timestamp, $1.id) }
        let reactionMessages = sorted.filter(\.isReaction)
        var displayMessages = sorted.filter { !$0.isReaction }

        for reaction in reactionMessages {
            guard let targetID = reaction.content?.targetMessageId,
                  let index = displayMessages.firstIndex(where: { $0.id == targetID }) else { continue }
            let actor = reaction.isFromMe ? "me" : (reaction.senderPhone ?? reaction.senderName ?? "unknown")
            var values = displayMessages[index].reactions ?? [:]
            for emoji in values.keys {
                values[emoji]?.removeAll { $0 == actor }
                if values[emoji]?.isEmpty == true { values.removeValue(forKey: emoji) }
            }
            if let emoji = reaction.content?.emoji, !emoji.isEmpty {
                values[emoji, default: []].append(actor)
            }
            displayMessages[index].reactions = values
        }
        return displayMessages
    }

    private func replaceMessage(
        _ messageID: String,
        in contactID: String,
        transform: (ChatMessage) -> ChatMessage
    ) {
        guard let index = messages[contactID]?.firstIndex(where: { $0.id == messageID }),
              let current = messages[contactID]?[index] else { return }
        messages[contactID]?[index] = transform(current)
        persistCacheSoon()
    }

    private var ownSenderJID: String? {
        guard let phone = backendStatus.phone?.filter(\.isNumber), !phone.isEmpty else { return nil }
        return "\(phone)@s.whatsapp.net"
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
        contacts = Self.orderedContacts(contacts)
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
        if let imageMessage = messages["virag@s.whatsapp.net"]?.first(where: \.isImage) {
            messageImages[imageMessage.id] = demoPhoto()
        }
        if demoConversationMode {
            selectedContactID = contacts.first?.id
            if ProcessInfo.processInfo.arguments.contains("-demoSending"),
               let selectedContactID {
                sendingContactIDs.insert(selectedContactID)
            }
        }
        phase = .ready
    }

    private func demoPhoto() -> PlatformImage {
        DemoImageFactory.landscape(size: CGSize(width: 640, height: 420))
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
            demo(id: "link", contactID: "virag@s.whatsapp.net", timestamp: 1_783_939_900_000, fromMe: false, body: "This looks lovely: https://example.com/weekend", translated: nil, sender: "Virág"),
            demoImage(contactID: "virag@s.whatsapp.net", id: "image", timestamp: 1_783_940_100_000),
            demo(id: "3", contactID: "virag@s.whatsapp.net", timestamp: 1_783_940_400_000, fromMe: false, body: "Tökéletes, akkor hat után várlak.", translated: "Perfect, I’ll be there just after six.", sender: "Virág"),
            demo(id: "4", contactID: "virag@s.whatsapp.net", timestamp: 1_783_940_520_000, fromMe: false, body: "Még tíz perc, és indulok.", translated: nil, sender: "Virág"),
        ]
    ]

    static func demoOutgoing(contactID: String, text: String) -> ChatMessage {
        demo(id: UUID().uuidString, contactID: contactID, timestamp: Int64(Date().timeIntervalSince1970 * 1_000), fromMe: true, body: text, translated: nil, sender: nil)
    }

    static func demoImage(
        contactID: String,
        id: String = UUID().uuidString,
        timestamp: Int64 = Int64(Date().timeIntervalSince1970 * 1_000)
    ) -> ChatMessage {
        ChatMessage(
            id: id,
            contactId: contactID,
            timestamp: timestamp,
            isFromMe: false,
            isForwarded: false,
            senderName: "Virág",
            senderPhone: "36305550142",
            contactName: "Virág Skinner",
            contactPhone: "+36 30 555 0142",
            chatType: "private",
            contentType: "Image",
            content: MessageContent(
                type: "image",
                body: nil,
                caption: "The light is gorgeous here today.",
                mimeType: "image/jpeg",
                hasMedia: true,
                showTranslatedPrimary: nil,
                replyContext: nil
            ),
            originalText: nil,
            translatedText: nil,
            sourceLanguage: nil,
            isTranslated: false
        )
    }

    static func demoTranslated(from message: ChatMessage) -> ChatMessage {
        ChatMessage(
            id: message.id,
            contactId: message.contactId,
            timestamp: message.timestamp,
            isFromMe: message.isFromMe,
            isForwarded: message.isForwarded,
            senderName: message.senderName,
            senderPhone: message.senderPhone,
            contactName: message.contactName,
            contactPhone: message.contactPhone,
            chatType: message.chatType,
            contentType: message.contentType,
            content: message.content,
            originalText: message.contentText,
            translatedText: "Translated: \(message.contentText ?? message.displayText)",
            sourceLanguage: "Hungarian",
            isTranslated: true,
            reactions: message.reactions
        )
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

private extension String {
    var nilIfBlank: String? {
        trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ? nil : self
    }

    var filenameSafe: String {
        let allowed = CharacterSet.alphanumerics.union(CharacterSet(charactersIn: "-_"))
        let value = unicodeScalars.map { allowed.contains($0) ? Character(String($0)) : "_" }
        return String(value)
    }
}
