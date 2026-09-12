import XCTest
@testable import WhatsAppTranslator

@MainActor
final class WatchBackendTests: XCTestCase {
    func testWatchSendUsesSelectedMessageConditionalQuoteAndOnlySendsOnceAfterRestart() async throws {
        let fixture = try Fixture()
        defer { fixture.cleanup() }
        let backend = fixture.backend()
        let feed = await backend.handle(WatchRequest(action: .feed))
        let snapshot = try JSONDecoder().decode(WatchFeedSnapshot.self, from: XCTUnwrap(feed.snapshot))
        let target = try XCTUnwrap(snapshot.messages.first)
        var draft = WatchReplyDraft(target: target, accountID: snapshot.accountID, text: "See you soon")
        let request = WatchRequest(action: .reply, reply: draft.submission())
        let response = await backend.handle(request)
        XCTAssertEqual(response.status, .sent)
        let payload = try XCTUnwrap(WatchBackendProtocol.state.payload)
        XCTAssertEqual(payload["contactId"] as? String, target.contactID)
        XCTAssertEqual(payload["replyTo"] as? String, target.messageID)
        XCTAssertEqual(payload["replyOnlyIfNotLatest"] as? Bool, true)
        XCTAssertNil(payload["translationEnabled"])
        let repeated = await fixture.backend().handle(request)
        XCTAssertEqual(repeated.status, .sent)
        XCTAssertEqual(WatchBackendProtocol.state.sends, 1)
    }

    func testUnconfirmedSendIsNeverRetriedAfterLostAcknowledgement() async throws {
        let fixture = try Fixture()
        defer { fixture.cleanup() }
        WatchBackendProtocol.state.failSend = true
        let backend = fixture.backend()
        let feed = await backend.handle(WatchRequest(action: .feed))
        let snapshot = try JSONDecoder().decode(WatchFeedSnapshot.self, from: XCTUnwrap(feed.snapshot))
        var draft = WatchReplyDraft(target: try XCTUnwrap(snapshot.messages.first), accountID: snapshot.accountID, text: "Hello")
        let request = WatchRequest(action: .reply, reply: draft.submission())
        let first = await backend.handle(request)
        let second = await fixture.backend().handle(request)
        XCTAssertEqual(first.status, .uncertain)
        XCTAssertEqual(second.status, .uncertain)
        XCTAssertEqual(WatchBackendProtocol.state.sends, 1)
    }

    func testWrongAccountAndMissingTargetCannotSendToAnotherConversation() async throws {
        let fixture = try Fixture()
        defer { fixture.cleanup() }
        let backend = fixture.backend()
        let feed = await backend.handle(WatchRequest(action: .feed))
        let snapshot = try JSONDecoder().decode(WatchFeedSnapshot.self, from: XCTUnwrap(feed.snapshot))
        let target = try XCTUnwrap(snapshot.messages.first)
        let wrongAccount = WatchReply(requestID: UUID(), accountID: "old-account", target: target, text: "Hello")
        let accountResult = await backend.handle(WatchRequest(action: .reply, reply: wrongAccount))
        XCTAssertEqual(accountResult.status, .rejected)
        let missing = WatchMessage(messageID: "deleted", contactID: target.contactID, conversation: target.conversation,
                                   sender: target.sender, text: target.text, timestamp: target.timestamp, isFromMe: false, isGroup: false)
        let stale = WatchReply(requestID: UUID(), accountID: snapshot.accountID, target: missing, text: "Hello")
        let staleResult = await backend.handle(WatchRequest(action: .reply, reply: stale))
        XCTAssertEqual(staleResult.status, .rejected)
        XCTAssertEqual(WatchBackendProtocol.state.sends, 0)
    }

    func testUnreadableReceiptCannotBecomePermissionToResend() async throws {
        let fixture = try Fixture()
        defer { fixture.cleanup() }
        let backend = fixture.backend()
        let feed = await backend.handle(WatchRequest(action: .feed))
        let snapshot = try JSONDecoder().decode(WatchFeedSnapshot.self, from: XCTUnwrap(feed.snapshot))
        var draft = WatchReplyDraft(target: try XCTUnwrap(snapshot.messages.first), accountID: snapshot.accountID, text: "Hello")
        let reply = draft.submission()
        try FileManager.default.createDirectory(at: fixture.directory, withIntermediateDirectories: true)
        try Data("interrupted receipt".utf8).write(to: fixture.directory.appending(path: reply.requestID.uuidString + ".json"))
        let result = await backend.handle(WatchRequest(action: .reply, reply: reply))
        XCTAssertEqual(result.status, .uncertain)
        XCTAssertEqual(WatchBackendProtocol.state.sends, 0)
    }

    private struct Fixture: Sendable {
        let suite = "watch-tests-" + UUID().uuidString
        let directory = URL.temporaryDirectory.appending(path: UUID().uuidString)
        let session: URLSession
        let configuration: ServerConfiguration

        init() throws {
            configuration = try ServerConfiguration.make(address: "https://watch-test.invalid", password: "fixture")
            let config = URLSessionConfiguration.ephemeral
            config.protocolClasses = [WatchBackendProtocol.self]
            session = URLSession(configuration: config)
            WatchBackendProtocol.state.reset()
        }
        func backend() -> WatchMessagingBackend {
            WatchMessagingBackend(api: APIClient(session: session), configurationProvider: { configuration },
                                  defaultsSuite: suite, receipts: WatchSendReceipts(directory: directory))
        }
        func cleanup() {
            session.invalidateAndCancel()
            UserDefaults.standard.removePersistentDomain(forName: suite)
            try? FileManager.default.removeItem(at: directory)
        }
    }
}

private final class WatchBackendProtocol: URLProtocol, @unchecked Sendable {
    final class State: @unchecked Sendable {
        private let lock = NSLock()
        private var count = 0
        private var body: [String: Any]?
        private var fail = false
        var sends: Int { lock.withLock { count } }
        var payload: [String: Any]? { lock.withLock { body } }
        var failSend: Bool {
            get { lock.withLock { fail } }
            set { lock.withLock { fail = newValue } }
        }
        func reset() { lock.withLock { count = 0; body = nil; fail = false } }
        func record(_ data: Data) { lock.withLock { count += 1; body = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any] } }
    }
    static let state = State()
    override class func canInit(with request: URLRequest) -> Bool { request.url?.host == "watch-test.invalid" }
    override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }
    override func stopLoading() {}

    override func startLoading() {
        let path = request.url!.path
        let json: String
        switch path {
        case "/api/auth/check": json = #"{"required":false}"#
        case "/api/contacts":
            json = #"[{"id":"family@g.us","name":"Family","type":"group","lastMessageTime":2,"unreadCount":2}]"#
        case "/api/feed", "/api/messages/family@g.us":
            json = #"{"messages":[{"id":"selected","contactId":"family@g.us","timestamp":1,"isFromMe":false,"isForwarded":false,"senderName":"Jordan","senderPhone":"123@s.whatsapp.net","chatType":"group","contentType":"text","originalText":"Original","translatedText":"English message","isTranslated":true},{"id":"newer","contactId":"family@g.us","timestamp":2,"isFromMe":false,"isForwarded":false,"chatType":"group","contentType":"text","originalText":"Newer","isTranslated":false}],"hasMore":false}"#
        case "/api/send":
            var data = request.httpBody ?? Data()
            if let stream = request.httpBodyStream {
                stream.open()
                defer { stream.close() }
                var buffer = [UInt8](repeating: 0, count: 4_096)
                while stream.hasBytesAvailable {
                    let count = stream.read(&buffer, maxLength: buffer.count)
                    if count <= 0 { break }
                    data.append(buffer, count: count)
                }
            }
            Self.state.record(data)
            if Self.state.failSend {
                client?.urlProtocol(self, didFailWithError: URLError(.networkConnectionLost))
                return
            }
            json = #"{"messageId":"sent","timestamp":3,"isTranslated":false,"originalFollowUpSent":false}"#
        default:
            client?.urlProtocol(self, didFailWithError: URLError(.unsupportedURL))
            return
        }
        client?.urlProtocol(self, didReceive: HTTPURLResponse(url: request.url!, statusCode: 200, httpVersion: nil, headerFields: ["Content-Type": "application/json"])!, cacheStoragePolicy: .notAllowed)
        client?.urlProtocol(self, didLoad: Data(json.utf8))
        client?.urlProtocolDidFinishLoading(self)
    }
}
