import AVFoundation
import SwiftUI

struct VoicePreferenceSection: View {
    @Environment(AppSession.self) private var session
    let scope: String
    @State private var preferences = VoicePreferences()
    @State private var sample: VoiceSample?
    @State private var sampleBusy = false
    @State private var ready = false
    @State private var saving = false
    @State private var error: String?

    var body: some View {
        Section {
            Picker(scope == "outgoing" ? "My translated voice" : "Contact’s translated voice", selection: $preferences.voice) {
                Text("Automatic · match pitch").tag("auto")
                Text("Masculine").tag("masculine")
                Text("Feminine").tag("feminine")
                Text("Neutral").tag("neutral")
            }
            .disabled(!ready || saving || sampleBusy)
            .onChange(of: preferences.voice) { _, _ in
                guard ready else { return }
                saving = true
                sample = nil
                Task {
                    do { try await session.saveVoicePreferences(preferences, scope: scope); error = nil }
                    catch { self.error = error.localizedDescription }
                    saving = false
                }
            }
            if saving { ProgressView("Saving voice…") }
            if let sample {
                VoiceAudioPreview(encoded: sample.audioData, title: preferences.voice == "auto" ? "neutral fallback sample" : "voice sample")
            } else {
                Button(sampleBusy ? "Preparing sample…" : "Preview voice") {
                    sampleBusy = true
                    Task {
                        defer { sampleBusy = false }
                        do { sample = try await session.voiceSample(preference: preferences.voice) }
                        catch { self.error = error.localizedDescription }
                    }
                }.disabled(!ready || saving || sampleBusy)
            }
            if let error { Text(error).foregroundStyle(.red).font(.caption) }
        } header: { Text("Voice translation") } footer: {
            Text("Saved on your server. Automatic approximately matches vocal pitch and uses a neutral voice when uncertain. Translated speech is AI-generated.")
        }
        .task {
            do { preferences = try await session.voicePreferences(scope: scope); ready = true }
            catch { self.error = error.localizedDescription }
        }
    }
}

struct VoiceAudioPreview: View {
    let encoded: String
    let title: String
    var compact = false
    @State private var player: AVAudioPlayer?
    @State private var playing = false
    @State private var error: String?
    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Button {
                do {
                    if playing { player?.pause(); playing = false; return }
                    guard let data = Data(base64Encoded: encoded) else { throw APIError.server("Audio could not be decoded.") }
                    #if os(iOS)
                    try AVAudioSession.sharedInstance().setCategory(.playback, mode: .default)
                    try AVAudioSession.sharedInstance().setActive(true)
                    #endif
                    if player == nil { player = try AVAudioPlayer(data: data) }
                    guard player?.play() == true else { throw APIError.server("Audio could not be played.") }
                    playing = true
                } catch { self.error = error.localizedDescription }
            } label: {
                if compact {
                    HStack(spacing: 10) {
                        Image(systemName: playing ? "pause.fill" : "play.fill")
                            .font(.system(size: 24))
                            .frame(width: 44, height: 44)
                        Image(systemName: "waveform")
                            .resizable().scaledToFit().frame(width: 145, height: 28)
                    }
                    .foregroundStyle(.secondary)
                    .accessibilityLabel(playing ? "Pause \(title)" : "Play \(title)")
                } else {
                    Label(playing ? "Pause \(title)" : "Play \(title)", systemImage: playing ? "pause.circle.fill" : "play.circle.fill")
                        .frame(minHeight: 44)
                }
            }
            .buttonStyle(.plain)
            if let error { Text(error).font(.caption).foregroundStyle(.red) }
        }
        .onChange(of: encoded) { _, _ in player?.stop(); player = nil; playing = false }
        .onDisappear { player?.stop() }
        .task {
            while !Task.isCancelled {
                try? await Task.sleep(for: .milliseconds(300))
                if playing && player?.isPlaying != true { playing = false }
            }
        }
    }
}

struct VoiceTranslationPlayer: View {
    @Environment(AppSession.self) private var session
    let message: ChatMessage
    let originalURL: URL?
    @State private var note: TranslatedVoiceNote?
    @State private var busy = false
    @State private var error: String?
    @State private var showOriginal = false

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            if let note {
                VoiceAudioPreview(encoded: showOriginal ? note.originalData : note.audioData, title: showOriginal ? "original" : "translation", compact: true)
                Menu {
                    Picker("Recording", selection: $showOriginal) {
                        Text("Translated · AI voice").tag(false)
                        Text("Original recording").tag(true)
                    }
                } label: {
                    Label(showOriginal ? "Original" : "\(note.targetLanguage) · AI voice", systemImage: "chevron.down")
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                }
                DisclosureGroup("Transcript") {
                    Text(showOriginal ? note.transcript : note.translation).font(.callout).textSelection(.enabled)
                }
            } else {
                if let originalURL {
                    AudioMessagePlayer(url: originalURL, title: "Original voice note", duration: message.content?.durationSeconds)
                }
                if busy { ProgressView("Translating voice…").font(.caption) }
                else {
                    Button("Translate", systemImage: "character.bubble") { translate() }
                        .font(.caption)
                        .buttonStyle(.plain)
                        .foregroundStyle(.secondary)
                }
                if let error { Text(error).font(.caption).foregroundStyle(.red) }
            }
        }
        .frame(maxWidth: 280)
        .onChange(of: session.voicePreferenceRevision) { _, _ in
            if note != nil { translate() }
        }
        .task(id: session.voiceReadyIDs.contains(message.id) || message.isTranslated) {
            if note == nil && (session.voiceReadyIDs.contains(message.id) || message.isTranslated) { translate() }
        }
    }
    private func translate() {
        busy = true; error = nil
        Task {
            defer { busy = false }
            do { note = try await session.translateVoice(messageID: message.id) }
            catch { self.error = error.localizedDescription }
        }
    }
}

struct VoiceComposerView: View {
    @Environment(AppSession.self) private var session
    @Environment(\.dismiss) private var dismiss
    let contactID: String
    let reply: MessageReplyTarget?
    let onSent: () -> Void
    @State private var recorder: AVAudioRecorder?
    @State private var recordingURL: URL?
    @State private var recording = false
    @State private var seconds = 0
    @State private var busy = false
    @State private var sending = false
    @State private var note: TranslatedVoiceNote?
    @State private var error: String?
    @State private var warning: String?
    @State private var sent = false

    var body: some View {
        NavigationStack {
            Form {
                VoicePreferenceSection(scope: "outgoing")
                Section {
                    if let note {
                        Text("Ready in \(note.targetLanguage)").font(.headline)
                        VoiceAudioPreview(encoded: note.audioData, title: "translated voice note")
                        VoiceAudioPreview(encoded: note.originalData, title: "original recording")
                        DisclosureGroup("Review transcript") {
                            Text(note.transcript).textSelection(.enabled)
                            Divider()
                            Text(note.translation).textSelection(.enabled)
                        }
                        if note.originalFollowUp { Text("Your original recording will follow the translation.").font(.caption) }
                        if !sent {
                            Button("Send translated voice note", systemImage: "paperplane.fill") { send() }
                                .disabled(busy)
                            Button("Record again", role: .destructive) { self.note = nil; cleanup() }
                                .disabled(busy || sending)
                        }
                    } else {
                        Label(recording ? "Recording · \(seconds)s / 180s" : "Record up to three minutes", systemImage: "waveform")
                        Button(recording ? "Stop and translate" : "Start recording", systemImage: recording ? "stop.circle.fill" : "mic.circle.fill") {
                            if recording { stopAndPrepare() } else { Task { await startRecording() } }
                        }.disabled(busy)
                    }
                    if busy { ProgressView(sending ? "Sending voice note…" : "Preparing translated voice…") }
                    if let error { Text(error).foregroundStyle(.red) }
                    if let warning { Text(warning).foregroundStyle(.orange) }
                    if sent { Label("Voice note sent", systemImage: "checkmark.circle.fill") }
                } header: { Text("Voice note") } footer: {
                    Text("The translation uses an AI-generated voice. Listen before sending. Changing the voice applies to the next preparation.")
                }
            }
            .navigationTitle("Translated voice note")
            .toolbar { ToolbarItem(placement: .cancellationAction) { Button(sent ? "Done" : "Close") { dismiss() }.disabled(busy) } }
        }
        .interactiveDismissDisabled(busy)
        #if os(macOS)
        .frame(minWidth: 480, minHeight: 460)
        #endif
        .onDisappear { cleanup() }
        .task {
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(1))
                if recording {
                    seconds = Int(recorder?.currentTime ?? Double(seconds + 1))
                    if seconds >= 180 || recorder?.isRecording == false { stopAndPrepare() }
                }
            }
        }
    }

    private func startRecording() async {
        busy = true
        defer { busy = false }
        error = nil
        cleanup()
        #if os(iOS)
        let allowed = await AVAudioApplication.requestRecordPermission()
        #else
        let allowed = await AVCaptureDevice.requestAccess(for: .audio)
        #endif
        guard allowed else { error = "Allow microphone access in system Settings to record a voice note."; return }
        do {
            #if os(iOS)
            try AVAudioSession.sharedInstance().setCategory(.playAndRecord, mode: .default, options: [.defaultToSpeaker])
            try AVAudioSession.sharedInstance().setActive(true)
            #endif
            let url = FileManager.default.temporaryDirectory.appendingPathComponent("voice-\(UUID().uuidString).m4a")
            recordingURL = url
            let recorder = try AVAudioRecorder(url: url, settings: [AVFormatIDKey: kAudioFormatMPEG4AAC, AVSampleRateKey: 44100, AVNumberOfChannelsKey: 1, AVEncoderBitRateKey: 64000])
            guard recorder.prepareToRecord() else { throw APIError.server("Microphone recording could not be prepared.") }
            #if os(iOS)
            try FileManager.default.setAttributes([.protectionKey: FileProtectionType.complete], ofItemAtPath: url.path)
            #endif
            guard recorder.record(forDuration: 180) else { throw APIError.server("Microphone recording could not start.") }
            self.recorder = recorder; recording = true; seconds = 0
        } catch { self.error = error.localizedDescription; cleanup() }
    }
    private func stopAndPrepare() {
        recorder?.stop(); recording = false
        guard let url = recordingURL else { return }
        busy = true
        Task {
            defer { busy = false }
            do { note = try await session.prepareVoice(data: Data(contentsOf: url), contactID: contactID, reply: reply) }
            catch { self.error = error.localizedDescription }
        }
    }
    private func send() {
        guard let note else { return }
        busy = true; sending = true; error = nil
        Task {
            defer { busy = false }
            do {
                let result = try await session.sendVoice(note)
                sent = result.success; warning = result.warning
                if sent { onSent() }
                if warning == nil { dismiss() }
            } catch { self.error = error.localizedDescription }
        }
    }
    private func cleanup() {
        recorder?.stop(); recorder = nil; recording = false
        if let recordingURL { try? FileManager.default.removeItem(at: recordingURL) }
        recordingURL = nil
        #if os(iOS)
        try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
        #endif
    }
}
