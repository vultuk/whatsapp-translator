import AVKit
import XCTest
@testable import WhatsAppTranslator

@MainActor
final class VideoPlaybackTests: XCTestCase {
    private func playingVideo() async throws -> MessageVideoPlayback {
        let url = try XCTUnwrap(Bundle.main.url(forResource: "video-playback-fixture", withExtension: "mp4"))
        let playback = MessageVideoPlayback(url: url)
        playback.attach()
        playback.player.play()
        for _ in 0..<100 {
            if playback.player.currentTime().seconds > 0.05 { return playback }
            try await Task.sleep(for: .milliseconds(50))
        }
        XCTFail("The local video must actually start and advance")
        return playback
    }

    func testLeavingAnOrdinaryVideoStopsPlayback() async throws {
        let playback = try await playingVideo()
        playback.detach()
        XCTAssertEqual(playback.player.rate, 0)
    }

    func testFullScreenAndPiPKeepTheSamePlayheadWhenBubbleDisappears() async throws {
        let playback = try await playingVideo()
        defer { MessageVideoPlayback.stopAll() }
        let item = playback.player.currentItem
        playback.setFullScreen(true)
        playback.detach()
        let before = playback.player.currentTime().seconds
        try await Task.sleep(for: .milliseconds(200))
        XCTAssertGreaterThan(playback.player.currentTime().seconds, before)
        playback.setPictureInPicture(true)
        playback.setFullScreen(false)
        let inPiP = playback.player.currentTime().seconds
        try await Task.sleep(for: .milliseconds(200))
        XCTAssertGreaterThan(playback.player.currentTime().seconds, inPiP)
        XCTAssertTrue(playback.player.currentItem === item)
        playback.setPictureInPicture(false)
        XCTAssertEqual(playback.player.rate, 0, "Closing detached PiP must release playback")
    }

    func testReturningFromPiPToAnAttachedMessageKeepsPlaying() async throws {
        let playback = try await playingVideo()
        defer { playback.detach() }
        playback.setPictureInPicture(true)
        playback.setPictureInPicture(false)
        let before = playback.player.currentTime().seconds
        try await Task.sleep(for: .milliseconds(200))
        XCTAssertGreaterThan(playback.player.currentTime().seconds, before)
    }

    func testStartingAnotherVideoPausesThePreviousVideoAndLogoutStopsPiP() async throws {
        let first = try await playingVideo()
        first.setPictureInPicture(true)
        first.detach()
        let second = try await playingVideo()
        XCTAssertEqual(first.player.rate, 0)
        MessageVideoPlayback.stopAll()
        XCTAssertEqual(second.player.rate, 0)
        XCTAssertNil(first.player.currentItem, "Account removal must clear a retained PiP video")
        second.detach()
    }
}
