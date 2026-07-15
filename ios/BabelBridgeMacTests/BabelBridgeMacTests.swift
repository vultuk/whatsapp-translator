import AppKit
import SwiftUI
import XCTest
import UserNotifications
@testable import BabelBridgeMac

final class BabelBridgeMacTests: XCTestCase {
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
            text: .constant("First line"),
            reply: nil,
            isSending: false,
            cancelReply: {},
            sendImage: { _, _, _ in true },
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
