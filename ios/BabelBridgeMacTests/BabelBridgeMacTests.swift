import AppKit
import SwiftUI
import XCTest
import UserNotifications
@testable import BabelBridgeMac

final class BabelBridgeMacTests: XCTestCase {
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


    func testMacNotificationCategorySupportsInlineReply() throws {
        let category = MessagingNotificationContract.category

        XCTAssertEqual(category.identifier, MessagingNotificationContract.categoryIdentifier)
        XCTAssertTrue(category.options.contains(.hiddenPreviewsShowTitle))
        XCTAssertTrue(category.options.contains(.hiddenPreviewsShowSubtitle))
        let reply = try XCTUnwrap(category.actions.first as? UNTextInputNotificationAction)
        XCTAssertEqual(reply.identifier, MessagingNotificationContract.replyActionIdentifier)
        XCTAssertEqual(reply.textInputButtonTitle, "Send")
    }

    func testMacDeclaresSendMessageUserActivity() {
        let activityTypes = Bundle.main.object(forInfoDictionaryKey: "NSUserActivityTypes") as? [String]

        XCTAssertTrue(activityTypes?.contains("INSendMessageIntent") == true)
    }

    func testCrossPlatformImageCanEncodeJPEGData() {
        let image = DemoImageFactory.landscape(size: CGSize(width: 320, height: 210))
        let data = image.platformJPEGData(compressionQuality: 0.8)

        XCTAssertNotNil(data)
        XCTAssertGreaterThan(data?.count ?? 0, 1_000)
    }

    func testMinimumMacWindowCanContainTheLargestMessageBubble() {
        let availableDetailWidth = MacChatLayoutMetrics.minimumWindowWidth
            - MacChatLayoutMetrics.minimumSidebarWidth
            - MacChatLayoutMetrics.timelineHorizontalPadding

        XCTAssertGreaterThanOrEqual(
            availableDetailWidth,
            MacChatLayoutMetrics.maximumBubbleWidth,
            "The minimum Mac window must not force message content outside its bubble."
        )
    }

    func testMacSheetsHaveEnoughRoomForTheirLabelsAndValues() {
        XCTAssertGreaterThanOrEqual(MacChatLayoutMetrics.settingsSheetMinimumWidth, 520)
        XCTAssertGreaterThanOrEqual(MacChatLayoutMetrics.costSheetMinimumWidth, 400)
        XCTAssertGreaterThanOrEqual(MacChatLayoutMetrics.mediaSheetMinimumWidth, 520)
    }

    func testMacMessageTextMakesDetectedURLsClickable() {
        let attributedText = MessageTextLinkifier.attributedString(
            from: "Watch https://youtube.com/shorts/example?is=abc now."
        )

        XCTAssertEqual(
            attributedText.runs.compactMap(\.link?.absoluteString),
            ["https://youtube.com/shorts/example?is=abc"]
        )
    }

    func testPhotoViewerZoomClampsAndDoubleClickTogglesMagnification() {
        XCTAssertEqual(PhotoViewerZoom.clampedScale(0.5), 1)
        XCTAssertEqual(PhotoViewerZoom.clampedScale(3), 3)
        XCTAssertEqual(PhotoViewerZoom.clampedScale(8), 5)
        XCTAssertEqual(PhotoViewerZoom.toggledScale(from: 1), 2.5)
        XCTAssertEqual(PhotoViewerZoom.toggledScale(from: 2.5), 1)
    }

    @MainActor
    func testComposerUsesAMultilineMacTextEditor() {
        let view = ComposerView(
            contactID: "test-contact",
            text: .constant("First line"),
            reply: nil,
            isSending: false,
            cancelReply: {},
            sendImages: { _, _ in true },
            send: {}
        )
        let hostingView = NSHostingView(rootView: view)
        hostingView.frame = NSRect(x: 0, y: 0, width: 640, height: 140)

        hostingView.layoutSubtreeIfNeeded()

        let textViews = hostingView.descendants(ofType: NSTextView.self)
        XCTAssertTrue(
            textViews.contains(where: { $0.isEditable && $0.isVerticallyResizable }),
            "The Mac composer must use an editable multiline text view so Return inserts a newline."
        )
    }

    @MainActor
    func testVideoMessageCanMountWithoutCrashing() {
        let message = ChatMessage(
            id: "video-message",
            contactId: "contact",
            timestamp: 0,
            isFromMe: false,
            isForwarded: false,
            senderName: "Contact",
            senderPhone: nil,
            contactName: "Contact",
            contactPhone: nil,
            chatType: "private",
            contentType: "video",
            content: MessageContent(
                type: "video",
                body: nil,
                showTranslatedPrimary: nil,
                replyContext: nil
            ),
            originalText: nil,
            translatedText: nil,
            sourceLanguage: nil,
            isTranslated: false
        )
        let view = RichMessageContentView(
            message: message,
            displayText: "Video",
            image: nil,
            mediaURL: URL(fileURLWithPath: "/tmp/babel-bridge-render-test.mp4"),
            isLoading: false,
            failed: false,
            retry: {}
        )
        let hostingView = NSHostingView(rootView: view)
        hostingView.frame = NSRect(x: 0, y: 0, width: 320, height: 240)

        hostingView.layoutSubtreeIfNeeded()
        hostingView.displayIfNeeded()

        XCTAssertEqual(hostingView.frame.size, NSSize(width: 320, height: 240))
    }

    @MainActor
    func testMacUsesTheSharedPinnedConversationOrdering() {
        let pinned = Contact(
            id: "pinned",
            name: "Pinned",
            phone: nil,
            type: "private",
            lastMessageTime: 1,
            unreadCount: 0,
            pinnedAt: 1,
            lastMessagePreview: nil
        )
        let recent = Contact(
            id: "recent",
            name: "Recent",
            phone: nil,
            type: "private",
            lastMessageTime: 2,
            unreadCount: 0,
            pinnedAt: nil,
            lastMessagePreview: nil
        )

        XCTAssertEqual(AppSession.orderedContacts([recent, pinned]).map(\.id), ["pinned", "recent"])
    }
}

private extension NSView {
    func descendants<T: NSView>(ofType type: T.Type) -> [T] {
        subviews.flatMap { view in
            (view as? T).map { [$0] } ?? view.descendants(ofType: type)
        }
    }
}


extension BabelBridgeMacTests {
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

extension BabelBridgeMacTests {
    @MainActor
    func testTranslatedImageCaptionRendersWithOriginalAlternative() throws {
        let json = #"{"id":"caption-fixture","contactId":"fixture@g.us","timestamp":1700000000000,"isFromMe":false,"isForwarded":false,"chatType":"group","contentType":"Image","content":{"type":"image","caption":"Jó reggelt Budapestről!","mime_type":"image/jpeg"},"originalText":"Jó reggelt Budapestről!","translatedText":"Good morning from Budapest!","sourceLanguage":"Hungarian","isTranslated":true}"#
        let message = try JSONDecoder().decode(ChatMessage.self, from: Data(json.utf8))
        XCTAssertEqual(message.displayText, "Good morning from Budapest!")
        let content = HStack(alignment: .top, spacing: 28) {
            VStack(alignment: .leading, spacing: 12) {
                Text("Translated caption").font(.headline)
                RichMessageContentView(message: message, displayText: message.displayText, image: nil, mediaURL: nil, isLoading: false, failed: false, retry: {})
            }
            VStack(alignment: .leading, spacing: 12) {
                Text("Show original").font(.headline)
                RichMessageContentView(message: message, displayText: message.originalText!, image: nil, mediaURL: nil, isLoading: false, failed: false, retry: {})
            }
        }.padding(24).frame(width: 640).background(.white).foregroundStyle(.black).environment(\.colorScheme, .light)
        let renderer = ImageRenderer(content: content)
        renderer.scale = 2
        let rendered = try XCTUnwrap(renderer.nsImage)
        let bitmap = try XCTUnwrap(NSBitmapImageRep(data: try XCTUnwrap(rendered.tiffRepresentation)))
        let png = try XCTUnwrap(bitmap.representation(using: .png, properties: [:]))
        let attachment = XCTAttachment(data: png, uniformTypeIdentifier: "public.png")
        attachment.name = "Translated caption and original"; attachment.lifetime = .keepAlways; add(attachment)
        try png.write(to: URL(fileURLWithPath: NSTemporaryDirectory()).appendingPathComponent("babelbridge-caption-build33.png"))
    }
}
