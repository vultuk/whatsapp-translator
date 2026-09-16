import XCTest
#if os(macOS)
@testable import BabelBridgeMac
#else
@testable import WhatsAppTranslator
#endif

@MainActor
final class MessageEditDeliveryTests: XCTestCase {
    private func message(id: String = "original", chat: String = "family@g.us", fromMe: Bool = false, body: String, revision: Int64 = 0) throws -> ChatMessage {
        var content: [String: Any] = ["type": "text", "body": body]
        if revision > 0 { content["edited_at_ms"] = revision }
        let json: [String: Any] = ["id": id, "contactId": chat, "timestamp": 100,
            "isFromMe": fromMe, "isForwarded": false, "chatType": chat.hasSuffix("@g.us") ? "group" : "private",
            "contentType": "Text", "content": content, "originalText": body, "isTranslated": false]
        return try JSONDecoder().decode(ChatMessage.self, from: JSONSerialization.data(withJSONObject: json))
    }

    private func update(_ message: ChatMessage) throws -> LiveEvent {
        let value = try JSONSerialization.jsonObject(with: JSONEncoder().encode(message))
        return try JSONDecoder().decode(LiveEvent.self, from: JSONSerialization.data(withJSONObject: ["type": "message_updated", "message": value]))
    }

    func testLiveEditsReplaceBothViewsKeepReactionsAndDoNotMoveTheChat() throws {
        for chat in ["family@g.us", "447700900123@s.whatsapp.net"] {
            for fromMe in [false, true] {
                let session = AppSession(demoMode: false)
                session.phase = .ready
                var original = try message(chat: chat, fromMe: fromMe, body: "Get")
                original.reactions = ["❤️": ["me"]]
                session.messages[chat] = [original]
                session.feedByID[original.id] = original
                session.contacts = [Contact(id: chat, name: "Test chat", phone: nil, type: "group", lastMessageTime: 200, unreadCount: 3, pinnedAt: nil, lastMessagePreview: "A later message")]
                let edit = try message(chat: chat, fromMe: fromMe, body: "Grr", revision: 300)
                try session.handle(update(edit))
                XCTAssertEqual(session.messages[chat]?.first?.displayText, "Grr")
                XCTAssertEqual(session.unifiedMessages.first?.displayText, "Grr")
                XCTAssertEqual(session.unifiedMessages.count, 1)
                XCTAssertEqual(session.unifiedMessages.first?.timestamp, 100)
                XCTAssertEqual(session.unifiedMessages.first?.reactions, ["❤️": ["me"]])
                XCTAssertEqual(session.unifiedMessages.first?.isEdited, true)
                XCTAssertEqual(session.contacts.first?.unreadCount, 3)
                XCTAssertEqual(session.contacts.first?.lastMessagePreview, "A later message")
                XCTAssertEqual(session.contacts.first?.lastMessageTime, 200)
                // Both stale events and stale HTTP snapshots must keep the visible edit.
                try session.handle(update(original))
                XCTAssertEqual(session.normalizeMessages([original]).first?.displayText, "Grr")
                XCTAssertEqual(session.unifiedMessages.first?.displayText, "Grr")
            }
        }
    }

    func testEditClockSurvivesCacheRestoreAndNewerEditsStillApply() throws {
        let edit = try message(body: "Grr", revision: 300)
        let restored = try JSONDecoder().decode(ChatMessage.self, from: JSONEncoder().encode(edit))
        XCTAssertEqual(restored.editRevision, 300)
        let session = AppSession(demoMode: false)
        session.phase = .ready
        let original = try message(body: "Get")
        session.messages[edit.contactId] = session.normalizeMessages([restored, original])
        session.feedByID[edit.id] = restored
        XCTAssertEqual(session.unifiedMessages.first?.displayText, "Grr")
        try session.handle(update(message(body: "Grr!", revision: 301)))
        try session.handle(update(restored))
        XCTAssertEqual(session.unifiedMessages.first?.displayText, "Grr!")
        XCTAssertEqual(session.messages[edit.contactId]?.count, 1)
    }

    func testTopicMembershipIsRemovedByAnEditAndAnOldPageCannotBringItBack() throws {
        let session = AppSession(demoMode: false)
        session.phase = .ready
        let original = try message(body: "Get")
        session.messages[original.contactId] = [original]
        session.topicPages["one"] = TopicPage(messages: [original])
        try session.handle(update(message(body: "Grr", revision: 300)))
        XCTAssertEqual(session.topicPages["one"]?.messages.count, 0)
        XCTAssertEqual(session.messages[original.contactId]?.first?.displayText, "Grr")
    }

    func testSameNamedTopicsCombineAcrossChatsAndDisablingPrunesTheSharedPage() throws {
        let session = AppSession(demoMode: true)
        let family = ChatTopic(id: "family-plan", contactId: "family@g.us", contactName: "Family", title: "Weekend plans", messageCount: 1, lastMessageTime: 100, categoryId: "category:weekend")
        let friends = ChatTopic(id: "friends-plan", contactId: "friends@g.us", contactName: "Friends", title: "Weekend plans", messageCount: 1, lastMessageTime: 110, categoryId: "category:weekend")
        session.applyTopicCatalog(TopicCatalog(topics: [family, friends], settings: [], available: true))
        XCTAssertEqual(session.topics().count, 1)
        XCTAssertEqual(session.topics().first?.messageCount, 2)
        XCTAssertEqual(session.topics().first?.lastMessageTime, 110)
        XCTAssertEqual(session.topicCatalog.contactIDs(for: "category:weekend"), ["family@g.us", "friends@g.us"])
        XCTAssertEqual(session.topics(for: "family@g.us").map(\.id), ["family-plan"])
        XCTAssertFalse(session.topicSetting(for: "unconfigured@g.us").enabled)
        session.topicPages[family.id] = TopicPage(messages: [try message(body: "Picnic")])
        session.topicPages["category:weekend"] = TopicPage(messages: [try message(body: "Picnic"), try message(chat: "friends@g.us", body: "Brunch")])
        session.applyTopicCatalog(TopicCatalog(topics: [friends], settings: [], available: true))
        XCTAssertNil(session.topicPages[family.id])
        XCTAssertEqual(session.topics().map(\.id), ["category:weekend"])
        XCTAssertEqual(session.topicPages["category:weekend"]?.messages.map(\.contactId), ["friends@g.us"])
        session.applyTopicCatalog(.empty)
        XCTAssertNil(session.topicPages["category:weekend"])
    }

    func testEmbeddedReplyQuoteSurvivesLiveDeliveryAndCacheWithoutOriginalHistory() throws {
        var value = try JSONSerialization.jsonObject(with: JSONEncoder().encode(message(body: "Thanks!"))) as! [String: Any]
        value["content"] = ["type": "text", "body": "Thanks!", "reply_context": ["messageId": "not-in-history", "senderName": "Alex", "text": "Yep I can"]]
        let reply = try JSONDecoder().decode(ChatMessage.self, from: JSONSerialization.data(withJSONObject: value))
        let session = AppSession(demoMode: false)
        session.phase = .ready
        let live = try JSONDecoder().decode(LiveEvent.self, from: JSONSerialization.data(withJSONObject: ["type": "message", "message": value]))
        session.handle(live)
        XCTAssertEqual(session.unifiedMessages.first?.content?.replyContext?.text, "Yep I can")
        XCTAssertEqual(session.messages[reply.contactId]?.first?.content?.replyContext?.senderName, "Alex")
        XCTAssertFalse(session.unifiedMessages.contains { $0.id == "not-in-history" })
        let cached = try JSONDecoder().decode(ChatMessage.self, from: JSONEncoder().encode(reply))
        XCTAssertEqual(cached.content?.replyContext?.messageId, "not-in-history")
    }

    func testCombinedOrganisingCountIgnoresPausedChats() {
        let catalog = TopicCatalog(topics: [], settings: [
            TopicSetting(contactId: "paused", enabled: false, pendingCount: 3, failedCount: 1),
            TopicSetting(contactId: "active", enabled: true, pendingCount: 2, failedCount: 0)
        ], available: true)
        XCTAssertEqual(catalog.pendingCount(), 2)
        XCTAssertEqual(catalog.pendingCount(contactID: "paused"), 0)
        XCTAssertEqual(catalog.pendingCount(contactID: "active"), 2)
    }

    func testMessageTopicMenuLoadsSavedAssignmentAndRejectsStaleEditMetadata() async throws {
        let configuration = URLSessionConfiguration.ephemeral
        configuration.protocolClasses = [TopicRefreshProtocol.self]
        let urlSession = URLSession(configuration: configuration)
        defer { urlSession.invalidateAndCancel(); TopicRefreshProtocol.state.reset() }
        let api = APIClient(session: urlSession)
        await api.configure(try ServerConfiguration.make(address: "https://topic-refresh.example.test", password: "test"))
        let session = AppSession(api: api, demoMode: false)
        let original = try message(body: "Hospital visit")
        session.messages[original.contactId] = [original]
        session.feedByID[original.id] = original
        TopicRefreshProtocol.state.responses = ["/api/topics/messages": Data(#"{"topics":[{"messageId":"original","contactId":"family@g.us","revision":0,"title":"Hospital","state":"assigned"}]}"#.utf8)]
        await session.refreshMessageTopics([original])
        XCTAssertEqual(session.messageTopicLabel(for: original), "Hospital")
        let edit = try message(body: "Shopping now", revision: 200)
        try session.handle(update(edit))
        XCTAssertNotEqual(session.messageTopicLabel(for: edit), "Hospital")
        await session.refreshMessageTopics([edit]) // old HTTP response is rejected
        XCTAssertNotEqual(session.messageTopicLabel(for: edit), "Hospital")
        TopicRefreshProtocol.state.responses = ["/api/topics/messages": Data(#"{"topics":[{"messageId":"original","contactId":"family@g.us","revision":200,"title":null,"state":"pending"}]}"#.utf8)]
        await session.refreshMessageTopics([edit])
        XCTAssertEqual(session.messageTopicLabel(for: edit), "Organising…")
        TopicRefreshProtocol.state.responses = ["/api/topics/messages": Data(#"{"topics":[{"messageId":"original","contactId":"family@g.us","revision":200,"title":"Shopping","state":"assigned"}]}"#.utf8)]
        await session.refreshMessageTopics([edit])
        XCTAssertEqual(session.messageTopicLabel(for: edit), "Shopping")
        XCTAssertFalse(TopicRefreshProtocol.state.requests.contains { $0.contains("/read") })
        for (state, label) in [("failed", "Needs retry"), ("off", "Off for this chat"), ("unassigned", "Not categorised")] {
            XCTAssertEqual(MessageTopic(messageId: "a", contactId: "x", revision: 0, title: nil, state: state).menuLabel, "\(label)")
        }
    }

    func testForegroundRefreshRecoversTopicsClassifiedWithoutALiveEvent() async throws {
        let configuration = URLSessionConfiguration.ephemeral
        configuration.protocolClasses = [TopicRefreshProtocol.self]
        let urlSession = URLSession(configuration: configuration)
        defer { urlSession.invalidateAndCancel(); TopicRefreshProtocol.state.reset() }
        let api = APIClient(session: urlSession)
        await api.configure(try ServerConfiguration.make(address: "https://topic-refresh.example.test", password: "test"))
        let session = AppSession(api: api, demoMode: false)
        session.phase = .ready
        session.mainTab = .chats
        let old = try message(body: "Visiting the hospital")
        let incoming = try message(id: "newly-classified", body: "The visit went well")
        let topic = ChatTopic(id: "hospital", contactId: old.contactId, contactName: "Family", title: "Hospital", messageCount: 1, lastMessageTime: 100, categoryId: "category:hospital")
        session.applyTopicCatalog(TopicCatalog(topics: [topic], settings: [], available: true))
        session.topicPages[topic.id] = TopicPage(messages: [old])
        session.topicPages["category:hospital"] = TopicPage(messages: [old])
        let page = try JSONSerialization.data(withJSONObject: ["messages": [old, incoming].map { try JSONSerialization.jsonObject(with: JSONEncoder().encode($0)) }, "hasMore": false])
        TopicRefreshProtocol.state.responses = [
            "/api/status": Data(#"{"connected":true}"#.utf8),
            "/api/contacts": Data("[]".utf8),
            "/api/topics": Data(#"{"available":true,"settings":[{"contactId":"family@g.us","enabled":true,"pendingCount":0,"failedCount":0}],"topics":[{"id":"hospital","categoryId":"category:hospital","contactId":"family@g.us","contactName":"Family","title":"Hospital","messageCount":2,"lastMessageTime":200}]}"#.utf8),
            "/api/topics/hospital/messages": page,
            "/api/topics/category:hospital/messages": page,
        ]
        // No topics_updated event is delivered: returning to the app must catch up itself.
        await session.becameActive()
        XCTAssertEqual(session.topics().first?.messageCount, 2)
        XCTAssertEqual(session.topicPages[topic.id]?.messages.map(\.id), [old.id, incoming.id].sorted())
        XCTAssertEqual(session.topicPages["category:hospital"]?.messages.count, 2)
        XCTAssertTrue(TopicRefreshProtocol.state.requests.contains("/api/topics"))
        XCTAssertFalse(TopicRefreshProtocol.state.requests.contains { $0.contains("mark-read") })
    }
}

private final class TopicRefreshProtocol: URLProtocol, @unchecked Sendable {
    static let state = TopicRefreshState()
    override class func canInit(with request: URLRequest) -> Bool { request.url?.host == "topic-refresh.example.test" }
    override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }
    override func startLoading() {
        let data = Self.state.respond(to: request)
        client?.urlProtocol(self, didReceive: HTTPURLResponse(url: request.url!, statusCode: 200, httpVersion: nil, headerFields: ["Content-Type": "application/json"])!, cacheStoragePolicy: .notAllowed)
        client?.urlProtocol(self, didLoad: data)
        client?.urlProtocolDidFinishLoading(self)
    }
    override func stopLoading() {}
}

private final class TopicRefreshState: @unchecked Sendable {
    private let lock = NSLock()
    private var payloads: [String: Data] = [:]
    private var recorded: [String] = []
    var responses: [String: Data] { get { lock.withLock { payloads } } set { lock.withLock { payloads = newValue } } }
    var requests: [String] { lock.withLock { recorded } }
    func respond(to request: URLRequest) -> Data {
        lock.withLock {
            let path = request.url?.path ?? ""
            recorded.append(path)
            return payloads[path] ?? Data("{}".utf8)
        }
    }
    func reset() { lock.withLock { payloads = [:]; recorded = [] } }
}
