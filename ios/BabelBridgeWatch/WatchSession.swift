import Foundation
import Observation
@preconcurrency import WatchConnectivity

@MainActor
@Observable
final class WatchSession: NSObject, WCSessionDelegate {
    var snapshot: WatchFeedSnapshot?
    var draft: WatchReplyDraft? {
        didSet { persistDraft() }
    }
    var loading = false
    var sending = false
    var error: String?
    var replyError: String?
    var sent = false
    private let demoMode = ProcessInfo.processInfo.arguments.contains("-demo")

    override init() {
        super.init()
        if demoMode { snapshot = .demo; return }
        if let data = try? Data(contentsOf: Self.draftURL) {
            draft = try? JSONDecoder().decode(WatchReplyDraft.self, from: data)
        }
        WCSession.default.delegate = self
        WCSession.default.activate()
    }

    func refresh() async {
        guard !demoMode, !loading else { return }
        loading = true
        defer { loading = false }
        do {
            let response = try await request(WatchRequest(action: .feed))
            guard response.status == .feed, let data = response.snapshot else {
                snapshot = nil
                error = response.message
                return
            }
            let next = try JSONDecoder().decode(WatchFeedSnapshot.self, from: data)
            if let draft, draft.accountID != next.accountID {
                self.draft = nil
                replyError = nil
            }
            snapshot = next
            error = nil
        } catch {
            self.error = "Keep your iPhone nearby and connected. Open Babel Bridge on it, then refresh."
        }
    }

    func select(_ message: WatchMessage) {
        guard let snapshot, !sending else { return }
        draft = WatchReplyDraft(target: message, accountID: snapshot.accountID)
        replyError = nil
        sent = false
    }

    func send() async {
        guard !sending, var current = draft,
              !current.text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return }
        let submission = current.submission()
        if !demoMode {
            do { try Self.save(current) }
            catch {
                replyError = "Couldn’t save this reply safely. Try again after freeing some space on your Watch."
                return
            }
        }
        draft = current
        sending = true
        replyError = nil
        defer { sending = false }
        if demoMode {
            draft = nil
            sent = true
            return
        }
        do {
            let response = try await request(WatchRequest(action: .reply, reply: submission))
            switch response.status {
            case .sent:
                draft = nil
                sent = true
                await refresh()
            case .rejected:
                // The phone confirms no send was attempted; the text can be edited.
                current.submitted = nil
                draft = current
                replyError = response.message
            case .uncertain, .feed:
                replyError = response.message ?? "Check delivery again in a moment."
            }
        } catch {
            // Retain the same request ID when delivery or its acknowledgement is lost.
            replyError = "Connection interrupted. Check delivery when your iPhone is reachable."
        }
    }

    private func request(_ request: WatchRequest) async throws -> WatchResponse {
        guard WCSession.default.activationState == .activated else { throw ConnectivityError.notReady }
        let data = try JSONEncoder().encode(request)
        let response = try await WatchMessageTransport.response(for: data) { data, reply, failure in
            WCSession.default.sendMessageData(data, replyHandler: reply, errorHandler: failure)
        }
        return try JSONDecoder().decode(WatchResponse.self, from: response)
    }

    nonisolated func session(_ session: WCSession, activationDidCompleteWith activationState: WCSessionActivationState, error: Error?) {
        Task { @MainActor [weak self] in await self?.refresh() }
    }

    nonisolated func sessionReachabilityDidChange(_ session: WCSession) {
        Task { @MainActor [weak self] in await self?.refresh() }
    }

    private static var draftURL: URL {
        URL.applicationSupportDirectory.appending(path: "watch-reply.json")
    }

    private func persistDraft() {
        guard !demoMode else { return }
        if let draft {
            try? Self.save(draft)
        } else {
            try? FileManager.default.removeItem(at: Self.draftURL)
        }
    }

    private static func save(_ draft: WatchReplyDraft) throws {
        let data = try JSONEncoder().encode(draft)
        try FileManager.default.createDirectory(at: draftURL.deletingLastPathComponent(), withIntermediateDirectories: true)
        try data.write(to: draftURL, options: [.atomic, .completeFileProtectionUntilFirstUserAuthentication])
    }

    private enum ConnectivityError: Error { case notReady }
}
