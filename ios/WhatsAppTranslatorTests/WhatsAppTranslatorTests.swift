import XCTest
@testable import WhatsAppTranslator

final class WhatsAppTranslatorTests: XCTestCase {
    func testServerConfigurationNormalizesAddress() throws {
        let configuration = try ServerConfiguration.make(
            address: " translator.example.com/ ",
            password: "secret"
        )
        XCTAssertEqual(configuration.baseURL.absoluteString, "https://translator.example.com")
        XCTAssertEqual(configuration.password, "secret")
    }

    func testContactAndMessageDecodeBackendPayloads() throws {
        let contactData = Data(#"{"id":"chat@g.us","name":"Family","phone":null,"type":"group","lastMessageTime":1700000000000,"unreadCount":3,"pinnedAt":null,"lastMessagePreview":"Hello"}"#.utf8)
        let contact = try JSONDecoder().decode(Contact.self, from: contactData)
        XCTAssertTrue(contact.isGroup)
        XCTAssertEqual(contact.displayName, "Family")

        let messageData = Data(#"{"id":"m1","contactId":"chat@g.us","timestamp":1700000000000,"isFromMe":false,"isForwarded":false,"senderName":"Virag","senderPhone":null,"contactName":"Family","contactPhone":null,"chatType":"group","contentType":"Text","content":{"type":"text","body":"Szia"},"originalText":"Szia","translatedText":"Hello","sourceLanguage":"Hungarian","isTranslated":true}"#.utf8)
        let message = try JSONDecoder().decode(ChatMessage.self, from: messageData)
        XCTAssertEqual(message.displayText, "Hello")
        XCTAssertEqual(message.alternateText, "Szia")
    }

    func testDraftStorePersistsAndRemovesPerChatDrafts() throws {
        let suite = "WhatsAppTranslatorTests-\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        let store = DraftStore(defaults: defaults)

        store.save("Still typing", for: "chat-1")
        XCTAssertEqual(store.text(for: "chat-1"), "Still typing")
        store.save("   ", for: "chat-1")
        XCTAssertEqual(store.text(for: "chat-1"), "")
    }

    func testPushDeviceRegistrationEncodesBackendContract() throws {
        let registration = PushDeviceRegistration(
            installationId: "installation-1",
            token: "0123456789abcdef",
            environment: "sandbox"
        )

        let data = try JSONEncoder().encode(registration)
        let payload = try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [String: String])

        XCTAssertEqual(payload["installationId"], "installation-1")
        XCTAssertEqual(payload["token"], "0123456789abcdef")
        XCTAssertEqual(payload["environment"], "sandbox")
    }
}
