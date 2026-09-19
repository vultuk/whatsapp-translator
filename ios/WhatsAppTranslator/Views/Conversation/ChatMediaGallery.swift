import AVKit
import Observation
import SwiftUI

@MainActor @Observable
final class ChatMediaGalleryModel {
    let contactID: String
    private(set) var messages: [ChatMessage] = []
    private(set) var hasMore = true
    private(set) var isLoading = false
    private(set) var hasLoaded = false
    private(set) var error: String?
    private var cursor: ChatMessage?
    private var records: [String: ChatMessage] = [:]

    init(contactID: String) { self.contactID = contactID }

    static func ordered(_ values: [ChatMessage], contactID: String) -> [ChatMessage] {
        var latest: [String: ChatMessage] = [:]
        for message in values where message.contactId == contactID && !message.isReaction {
            if let previous = latest[message.id], !Self.shouldReplace(previous, with: message) { continue }
            latest[message.id] = message
        }
        return latest.values.filter { $0.mediaKind == .image || $0.mediaKind == .video }.sorted {
            $0.timestamp == $1.timestamp ? $0.id > $1.id : $0.timestamp > $1.timestamp
        }
    }

    func merge(_ values: [ChatMessage]) {
        for message in values where message.contactId == contactID && !message.isReaction {
            if let previous = records[message.id], !Self.shouldReplace(previous, with: message) { continue }
            records[message.id] = message
        }
        messages = Self.ordered(Array(records.values), contactID: contactID)
    }

    private static func shouldReplace(_ previous: ChatMessage, with message: ChatMessage) -> Bool {
        // Revocations do not always carry an edit timestamp and are terminal.
        if previous.normalizedContentType == "revoked" { return false }
        if message.normalizedContentType == "revoked" { return true }
        return message.editRevision >= previous.editRevision
    }

    func load(using fetch: (ChatMessage?) async throws -> MessagesResponse) async {
        guard !isLoading, hasMore else { return }
        isLoading = true
        error = nil
        defer { isLoading = false }
        do {
            let response = try await fetch(cursor)
            try Task.checkCancellation()
            merge(response.messages)
            let page = response.messages.filter { $0.contactId == contactID }.sorted {
                $0.timestamp == $1.timestamp ? $0.id < $1.id : $0.timestamp < $1.timestamp
            }
            cursor = page.first ?? cursor
            hasMore = response.hasMore && !page.isEmpty
            hasLoaded = true
        } catch is CancellationError {
        } catch {
            if !Task.isCancelled { self.error = "Couldn’t load the gallery. Please try again." }
        }
    }
}

struct ChatMediaGallery: View {
    @Environment(AppSession.self) private var session
    @Environment(\.dismiss) private var dismiss
    let contact: Contact
    @State private var model: ChatMediaGalleryModel
    @State private var selectedMedia: ChatMessage?

    init(contact: Contact) {
        self.contact = contact
        _model = State(initialValue: ChatMediaGalleryModel(contactID: contact.id))
    }

    private var days: [Date] {
        Array(Set(model.messages.map { Calendar.current.startOfDay(for: $0.date) })).sorted(by: >)
    }

    var body: some View {
        NavigationStack {
            GeometryReader { geometry in
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 20) {
                        if model.hasLoaded && model.messages.isEmpty {
                            ContentUnavailableView("No photos or videos yet", systemImage: "photo.on.rectangle.angled", description: Text("Photos and videos shared in this chat will appear here."))
                                .frame(maxWidth: .infinity).padding(.top, 70)
                        }
                        ForEach(days, id: \.self) { day in
                            VStack(alignment: .leading, spacing: 8) {
                                Text(day, format: .dateTime.day().month(.wide).year())
                                    .font(.headline).padding(.horizontal, 16)
                                LazyVGrid(columns: columns(for: geometry.size.width), spacing: 2) {
                                    ForEach(model.messages.filter { Calendar.current.isDate($0.date, inSameDayAs: day) }) { message in
                                        GalleryThumbnail(message: message) { selectedMedia = message }
                                            .onAppear {
                                                if message.id == model.messages.last?.id {
                                                    Task { await loadMore() }
                                                }
                                            }
                                    }
                                }
                            }
                        }
                        if let error = model.error {
                            VStack(spacing: 8) {
                                Text(error).foregroundStyle(.secondary)
                                Button("Try again") { Task { await loadMore() } }
                            }.frame(maxWidth: .infinity).padding()
                        } else if !model.hasLoaded {
                            ProgressView("Loading photos and videos…")
                                .frame(maxWidth: .infinity).padding()
                        } else if model.hasMore {
                            ProgressView("Loading photos and videos…")
                                .frame(maxWidth: .infinity).padding()
                        }
                    }
                    .padding(.top, 12).padding(.bottom, 24)
                }
                .accessibilityIdentifier("chat-media-gallery")
            }
            .navigationTitle("Gallery")
            .platformInlineNavigationTitle()
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Done") { dismiss() }.accessibilityIdentifier("close-chat-gallery")
                }
                ToolbarItem(placement: .principal) {
                    VStack(spacing: 2) {
                        Text("Gallery").font(.headline)
                        Text(session.displayName(for: contact)).font(.caption).foregroundStyle(.secondary).lineLimit(1)
                    }
                }
            }
        }
        .task { await loadMore() }
        .onChange(of: session.messages[contact.id]) { _, messages in model.merge(messages ?? []) }
        #if os(iOS)
        .fullScreenCover(item: $selectedMedia) { selected in
            GalleryMediaViewer(model: model, initialID: selected.id, close: { selectedMedia = nil }, loadMore: loadMore)
                .environment(session)
        }
        #else
        .frame(minWidth: 760, minHeight: 600)
        .background {
            if let selectedMedia {
                GalleryFullScreenWindow {
                    GalleryMediaViewer(model: model, initialID: selectedMedia.id, close: { self.selectedMedia = nil }, loadMore: loadMore)
                        .environment(session)
                }
            }
        }
        #endif
    }

    private func columns(for width: CGFloat) -> [GridItem] {
        let count = width < 500 ? 3 : max(4, Int(width / 145))
        return Array(repeating: GridItem(.flexible(), spacing: 2), count: count)
    }

    private func loadMore() async {
        await model.load { cursor in try await session.galleryPage(contactID: contact.id, before: cursor) }
    }
}

private struct GalleryThumbnail: View {
    @Environment(AppSession.self) private var session
    let message: ChatMessage
    let open: () -> Void
    @State private var image: PlatformImage?
    @State private var failed = false

    var body: some View {
        Button(action: open) {
            Color.secondary.opacity(0.12)
                .aspectRatio(1, contentMode: .fit)
                .overlay {
                    if let image {
                        GeometryReader { geometry in
                            Image(platformImage: image).resizable().scaledToFill()
                                .frame(width: geometry.size.width, height: geometry.size.height).clipped()
                        }
                    } else if failed {
                        Image(systemName: message.mediaKind == .video ? "video" : "photo").foregroundStyle(.secondary)
                    } else { ProgressView().controlSize(.small) }
                }
                .overlay(alignment: .bottomTrailing) {
                    if message.mediaKind == .video {
                        Label(duration, systemImage: "play.fill")
                            .font(.caption2.monospacedDigit().weight(.semibold))
                            .foregroundStyle(.white).padding(5)
                            .background(.black.opacity(0.65), in: RoundedRectangle(cornerRadius: 5)).padding(5)
                    }
                }
                .clipped().contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel("\(message.mediaKind == .video ? "Video" : "Photo"), \(message.date.formatted(date: .abbreviated, time: .shortened))")
        .accessibilityIdentifier("gallery-item-\(message.id)")
        .task(id: message.editRevision) {
            do {
                let result = try await session.galleryThumbnail(for: message)
                try Task.checkCancellation()
                image = result
                failed = false
            } catch { if !Task.isCancelled { failed = true } }
        }
        .onDisappear { image = nil }
    }

    private var duration: String {
        guard let duration = message.content?.durationSeconds else { return "Video" }
        let seconds = max(0, Int(duration))
        return String(format: "%d:%02d", seconds / 60, seconds % 60)
    }
}

struct GalleryMediaViewer: View {
    @Environment(AppSession.self) private var session
    let model: ChatMediaGalleryModel
    let initialID: String
    let close: () -> Void
    let loadMore: () async -> Void
    @State private var selectedID: String?

    private var index: Int { model.messages.firstIndex { $0.id == (selectedID ?? initialID) } ?? 0 }

    var body: some View {
        VStack(spacing: 0) {
            if !model.messages.isEmpty {
                let message = model.messages[index]
                media(message).frame(maxWidth: .infinity, maxHeight: .infinity).id(message.id)
                VStack(spacing: 10) {
                    HStack {
                        Button("Previous media", systemImage: "chevron.left") { move(-1) }
                            .labelStyle(.iconOnly).disabled(index == 0).keyboardShortcut(.leftArrow, modifiers: [])
                        Spacer()
                        VStack(spacing: 3) {
                            Text(message.date, format: .dateTime.day().month().year().hour().minute()).font(.caption)
                            Text("\(index + 1) of \(model.messages.count)\(model.hasMore ? "+" : "")")
                                .font(.caption.monospacedDigit()).foregroundStyle(.secondary)
                                .accessibilityIdentifier("gallery-position")
                        }
                        Spacer()
                        Button("Next media", systemImage: "chevron.right") { move(1) }
                            .labelStyle(.iconOnly).disabled(index == model.messages.count - 1 && !model.hasMore)
                            .keyboardShortcut(.rightArrow, modifiers: [])
                    }
                    if let caption = message.contentText { Text(caption).font(.caption).lineLimit(2) }
                }.buttonStyle(.bordered).padding(.horizontal, 20).padding(.vertical, 12)
                .task(id: message.id) {
                    await session.loadMedia(for: message)
                    if index >= model.messages.count - 5 && model.hasMore { await loadMore() }
                }
            } else {
                ContentUnavailableView("Media is no longer available", systemImage: "photo")
                Button("Close", action: close).padding()
            }
        }
        .background(.black).foregroundStyle(.white).preferredColorScheme(.dark)
        .photoViewerExitCommand(close)
    }

    @ViewBuilder private func media(_ message: ChatMessage) -> some View {
        if message.mediaKind == .image, let image = session.messageImages[message.id] {
            PhotoViewer(image: image, close: close, onSwipe: move)
        } else {
            ZStack(alignment: .topLeading) {
                Color.black
                if message.mediaKind == .video, let url = session.messageMediaURLs[message.id] {
                    GalleryVideoPlayer(url: url)
                        .accessibilityIdentifier("gallery-video-player")
                        .simultaneousGesture(DragGesture(minimumDistance: 30).onEnded { swipe($0.translation) })
                } else {
                    VStack(spacing: 12) {
                        if session.mediaLoadingIDs.contains(message.id) { ProgressView().tint(.white) }
                        else {
                            Label("Media unavailable", systemImage: "exclamationmark.triangle")
                            Button("Try again") { Task { await session.retryMedia(for: message) } }
                        }
                    }.frame(maxWidth: .infinity, maxHeight: .infinity)
                        .contentShape(Rectangle()).gesture(DragGesture().onEnded { swipe($0.translation) })
                }
                Button("Close media", systemImage: "xmark", action: close)
                    .labelStyle(.iconOnly).keyboardShortcut(.cancelAction).buttonStyle(.bordered)
                    .buttonBorderShape(.circle).padding(16)
            }
        }
    }

    private func swipe(_ translation: CGSize) {
        guard abs(translation.width) > 50, abs(translation.width) > abs(translation.height) else { return }
        move(translation.width < 0 ? 1 : -1)
    }

    private func move(_ offset: Int) {
        let target = index + offset
        if model.messages.indices.contains(target) { selectedID = model.messages[target].id }
        else if offset > 0, model.hasMore {
            Task {
                await loadMore()
                if model.messages.indices.contains(target) { selectedID = model.messages[target].id }
            }
        }
    }
}

private struct GalleryVideoPlayer: View {
    @State private var player: AVPlayer
    init(url: URL) { _player = State(initialValue: AVPlayer(url: url)) }
    var body: some View {
        VideoPlayer(player: player)
            .onAppear {
                #if os(iOS)
                try? AVAudioSession.sharedInstance().setCategory(.playback, mode: .moviePlayback)
                try? AVAudioSession.sharedInstance().setActive(true)
                #endif
                player.play()
            }
            .onDisappear { player.pause() }
    }
}

enum GalleryVideoPreview {
    @MainActor static func image(url: URL) async throws -> PlatformImage {
        let generator = AVAssetImageGenerator(asset: AVURLAsset(url: url))
        generator.appliesPreferredTrackTransform = true
        generator.maximumSize = CGSize(width: 384, height: 384)
        let result = try await generator.image(at: .zero)
        #if os(iOS)
        return PlatformImage(cgImage: result.image)
        #else
        return PlatformImage(cgImage: result.image, size: .zero)
        #endif
    }
}

#if os(macOS)
private final class MediaFullScreenWindow: NSWindow {
    override var canBecomeKey: Bool { true }
}

private struct GalleryFullScreenWindow<Content: View>: NSViewRepresentable {
    @ViewBuilder let content: () -> Content
    final class Coordinator { var window: NSWindow? }
    func makeCoordinator() -> Coordinator { Coordinator() }
    func makeNSView(context: Context) -> NSView {
        let window = MediaFullScreenWindow(contentRect: NSScreen.main?.frame ?? .zero, styleMask: [.borderless], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.backgroundColor = .black
        window.contentView = NSHostingView(rootView: content())
        window.makeKeyAndOrderFront(nil)
        context.coordinator.window = window
        return NSView()
    }
    func updateNSView(_ view: NSView, context: Context) {
        (context.coordinator.window?.contentView as? NSHostingView<Content>)?.rootView = content()
    }
    static func dismantleNSView(_ view: NSView, coordinator: Coordinator) { coordinator.window?.close() }
}
#endif
