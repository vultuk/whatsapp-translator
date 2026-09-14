import XCTest
#if os(macOS)
@testable import BabelBridgeMac
#else
@testable import WhatsAppTranslator
#endif

@MainActor
final class ReactionDeliveryTests: XCTestCase {
    private func message(_ id: String = "target", emoji: String? = nil, time: Int64 = 100) throws -> ChatMessage {
        var json: [String: Any] = ["id": id, "contactId": "family@g.us", "timestamp": time, "isFromMe": true, "isForwarded": false, "chatType": "group", "contentType": "Text", "content": ["type": "text", "body": "See you at six"], "isTranslated": false]
        if let emoji {
            json["contentType"] = "Reaction"
            json["content"] = ["type": "reaction", "target_message_id": "target", "emoji": emoji]
        }
        return try JSONDecoder().decode(ChatMessage.self, from: JSONSerialization.data(withJSONObject: json))
    }

    func testConfirmedSendUpdatesBothViewsWithoutSocketOrRefreshAndRejectsReplays() async throws {
        let configuration = URLSessionConfiguration.ephemeral
        configuration.protocolClasses = [ReactionDeliveryProtocol.self]
        let urlSession = URLSession(configuration: configuration)
        defer { urlSession.invalidateAndCancel(); ReactionDeliveryProtocol.state.reset() }
        let api = APIClient(session: urlSession)
        await api.configure(try ServerConfiguration.make(address: "https://reaction-delivery.example.test", password: "test"))
        let session = AppSession(api: api, demoMode: false)
        session.phase = .ready
        let target = try message()
        session.messages[target.contactId] = [target]
        session.feedByID[target.id] = target
        let heart = try message("z-heart", emoji: "❤️", time: 1700000000100)
        for (index, emoji) in ["❤️", "👍", ""].enumerated() {
            // IDs deliberately descend; the sender's millisecond clock determines order.
            let reaction = try message(index == 0 ? heart.id : "a-\(index)", emoji: emoji, time: 1700000000100 + Int64(index))
            let payload = try JSONSerialization.jsonObject(with: JSONEncoder().encode(reaction))
            ReactionDeliveryProtocol.state.response = try JSONSerialization.data(withJSONObject: ["success": true, "reaction": payload])
            await session.react(to: target, emoji: emoji)
            session.handle(.reaction(heart))
            let expected: [String: [String]] = emoji.isEmpty ? [:] : [emoji: ["me"]]
            XCTAssertEqual(session.messages[target.contactId]?.first?.reactions, expected)
            XCTAssertEqual(session.unifiedMessages.first?.reactions, expected)
            XCTAssertEqual(session.unifiedMessages.count, 1)
        }
        XCTAssertEqual(ReactionDeliveryProtocol.state.requests, Array(repeating: "POST /api/react", count: 3))
        XCTAssertNil(session.errorMessage)
    }

    func testLegacyConfirmationUpdatesLocallyButFailureKeepsExistingChoice() async throws {
        let configuration = URLSessionConfiguration.ephemeral
        configuration.protocolClasses = [ReactionDeliveryProtocol.self]
        let urlSession = URLSession(configuration: configuration)
        defer { urlSession.invalidateAndCancel(); ReactionDeliveryProtocol.state.reset() }
        let api = APIClient(session: urlSession)
        await api.configure(try ServerConfiguration.make(address: "https://reaction-delivery.example.test", password: "test"))
        let session = AppSession(api: api, demoMode: false)
        session.phase = .ready
        let target = try message()
        session.messages[target.contactId] = [target]
        session.feedByID[target.id] = target
        ReactionDeliveryProtocol.state.response = Data(#"{"success":true}"#.utf8)
        await session.react(to: target, emoji: "❤️")
        XCTAssertEqual(session.unifiedMessages.first?.ownReactionEmoji, "❤️")
        ReactionDeliveryProtocol.state.response = Data(#"{"success":false,"error":"Rejected"}"#.utf8)
        await session.react(to: target, emoji: "👍")
        XCTAssertEqual(session.messages[target.contactId]?.first?.ownReactionEmoji, "❤️")
        XCTAssertNotNil(session.errorMessage)
    }

    func testSnapshotRemovalSurvivesCacheRestartAndStaleEvents() throws {
        let session = AppSession(demoMode: false)
        var target = try message()
        target.reactions = [:]
        target.reactionStates = ["me": MessageReactionState(id: "remove", timestamp: 300, emoji: "")]
        let saved = try JSONEncoder().encode(target)
        let restored = try JSONDecoder().decode(ChatMessage.self, from: saved)
        var values = session.normalizeMessages([restored, try message("old", emoji: "❤️", time: 200)])
        XCTAssertEqual(values.first?.reactions, [:])
        values = session.normalizeMessages(values + [try message("new", emoji: "👍", time: 301)])
        XCTAssertEqual(values.first?.reactions, ["👍": ["me"]])
        XCTAssertEqual(session.normalizeMessages([target]).first?.reactions, ["👍": ["me"]])
    }

    func testEmojiPickerAcceptsComposedEmojiAndRejectsText() {
        for emoji in ["❤️", "👍🏽", "👨‍👩‍👧‍👦", "🇬🇧", "1️⃣"] { XCTAssertTrue(MessageReactionChoices.isSingleEmoji(emoji), emoji) }
        for invalid in ["", "hello", "1", "👍❤️"] { XCTAssertFalse(MessageReactionChoices.isSingleEmoji(invalid), invalid) }
    }
}

private final class ReactionDeliveryProtocol: URLProtocol, @unchecked Sendable {
    static let state = ReactionDeliveryState()
    override class func canInit(with request: URLRequest) -> Bool { request.url?.host == "reaction-delivery.example.test" }
    override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }
    override func startLoading() {
        let data = Self.state.respond(to: request)
        client?.urlProtocol(self, didReceive: HTTPURLResponse(url: request.url!, statusCode: 200, httpVersion: nil, headerFields: ["Content-Type": "application/json"])!, cacheStoragePolicy: .notAllowed)
        client?.urlProtocol(self, didLoad: data)
        client?.urlProtocolDidFinishLoading(self)
    }
    override func stopLoading() {}
}

private final class ReactionDeliveryState: @unchecked Sendable {
    private let lock = NSLock()
    private var data = Data()
    private var recorded: [String] = []
    var response: Data { get { lock.withLock { data } } set { lock.withLock { data = newValue } } }
    var requests: [String] { lock.withLock { recorded } }
    func respond(to request: URLRequest) -> Data {
        lock.withLock { recorded.append("\(request.httpMethod ?? "") \(request.url?.path ?? "")"); return data }
    }
    func reset() { lock.withLock { recorded = []; data = Data() } }
}
