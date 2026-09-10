import XCTest
import Intents
import SwiftUI
import UserNotifications
@testable import WhatsAppTranslator

final class WhatsAppTranslatorTests: XCTestCase {

    @MainActor
    func testUnifiedAttachmentsCaptureBeforePickingAndPreserveTextDraft() async throws {
        func message(_ id: String, _ contact: String, _ time: Int64) throws -> ChatMessage {
            let json: [String: Any] = ["id": id, "contactId": contact, "timestamp": time, "isFromMe": false, "isForwarded": false, "chatType": "group", "contentType": "Text", "isTranslated": false]
            return try JSONDecoder().decode(ChatMessage.self, from: JSONSerialization.data(withJSONObject: json))
        }
        let first = try message("selected", "one@g.us", 100)
        let newer = try message("newer", "one@g.us", 200)
        let elsewhere = try message("elsewhere", "two@g.us", 300)
        var draft = UnifiedReplyDraft()
        XCTAssertNil(draft.beginAttachment(latestMessage: nil))
        XCTAssertFalse(draft.isFocused)
        let target = try XCTUnwrap(draft.beginAttachment(latestMessage: first))
        XCTAssertTrue(draft.isFocused)
        XCTAssertEqual(draft.beginAttachment(latestMessage: elsewhere)?.id, first.id)
        draft.updateText("Keep this draft", latestMessage: newer)
        let session = AppSession(demoMode: true)
        session.messages = [first.contactId: [first, newer], elsewhere.contactId: [elsewhere]]
        await session.loadFeed()
        let attachment = OutgoingAttachment(data: Data("Test file".utf8), mimeType: "text/plain", fileName: "Note.txt", kind: "document")
        let sent = await session.sendAttachment(attachment, caption: "Attached", to: target.contactId, reply: session.replyTarget(for: target), replyOnlyIfNotLatest: true)
        XCTAssertTrue(sent)
        XCTAssertEqual(session.messages[first.contactId]?.last?.content?.replyContext?.messageId, first.id)
        XCTAssertEqual(session.messages[elsewhere.contactId]?.count, 1)
        XCTAssertEqual(session.unifiedMessages.last?.contactId, first.contactId)
        draft.finishMediaSending(to: target)
        XCTAssertNil(draft.selected)
        XCTAssertEqual(draft.drafts[first.contactId], "Keep this draft")
        draft.select(elsewhere)
        XCTAssertEqual(draft.beginAttachment(latestMessage: newer)?.id, elsewhere.id)
        let normal = await session.sendAttachment(attachment, caption: nil, to: elsewhere.contactId, reply: session.replyTarget(for: elsewhere), replyOnlyIfNotLatest: true)
        XCTAssertTrue(normal)
        XCTAssertNil(session.messages[elsewhere.contactId]?.last?.content?.replyContext)
        draft.cancelSelection()
        XCTAssertEqual(draft.beginAttachment(latestMessage: newer)?.id, newer.id)
    }

    func testUnifiedAttachmentRequestsRetainTargetAndConditionalQuote() throws {
        let media = SendImageRequest(mediaKind: "video", fileName: "Video.mp4", replyOnlyIfNotLatest: true, contactId: "one@g.us", mediaData: "ZmlsZQ==", mimeType: "video/mp4", caption: nil, replyTo: "selected", replyToSender: "sam@s.whatsapp.net", replyToText: "Original", replyToSenderName: "Sam")
        let encoded = try XCTUnwrap(JSONSerialization.jsonObject(with: JSONEncoder().encode(media)) as? [String: Any])
        XCTAssertEqual(encoded["contactId"] as? String, "one@g.us")
        XCTAssertEqual(encoded["replyTo"] as? String, "selected")
        XCTAssertEqual(encoded["replyOnlyIfNotLatest"] as? Bool, true)
        XCTAssertEqual(encoded["mediaKind"] as? String, "video")
        let album = CreatePhotoAlbumRequest(replyOnlyIfNotLatest: true, jobId: "album-job", contactId: "one@g.us", photoCount: 2, caption: nil, replyTo: "selected", replyToSender: nil, replyToText: nil, replyToSenderName: nil)
        let staged = try XCTUnwrap(JSONSerialization.jsonObject(with: JSONEncoder().encode(album)) as? [String: Any])
        XCTAssertEqual(staged["replyTo"] as? String, "selected")
        XCTAssertEqual(staged["replyOnlyIfNotLatest"] as? Bool, true)
        XCTAssertTrue(SendRecoveryStore.isSend(path: "/api/send-media", method: "POST"))
        let voice = PrepareVoiceRequest(contactId: "one@g.us", mediaData: "ZmlsZQ==", replyTo: "selected", replyToSender: nil, replyToText: "Original", replyOnlyIfNotLatest: true)
        let recording = try XCTUnwrap(JSONSerialization.jsonObject(with: JSONEncoder().encode(voice)) as? [String: Any])
        XCTAssertEqual(recording["replyTo"] as? String, "selected")
        XCTAssertEqual(recording["replyOnlyIfNotLatest"] as? Bool, true)
        XCTAssertNil(recording["replyToSender"])
    }

    @MainActor
    func testWallpaperUpgradePreservesExistingPreferencesAndPersistsSelection() throws {
        let suite = "WallpaperMigration-\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        let legacy = Data(#"{"starredMessageIDs":{"chat":["message"]},"conversations":{"chat":{"nickname":"Friend","timezoneIdentifier":"Europe/London"}},"theme":"ocean","colorMode":"dark"}"#.utf8)
        defaults.set(legacy, forKey: "whatsapp-translator-ios-preferences-v1")
        let store = AppPreferencesStore(defaults: defaults)
        XCTAssertEqual(store.wallpaper, .classic)
        XCTAssertEqual(store.theme, .ocean)
        XCTAssertEqual(store.colorMode, .dark)
        XCTAssertTrue(store.isStarred(messageID: "message", contactID: "chat"))
        XCTAssertEqual(store.nickname(for: "chat"), "Friend")
        store.wallpaper = .celestial
        let restored = AppPreferencesStore(defaults: defaults)
        XCTAssertEqual(restored.wallpaper, .celestial)
        XCTAssertEqual(restored.theme, .ocean)
        XCTAssertTrue(restored.isStarred(messageID: "message", contactID: "chat"))
    }

    func testAllTenGeneratedWallpaperAssetsAreBundled() throws {
        let assets = AppWallpaper.allCases.compactMap(\.assetName)
        XCTAssertEqual(assets.count, 10)
        XCTAssertEqual(Set(assets).count, 10)
        for asset in assets { XCTAssertNotNil(UIImage(named: asset), asset) }
    }

    func testOversizedPhotoIsAutomaticallyReducedBelowUploadLimit() throws {
        let image = DemoImageFactory.landscape(size: CGSize(width: 1_200, height: 900))
        var oversizedData = try XCTUnwrap(image.platformJPEGData(compressionQuality: 0.95))
        oversizedData.append(Data(repeating: 0, count: 17 * 1_024 * 1_024))

        let prepared = try XCTUnwrap(
            PhotoUploadPreparer.prepare(
                data: oversizedData,
                mimeType: "image/jpeg",
                image: image,
                maximumBytes: PhotoUploadPreparer.maximumBytes(forPhotoCount: 1)
            )
        )

        XCTAssertLessThanOrEqual(prepared.data.count, 15 * 1_024 * 1_024)
        XCTAssertEqual(prepared.mimeType, "image/jpeg")
        XCTAssertLessThan(prepared.data.count, oversizedData.count)
    }

    func testAlbumPhotoBudgetKeepsCombinedUploadBelowSixtyMegabytes() {
        XCTAssertEqual(PhotoUploadPreparer.maximumBytes(forPhotoCount: 1), 15 * 1_024 * 1_024)
        XCTAssertEqual(PhotoUploadPreparer.maximumBytes(forPhotoCount: 4), 15 * 1_024 * 1_024)
        XCTAssertEqual(PhotoUploadPreparer.maximumBytes(forPhotoCount: 30), 2 * 1_024 * 1_024)
    }

    func testStandaloneEmojiPresentationAcceptsUpToThreeEmojiOnly() {
        func message(_ body: String) -> ChatMessage {
            ChatMessage(
                id: UUID().uuidString,
                contactId: "family@g.us",
                timestamp: 1_700_000_000_000,
                isFromMe: false,
                isForwarded: false,
                senderName: "Virág",
                senderPhone: nil,
                contactName: "Family",
                contactPhone: nil,
                chatType: "group",
                contentType: "Text",
                content: MessageContent(type: "text", body: body, showTranslatedPrimary: nil, replyContext: nil),
                originalText: nil,
                translatedText: nil,
                sourceLanguage: nil,
                isTranslated: false
            )
        }

        XCTAssertEqual(message("😉").standaloneEmojiText, "😉")
        XCTAssertEqual(message("👨‍👩‍👧‍👦👍🏽🥳").standaloneEmojiText, "👨‍👩‍👧‍👦👍🏽🥳")
        XCTAssertNil(message("😀😃🥳❤️").standaloneEmojiText)
        XCTAssertNil(message("Hello 👋").standaloneEmojiText)
        XCTAssertNil(message("123").standaloneEmojiText)
    }

    func testMediaCachePersistsAcrossStoreInstancesAndEvictsOldestFiles() async throws {
        let root = FileManager.default.temporaryDirectory
            .appending(path: "MediaCacheTests-\(UUID().uuidString)", directoryHint: .isDirectory)
        defer { try? FileManager.default.removeItem(at: root) }

        let firstStore = MediaCacheStore(directoryURL: root, maximumBytes: 7)
        let firstURL = try await firstStore.store(
            Data([1, 2, 3, 4]),
            messageID: "message/one",
            fileExtension: "jpg"
        )
        try FileManager.default.setAttributes(
            [.modificationDate: Date(timeIntervalSince1970: 1)],
            ofItemAtPath: firstURL.path
        )

        let relaunchedStore = MediaCacheStore(directoryURL: root, maximumBytes: 7)
        let cachedURL = try await relaunchedStore.cachedURL(for: "message/one")
        let restoredURL = try XCTUnwrap(cachedURL)
        XCTAssertEqual(try Data(contentsOf: restoredURL), Data([1, 2, 3, 4]))

        _ = try await relaunchedStore.store(
            Data([5, 6, 7, 8]),
            messageID: "message/two",
            fileExtension: "mp4"
        )

        let evictedURL = try await relaunchedStore.cachedURL(for: "message/one")
        let retainedURL = try await relaunchedStore.cachedURL(for: "message/two")
        XCTAssertNil(evictedURL)
        XCTAssertNotNil(retainedURL)
    }

    func testExpectedRequestCancellationsDoNotBecomeUserFacingErrors() {
        XCTAssertTrue(AppSession.isExpectedCancellation(CancellationError()))
        XCTAssertTrue(AppSession.isExpectedCancellation(URLError(.cancelled)))
        XCTAssertFalse(AppSession.isExpectedCancellation(URLError(.timedOut)))
    }

    func testNotificationSenderUsesExactPhoneNumberIdentity() {
        let identity = NotificationPersonIdentity.sender(
            senderID: "447700900123",
            senderName: "Eileen Skinner"
        )

        XCTAssertEqual(identity.handleValue, "+447700900123")
        XCTAssertEqual(identity.handleType, .phoneNumber)
        XCTAssertFalse(identity.isContactSuggestion)
        XCTAssertEqual(identity.suggestionType, .none)

        let formattedIdentity = NotificationPersonIdentity.sender(
            senderID: "+44 (7700) 900-123",
            senderName: "Eileen Skinner"
        )
        XCTAssertEqual(formattedIdentity.handleValue, "+447700900123")

        let appIdentity = NotificationPersonIdentity.sender(
            senderID: "whatsapp-user-id",
            senderName: "Eileen Skinner"
        )
        XCTAssertEqual(appIdentity.handleType, .unknown)
        XCTAssertTrue(appIdentity.isContactSuggestion)
        XCTAssertEqual(appIdentity.suggestionType, .instantMessageAddress)
    }

    func testGroupNotificationKeepsSenderGroupAndTranslationOnSeparateLines() throws {
        let content = UNMutableNotificationContent()
        content.title = "Judit"
        content.body = "Szia"
        content.userInfo = [
            "contactId": "family@g.us", "senderName": "Judit",
            "conversationName": "Family", "recipientCount": 3,
            "messageBody": "[Family] Hello"
        ]
        let prepared = NotificationMessagePresentation.preparedContent(content)
        let communication = NotificationMessagePresentation.messagingContent(content, avatarData: nil, donate: false)
        XCTAssertEqual(communication.title, "Judit")
        XCTAssertEqual(communication.subtitle, "Family")
        XCTAssertEqual(communication.body, "Hello")
        XCTAssertEqual(prepared.title, "Judit")
        XCTAssertEqual(prepared.subtitle, "Family")
        XCTAssertEqual(prepared.body, "Hello")
        XCTAssertEqual(NotificationMessagePresentation.preparedContent(prepared).body, "Hello")

        content.userInfo["messageBody"] = "Hello"
        XCTAssertEqual(NotificationMessagePresentation.preparedContent(content).body, "Hello")

        content.userInfo["messageBodyIncludesGroup"] = false
        content.userInfo["messageBody"] = "[Family] Hello"
        XCTAssertEqual(NotificationMessagePresentation.preparedContent(content).body, "[Family] Hello")
        content.userInfo["messageBody"] = "Hello"

        content.userInfo.removeValue(forKey: "recipientCount")
        let fallback = NotificationMessagePresentation.messagingContent(content, avatarData: nil, donate: false)
        XCTAssertEqual(fallback.title, "Judit")
        XCTAssertEqual(fallback.subtitle, "Family")
        XCTAssertEqual(fallback.body, "Hello")

        content.userInfo["contactId"] = "friend@s.whatsapp.net"
        XCTAssertEqual(NotificationMessagePresentation.preparedContent(content).body, "Hello")
        XCTAssertEqual(NotificationMessagePresentation.messagingContent(content, avatarData: nil, donate: false).subtitle, "")
    }

    func testAccessibilityLayoutUsesStackedRowsAndCompactMessageChrome() {
        XCTAssertFalse(NativeLayoutPolicy.usesStackedChatRow(for: .large))
        XCTAssertFalse(NativeLayoutPolicy.usesCompactMessageChrome(for: .large))
        XCTAssertTrue(NativeLayoutPolicy.usesStackedChatRow(for: .accessibility1))
        XCTAssertTrue(NativeLayoutPolicy.usesCompactMessageChrome(for: .accessibility1))
        XCTAssertTrue(NativeLayoutPolicy.usesCompactMessageChrome(for: .accessibility5))
    }

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

    func testOutgoingMessageDecodesDeliveryAndReadStates() throws {
        let delivered = try JSONDecoder().decode(
            ChatMessage.self,
            from: Data(#"{"id":"m1","contactId":"chat@g.us","timestamp":1700000000000,"isFromMe":true,"isForwarded":false,"senderName":null,"senderPhone":null,"contactName":"Family","contactPhone":null,"chatType":"group","contentType":"Text","content":{"type":"text","body":"Hello"},"originalText":null,"translatedText":null,"sourceLanguage":null,"isTranslated":false,"deliveryStatus":"delivered"}"#.utf8)
        )
        let read = try JSONDecoder().decode(
            ChatMessage.self,
            from: Data(#"{"id":"m2","contactId":"chat@g.us","timestamp":1700000000000,"isFromMe":true,"isForwarded":false,"senderName":null,"senderPhone":null,"contactName":"Family","contactPhone":null,"chatType":"group","contentType":"Text","content":{"type":"text","body":"Hello"},"originalText":null,"translatedText":null,"sourceLanguage":null,"isTranslated":false,"deliveryStatus":"read"}"#.utf8)
        )

        XCTAssertEqual(delivered.deliveryState, .delivered)
        XCTAssertEqual(read.deliveryState, .read)
        XCTAssertEqual(read.deliveryState.accessibilityLabel, "Read")
    }

    func testLiveReceiptAndReadEventsDecodeBackendFieldNames() throws {
        let receipt = try JSONDecoder().decode(
            LiveEvent.self,
            from: Data(#"{"type":"receipt","message_ids":["m1","m2"],"status":"read"}"#.utf8)
        )
        let markRead = try JSONDecoder().decode(
            LiveEvent.self,
            from: Data(#"{"type":"mark_as_read","chat_id":"family@g.us"}"#.utf8)
        )

        XCTAssertEqual(receipt.messageIds, ["m1", "m2"])
        XCTAssertEqual(receipt.status, "read")
        XCTAssertEqual(markRead.chatId, "family@g.us")
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

    func testMessagingNotificationCategorySupportsInlineReplyWatchAndCarPlay() throws {
        let category = MessagingNotificationContract.category

        XCTAssertEqual(category.identifier, MessagingNotificationContract.categoryIdentifier)
        XCTAssertTrue(category.options.contains(.allowInCarPlay))
        XCTAssertTrue(category.options.contains(.allowAnnouncement))
        XCTAssertTrue(category.options.contains(.hiddenPreviewsShowTitle))
        XCTAssertTrue(category.intentIdentifiers.contains(INSendMessageIntentIdentifier))
        XCTAssertTrue(category.intentIdentifiers.contains(INSearchForMessagesIntentIdentifier))
        let reply = try XCTUnwrap(category.actions.first as? UNTextInputNotificationAction)
        XCTAssertEqual(reply.identifier, MessagingNotificationContract.replyActionIdentifier)
        XCTAssertEqual(reply.textInputButtonTitle, "Send")
    }

    func testSiriVocabularyProvidesEnglishExamplesForEverySupportedMessagingIntent() throws {
        let vocabularyURL = try XCTUnwrap(
            Bundle.main.url(
                forResource: "AppIntentVocabulary",
                withExtension: "plist",
                subdirectory: nil,
                localization: "en"
            )
        )
        let data = try Data(contentsOf: vocabularyURL)
        let propertyList = try PropertyListSerialization.propertyList(from: data, format: nil)
        let root = try XCTUnwrap(propertyList as? [String: Any])
        let phraseEntries = try XCTUnwrap(root["IntentPhrases"] as? [[String: Any]])
        let examplesByIntent = Dictionary(
            uniqueKeysWithValues: phraseEntries.compactMap { entry -> (String, [String])? in
                guard let intentName = entry["IntentName"] as? String,
                      let examples = entry["IntentExamples"] as? [String] else {
                    return nil
                }
                return (intentName, examples)
            }
        )

        for intentName in [
            "INSendMessageIntent",
            "INSearchForMessagesIntent",
            "INSetMessageAttributeIntent",
        ] {
            let examples = try XCTUnwrap(examplesByIntent[intentName], "Missing examples for \(intentName)")
            XCTAssertFalse(examples.isEmpty, "Expected at least one example for \(intentName)")
            XCTAssertTrue(examples.allSatisfy { !$0.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty })
        }
    }

    func testMessagingNotificationRoutingReadsBackendMetadata() {
        let routing = MessagingNotificationRouting(userInfo: [
            "contactId": "family@g.us",
            "messageId": "message-1",
            "senderName": "Virág",
            "conversationName": "The Skinners",
            "chatType": "group",
        ])

        XCTAssertEqual(routing?.contactID, "family@g.us")
        XCTAssertEqual(routing?.messageID, "message-1")
        XCTAssertEqual(routing?.senderName, "Virág")
        XCTAssertEqual(routing?.conversationName, "The Skinners")
        XCTAssertTrue(routing?.isGroup == true)
    }

    func testReadingConversationRemovesOnlyItsDeliveredNotifications() {
        let deliveries = [
            DeliveredMessagingNotification(identifier: "family-message-1", contactID: "family@g.us"),
            DeliveredMessagingNotification(identifier: "family-message-2", contactID: "family@g.us"),
            DeliveredMessagingNotification(identifier: "virag-message-1", contactID: "virag@s.whatsapp.net"),
        ]

        XCTAssertEqual(
            MessagingNotificationReadState.identifiersToRemove(
                for: "family@g.us",
                from: deliveries
            ),
            ["family-message-1", "family-message-2"]
        )
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

    func testPhotoAlbumMetadataDecodesAndGroupsInWhatsAppOrder() throws {
        let later = try decodeMessage(
            id: "photo-2",
            contentType: "Image",
            content: #"{"type":"image","mime_type":"image/jpeg","album_id":"album-1","album_index":2}"#
        )
        let first = try decodeMessage(
            id: "photo-0",
            contentType: "Image",
            content: #"{"type":"image","mime_type":"image/jpeg","album_id":"album-1","album_index":0}"#
        )
        let middle = try decodeMessage(
            id: "photo-1",
            contentType: "Image",
            content: #"{"type":"image","mime_type":"image/jpeg","album_id":"album-1","album_index":1}"#
        )

        let items = ConversationTimelineBuilder.items(from: [later, first, middle])

        XCTAssertEqual(items.count, 1)
        guard case let .photoAlbum(album) = items[0] else {
            return XCTFail("Expected one grouped photo album")
        }
        XCTAssertEqual(album.id, "photo-2")
        XCTAssertEqual(album.messages.map(\.id), ["photo-0", "photo-1", "photo-2"])
        XCTAssertEqual(album.messages.map(\.content?.albumIndex), [0, 1, 2])
    }

    func testEveryWebVisibleRichMessageTypeHasANativePresentationContract() throws {
        let video = try decodeMessage(
            id: "video",
            contentType: "Video",
            content: #"{"type":"video","caption":"At the park","mime_type":"video/mp4","has_media":true,"duration_seconds":12.5}"#
        )
        XCTAssertEqual(video.mediaKind, .video)
        XCTAssertEqual(video.displayText, "At the park")

        let voiceNote = try decodeMessage(
            id: "voice",
            contentType: "Audio",
            content: #"{"type":"audio","mime_type":"audio/ogg","has_media":true,"is_voice_note":true,"duration_seconds":9}"#
        )
        XCTAssertEqual(voiceNote.mediaKind, .audio)
        XCTAssertEqual(voiceNote.displayText, "Voice note")

        let document = try decodeMessage(
            id: "document",
            contentType: "Document",
            content: #"{"type":"document","mime_type":"application/pdf","has_media":true,"file_name":"travel-plan.pdf","file_size":4096}"#
        )
        XCTAssertEqual(document.mediaKind, .document)
        XCTAssertEqual(document.displayText, "travel-plan.pdf")

        let sticker = try decodeMessage(
            id: "sticker",
            contentType: "Sticker",
            content: #"{"type":"sticker","mime_type":"image/webp","has_media":true,"is_animated":false}"#
        )
        XCTAssertEqual(sticker.mediaKind, .sticker)
        XCTAssertEqual(sticker.displayText, "Sticker")

        let location = try decodeMessage(
            id: "location",
            contentType: "Location",
            content: #"{"type":"location","name":"Budapest Parliament","address":"Kossuth Lajos tér","latitude":47.5071,"longitude":19.0457}"#
        )
        XCTAssertEqual(location.displayText, "Budapest Parliament")
        XCTAssertEqual(location.locationURL?.host(), "maps.apple.com")

        let contact = try decodeMessage(
            id: "contact",
            contentType: "Contact",
            content: #"{"type":"contact","display_name":"Eileen Skinner","vcard":"BEGIN:VCARD\\nTEL:+447700900123\\nEND:VCARD"}"#
        )
        XCTAssertEqual(contact.displayText, "Eileen Skinner")

        let poll = try decodeMessage(
            id: "poll",
            contentType: "Poll",
            content: #"{"type":"poll","question":"Dinner?","options":["Pizza","Curry"]}"#
        )
        XCTAssertEqual(poll.displayText, "Dinner?")
        XCTAssertEqual(poll.content?.options, ["Pizza", "Curry"])

        let revoked = try decodeMessage(
            id: "revoked",
            contentType: "Revoked",
            content: #"{"type":"revoked"}"#
        )
        XCTAssertEqual(revoked.displayText, "This message was deleted")
    }

    func testCompleteConversationSearchRequestsAllMessages() {
        XCTAssertEqual(
            APIClient.messagesPath(contactID: "family@g.us", limit: 0),
            "/api/messages/family@g.us?limit=0"
        )
    }

    func testMessageActionAvailabilityMatchesVisibleControls() throws {
        let incoming = try decodeMessage(
            id: "incoming",
            contentType: "Text",
            content: #"{"type":"text","body":"Szia"}"#
        )
        XCTAssertEqual(incoming.availableActions, [.reply, .translate, .aiReply, .star, .react])

        let outgoingData = Data(#"{"id":"outgoing","contactId":"chat@g.us","timestamp":1700000000000,"isFromMe":true,"isForwarded":false,"senderName":null,"senderPhone":"447700900123","contactName":"Family","contactPhone":null,"chatType":"group","contentType":"Text","content":{"type":"text","body":"Hello"},"originalText":null,"translatedText":null,"sourceLanguage":null,"isTranslated":false}"#.utf8)
        let outgoing = try JSONDecoder().decode(ChatMessage.self, from: outgoingData)
        XCTAssertEqual(outgoing.availableActions, [.reply, .star, .react])
    }

    func testSwipeToReplyRequiresADeliberateHorizontalGesture() {
        XCTAssertFalse(MessageSwipeReply.shouldReply(translation: CGSize(width: 42, height: 3)))
        XCTAssertTrue(MessageSwipeReply.shouldReply(translation: CGSize(width: 64, height: 4)))
        XCTAssertFalse(MessageSwipeReply.shouldReply(translation: CGSize(width: 70, height: 80)))
        XCTAssertFalse(MessageSwipeReply.shouldReply(translation: CGSize(width: -80, height: 2)))
        XCTAssertFalse(MessageSwipeReply.shouldBegin(velocity: CGPoint(x: 20, y: 80)))
        XCTAssertFalse(MessageSwipeReply.shouldBegin(velocity: CGPoint(x: -80, y: 2)))
        XCTAssertTrue(MessageSwipeReply.shouldBegin(velocity: CGPoint(x: 80, y: 20)))
    }

    func testSwipeToReplyOffsetIgnoresVerticalAndLeftwardDragsAndClampsItsReveal() {
        XCTAssertEqual(MessageSwipeReply.offset(translation: CGSize(width: -20, height: 0)), 0)
        XCTAssertEqual(MessageSwipeReply.offset(translation: CGSize(width: 30, height: 40)), 0)
        XCTAssertEqual(MessageSwipeReply.offset(translation: CGSize(width: 35, height: 2)), 35)
        XCTAssertEqual(MessageSwipeReply.offset(translation: CGSize(width: 120, height: 2)), 72)
    }

    func testPhotoViewerZoomClampsAndDoubleTapTogglesMagnification() {
        XCTAssertEqual(PhotoViewerZoom.clampedScale(0.5), 1)
        XCTAssertEqual(PhotoViewerZoom.clampedScale(3), 3)
        XCTAssertEqual(PhotoViewerZoom.clampedScale(8), 5)
        XCTAssertEqual(PhotoViewerZoom.toggledScale(from: 1), 2.5)
        XCTAssertEqual(PhotoViewerZoom.toggledScale(from: 2.5), 1)
    }

    @MainActor
    func testPinnedContactsStayAboveUnpinnedChatsInPinOrder() {
        let recent = Contact(
            id: "recent",
            name: "Recent",
            phone: nil,
            type: "private",
            lastMessageTime: 300,
            unreadCount: 0,
            pinnedAt: nil,
            lastMessagePreview: nil
        )
        let secondPinned = Contact(
            id: "second-pinned",
            name: "Second pinned",
            phone: nil,
            type: "private",
            lastMessageTime: 200,
            unreadCount: 0,
            pinnedAt: 200,
            lastMessagePreview: nil
        )
        let firstPinned = Contact(
            id: "first-pinned",
            name: "First pinned",
            phone: nil,
            type: "private",
            lastMessageTime: 100,
            unreadCount: 0,
            pinnedAt: 100,
            lastMessagePreview: nil
        )

        XCTAssertEqual(
            AppSession.orderedContacts([recent, secondPinned, firstPinned]).map(\.id),
            ["first-pinned", "second-pinned", "recent"]
        )
        XCTAssertEqual(
            APIClient.pinPath(contactID: "family/parents@g.us"),
            "/api/contacts/family%2Fparents@g.us/pin"
        )
    }

    @MainActor
    func testCompactMessageLayoutHugsShortTextAndWrapsLongMessages() {
        func measured(_ text: String) -> CGSize {
            let host = UIHostingController(rootView: CompactMessageLayout {
                Text(text).font(.body)
                Text("16:06").font(.caption2).fixedSize()
            }.padding(9))
            return host.sizeThatFits(in: CGSize(width: 300, height: 1_000))
        }
        let short = measured("I do…")
        let long = measured(String(repeating: "A longer message that needs to wrap. ", count: 5))
        XCTAssertLessThan(short.width, 180)
        XCTAssertLessThan(short.height, 60)
        XCTAssertLessThanOrEqual(long.width, 300)
        XCTAssertGreaterThan(long.height, short.height)
    }

    func testChatFiltersDistinguishUnreadAndGroupConversations() {
        let direct = Contact(id: "direct", name: "Direct", phone: nil, type: "private", lastMessageTime: 1, unreadCount: 2, pinnedAt: nil, lastMessagePreview: nil)
        let group = Contact(id: "group", name: "Group", phone: nil, type: "group", lastMessageTime: 2, unreadCount: 0, pinnedAt: nil, lastMessagePreview: nil)
        XCTAssertTrue(ChatFilter.all.includes(direct))
        XCTAssertTrue(ChatFilter.all.includes(group))
        XCTAssertTrue(ChatFilter.unread.includes(direct))
        XCTAssertFalse(ChatFilter.unread.includes(group))
        XCTAssertFalse(ChatFilter.groups.includes(direct))
        XCTAssertTrue(ChatFilter.groups.includes(group))
    }

    func testWhatsAppStatusFeedBecomesDedicatedUpdatesToolbarItem() {
        let updates = Contact(
            id: "status@broadcast",
            name: nil,
            phone: nil,
            type: "broadcast",
            lastMessageTime: 50,
            unreadCount: 2,
            pinnedAt: nil,
            lastMessagePreview: "A status preview that must not be shown"
        )
        XCTAssertTrue(updates.isUpdates)
        XCTAssertEqual(updates.displayName, "Updates")
        XCTAssertTrue(updates.showsAsUpdatesToolbarItem)
        XCTAssertFalse(updates.showsInChatList)
    }

    func testMessageExtractsEveryLinkForMultiplePreviewCards() throws {
        let message = try decodeMessage(
            id: "links",
            contentType: "Text",
            content: #"{"type":"text","body":"Compare https://example.com/one and https://example.org/two"}"#
        )
        XCTAssertEqual(message.extractedURLs.map(\.absoluteString), [
            "https://example.com/one",
            "https://example.org/two",
        ])
    }

    func testMessageTextLinkifierMakesEveryDetectedURLClickable() throws {
        let text = "Watch https://youtube.com/shorts/example?is=abc and visit https://example.org/help."
        let attributedText = MessageTextLinkifier.attributedString(from: text)

        XCTAssertEqual(
            attributedText.runs.compactMap(\.link?.absoluteString),
            [
                "https://youtube.com/shorts/example?is=abc",
                "https://example.org/help",
            ]
        )
    }

    @MainActor
    func testUnifiedQuickReplyLocksItsTargetAcrossNewMessages() async throws {
        func message(_ id: String, _ contact: String, _ timestamp: Int64) throws -> ChatMessage {
            let data = try JSONSerialization.data(withJSONObject: ["id": id, "contactId": contact, "timestamp": timestamp, "isFromMe": false, "isForwarded": false, "chatType": "group", "contentType": "Text", "isTranslated": false])
            return try JSONDecoder().decode(ChatMessage.self, from: data)
        }
        let original = try message("original", "group@g.us", 100)
        let newer = try message("newer", "group@g.us", 200)
        let elsewhere = try message("elsewhere", "other@g.us", 300)
        var draft = UnifiedReplyDraft()
        draft.updateText("", latestMessage: original)
        XCTAssertNil(draft.selected)
        XCTAssertFalse(draft.isFocused)
        draft.updateText("H", latestMessage: original)
        XCTAssertEqual(draft.selected?.id, original.id)
        XCTAssertTrue(draft.isFocused)
        draft.updateText("Hello", latestMessage: elsewhere)
        XCTAssertEqual(draft.selected?.contactId, original.contactId)
        draft.updateText("", latestMessage: newer)
        draft.updateText("Hello again", latestMessage: newer)
        XCTAssertEqual(draft.selected?.id, original.id)
        XCTAssertTrue(draft.isFocused)

        let session = AppSession(demoMode: true)
        session.messages = [original.contactId: [original, newer], elsewhere.contactId: [elsewhere]]
        await session.loadFeed()
        let target = try XCTUnwrap(draft.selected)
        XCTAssertTrue(session.feedReplyNeedsQuote(target))
        let sent = await session.send(text: draft.text, to: target.contactId, reply: session.replyTarget(for: target), replyOnlyIfNotLatest: true)
        XCTAssertTrue(sent)
        XCTAssertEqual(session.messages[original.contactId]?.last?.content?.replyContext?.messageId, original.id)
        XCTAssertEqual(session.messages[elsewhere.contactId]?.count, 1)
        draft.finishSending(to: target)
        XCTAssertNil(draft.selected)
        XCTAssertEqual(draft.text, "")
        draft.updateText("Next reply", latestMessage: elsewhere)
        XCTAssertEqual(draft.selected?.id, elsewhere.id)
    }

    func testUnifiedQuickReplyManualSelectionAndEmptyFeedStaySafe() throws {
        func message(_ id: String, _ contact: String) throws -> ChatMessage {
            let data = try JSONSerialization.data(withJSONObject: ["id": id, "contactId": contact, "timestamp": 100, "isFromMe": false, "isForwarded": false, "chatType": "group", "contentType": "Text", "isTranslated": false])
            return try JSONDecoder().decode(ChatMessage.self, from: data)
        }
        let first = try message("first", "first@g.us")
        let second = try message("second", "second@g.us")
        var draft = UnifiedReplyDraft()
        draft.updateText("Cannot route", latestMessage: nil)
        XCTAssertNil(draft.selected)
        XCTAssertTrue(draft.drafts.isEmpty)
        draft.updateText("First draft", latestMessage: first)
        draft.select(second)
        XCTAssertTrue(draft.isFocused)
        XCTAssertEqual(draft.text, "")
        draft.updateText("Second draft", latestMessage: first)
        XCTAssertEqual(draft.selected?.id, second.id)
        draft.select(first)
        XCTAssertEqual(draft.text, "First draft")
        draft.cancelSelection()
        XCTAssertFalse(draft.isFocused)
        XCTAssertNil(draft.selected)
        draft.updateText("Fresh reply", latestMessage: second)
        XCTAssertEqual(draft.text, "Fresh reply")
        XCTAssertEqual(draft.selected?.id, second.id)
    }

    @MainActor
    func testUnifiedFeedOrdersAcrossChatsAndScopesReplyContextToDestination() async throws {
        func message(_ id: String, _ contact: String, _ timestamp: Int64) throws -> ChatMessage {
            let data = try JSONSerialization.data(withJSONObject: ["id": id, "contactId": contact, "timestamp": timestamp, "isFromMe": false, "isForwarded": false, "chatType": "group", "contentType": "Text", "isTranslated": false])
            return try JSONDecoder().decode(ChatMessage.self, from: data)
        }
        let session = AppSession(demoMode: true)
        let first = try message("a", "group@g.us", 100)
        let latest = try message("b", "group@g.us", 100)
        let elsewhere = try message("c", "other@g.us", 200)
        session.messages = [first.contactId: [first, latest], elsewhere.contactId: [elsewhere]]
        await session.loadFeed()
        XCTAssertEqual(session.mainTab, .messages)
        XCTAssertEqual(session.unifiedMessages.map(\.id), ["a", "b", "c"])
        XCTAssertTrue(session.feedReplyNeedsQuote(first))
        XCTAssertFalse(session.feedReplyNeedsQuote(latest))
        XCTAssertFalse(session.feedReplyNeedsQuote(elsewhere))
        // Loading another chat must not remove older messages from the feed.
        session.messages[first.contactId] = [latest]
        XCTAssertEqual(session.unifiedMessages.map(\.id), ["a", "b", "c"])
        let normalSent = await session.send(text: "Normal", to: elsewhere.contactId, reply: session.replyTarget(for: elsewhere), replyOnlyIfNotLatest: true)
        XCTAssertTrue(normalSent)
        XCTAssertNil(session.messages[elsewhere.contactId]?.last?.content?.replyContext)
        let quotedSent = await session.send(text: "Quoted", to: first.contactId, reply: session.replyTarget(for: first), replyOnlyIfNotLatest: true)
        XCTAssertTrue(quotedSent)
        XCTAssertEqual(session.messages[first.contactId]?.last?.content?.replyContext?.messageId, first.id)
    }

    func testUnifiedFeedSendEncodesConditionalQuoteWithoutChangingOrdinaryReplies() throws {
        var request = SendMessageRequest(contactId: "group@g.us", text: "Reply", replyTo: "selected", replyToSender: nil, replyToText: "Original", replyToSenderName: nil)
        let ordinary = try XCTUnwrap(JSONSerialization.jsonObject(with: JSONEncoder().encode(request)) as? [String: Any])
        XCTAssertNil(ordinary["replyOnlyIfNotLatest"])
        request.replyOnlyIfNotLatest = true
        let feed = try XCTUnwrap(JSONSerialization.jsonObject(with: JSONEncoder().encode(request)) as? [String: Any])
        XCTAssertEqual(feed["replyOnlyIfNotLatest"] as? Bool, true)
        XCTAssertEqual(feed["contactId"] as? String, "group@g.us")
        XCTAssertEqual(feed["replyTo"] as? String, "selected")
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

    func testSendImagesRequestEncodesOrderedAlbumAndReplyContext() throws {
        let request = SendImagesRequest(
            contactId: "chat@g.us",
            progressId: "job-7",
            images: [
                SendImageItemRequest(mediaData: "first", mimeType: "image/jpeg"),
                SendImageItemRequest(mediaData: "second", mimeType: "image/png"),
            ],
            caption: "Holiday",
            replyTo: "message-1",
            replyToSender: "447700900123@s.whatsapp.net",
            replyToText: "Send photos",
            replyToSenderName: "Virág"
        )

        let data = try JSONEncoder().encode(request)
        let payload = try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [String: Any])
        let images = try XCTUnwrap(payload["images"] as? [[String: Any]])
        XCTAssertEqual(images.count, 2)
        XCTAssertEqual(images[0]["mediaData"] as? String, "first")
        XCTAssertEqual(images[1]["mimeType"] as? String, "image/png")
        XCTAssertEqual(payload["caption"] as? String, "Holiday")
        XCTAssertEqual(payload["replyTo"] as? String, "message-1")
        XCTAssertEqual(payload["progressId"] as? String, "job-7")
    }

    func testPhotoSendProgressUsesNamedStagesAndCounts() {
        let progress = PhotoSendProgress(
            id: "job-7", contactID: "chat@g.us", stage: .preparing,
            completed: 3, total: 10, error: nil
        )
        XCTAssertEqual(progress.statusText, "Preparing 3 of 10")
        XCTAssertEqual(progress.fractionCompleted, 0.3, accuracy: 0.001)
        var transferring = progress
        transferring.stage = .transferring
        XCTAssertEqual(transferring.statusText, "Transferring 3 of 10")
    }

    func testStagedAlbumRequestsKeepTransferCheckpointIdentity() throws {
        let create = CreatePhotoAlbumRequest(
            jobId: "job-12", contactId: "chat@g.us", photoCount: 12,
            caption: "Walk", replyTo: nil, replyToSender: nil,
            replyToText: nil, replyToSenderName: nil
        )
        let createPayload = try XCTUnwrap(
            JSONSerialization.jsonObject(with: JSONEncoder().encode(create)) as? [String: Any]
        )
        XCTAssertEqual(createPayload["jobId"] as? String, "job-12")
        XCTAssertEqual(createPayload["photoCount"] as? Int, 12)

        let item = StagePhotoAlbumItemRequest(mediaData: "photo-two", mimeType: "image/jpeg")
        let itemPayload = try XCTUnwrap(
            JSONSerialization.jsonObject(with: JSONEncoder().encode(item)) as? [String: Any]
        )
        XCTAssertEqual(itemPayload["mediaData"] as? String, "photo-two")
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

    @MainActor
    func testDeclaredReactionTypeCannotRenderAsAStandaloneMessage() throws {
        let target = try decodeMessage(
            id: "m1",
            contentType: "Text",
            content: #"{"type":"text","body":"Agreed"}"#
        )
        let reaction = try decodeMessage(
            id: "r1",
            contentType: "Reaction",
            content: #"{"type":"text","body":"Reaction","emoji":"❤️","target_message_id":"m1"}"#
        )

        XCTAssertTrue(reaction.isReaction)
        let normalized = AppSession(demoMode: false).normalizeMessages([target, reaction])
        XCTAssertEqual(normalized.map(\.id), ["m1"])
        XCTAssertEqual(normalized.first?.reactions?["❤️"], ["3630"])
    }

    private func decodeMessage(id: String, contentType: String, content: String) throws -> ChatMessage {
        let data = Data(
            """
            {"id":"\(id)","contactId":"chat@g.us","timestamp":1700000000000,"isFromMe":false,"isForwarded":false,"senderName":"Virag","senderPhone":"3630","contactName":"Family","contactPhone":null,"chatType":"group","contentType":"\(contentType)","content":\(content),"originalText":null,"translatedText":null,"sourceLanguage":null,"isTranslated":false}
            """.utf8
        )
        return try JSONDecoder().decode(ChatMessage.self, from: data)
    }
}


extension WhatsAppTranslatorTests {
    func testTranslatedVoiceContractPreservesOriginalAndTranslatedPlayback() throws {
        let json = #"{"id":"prepared-note","contactId":"contact","transcript":"Good morning","translation":"Jó reggelt","targetLanguage":"Hungarian","voice":"shimmer","audioData":"dHJhbnNsYXRlZA==","originalData":"b3JpZ2luYWw=","mimeType":"audio/mpeg","durationSeconds":2,"originalFollowUp":true}"#
        let note = try JSONDecoder.backend.decode(TranslatedVoiceNote.self, from: Data(json.utf8))
        XCTAssertEqual(note.voice, "shimmer")
        XCTAssertEqual(Data(base64Encoded: note.originalData), Data("original".utf8))
        XCTAssertEqual(Data(base64Encoded: note.audioData), Data("translated".utf8))
        XCTAssertTrue(note.originalFollowUp)
    }

    func testVoiceReadinessEventDecodesWithoutTreatingItAsANewMessage() throws {
        let json = #"{"type":"voice_ready","message_id":"voice-message"}"#
        let event = try JSONDecoder.backend.decode(LiveEvent.self, from: Data(json.utf8))
        XCTAssertEqual(event.messageId, "voice-message")
        XCTAssertNil(event.message)
    }

    func testMicrophonePurposeIsDeclared() throws {
        let purpose = try XCTUnwrap(Bundle.main.object(forInfoDictionaryKey: "NSMicrophoneUsageDescription") as? String)
        XCTAssertTrue(purpose.contains("voice"))
    }
}

@MainActor
final class LiveReactionTests: XCTestCase {
    private func message(_ id: String = "target", contact: String = "family@g.us", emoji: String? = nil, time: Int64 = 100, actor: String = "447700900123") throws -> ChatMessage {
        var json: [String: Any] = ["id": id, "contactId": contact, "timestamp": time, "isFromMe": false, "isForwarded": false, "senderName": "Alex", "senderPhone": actor, "chatType": "group", "contentType": "Text", "content": ["type": "text", "body": "See you at six"], "isTranslated": false]
        if let emoji {
            json["contentType"] = "Reaction"
            json["content"] = ["type": "reaction", "target_message_id": "target", "emoji": emoji]
        }
        return try JSONDecoder().decode(ChatMessage.self, from: JSONSerialization.data(withJSONObject: json))
    }

    private func event(_ message: ChatMessage, type: String = "reaction") throws -> LiveEvent {
        let payload = try JSONSerialization.jsonObject(with: JSONEncoder().encode(message))
        return try JSONDecoder().decode(LiveEvent.self, from: JSONSerialization.data(withJSONObject: ["type": type, "message": payload]))
    }

    func testLiveReactionUpdatesFeedWhenConversationDoesNotContainTarget() throws {
        let session = AppSession(demoMode: false)
        session.phase = .ready
        let target = try message()
        session.feedByID[target.id] = target
        session.handle(try event(message("heart", emoji: "❤️", time: 200)))
        XCTAssertEqual(session.unifiedMessages.first?.reactions, ["❤️": ["447700900123"]])
        XCTAssertEqual(session.feedByID[target.id]?.reactions, ["❤️": ["447700900123"]])
        XCTAssertEqual(session.unifiedMessages.map(\.id), [target.id])
    }

    func testTranslationUpdateCannotEraseLiveReaction() throws {
        let session = AppSession(demoMode: false)
        session.phase = .ready
        let target = try message()
        session.messages[target.contactId] = [target]
        session.feedByID[target.id] = target
        session.handle(try event(message("heart", emoji: "❤️", time: 200)))
        session.handle(try event(target, type: "message_updated"))
        XCTAssertEqual(session.messages[target.contactId]?.first?.reactions, ["❤️": ["447700900123"]])
        XCTAssertEqual(session.unifiedMessages.first?.reactions, ["❤️": ["447700900123"]])
    }

    func testReactionsBeforeTargetRetainLatestActorChoiceAndRemoval() throws {
        let session = AppSession(demoMode: false)
        let heart = try message("heart", emoji: "❤️", time: 200)
        let thumb = try message("thumb", emoji: "👍", time: 300)
        XCTAssertTrue(session.normalizeMessages([thumb, heart]).isEmpty)
        let target = try message()
        var normalized = session.normalizeMessages([target])
        XCTAssertEqual(normalized.first?.reactions, ["👍": ["447700900123"]])
        normalized = session.normalizeMessages(normalized + [heart, try message("other", emoji: "❤️", time: 250, actor: "447700900456")])
        XCTAssertEqual(normalized.first?.reactions, ["👍": ["447700900123"], "❤️": ["447700900456"]])
        normalized = session.normalizeMessages(normalized + [try message("remove", emoji: "", time: 400)])
        normalized = session.normalizeMessages(normalized + [thumb])
        XCTAssertEqual(normalized.first?.reactions, ["❤️": ["447700900456"]])
        XCTAssertNil(session.normalizeMessages([try message(contact: "other@g.us")]).first?.reactions)
    }

    func testInFlightFeedAndConversationSnapshotsKeepLiveReactions() async throws {
        let configuration = URLSessionConfiguration.ephemeral
        configuration.protocolClasses = [ReactionSnapshotProtocol.self]
        let urlSession = URLSession(configuration: configuration)
        defer { urlSession.invalidateAndCancel(); ReactionSnapshotProtocol.state.handler = nil }
        let api = APIClient(session: urlSession)
        await api.configure(try ServerConfiguration.make(address: "https://reactions.example.test", password: "test"))
        let session = AppSession(api: api, demoMode: false)
        session.phase = .ready
        let target = try message()
        session.messages[target.contactId] = [target]
        session.feedByID[target.id] = target
        let payload = try JSONSerialization.jsonObject(with: JSONEncoder().encode(target))
        let snapshot = try JSONSerialization.data(withJSONObject: ["messages": [payload], "hasMore": true])
        for operation in 0..<3 {
            let started = expectation(description: "Snapshot request started")
            let pending = ReactionRequestState()
            ReactionSnapshotProtocol.state.handler = { request in
                if request.request.url?.path.hasSuffix("/read") == true {
                    request.respond(Data(#"{"success":true}"#.utf8))
                } else {
                    pending.request = request
                    started.fulfill()
                }
            }
            let request = Task {
                if operation == 0 { await session.loadFeed() }
                else { await session.loadMessages(for: target.contactId, older: operation == 2) }
            }
            let waitResult = await XCTWaiter.fulfillment(of: [started], timeout: 3)
            XCTAssertEqual(waitResult, .completed)
            let emoji = operation == 0 ? "❤️" : operation == 1 ? "👍" : ""
            session.handle(try event(message("live-\(operation)", emoji: emoji, time: Int64(200 + operation))))
            try XCTUnwrap(pending.request).respond(snapshot)
            await request.value
            let expected: [String: [String]] = emoji.isEmpty ? [:] : [emoji: ["447700900123"]]
            XCTAssertEqual(session.messages[target.contactId]?.first?.reactions, expected)
            XCTAssertEqual(session.unifiedMessages.first?.reactions, expected)
        }
    }
}

private final class ReactionSnapshotProtocol: URLProtocol, @unchecked Sendable {
    static let state = ReactionRequestState()
    override class func canInit(with request: URLRequest) -> Bool { request.url?.host == "reactions.example.test" }
    override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }
    override func startLoading() { Self.state.handler?(self) }
    override func stopLoading() {}
    func respond(_ data: Data) {
        client?.urlProtocol(self, didReceive: HTTPURLResponse(url: request.url!, statusCode: 200, httpVersion: nil, headerFields: ["Content-Type": "application/json"])!, cacheStoragePolicy: .notAllowed)
        client?.urlProtocol(self, didLoad: data)
        client?.urlProtocolDidFinishLoading(self)
    }
}

private final class ReactionRequestState: @unchecked Sendable {
    private let lock = NSLock()
    private var storedHandler: (@Sendable (ReactionSnapshotProtocol) -> Void)?
    private var storedRequest: ReactionSnapshotProtocol?
    var handler: (@Sendable (ReactionSnapshotProtocol) -> Void)? {
        get { lock.withLock { storedHandler } }
        set { lock.withLock { storedHandler = newValue } }
    }
    var request: ReactionSnapshotProtocol? {
        get { lock.withLock { storedRequest } }
        set { lock.withLock { storedRequest = newValue } }
    }
}

final class PhotoGalleryTests: XCTestCase {
    func testConsecutivePhotosWithoutAlbumMetadataBecomeOneGallery() throws {
        let photos = try (0..<30).map { try photo("p\($0)", timestamp: 1_783_940_000_000 + Int64($0) * 1_000) }
        let items = ConversationTimelineBuilder.items(from: photos)
        XCTAssertEqual(items.count, 1)
        XCTAssertEqual(items.first?.messages.map(\.id), photos.map(\.id))
        XCTAssertEqual(items.first?.id, photos.first?.id)
        XCTAssertEqual(ConversationTimelineBuilder.items(from: []).count, 0)
    }

    func testInterveningMessagesSendersChatsAndDayChangesBreakGalleries() throws {
        let first = try photo("first", album: "shared")
        let last = try photo("last", album: "shared")
        let boundaries = [
            try photo("text", kind: "text"),
            try photo("video", kind: "video"),
            try photo("someone-else", sender: "447700900456"),
            try photo("another-chat", contact: "another@g.us"),
            try photo("outgoing", fromMe: true),
            try photo("tomorrow", timestamp: 1_783_940_000_000 + 86_400_000),
        ]
        for boundary in boundaries {
            let items = ConversationTimelineBuilder.items(from: [first, boundary, last])
            XCTAssertEqual(items.count, 3, boundary.id)
            XCTAssertEqual(items.flatMap(\.messages).map(\.id), [first.id, boundary.id, last.id])
        }
    }

    func testAdjacentSeparateAlbumsAndStandaloneImagesShareOneGallery() throws {
        let photos = [try photo("a", album: "first"), try photo("b", album: "second"), try photo("c")]
        XCTAssertEqual(ConversationTimelineBuilder.items(from: photos).count, 1)
        XCTAssertEqual(ConversationTimelineBuilder.items(from: photos).first?.messages.map(\.id), ["a", "b", "c"])
    }

    func testJoinedAlbumsRetainTheirOwnWhatsAppPhotoOrder() throws {
        let photos = [try photo("a1", album: "a", albumIndex: 1), try photo("a0", album: "a", albumIndex: 0), try photo("single"), try photo("b1", album: "b", albumIndex: 1), try photo("b0", album: "b", albumIndex: 0)]
        let items = ConversationTimelineBuilder.items(from: photos)
        XCTAssertEqual(items.count, 1)
        XCTAssertEqual(items.first?.messages.map(\.id), ["a0", "a1", "single", "b0", "b1"])
    }

    func testLiveAppendAndHistoryPagesPreserveEveryPhotoAndStableFirstAnchor() throws {
        let photos = try (0..<8).map { try photo("p\($0)") }
        let first = ConversationTimelineBuilder.items(from: [photos[0]])
        let extended = ConversationTimelineBuilder.items(from: photos)
        XCTAssertEqual(first.first?.id, extended.first?.id)
        let recent = Array(photos.suffix(3))
        XCTAssertEqual(ConversationTimelineBuilder.items(from: recent).count, 1)
        XCTAssertEqual(ConversationTimelineBuilder.items(from: Array(photos.prefix(5)) + recent).first?.messages.count, 8)
        XCTAssertEqual(extended.first?.messages[7].content?.caption, "Caption p7")
        XCTAssertEqual(extended.first?.messages[7].reactions, ["❤️": ["actor"]])
        XCTAssertEqual(extended.first?.messages[7].replyTarget.messageID, "p7")
    }

    func testUnknownGroupSenderDoesNotMergeNamesButDirectAndOutgoingPhotosGroup() throws {
        let unknown = [try photo("a", sender: nil), try photo("b", sender: nil)]
        XCTAssertEqual(ConversationTimelineBuilder.items(from: unknown).count, 2)
        XCTAssertEqual(ConversationTimelineBuilder.items(from: [try photo("a", sender: nil, contact: "one@s.whatsapp.net"), try photo("b", sender: nil, contact: "one@s.whatsapp.net")]).count, 1)
        XCTAssertEqual(ConversationTimelineBuilder.items(from: [try photo("a", sender: nil, fromMe: true), try photo("b", sender: nil, fromMe: true)]).count, 1)
    }

    func testCompactPreviewAndGalleryNavigationKeepEveryPageReachable() {
        XCTAssertEqual(PhotoGalleryLayout.previewLimit, 4)
        XCTAssertEqual(PhotoGalleryLayout.hiddenCount(total: 30), 26)
        XCTAssertEqual(PhotoGalleryLayout.hiddenCount(total: 2), 0)
        var page = 0
        for expected in 1..<30 {
            page = PhotoGalleryLayout.page(after: 1, current: page, count: 30)
            XCTAssertEqual(page, expected)
        }
        XCTAssertEqual(PhotoGalleryLayout.page(after: 1, current: page, count: 30), 29)
        XCTAssertEqual(PhotoGalleryLayout.page(after: -1, current: 0, count: 30), 0)
    }

    private func photo(_ id: String, sender: String? = "447700900123", contact: String = "gallery@g.us", fromMe: Bool = false, kind: String = "image", timestamp: Int64 = 1_783_940_000_000, album: String? = nil, albumIndex: Int? = nil) throws -> ChatMessage {
        var content: [String: Any] = ["type": kind, "caption": "Caption \(id)", "has_media": true]
        if let album { content["album_id"] = album }
        if let albumIndex { content["album_index"] = albumIndex }
        var payload: [String: Any] = [
            "id": id, "contactId": contact, "timestamp": timestamp, "isFromMe": fromMe,
            "isForwarded": false, "senderName": "Alex", "contactName": "Weekend photos",
            "chatType": contact.contains("@g.us") ? "group" : "private", "contentType": kind,
            "content": content, "isTranslated": false, "reactions": ["❤️": ["actor"]],
        ]
        if let sender { payload["senderPhone"] = sender }
        return try JSONDecoder().decode(ChatMessage.self, from: JSONSerialization.data(withJSONObject: payload))
    }
}
