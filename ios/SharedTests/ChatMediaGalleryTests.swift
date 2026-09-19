import XCTest
#if os(macOS)
@testable import BabelBridgeMac
#else
@testable import WhatsAppTranslator
#endif

@MainActor
final class ChatMediaGalleryTests: XCTestCase {
    private func media(_ id: String, kind: String = "image", chat: String = "gallery@g.us", timestamp: Int64 = 100, revision: Int64 = 0) throws -> ChatMessage {
        let value: [String: Any] = [
            "id": id, "contactId": chat, "timestamp": timestamp,
            "isFromMe": false, "isForwarded": false, "chatType": "group",
            "contentType": kind, "isTranslated": false,
            "content": ["type": kind, "has_media": true, "edited_at_ms": revision]
        ]
        return try JSONDecoder().decode(ChatMessage.self, from: JSONSerialization.data(withJSONObject: value))
    }

    func testGalleryIncludesOnlyChatPhotosAndVideosInStableNewestFirstOrder() throws {
        let items = try [media("photo"), media("video", kind: "video"), media("audio", kind: "audio"), media("sticker", kind: "sticker"), media("document", kind: "document"), media("text", kind: "text"), media("other", chat: "other@g.us"), media("recent", timestamp: 200)]
        XCTAssertEqual(ChatMediaGalleryModel.ordered(items, contactID: "gallery@g.us").map(\.id), ["recent", "video", "photo"])
    }

    func testPaginationUsesServerCursorWithoutSkippingHistoryAfterLiveArrival() async throws {
        let model = ChatMediaGalleryModel(contactID: "gallery@g.us")
        let latest = try media("latest", timestamp: 200)
        let boundary = try media("boundary", kind: "video", timestamp: 100)
        await model.load { cursor in
            XCTAssertNil(cursor)
            return MessagesResponse(messages: [boundary, latest], hasMore: true)
        }
        model.merge([try media("live-old", timestamp: 20), try media("live-new", timestamp: 300)])
        await model.load { cursor in
            XCTAssertEqual(cursor?.id, "boundary")
            return MessagesResponse(messages: [try self.media("older", timestamp: 50)], hasMore: false)
        }
        XCTAssertEqual(model.messages.map(\.id), ["live-new", "latest", "boundary", "older", "live-old"])
        XCTAssertFalse(model.hasMore)
    }

    func testRevokedPhotoCannotReappearFromAnOlderPage() throws {
        let model = ChatMediaGalleryModel(contactID: "gallery@g.us")
        let original = try media("photo", revision: 500)
        model.merge([original])
        let revoked = try media("photo", kind: "revoked")
        model.merge([revoked])
        model.merge([original])
        XCTAssertTrue(model.messages.isEmpty)
        XCTAssertTrue(ChatMediaGalleryModel.ordered([original, revoked, original], contactID: model.contactID).isEmpty)
    }

    func testFailedPageCanRetryWithoutLosingLoadedMedia() async throws {
        let model = ChatMediaGalleryModel(contactID: "gallery@g.us")
        await model.load { _ in MessagesResponse(messages: [try self.media("photo")], hasMore: true) }
        await model.load { _ in throw URLError(.notConnectedToInternet) }
        XCTAssertEqual(model.messages.count, 1)
        XCTAssertNotNil(model.error)
        await model.load { cursor in
            XCTAssertEqual(cursor?.id, "photo")
            return MessagesResponse(messages: [try self.media("older", timestamp: 50)], hasMore: false)
        }
        XCTAssertNil(model.error)
        XCTAssertEqual(model.messages.count, 2)
    }
}
