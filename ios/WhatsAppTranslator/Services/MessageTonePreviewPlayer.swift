import AVFoundation
import Observation

@MainActor
@Observable
final class MessageTonePreviewPlayer {
    private var player: AVAudioPlayer?
    private var finishTask: Task<Void, Never>?
    private(set) var playingTone: MessageTone?

    func play(_ tone: MessageTone, bundle: Bundle = .main) throws {
        stop()
        guard tone.filename != nil else { return }
        guard let url = tone.resourceURL(in: bundle) else {
            throw APIError.server("This ringtone is missing. Please update Babel Bridge.")
        }
        #if os(iOS)
        // An explicit preview should be audible even in silent mode. Actual message
        // alerts use the notification system and still respect its sound settings.
        try AVAudioSession.sharedInstance().setCategory(.playback, mode: .default, options: [.mixWithOthers])
        try AVAudioSession.sharedInstance().setActive(true)
        #endif
        let player = try AVAudioPlayer(contentsOf: url)
        player.prepareToPlay()
        guard player.play() else { throw APIError.server("The ringtone couldn’t be played.") }
        self.player = player
        playingTone = tone
        let duration = player.duration
        finishTask = Task { [weak self] in
            do { try await Task.sleep(for: .seconds(duration)) }
            catch { return }
            self?.stop()
        }
    }

    var isPlaying: Bool { player?.isPlaying == true }

    func stop() {
        finishTask?.cancel()
        finishTask = nil
        player?.stop()
        player = nil
        playingTone = nil
    }
}
