import XCTest
@testable import BabelBridgeMac

final class SendRecoveryTests: XCTestCase {
    func testRetryIdentitySurvivesRestartAndCanonicalizesTheDraft() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: directory) }
        let server = URL(string: "https://translator.example.test")!
        let first = SendRecoveryStore(directory: directory)
        let original = try await first.identity(server: server, path: "/api/send", method: "POST", body: Data(#"{"contactId":"one","text":"Hello"}"#.utf8))
        let resumed = SendRecoveryStore(directory: directory)
        let retry = try await resumed.identity(server: server, path: "/api/send", method: "POST", body: Data(#"{"text":"Hello","contactId":"one"}"#.utf8))
        XCTAssertEqual(original?.key, retry?.key)
        let uncertain = HTTPURLResponse(url: server, statusCode: 409, httpVersion: nil, headerFields: ["X-Delivery-State": "uncertain"])!
        try await resumed.settle(retry, response: uncertain)
        let stillPending = try await resumed.identity(server: server, path: "/api/send", method: "POST", body: Data(#"{"contactId":"one","text":"Hello"}"#.utf8))
        XCTAssertEqual(retry?.key, stillPending?.key)
        let confirmed = HTTPURLResponse(url: server, statusCode: 200, httpVersion: nil, headerFields: ["X-Delivery-State": "confirmed"])!
        try await resumed.settle(retry, response: confirmed)
        let nextSend = try await resumed.identity(server: server, path: "/api/send", method: "POST", body: Data(#"{"contactId":"one","text":"Hello"}"#.utf8))
        XCTAssertNotEqual(retry?.key, nextSend?.key)
        try await resumed.settle(retry, response: confirmed)
        let afterStaleResult = try await resumed.identity(server: server, path: "/api/send", method: "POST", body: Data(#"{"contactId":"one","text":"Hello"}"#.utf8))
        XCTAssertEqual(nextSend?.key, afterStaleResult?.key)
    }

    func testDifferentRecipientsDoNotShareRetryIdentity() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: directory) }
        let store = SendRecoveryStore(directory: directory)
        let server = URL(string: "https://translator.example.test")!
        let one = try await store.identity(server: server, path: "/api/send", method: "POST", body: Data(#"{"contactId":"one","text":"Hello"}"#.utf8))
        let two = try await store.identity(server: server, path: "/api/send", method: "POST", body: Data(#"{"contactId":"two","text":"Hello"}"#.utf8))
        XCTAssertNotEqual(one?.key, two?.key)
        let read = try await store.identity(server: server, path: "/api/status", method: "GET", body: nil)
        XCTAssertNil(read)
    }

    func testUncertainDeliveryIsNeverDisplayedAsSent() throws {
        let message = try JSONDecoder.backend.decode(ChatMessage.self, from: Data(#"{"id":"pending","contactId":"one","timestamp":1,"isFromMe":true,"isForwarded":false,"chatType":"private","contentType":"text","isTranslated":false,"deliveryStatus":"uncertain"}"#.utf8))
        XCTAssertEqual(message.deliveryState, .uncertain)
        XCTAssertTrue(message.deliveryState.accessibilityLabel.contains("uncertain"))
    }
}
