import CarPlay
import Intents
import UserNotifications
import XCTest
@testable import WhatsAppTranslator

final class CarPlayMessagingTests: XCTestCase {
    func testTappedNotificationReadsOnlyItsTranslatedMessageAndKeepsReplyDestination() throws {
        let first = try notification("alert-one", chat: "family@g.us", message: "message-one", text: "We will arrive at six.")
        let other = try notification("alert-two", chat: "other@g.us", message: "message-two", text: "An unrelated message.")
        let filter = MessageSearchFilter(intent: search(notifications: ["alert-one"]))
        let results = filter.notificationResults([other, first])
        XCTAssertEqual(results.count, 1)
        let result = try XCTUnwrap(results.first)
        XCTAssertEqual(result.intentMessage.content, "We will arrive at six.")
        XCTAssertEqual(result.intentMessage.conversationIdentifier, "family@g.us")
        XCTAssertEqual(result.intentMessage.groupName?.spokenPhrase, "Family")
        XCTAssertEqual(MessagingMessageIdentity.decode(result.intentMessage.identifier), MessagingMessageIdentity(contactID: "family@g.us", messageID: "message-one"))
    }

    func testMissingNotificationDoesNotReadAnotherDeliveredAlert() throws {
        let other = try notification("unrelated", chat: "other@g.us", message: "other", text: "Private message")
        let filter = MessageSearchFilter(intent: search(notifications: ["expired"]))
        XCTAssertTrue(filter.notificationResults([other]).isEmpty)
        let content = UNMutableNotificationContent()
        content.body = "Missing routing data"
        XCTAssertNil(MessagingIntentNotification(identifier: "expired", content: content, date: Date()))
    }

    func testExplicitConversationCannotExpandToAnotherChatWithTheSameName() {
        let first = contact("one@g.us", name: "Family")
        let other = contact("two@g.us", name: "Family")
        let intent = INSearchForMessagesIntent(recipients: nil, senders: nil, searchTerms: nil, attributes: [], dateTime: nil, identifiers: nil, notificationIdentifiers: nil, speakableGroupNames: [INSpeakableString(spokenPhrase: "Family")], conversationIdentifiers: [first.id])
        XCTAssertEqual(MessageSearchFilter(intent: intent).selectContacts(from: [other, first]).map(\.id), [first.id])
    }

    func testUnreadSearchOmitsOutgoingOlderAndReactionMessages() throws {
        let chat = contact("family@g.us", unread: 1)
        let older = try message("older", chat: chat.id, time: 1)
        let incoming = try message("new", chat: chat.id, time: 2, translated: "See you soon")
        let outgoing = try message("outgoing", chat: chat.id, time: 3, fromMe: true)
        let reaction = try message("reaction", chat: chat.id, time: 4, contentType: "reaction")
        let results = MessageSearchFilter(intent: search(conversations: [chat.id], attributes: .unread))
            .results(contact: chat, messages: [reaction, older, outgoing, incoming])
        XCTAssertEqual(results.map(\.message.id), [incoming.id])
        XCTAssertEqual(results.first?.intentMessage.content, "See you soon")
        let allRead = contact(chat.id, unread: 0)
        XCTAssertTrue(MessageSearchFilter(intent: search(attributes: .unread)).selectContacts(from: [allRead]).isEmpty)
    }

    func testStableRecipientIdentityWinsOverDuplicateNames() {
        let first = contact("alex-one@s.whatsapp.net", name: "Alex")
        let other = contact("alex-two@s.whatsapp.net", name: "Alex")
        let exact = RecipientQuery(first.intentPerson)
        XCTAssertEqual([other, first].matching(exact).map(\.id), [first.id])
        let ambiguous = RecipientQuery(INPerson(personHandle: INPersonHandle(value: "Alex", type: .unknown), nameComponents: nil, displayName: "Alex", image: nil, contactIdentifier: nil, customIdentifier: nil))
        XCTAssertEqual([other, first].matching(ambiguous).count, 2)
        let removed = RecipientQuery(INPerson.messagingPerson(id: "removed", name: "Alex"))
        XCTAssertTrue([other, first].matching(removed).isEmpty)
    }

    func testMessageIdentityRetainsExactChatForReadReceiptsAndSearch() throws {
        let identity = MessagingMessageIdentity(contactID: "group:with/slash@g.us", messageID: "message:/+ü")
        XCTAssertEqual(MessagingMessageIdentity.decode(identity.encoded), identity)
        XCTAssertNil(MessagingMessageIdentity.decode("old-message-id"))
        XCTAssertNil(MessagingMessageIdentity.decode("babelbridge.message:invalid"))
        let chat = contact(identity.contactID)
        let wrong = contact("other@g.us")
        let filter = MessageSearchFilter(intent: search(identifiers: [identity.encoded]))
        XCTAssertEqual(filter.selectContacts(from: [wrong, chat]).map(\.id), [chat.id])
        XCTAssertTrue(filter.includes(IntentMessageResult(contact: chat, message: try message(identity.messageID, chat: chat.id, time: 1))))
        XCTAssertFalse(filter.includes(IntentMessageResult(contact: wrong, message: try message(identity.messageID, chat: wrong.id, time: 1))))
    }

    @MainActor
    func testCarPlayListPrioritizesUnreadAndExcludesStatusWithinTheCarLimit() throws {
        let read = contact("read", name: "Sam", unread: 0)
        let unread = contact("unread@g.us", name: "Family", unread: 3)
        let status = contact("status@broadcast", name: "Status", unread: 9)
        let sections = CarPlayConversations.sections([status, read, unread], limit: 1)
        XCTAssertEqual(sections.count, 1)
        XCTAssertEqual(sections.first?.header, "Unread")
        let item = try XCTUnwrap(sections.first?.items.first as? CPMessageListItem)
        XCTAssertEqual(item.conversationIdentifier, unread.id)
        XCTAssertTrue(item.leadingConfiguration.isUnread)
        XCTAssertEqual(item.detailText, "3 unread messages")
        XCTAssertTrue(CarPlayConversations.sections([status], limit: 12).isEmpty)
        XCTAssertFalse(CarPlayConversations.item(read).leadingConfiguration.isUnread)
    }

    private func search(conversations: [String]? = nil, notifications: [String]? = nil, identifiers: [String]? = nil, attributes: INMessageAttributeOptions = []) -> INSearchForMessagesIntent {
        INSearchForMessagesIntent(recipients: nil, senders: nil, searchTerms: nil, attributes: attributes, dateTime: nil, identifiers: identifiers, notificationIdentifiers: notifications, speakableGroupNames: nil, conversationIdentifiers: conversations)
    }

    private func contact(_ id: String, name: String = "Family", unread: Int = 0) -> Contact {
        Contact(id: id, name: name, phone: nil, type: id.hasSuffix("@g.us") ? "group" : "private", lastMessageTime: 10, unreadCount: unread, pinnedAt: nil, lastMessagePreview: "Never show this body while driving")
    }

    private func message(_ id: String, chat: String, time: Int64, fromMe: Bool = false, contentType: String = "text", translated: String? = nil) throws -> ChatMessage {
        var values: [String: Any] = ["id": id, "contactId": chat, "timestamp": time, "isFromMe": fromMe, "isForwarded": false, "chatType": "group", "contentType": contentType, "isTranslated": translated != nil]
        values["translatedText"] = translated
        return try JSONDecoder().decode(ChatMessage.self, from: JSONSerialization.data(withJSONObject: values))
    }

    private func notification(_ id: String, chat: String, message: String, text: String) throws -> MessagingIntentNotification {
        let content = UNMutableNotificationContent()
        content.title = "Alex"
        content.subtitle = "Family"
        content.body = text
        content.userInfo = ["contactId": chat, "messageId": message, "senderId": "alex", "senderName": "Alex", "conversationName": "Family", "chatType": "group"]
        return try XCTUnwrap(MessagingIntentNotification(identifier: id, content: content, date: Date(timeIntervalSince1970: 1)))
    }
}
