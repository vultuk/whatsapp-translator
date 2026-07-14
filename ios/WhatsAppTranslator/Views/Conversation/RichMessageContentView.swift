import AVKit
import SwiftUI

struct RichMessageContentView: View {
    let message: ChatMessage
    let displayText: String
    let image: PlatformImage?
    let mediaURL: URL?
    let isLoading: Bool
    let failed: Bool
    let retry: () -> Void

    var body: some View {
        Group {
            switch message.mediaKind {
            case .image:
                imageContent(maxWidth: 280, maxHeight: 280)
                caption
            case .sticker:
                imageContent(maxWidth: 180, maxHeight: 180)
            case .video:
                if let mediaURL {
                    InlineVideoPlayer(url: mediaURL)
                } else {
                    mediaPlaceholder(systemImage: "video.fill", title: "Video")
                }
                caption
            case .audio:
                if let mediaURL {
                    AudioMessagePlayer(
                        url: mediaURL,
                        title: message.content?.isVoiceNote == true ? "Voice note" : "Audio",
                        duration: message.content?.durationSeconds
                    )
                } else {
                    mediaPlaceholder(systemImage: "waveform", title: message.displayText)
                }
            case .document:
                if let mediaURL {
                    DocumentMessageView(
                        url: mediaURL,
                        name: message.content?.fileName ?? "Document",
                        fileSize: message.content?.fileSize
                    )
                } else {
                    mediaPlaceholder(systemImage: "doc.fill", title: message.displayText)
                }
            case nil:
                nonMediaContent
            }
        }
    }

    @ViewBuilder
    private func imageContent(maxWidth: CGFloat, maxHeight: CGFloat) -> some View {
        if let image {
            Image(platformImage: image)
                .resizable()
                .scaledToFit()
                .frame(maxWidth: maxWidth, maxHeight: maxHeight)
                .clipShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
        } else {
            mediaPlaceholder(
                systemImage: message.mediaKind == .sticker ? "face.smiling" : "photo.fill",
                title: message.mediaKind == .sticker ? "Sticker" : "Image"
            )
        }
    }

    @ViewBuilder
    private var caption: some View {
        if let caption = message.content?.caption?.trimmingCharacters(in: .whitespacesAndNewlines), !caption.isEmpty {
            Text(caption)
                .font(.body)
                .textSelection(.enabled)
        }
    }

    @ViewBuilder
    private var nonMediaContent: some View {
        switch message.normalizedContentType {
        case "location":
            if let url = message.locationURL {
                Link(destination: url) {
                    Label {
                        VStack(alignment: .leading, spacing: 2) {
                            Text(message.content?.name ?? "Location").fontWeight(.semibold)
                            if let address = message.content?.address { Text(address).font(.caption).foregroundStyle(.secondary) }
                        }
                    } icon: {
                        Image(systemName: "map.fill").font(.title2)
                    }
                    .padding(9)
                    .background(.primary.opacity(0.06), in: RoundedRectangle(cornerRadius: 11))
                }
                .buttonStyle(.plain)
            } else {
                Label(displayText, systemImage: "map.fill")
            }
        case "contact":
            Label {
                VStack(alignment: .leading, spacing: 2) {
                    Text(message.content?.displayName ?? message.content?.name ?? "Contact").fontWeight(.semibold)
                    if let vcard = message.content?.vcard, !vcard.isEmpty {
                        Text(vcard.components(separatedBy: .newlines).first(where: { $0.hasPrefix("TEL") }) ?? "Contact card")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                }
            } icon: {
                Image(systemName: "person.crop.circle.fill").font(.title2)
            }
            .padding(9)
            .background(.primary.opacity(0.06), in: RoundedRectangle(cornerRadius: 11))
        case "poll":
            VStack(alignment: .leading, spacing: 7) {
                Label(message.content?.question ?? "Poll", systemImage: "chart.bar.fill")
                    .font(.body.weight(.semibold))
                ForEach(message.content?.options ?? [], id: \.self) { option in
                    Label(option, systemImage: "circle")
                        .font(.callout)
                }
            }
            .padding(9)
            .background(.primary.opacity(0.06), in: RoundedRectangle(cornerRadius: 11))
        case "revoked":
            Label(displayText, systemImage: "nosign")
                .font(.body.italic())
                .foregroundStyle(.secondary)
        default:
            Text(displayText)
                .font(.body)
                .textSelection(.enabled)
                .fixedSize(horizontal: false, vertical: true)
        }
    }

    private func mediaPlaceholder(systemImage: String, title: String) -> some View {
        HStack(spacing: 10) {
            if isLoading {
                ProgressView()
            } else {
                Image(systemName: failed ? "exclamationmark.arrow.trianglehead.2.clockwise.rotate.90" : systemImage)
                    .font(.title2)
            }
            VStack(alignment: .leading, spacing: 2) {
                Text(title).font(.callout.weight(.semibold))
                if failed { Text("Tap to try again").font(.caption).foregroundStyle(.secondary) }
            }
        }
        .padding(12)
        .frame(minWidth: 190, minHeight: 74, alignment: .leading)
        .background(.primary.opacity(0.06), in: RoundedRectangle(cornerRadius: 12, style: .continuous))
        .contentShape(Rectangle())
        .onTapGesture { if failed { retry() } }
        .accessibilityLabel(failed ? "Media failed to load. Try again." : title)
    }
}

private struct InlineVideoPlayer: View {
    let url: URL
    @State private var player: AVPlayer

    init(url: URL) {
        self.url = url
        _player = State(initialValue: AVPlayer(url: url))
    }

    var body: some View {
        #if os(macOS)
        MacInlineVideoPlayer(player: player)
            .frame(width: 280, height: 190)
            .clipShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
            .onDisappear { player.pause() }
        #else
        VideoPlayer(player: player)
            .frame(width: 280, height: 190)
            .clipShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
            .onDisappear { player.pause() }
        #endif
    }
}

#if os(macOS)
private struct MacInlineVideoPlayer: NSViewRepresentable {
    let player: AVPlayer

    func makeNSView(context: Context) -> AVPlayerView {
        let playerView = AVPlayerView()
        playerView.player = player
        playerView.controlsStyle = .inline
        playerView.videoGravity = .resizeAspect
        return playerView
    }

    func updateNSView(_ playerView: AVPlayerView, context: Context) {
        if playerView.player !== player {
            playerView.player = player
        }
    }

    static func dismantleNSView(_ playerView: AVPlayerView, coordinator: Void) {
        playerView.player?.pause()
        playerView.player = nil
    }
}
#endif

private struct AudioMessagePlayer: View {
    let url: URL
    let title: String
    let duration: Double?
    @State private var player: AVPlayer?
    @State private var isPlaying = false

    var body: some View {
        HStack(spacing: 10) {
            Button(isPlaying ? "Pause" : "Play", systemImage: isPlaying ? "pause.fill" : "play.fill") {
                if player == nil { player = AVPlayer(url: url) }
                if isPlaying { player?.pause() } else { player?.play() }
                isPlaying.toggle()
            }
            .labelStyle(.iconOnly)
            .buttonStyle(.borderedProminent)
            .buttonBorderShape(.circle)
            Image(systemName: "waveform").foregroundStyle(.secondary)
            VStack(alignment: .leading, spacing: 1) {
                Text(title).font(.callout.weight(.semibold))
                if let duration { Text(duration.formatted(.number.precision(.fractionLength(0))) + " sec").font(.caption).foregroundStyle(.secondary) }
            }
        }
        .padding(9)
        .frame(minWidth: 220, alignment: .leading)
        .background(.primary.opacity(0.06), in: RoundedRectangle(cornerRadius: 12))
        .onDisappear { player?.pause() }
    }
}

private struct DocumentMessageView: View {
    let url: URL
    let name: String
    let fileSize: Int?

    var body: some View {
        HStack(spacing: 10) {
            Link(destination: url) {
                Label {
                    VStack(alignment: .leading, spacing: 2) {
                        Text(name).font(.callout.weight(.semibold)).lineLimit(2)
                        if let fileSize {
                            Text(ByteCountFormatter.string(fromByteCount: Int64(fileSize), countStyle: .file))
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                    }
                } icon: {
                    Image(systemName: "doc.fill").font(.title2)
                }
            }
            .buttonStyle(.plain)
            Spacer(minLength: 4)
            ShareLink(item: url) { Image(systemName: "square.and.arrow.up") }
                .labelStyle(.iconOnly)
        }
        .padding(10)
        .frame(minWidth: 230, alignment: .leading)
        .background(.primary.opacity(0.06), in: RoundedRectangle(cornerRadius: 12))
    }
}
