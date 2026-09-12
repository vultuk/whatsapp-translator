import XCTest
import CarPlay
@testable import WhatsAppTranslator

@MainActor
final class WatchMessagingTests: XCTestCase {
    func testWatchDraftKeepsSelectedMessageAndRequestAcrossNewMessagesAndRestart() throws {
        let snapshot = WatchFeedSnapshot.demo
        let selected = try XCTUnwrap(snapshot.messages.first)
        var draft = WatchReplyDraft(target: selected, accountID: snapshot.accountID)
        draft.text = "  See you there  "
        let submission = draft.submission()
        let data = try JSONEncoder().encode(draft)
        var restored = try JSONDecoder().decode(WatchReplyDraft.self, from: data)
        XCTAssertEqual(restored.submission(), submission)
        XCTAssertEqual(submission.target.messageID, selected.messageID)
        XCTAssertNotEqual(submission.target.contactID, snapshot.messages.last?.contactID)
        XCTAssertEqual(submission.text, "See you there")
    }

    func testWatchSnapshotFitsConnectivityBudgetWithoutLosingLatestMessage() throws {
        let messages = (0..<30).map { index in
            WatchMessage(messageID: String(index), contactID: "group", conversation: "Family", sender: "Jordan",
                         text: String(repeating: "👩🏽‍🚀", count: 1_000), timestamp: Int64(index), isFromMe: false, isGroup: true)
        }
        let data = try WatchFeedSnapshot(accountID: "account", messages: messages, updatedAt: Date()).encoded()
        XCTAssertLessThanOrEqual(data.count, 48_000)
        XCTAssertLessThanOrEqual(try WatchResponse(status: .feed, snapshot: data).encoded().count, 65_536)
        let decoded = try JSONDecoder().decode(WatchFeedSnapshot.self, from: data)
        XCTAssertEqual(decoded.messages.last?.messageID, "29")
        XCTAssertFalse(decoded.messages.isEmpty)
    }

    func testReceiptSurvivesPhoneRestartAndDoesNotDependOnJSONKeyOrder() throws {
        let directory = URL.temporaryDirectory.appending(path: UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: directory) }
        let store = WatchSendReceipts(directory: directory)
        var draft = WatchReplyDraft(target: WatchFeedSnapshot.demo.messages[0], accountID: "account", text: "Hello")
        let reply = draft.submission()
        let signature = try WatchSendReceipts.signature(reply)
        for _ in 0..<10 { XCTAssertEqual(try WatchSendReceipts.signature(reply), signature) }
        try store.write(reply.requestID, signature: signature, sent: false)
        XCTAssertEqual(try WatchSendReceipts(directory: directory).read(reply.requestID)?.sent, false)
        try store.write(reply.requestID, signature: signature, sent: true)
        XCTAssertEqual(try WatchSendReceipts(directory: directory).read(reply.requestID)?.sent, true)
        XCTAssertNil(try store.read(UUID()))
    }

    func testCarPlayUnifiedTimelineInterleavesChatsAndRetainsExactMessageRoute() throws {
        let messages = CarPlayConversations.demoMessages
        let sections = CarPlayConversations.messageSections(messages.reversed(), contacts: CarPlayConversations.demoContacts, limit: 3)
        let items = try XCTUnwrap(sections.first?.items as? [CPMessageListItem])
        let identities = items.compactMap { $0.conversationIdentifier.flatMap(MessagingMessageIdentity.decode) }
        XCTAssertEqual(identities.map(\.messageID), messages.map(\.id))
        XCTAssertEqual(identities.map(\.contactID), messages.map(\.contactId))
        XCTAssertEqual(identities.first?.contactID, identities.last?.contactID)
        XCTAssertNotEqual(identities.first?.contactID, identities[1].contactID)
        XCTAssertFalse(items.contains { $0.detailText?.contains("See you soon") == true })
        XCTAssertTrue(CarPlayConversations.messageSections(messages, contacts: [], limit: 12).isEmpty)
    }

    func testWatchFeedExcludesStatusReactionsAndDeletedMessagesAndKeepsTranslatedText() throws {
        let contacts = CarPlayConversations.demoContacts
        var messages = CarPlayConversations.demoMessages
        let base = messages[0]
        for type in ["reaction", "revoked"] {
            messages.append(ChatMessage(id: type, contactId: base.contactId, timestamp: base.timestamp + 1,
                isFromMe: false, isForwarded: false, senderName: nil, senderPhone: nil, contactName: nil,
                contactPhone: nil, chatType: "private", contentType: type, content: nil, originalText: nil,
                translatedText: "Excluded", sourceLanguage: nil, isTranslated: true))
        }
        let snapshot = WatchMessagingBackend.snapshot(messages: messages, contacts: contacts, accountID: "account")
        XCTAssertEqual(snapshot.messages.count, 3)
        XCTAssertEqual(snapshot.messages.last?.messageID, base.id)
        XCTAssertEqual(snapshot.messages.last?.text, "See you soon.")
        XCTAssertTrue(WatchMessagingBackend.snapshot(messages: messages, contacts: [], accountID: "account").messages.isEmpty)
    }
}
