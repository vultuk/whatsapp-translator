import XCTest
#if os(macOS)
@testable import BabelBridgeMac
#else
@testable import WhatsAppTranslator
#endif

@MainActor
final class ConnectionBannerTests: XCTestCase {
    private func event(_ json: String) throws -> LiveEvent {
        try JSONDecoder.backend.decode(LiveEvent.self, from: Data(json.utf8))
    }

    private func assertNeverSaysReconnecting(
        _ banner: WhatsAppConnectionBanner,
        file: StaticString = #filePath,
        line: UInt = #line
    ) {
        let copy = (banner.displayedCopy + " " + banner.statusLabel).lowercased()
        XCTAssertFalse(copy.contains("reconnecting"), copy, file: file, line: line)
        XCTAssertNotEqual(banner.kind, .reconnecting, file: file, line: line)
    }

    func testReconnectingStatusShowsReconnectingBanner() {
        let banner = WhatsAppConnectionBanner.resolve(connected: false, connectionState: "reconnecting")
        XCTAssertEqual(banner.kind, .reconnecting)
        XCTAssertTrue(banner.showsBanner)
        XCTAssertTrue(banner.displayedCopy.lowercased().contains("reconnecting"))
        XCTAssertFalse(banner.displayedCopy.lowercased().contains("disconnected"))
        XCTAssertEqual(banner.statusLabel, "Reconnecting")
    }

    func testStoppedStatusShowsDisconnectedBanner() {
        let banner = WhatsAppConnectionBanner.resolve(connected: false, connectionState: "disconnected")
        XCTAssertEqual(banner.kind, .disconnected)
        XCTAssertTrue(banner.showsBanner)
        XCTAssertTrue(banner.displayedCopy.lowercased().contains("disconnected"))
        XCTAssertEqual(banner.statusLabel, "Disconnected")
        assertNeverSaysReconnecting(banner)
    }

    func testLoggedOutStatusIncludesRelinkHint() {
        let banner = WhatsAppConnectionBanner.resolve(connected: false, connectionState: "linking_required")
        XCTAssertEqual(banner.kind, .linkingRequired)
        XCTAssertFalse(banner.showsBanner)
        XCTAssertEqual(banner.hint, WhatsAppConnectionBanner.relinkHint)
        XCTAssertTrue(banner.displayedCopy.lowercased().contains("link"))
        XCTAssertTrue(banner.displayedCopy.lowercased().contains("phone"))
        XCTAssertEqual(banner.statusLabel, "Needs linking")
        assertNeverSaysReconnecting(banner)
    }

    func testConnectedStatusHidesBanner() {
        let states: [String?] = ["connected", nil, "reconnecting", "disconnected"]
        for state in states {
            let banner = WhatsAppConnectionBanner.resolve(connected: true, connectionState: state)
            XCTAssertEqual(banner.kind, .hidden, state ?? "nil")
            XCTAssertFalse(banner.showsBanner)
            XCTAssertEqual(banner.displayedCopy, "")
            XCTAssertEqual(banner.statusLabel, "Connected")
        }
    }

    func testMissingAndUnknownStatusDoNotShowReconnecting() {
        let states: [String?] = [nil, "", "   ", "banned", "client_outdated", "logged_out", "CONNECTED"]
        for state in states {
            let banner = WhatsAppConnectionBanner.resolve(connected: false, connectionState: state)
            XCTAssertEqual(banner.kind, .disconnected, state ?? "nil")
            XCTAssertTrue(banner.displayedCopy.lowercased().contains("disconnected"), state ?? "nil")
            assertNeverSaysReconnecting(banner)
        }
    }

    func testInitialConnectingStateIsNotARetryOrAStop() {
        let banner = WhatsAppConnectionBanner.resolve(connected: false, connectionState: "connecting")
        XCTAssertEqual(banner.kind, .connecting)
        XCTAssertTrue(banner.showsBanner)
        XCTAssertEqual(banner.statusLabel, "Connecting")
        XCTAssertFalse(banner.displayedCopy.lowercased().contains("disconnected"))
        assertNeverSaysReconnecting(banner)
    }

    func testKnownStatesIgnoreSurroundingSpaceAndCase() {
        XCTAssertEqual(
            WhatsAppConnectionBanner.resolve(connected: false, connectionState: " Reconnecting ").kind,
            .reconnecting
        )
        XCTAssertEqual(
            WhatsAppConnectionBanner.resolve(connected: false, connectionState: "LINKING_REQUIRED").kind,
            .linkingRequired
        )
    }

    func testBareDisconnectPreservesRetryAndLinkAndOtherwiseStops() {
        XCTAssertEqual(WhatsAppConnectionBanner.statePreservedByBareDisconnect(current: "reconnecting"), "reconnecting")
        XCTAssertEqual(WhatsAppConnectionBanner.statePreservedByBareDisconnect(current: "linking_required"), "linking_required")
        for current in [nil, "", "disconnected", "connecting", "connected", "banned"] {
            XCTAssertEqual(
                WhatsAppConnectionBanner.statePreservedByBareDisconnect(current: current),
                "disconnected",
                current ?? "nil"
            )
        }
    }

    func testFetchedStoppedStatusStaysDisconnectedAfterCompanionEvent() throws {
        let session = readySession()
        session.handle(try event(#"{"type":"status","connected":false,"connection_state":"disconnected","qr":null}"#))
        session.handle(try event(#"{"type":"disconnected"}"#))
        let banner = session.connectionBanner
        XCTAssertEqual(banner.kind, .disconnected)
        XCTAssertFalse(session.requiresWhatsAppLink)
        assertNeverSaysReconnecting(banner)
    }

    func testFetchedRetryingStatusStaysReconnectingAfterCompanionEvent() throws {
        let session = readySession()
        session.handle(try event(#"{"type":"status","connected":false,"connection_state":"reconnecting","qr":null}"#))
        session.handle(try event(#"{"type":"disconnected"}"#))
        XCTAssertEqual(session.connectionBanner.kind, .reconnecting)
        XCTAssertTrue(session.connectionBanner.displayedCopy.lowercased().contains("reconnecting"))
        XCTAssertFalse(session.requiresWhatsAppLink)
    }

    func testLoggedOutStatusKeepsRelinkHintAfterCompanionEvent() throws {
        let session = readySession()
        session.handle(try event(#"{"type":"status","connected":false,"connection_state":"linking_required","qr":null}"#))
        session.handle(try event(#"{"type":"disconnected"}"#))
        XCTAssertTrue(session.requiresWhatsAppLink)
        XCTAssertEqual(session.connectionBanner.kind, .linkingRequired)
        XCTAssertEqual(session.connectionBanner.hint, WhatsAppConnectionBanner.relinkHint)
        assertNeverSaysReconnecting(session.connectionBanner)
    }

    func testConnectedStatusClearsTheBanner() throws {
        let session = readySession()
        session.handle(try event(#"{"type":"status","connected":false,"connection_state":"disconnected","qr":null}"#))
        session.handle(try event(#"{"type":"status","connected":true,"connection_state":"connected","qr":null}"#))
        XCTAssertEqual(session.connectionBanner.kind, .hidden)
        XCTAssertFalse(session.connectionBanner.showsBanner)
        XCTAssertFalse(session.requiresWhatsAppLink)
    }

    func testLegacyStatusWithoutConnectionStateDoesNotShowReconnecting() throws {
        let session = readySession()
        session.handle(try event(#"{"type":"status","connected":false}"#))
        assertNeverSaysReconnecting(session.connectionBanner)
        XCTAssertEqual(session.connectionBanner.kind, .disconnected)
        XCTAssertFalse(session.requiresWhatsAppLink)
        session.handle(try event(#"{"type":"status","connected":true}"#))
        XCTAssertEqual(session.connectionBanner.kind, .hidden)
    }

    private func readySession() -> AppSession {
        let session = AppSession(demoMode: true)
        session.phase = .ready
        return session
    }
}
