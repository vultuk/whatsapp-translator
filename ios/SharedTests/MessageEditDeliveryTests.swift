import XCTest
#if os(macOS)
@testable import BabelBridgeMac
#else
@testable import WhatsAppTranslator
#endif

@MainActor
final class MessageEditDeliveryTests: XCTestCase {
    private func message(chat: String = "family@g.us", fromMe: Bool = false, body: String, revision: Int64 = 0) throws -> ChatMessage {
        var content: [String: Any] = ["type": "text", "body": body]
        if revision > 0 { content["edited_at_ms"] = revision }
        let json: [String: Any] = ["id": "original", "contactId": chat, "timestamp": 100,
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

    func testSameNamedTopicsRemainSeparateByChatAndDisablingDropsTheirPages() throws {
        let session = AppSession(demoMode: true)
        let family = ChatTopic(id: "family-plan", contactId: "family@g.us", contactName: "Family", title: "Weekend plans", messageCount: 1, lastMessageTime: 100)
        let friends = ChatTopic(id: "friends-plan", contactId: "friends@g.us", contactName: "Friends", title: "Weekend plans", messageCount: 1, lastMessageTime: 100)
        session.applyTopicCatalog(TopicCatalog(topics: [family, friends], settings: [], available: true))
        XCTAssertEqual(session.topics().count, 2)
        XCTAssertEqual(session.topics(for: "family@g.us").map(\.id), ["family-plan"])
        XCTAssertFalse(session.topicSetting(for: "unconfigured@g.us").enabled)
        session.topicPages[family.id] = TopicPage(messages: [try message(body: "Picnic")])
        session.applyTopicCatalog(TopicCatalog(topics: [friends], settings: [], available: true))
        XCTAssertNil(session.topicPages[family.id])
        XCTAssertEqual(session.topics().map(\.id), [friends.id])
    }
}
