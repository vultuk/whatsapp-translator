import XCTest
import Intents
import SwiftUI
import UserNotifications
@testable import WhatsAppTranslator

final class WhatsAppTranslatorTests: XCTestCase {
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
        XCTAssertEqual(album.id, "album-1")
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
