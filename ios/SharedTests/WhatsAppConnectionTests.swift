import XCTest
#if os(macOS)
@testable import BabelBridgeMac
#else
@testable import WhatsAppTranslator
#endif

@MainActor
final class WhatsAppConnectionTests: XCTestCase {
    private func event(_ json: String) throws -> LiveEvent {
        try JSONDecoder.backend.decode(LiveEvent.self, from: Data(json.utf8))
    }

    func testSessionRemovalOpensLinkingAndReconnectPreservesChats() async throws {
        let session = AppSession(demoMode: true)
        await session.start()
        let contacts = session.contacts
        let messages = session.messages
        session.configuration = try ServerConfiguration.make(address: "https://example.com", password: "test-only")
        session.handle(try event(#"{"type":"status","connected":false,"connection_state":"linking_required","qr":null}"#))
        XCTAssertTrue(session.requiresWhatsAppLink)
        XCTAssertNil(session.whatsAppQRCode)
        session.handle(try event(#"{"type":"qr","data":"test-link-code"}"#))
        XCTAssertEqual(session.whatsAppQRCode, "test-link-code")
        session.handle(try event(#"{"type":"connected","name":"Preview","phone":"447700900123"}"#))
        XCTAssertFalse(session.requiresWhatsAppLink)
        XCTAssertTrue(session.backendStatus.connected)
        XCTAssertNil(session.whatsAppQRCode)
        XCTAssertEqual(session.backendStatus.name, "Preview")
        XCTAssertEqual(session.contacts, contacts)
        XCTAssertEqual(session.messages, messages)
        XCTAssertEqual(session.configuration?.password, "test-only")
    }

    func testTemporaryDisconnectKeepsInboxAndDoesNotRequestNewLink() async throws {
        let session = AppSession(demoMode: true)
        await session.start()
        session.handle(try event(#"{"type":"disconnected"}"#))
        XCTAssertFalse(session.backendStatus.connected)
        XCTAssertFalse(session.requiresWhatsAppLink)
        XCTAssertEqual(session.phase, .ready)
        XCTAssertFalse(session.contacts.isEmpty)
    }

    func testFreshStatusRecoversMissedLinkEventAndRemovesExpiredCode() throws {
        let session = AppSession(demoMode: true)
        session.phase = .ready
        let status = try JSONDecoder.backend.decode(BackendStatus.self, from: Data(#"{"connected":false,"connection_state":"linking_required","qr":"fresh-code"}"#.utf8))
        session.applyBackendStatus(status)
        XCTAssertTrue(session.requiresWhatsAppLink)
        XCTAssertEqual(session.whatsAppQRCode, "fresh-code")
        session.handle(try event(#"{"type":"status","connected":false,"connection_state":"linking_required","qr":null}"#))
        XCTAssertTrue(session.requiresWhatsAppLink)
        XCTAssertNil(session.whatsAppQRCode)
        session.handle(try event(#"{"type":"status","connected":true,"connection_state":"connected","qr":null}"#))
        XCTAssertFalse(session.requiresWhatsAppLink)
    }

    func testLegacyStatusAndQRRemainCompatible() throws {
        let session = AppSession(demoMode: true)
        session.phase = .ready
        let status = try JSONDecoder.backend.decode(BackendStatus.self, from: Data(#"{"connected":false,"phone":null,"name":null}"#.utf8))
        session.applyBackendStatus(status)
        XCTAssertFalse(session.requiresWhatsAppLink)
        session.handle(try event(#"{"type":"qr","data":"legacy-code"}"#))
        XCTAssertTrue(session.requiresWhatsAppLink)
        session.handle(try event(#"{"type":"status","connected":true}"#))
        XCTAssertFalse(session.requiresWhatsAppLink)
        XCTAssertNil(session.whatsAppQRCode)
    }
}
