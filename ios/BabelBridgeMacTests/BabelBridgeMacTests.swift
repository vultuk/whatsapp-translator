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
