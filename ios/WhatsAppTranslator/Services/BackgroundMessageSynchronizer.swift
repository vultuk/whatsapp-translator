import Foundation

enum BackgroundMessageSyncOutcome: Sendable {
    case newData
    case noData
    case failed
}

actor BackgroundMessageSynchronizer {
    static let shared = BackgroundMessageSynchronizer()

    private let credentials: CredentialStore
    private let cache: ChatCacheStore

    init(
        credentials: CredentialStore = CredentialStore(),
        cache: ChatCacheStore = .shared
    ) {
        self.credentials = credentials
        self.cache = cache
    }

    func sync(contactID: String) async -> BackgroundMessageSyncOutcome {
        guard !contactID.isEmpty, let configuration = credentials.load() else {
            return .noData
        }

        let sessionConfiguration = URLSessionConfiguration.ephemeral
        sessionConfiguration.timeoutIntervalForRequest = 10
        sessionConfiguration.timeoutIntervalForResource = 25
        let api = APIClient(session: URLSession(configuration: sessionConfiguration))
        await api.configure(configuration)

        do {
            try await api.prepareAuthenticatedRequests()
            async let fetchedContacts = api.contacts()
            async let fetchedMessages = api.messages(contactID: contactID)
            let (contacts, response) = try await (fetchedContacts, fetchedMessages)

            let previous = await cache.load(for: configuration)
            let base = previous ?? ChatCacheSnapshot(
                serverBaseURL: configuration.baseURL.absoluteString,
                contacts: [],
                messages: [:],
                updatedAt: .distantPast
            )
            let next = base.merging(
                contacts: contacts,
                messages: response.messages,
                for: contactID
            )
            let changed = previous?.contacts != next.contacts
                || previous?.messages[contactID] != next.messages[contactID]
            await cache.save(next)
            return changed ? .newData : .noData
        } catch {
            #if DEBUG
            print("Background message sync failed: \(error.localizedDescription)")
            #endif
            return .failed
        }
    }
}
