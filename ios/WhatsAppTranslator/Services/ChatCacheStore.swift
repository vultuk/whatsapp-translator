import Foundation

struct ChatCacheSnapshot: Codable, Equatable, Sendable {
    let serverBaseURL: String
    var contacts: [Contact]
    var messages: [String: [ChatMessage]]
    let updatedAt: Date

    static let maximumMessagesPerChat = 100

    func merging(
        contacts newContacts: [Contact],
        messages newMessages: [ChatMessage],
        for contactID: String,
        now: Date = Date()
    ) -> ChatCacheSnapshot {
        var nextMessages = messages
        nextMessages[contactID] = Self.merge(
            (messages[contactID] ?? []) + newMessages
        )
        return ChatCacheSnapshot(
            serverBaseURL: serverBaseURL,
            contacts: newContacts,
            messages: nextMessages,
            updatedAt: now
        )
    }

    private static func merge(_ values: [ChatMessage]) -> [ChatMessage] {
        let sorted = Array(
            Dictionary(grouping: values, by: \ChatMessage.id)
                .compactMap { $0.value.last }
        )
        .sorted { ($0.timestamp, $0.id) < ($1.timestamp, $1.id) }
        return Array(sorted.suffix(maximumMessagesPerChat))
    }
}

actor ChatCacheStore {
    static let shared = ChatCacheStore()

    private let directoryURL: URL
    private let fileURL: URL

    init(directoryURL: URL? = nil) {
        let root = directoryURL
            ?? FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
                .appending(path: "WhatsAppTranslator", directoryHint: .isDirectory)
        self.directoryURL = root
        self.fileURL = root.appending(path: "chat-cache.json", directoryHint: .notDirectory)
    }

    func load(for configuration: ServerConfiguration) -> ChatCacheSnapshot? {
        guard let data = try? Data(contentsOf: fileURL),
              let snapshot = try? JSONDecoder.cache.decode(ChatCacheSnapshot.self, from: data),
              snapshot.serverBaseURL == configuration.baseURL.absoluteString else {
            return nil
        }
        return snapshot
    }

    func save(_ snapshot: ChatCacheSnapshot) {
        do {
            try FileManager.default.createDirectory(
                at: directoryURL,
                withIntermediateDirectories: true
            )
            if let existingData = try? Data(contentsOf: fileURL),
               let existing = try? JSONDecoder.cache.decode(ChatCacheSnapshot.self, from: existingData),
               existing.serverBaseURL == snapshot.serverBaseURL,
               existing.updatedAt > snapshot.updatedAt {
                return
            }
            let data = try JSONEncoder.cache.encode(snapshot)
            try data.write(
                to: fileURL,
                options: [.atomic, .completeFileProtectionUntilFirstUserAuthentication]
            )
        } catch {
            #if DEBUG
            print("Chat cache save failed: \(error.localizedDescription)")
            #endif
        }
    }

    func clear() {
        try? FileManager.default.removeItem(at: fileURL)
    }
}

private extension JSONDecoder {
    static var cache: JSONDecoder {
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .millisecondsSince1970
        return decoder
    }
}

private extension JSONEncoder {
    static var cache: JSONEncoder {
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .millisecondsSince1970
        encoder.outputFormatting = [.sortedKeys]
        return encoder
    }
}
