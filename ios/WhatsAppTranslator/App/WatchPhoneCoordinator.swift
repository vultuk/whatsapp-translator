#if os(iOS)
import Foundation
import CryptoKit
@preconcurrency import WatchConnectivity

final class WatchPhoneCoordinator: NSObject, WCSessionDelegate, @unchecked Sendable {
    static let shared = WatchPhoneCoordinator()
    private let backend = WatchMessagingBackend()

    func start() {
        guard WCSession.isSupported() else { return }
        WCSession.default.delegate = self
        WCSession.default.activate()
    }

    func session(_ session: WCSession, activationDidCompleteWith activationState: WCSessionActivationState, error: Error?) {}
    func sessionDidBecomeInactive(_ session: WCSession) {}
    func sessionDidDeactivate(_ session: WCSession) { session.activate() }

    func session(_ session: WCSession, didReceiveMessageData messageData: Data, replyHandler: @escaping (Data) -> Void) {
        let completion = WatchCompletion(replyHandler)
        Task {
            let response: WatchResponse
            if let request = try? JSONDecoder().decode(WatchRequest.self, from: messageData) {
                response = await backend.handle(request)
            } else {
                response = WatchResponse(status: .rejected, message: "Update Babel Bridge on both devices.")
            }
            if let data = try? response.encoded() { completion.call(data) }
        }
    }
}

private final class WatchCompletion: @unchecked Sendable {
    let callback: (Data) -> Void
    init(_ callback: @escaping (Data) -> Void) { self.callback = callback }
    func call(_ data: Data) { callback(data) }
}

actor WatchMessagingBackend {
    private let api: APIClient
    private var configuration: ServerConfiguration?
    private let configurationProvider: @Sendable () -> ServerConfiguration?
    private let defaults: UserDefaults
    private let receipts: WatchSendReceipts
    private var inFlight: Set<UUID> = []

    init(api: APIClient = APIClient(), configurationProvider: @escaping @Sendable () -> ServerConfiguration? = { CredentialStore().load() },
         defaultsSuite: String? = nil, receipts: WatchSendReceipts = WatchSendReceipts()) {
        self.api = api
        self.configurationProvider = configurationProvider
        self.defaults = defaultsSuite.flatMap(UserDefaults.init(suiteName:)) ?? .standard
        self.receipts = receipts
    }

    func handle(_ request: WatchRequest) async -> WatchResponse {
        do {
            guard let stored = configurationProvider() else {
                configuration = nil
                return WatchResponse(status: .rejected, message: "Open Babel Bridge on your iPhone to connect your account.")
            }
            if configuration != stored {
                await api.configure(stored)
                try await api.prepareAuthenticatedRequests()
                configuration = stored
            }
            let accountID = accountID(stored)
            switch request.action {
            case .feed:
                async let contacts = api.contacts()
                async let feed = api.feed()
                let snapshot = Self.snapshot(messages: try await feed.messages, contacts: try await contacts, accountID: accountID)
                guard configurationProvider() == stored else {
                    return WatchResponse(status: .rejected, message: "Your account changed. Refresh Messages first.")
                }
                return WatchResponse(status: .feed, snapshot: try snapshot.encoded())
            case .reply:
                guard let reply = request.reply, reply.accountID == accountID,
                      !reply.text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
                      reply.text.utf8.count <= 16_000 else {
                    return WatchResponse(status: .rejected, message: "Refresh Messages and choose a reply again.")
                }
                // A repeated request only reads its receipt. Persist BEFORE sending,
                // so a lost Watch reply or iOS termination cannot duplicate a message.
                let signature = try WatchSendReceipts.signature(reply)
                if let receipt = try receipts.read(reply.requestID) {
                    guard receipt.signature == signature else {
                        return WatchResponse(status: .rejected, message: "This reply has already been submitted.")
                    }
                    if receipt.sent { return WatchResponse(status: .sent) }
                    return WatchResponse(status: .uncertain, message: inFlight.contains(reply.requestID)
                        ? "Still sending. Check delivery again in a moment."
                        : "Delivery could not be confirmed. Check this conversation on your iPhone before sending again.")
                }
                let contacts = try await api.contacts()
                guard contacts.contains(where: { $0.id == reply.target.contactID && $0.showsInChatList }) else {
                    return WatchResponse(status: .rejected, message: "This conversation is no longer available.")
                }
                let messages = try await api.messages(contactID: reply.target.contactID, limit: 200).messages
                guard let target = messages.first(where: { $0.id == reply.target.messageID && $0.contactId == reply.target.contactID }),
                      !target.isReaction, target.normalizedContentType != "revoked" else {
                    return WatchResponse(status: .rejected, message: "That message is no longer available. Refresh and choose another.")
                }
                guard configurationProvider() == stored else {
                    return WatchResponse(status: .rejected, message: "Your account changed. Refresh Messages first.")
                }
                // Recheck after suspension: another request with this ID may have arrived.
                guard try receipts.read(reply.requestID) == nil else { return await handle(request) }
                try receipts.write(reply.requestID, signature: signature, sent: false)
                inFlight.insert(reply.requestID)
                defer { inFlight.remove(reply.requestID) }
                do {
                    _ = try await api.send(contactID: target.contactId, text: reply.text,
                                           reply: target.replyTarget, replyOnlyIfNotLatest: true)
                    try receipts.write(reply.requestID, signature: signature, sent: true)
                    return WatchResponse(status: .sent)
                } catch {
                    return WatchResponse(status: .uncertain, message: "Delivery could not be confirmed. Check this conversation on your iPhone before sending again.")
                }
            }
        } catch {
            if request.action == .reply {
                // A receipt read error is not proof that a previous send failed.
                // Preserve the request ID even when reconnecting/authentication fails.
                return WatchResponse(status: .uncertain, message: "Couldn’t confirm delivery. Reconnect your iPhone and check delivery again.")
            }
            return WatchResponse(status: .rejected, message: "Couldn’t connect. Check Babel Bridge on your iPhone, then try again.")
        }
    }

    private func accountID(_ configuration: ServerConfiguration) -> String {
        // Only the random account identifier leaves the phone, never credentials
        // or a password fingerprint. A changed setup invalidates old Watch drafts.
        let fingerprint = SHA256.hash(data: Data((configuration.baseURL.absoluteString + "\n" + configuration.password).utf8))
            .map { String(format: "%02x", $0) }.joined()
        if defaults.string(forKey: "watch-account-fingerprint") != fingerprint || defaults.string(forKey: "watch-account-id") == nil {
            defaults.set(fingerprint, forKey: "watch-account-fingerprint")
            defaults.set(UUID().uuidString, forKey: "watch-account-id")
        }
        return defaults.string(forKey: "watch-account-id")!
    }

    static func snapshot(messages: [ChatMessage], contacts: [Contact], accountID: String) -> WatchFeedSnapshot {
        let visible = Dictionary(contacts.filter(\.showsInChatList).map { ($0.id, $0) }, uniquingKeysWith: { first, _ in first })
        let messages = messages.filter { visible[$0.contactId] != nil && !$0.isReaction && $0.normalizedContentType != "revoked" }
            .sorted { $0.timestamp == $1.timestamp ? $0.id < $1.id : $0.timestamp < $1.timestamp }
            .suffix(30).map { message in
                let contact = visible[message.contactId]!
                return WatchMessage(messageID: message.id, contactID: message.contactId,
                                    conversation: contact.displayName, sender: message.isFromMe ? "You" : (message.senderName ?? contact.displayName),
                                    text: String(message.displayText.prefix(1_000)), timestamp: message.timestamp,
                                    isFromMe: message.isFromMe, isGroup: contact.isGroup)
            }
        return WatchFeedSnapshot(accountID: accountID, messages: Array(messages), updatedAt: Date())
    }
}

struct WatchSendReceipts: Sendable {
    struct Receipt: Codable { let signature: String; let sent: Bool }
    var directory = URL.applicationSupportDirectory.appending(path: "WatchSendReceipts", directoryHint: .isDirectory)

    static func signature(_ reply: WatchReply) throws -> String {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys]
        return SHA256.hash(data: try encoder.encode(reply)).map { String(format: "%02x", $0) }.joined()
    }

    func read(_ requestID: UUID) throws -> Receipt? {
        let url = directory.appending(path: requestID.uuidString + ".json")
        guard FileManager.default.fileExists(atPath: url.path) else { return nil }
        return try JSONDecoder().decode(Receipt.self, from: Data(contentsOf: url))
    }

    func write(_ requestID: UUID, signature: String, sent: Bool) throws {
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let data = try JSONEncoder().encode(Receipt(signature: signature, sent: sent))
        try data.write(to: directory.appending(path: requestID.uuidString + ".json"),
                       options: [.atomic, .completeFileProtectionUntilFirstUserAuthentication])
    }
}
#endif
