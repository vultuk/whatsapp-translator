import XCTest
@testable import WhatsAppTranslator

final class WatchMessageTransportTests: XCTestCase {
    @MainActor
    func testBackgroundReplyReturnsToMainActor() async throws {
        let expected = try WatchResponse(status: .feed, snapshot: WatchFeedSnapshot.demo.encoded()).encoded()
        let request = try JSONEncoder().encode(WatchRequest(action: .feed))
        let response = try await WatchMessageTransport.response(for: request) { data, reply, _ in
            XCTAssertEqual(data, request)
            DispatchQueue.global().async { reply(expected) }
        }
        MainActor.assertIsolated()
        XCTAssertEqual(response, expected)
        XCTAssertEqual(try JSONDecoder().decode(WatchResponse.self, from: response).status, .feed)
    }

    @MainActor
    func testBackgroundConnectionFailureThrowsWithoutCrashing() async {
        do {
            _ = try await WatchMessageTransport.response(for: Data()) { _, _, failure in
                DispatchQueue.global().async { failure(URLError(.notConnectedToInternet)) }
            }
            XCTFail("Connection errors must reach the Watch recovery UI.")
        } catch {
            MainActor.assertIsolated()
            XCTAssertEqual((error as? URLError)?.code, .notConnectedToInternet)
        }
    }
}
