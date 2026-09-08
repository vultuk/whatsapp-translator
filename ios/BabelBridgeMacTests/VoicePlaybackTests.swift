import AVFoundation
import XCTest
@testable import BabelBridgeMac

final class VoicePlaybackTests: XCTestCase {
    @MainActor
    func testConvertedWhatsAppAudioActuallyStartsAndAdvances() async throws {
        let url = try XCTUnwrap(Bundle(for: Self.self).url(forResource: "playback-fixture", withExtension: "mp3"))
        let player = try AVAudioPlayer(contentsOf: url)
        #if os(iOS)
        try AVAudioSession.sharedInstance().setCategory(.playback, mode: .default)
        try AVAudioSession.sharedInstance().setActive(true)
        #endif
        defer { player.stop() }
        XCTAssertTrue(player.play())
        try await Task.sleep(for: .milliseconds(250))
        XCTAssertTrue(player.isPlaying)
        XCTAssertGreaterThan(player.currentTime, 0.05)
        XCTAssertGreaterThan(player.duration, 0.9)
    }
}
