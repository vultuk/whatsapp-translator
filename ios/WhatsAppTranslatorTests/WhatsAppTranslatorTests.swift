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

    func testChatCachePersistsMessagesForTheConfiguredServer() async throws {
        let directory = FileManager.default.temporaryDirectory
            .appending(path: "WhatsAppTranslatorTests-\(UUID().uuidString)", directoryHint: .isDirectory)
        defer { try? FileManager.default.removeItem(at: directory) }

        let configuration = try ServerConfiguration.make(
            address: "https://translator.example.com",
            password: "secret"
        )
        let contact = try JSONDecoder().decode(
            Contact.self,
            from: Data(#"{"id":"chat@g.us","name":"Family","phone":null,"type":"group","lastMessageTime":1700000000000,"unreadCount":3,"pinnedAt":null,"lastMessagePreview":"Hello"}"#.utf8)
        )
        let message = try JSONDecoder().decode(
            ChatMessage.self,
            from: Data(#"{"id":"m1","contactId":"chat@g.us","timestamp":1700000000000,"isFromMe":false,"isForwarded":false,"senderName":"Virag","senderPhone":null,"contactName":"Family","contactPhone":null,"chatType":"group","contentType":"Text","content":{"type":"text","body":"Szia"},"originalText":"Szia","translatedText":"Hello","sourceLanguage":"Hungarian","isTranslated":true}"#.utf8)
        )
        let store = ChatCacheStore(directoryURL: directory)

        await store.save(
            ChatCacheSnapshot(
                serverBaseURL: configuration.baseURL.absoluteString,
                contacts: [contact],
                messages: [contact.id: [message]],
                updatedAt: Date(timeIntervalSince1970: 1_700_000_000)
            )
        )

        let restored = await store.load(for: configuration)
        XCTAssertEqual(restored?.contacts, [contact])
        XCTAssertEqual(restored?.messages[contact.id], [message])
    }

    func testChatCacheDoesNotRestoreAnotherServer() async throws {
        let directory = FileManager.default.temporaryDirectory
            .appending(path: "WhatsAppTranslatorTests-\(UUID().uuidString)", directoryHint: .isDirectory)
        defer { try? FileManager.default.removeItem(at: directory) }
        let first = try ServerConfiguration.make(address: "https://one.example.com", password: "secret")
        let second = try ServerConfiguration.make(address: "https://two.example.com", password: "secret")
        let store = ChatCacheStore(directoryURL: directory)

        await store.save(
            ChatCacheSnapshot(
                serverBaseURL: first.baseURL.absoluteString,
                contacts: [],
                messages: [:],
                updatedAt: Date()
            )
        )

        let restored = await store.load(for: second)
        XCTAssertNil(restored)
    }

    func testOlderChatCacheWriteCannotReplaceNewerBackgroundData() async throws {
        let directory = FileManager.default.temporaryDirectory
            .appending(path: "WhatsAppTranslatorTests-\(UUID().uuidString)", directoryHint: .isDirectory)
        defer { try? FileManager.default.removeItem(at: directory) }
        let configuration = try ServerConfiguration.make(
            address: "https://translator.example.com",
            password: "secret"
        )
        let store = ChatCacheStore(directoryURL: directory)
        let newerDate = Date(timeIntervalSince1970: 1_700_000_100)

        await store.save(
            ChatCacheSnapshot(
                serverBaseURL: configuration.baseURL.absoluteString,
                contacts: [],
                messages: [:],
                updatedAt: newerDate
            )
        )
        await store.save(
            ChatCacheSnapshot(
                serverBaseURL: configuration.baseURL.absoluteString,
                contacts: [],
                messages: [:],
                updatedAt: Date(timeIntervalSince1970: 1_700_000_000)
            )
        )

        let restored = await store.load(for: configuration)
        XCTAssertEqual(restored?.updatedAt, newerDate)
    }

    func testMessageContentDecodesRichMediaAndReactionContracts() throws {
        let image = try JSONDecoder().decode(
            MessageContent.self,
            from: Data(#"{"type":"image","caption":"A view","mime_type":"image/jpeg","has_media":true,"file_size":2048}"#.utf8)
        )
        XCTAssertEqual(image.caption, "A view")
        XCTAssertEqual(image.mimeType, "image/jpeg")
        XCTAssertEqual(image.hasMedia, true)
        XCTAssertEqual(image.fileSize, 2_048)

        let reaction = try JSONDecoder().decode(
            MessageContent.self,
            from: Data(#"{"type":"reaction","emoji":"❤️","target_message_id":"m1"}"#.utf8)
        )
        XCTAssertEqual(reaction.emoji, "❤️")
        XCTAssertEqual(reaction.targetMessageId, "m1")
    }

    func testSendMessageRequestEncodesReplyContext() throws {
        let request = SendMessageRequest(
            contactId: "chat@g.us",
            text: "On my way",
            replyTo: "message-1",
            replyToSender: "447700900123@s.whatsapp.net",
            replyToText: "Where are you?",
            replyToSenderName: "Virág"
        )

        let data = try JSONEncoder().encode(request)
        let payload = try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [String: Any])
        XCTAssertEqual(payload["replyTo"] as? String, "message-1")
        XCTAssertEqual(payload["replyToSender"] as? String, "447700900123@s.whatsapp.net")
        XCTAssertEqual(payload["replyToText"] as? String, "Where are you?")
        XCTAssertEqual(payload["replyToSenderName"] as? String, "Virág")
    }

    func testAppPreferencesPersistStarsConversationPresentationAndTheme() throws {
        let suite = "WhatsAppTranslatorPreferencesTests-\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }

        var store = AppPreferencesStore(defaults: defaults)
        store.toggleStar(messageID: "m1", contactID: "chat-1")
        store.setConversationPreferences(
            ConversationPresentationPreferences(nickname: "Mum", timezoneIdentifier: "Europe/Budapest"),
            for: "chat-1"
        )
        store.theme = .dracula
        store.colorMode = .dark

        store = AppPreferencesStore(defaults: defaults)
        XCTAssertTrue(store.isStarred(messageID: "m1", contactID: "chat-1"))
        XCTAssertEqual(store.conversationPreferences(for: "chat-1").nickname, "Mum")
        XCTAssertEqual(store.conversationPreferences(for: "chat-1").timezoneIdentifier, "Europe/Budapest")
        XCTAssertEqual(store.theme, .dracula)
        XCTAssertEqual(store.colorMode, .dark)

        store.toggleStar(messageID: "m1", contactID: "chat-1")
        XCTAssertFalse(store.isStarred(messageID: "m1", contactID: "chat-1"))
    }

    func testLinkPreviewDecodesBackendContract() throws {
        let data = Data(#"{"url":"https://example.com/story","title":"A story","description":"Preview text","imageUrl":"https://example.com/image.jpg","siteName":"Example"}"#.utf8)
        let preview = try JSONDecoder().decode(LinkPreview.self, from: data)
        XCTAssertEqual(preview.title, "A story")
        XCTAssertEqual(preview.imageURL?.absoluteString, "https://example.com/image.jpg")
        XCTAssertEqual(preview.siteName, "Example")
    }

    @MainActor
    func testReactionMessagesAreAppliedToTheirTargetAndHidden() throws {
        let target = try JSONDecoder().decode(
            ChatMessage.self,
            from: Data(#"{"id":"m1","contactId":"chat@g.us","timestamp":1700000000000,"isFromMe":false,"isForwarded":false,"senderName":"Virag","senderPhone":"3630","contactName":"Family","contactPhone":null,"chatType":"group","contentType":"Text","content":{"type":"text","body":"Hello"},"originalText":null,"translatedText":null,"sourceLanguage":null,"isTranslated":false}"#.utf8)
        )
        let reaction = try JSONDecoder().decode(
            ChatMessage.self,
            from: Data(#"{"id":"r1","contactId":"chat@g.us","timestamp":1700000000100,"isFromMe":false,"isForwarded":false,"senderName":"Virag","senderPhone":"3630","contactName":"Family","contactPhone":null,"chatType":"group","contentType":"Reaction","content":{"type":"reaction","emoji":"❤️","target_message_id":"m1"},"originalText":null,"translatedText":null,"sourceLanguage":null,"isTranslated":false}"#.utf8)
        )

        let normalized = AppSession(demoMode: false).normalizeMessages([target, reaction])
        XCTAssertEqual(normalized.map(\.id), ["m1"])
        XCTAssertEqual(normalized.first?.reactions?["❤️"], ["3630"])
    }
}
