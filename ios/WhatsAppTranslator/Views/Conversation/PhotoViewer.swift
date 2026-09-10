import SwiftUI

enum PhotoViewerZoom {
    static let minimumScale: CGFloat = 1
    static let maximumScale: CGFloat = 5
    static let doubleTapScale: CGFloat = 2.5

    static func clampedScale(_ scale: CGFloat) -> CGFloat {
        min(maximumScale, max(minimumScale, scale))
    }

    static func toggledScale(from scale: CGFloat) -> CGFloat {
        scale > minimumScale ? minimumScale : doubleTapScale
    }
}

struct PhotoViewer: View {
    let image: PlatformImage
    let close: () -> Void
    var onSwipe: (Int) -> Void = { _ in }

    @State private var scale = PhotoViewerZoom.minimumScale
    @State private var settledScale = PhotoViewerZoom.minimumScale
    @State private var offset = CGSize.zero
    @State private var settledOffset = CGSize.zero

    var body: some View {
        GeometryReader { proxy in
            ZStack {
                Color.black.ignoresSafeArea()

                Image(platformImage: image)
                    .resizable()
                    .scaledToFit()
                    .frame(
                        maxWidth: max(0, proxy.size.width),
                        maxHeight: max(0, proxy.size.height)
                    )
                    .scaleEffect(scale)
                    .offset(offset)
                    .contentShape(Rectangle())
                    .gesture(zoomAndPanGesture)
                    .onTapGesture(count: 2, perform: toggleZoom)
                    .accessibilityLabel("Full-screen photo")
                    .accessibilityHint("Pinch to zoom and drag to move around the photo")

                viewerControls
            }
        }
        .preferredColorScheme(.dark)
        .photoViewerExitCommand(close)
    }

    private var zoomAndPanGesture: some Gesture {
        SimultaneousGesture(
            MagnifyGesture()
                .onChanged { value in
                    scale = PhotoViewerZoom.clampedScale(settledScale * value.magnification)
                }
                .onEnded { _ in
                    settleZoom()
                },
            DragGesture(minimumDistance: 4)
                .onChanged { value in
                    guard scale > PhotoViewerZoom.minimumScale else { return }
                    offset = CGSize(
                        width: settledOffset.width + value.translation.width,
                        height: settledOffset.height + value.translation.height
                    )
                }
                .onEnded { value in
                    if scale == PhotoViewerZoom.minimumScale,
                       abs(value.translation.width) > 50,
                       abs(value.translation.width) > abs(value.translation.height) {
                        onSwipe(value.translation.width < 0 ? 1 : -1)
                    }
                    settledOffset = offset
                }
        )
    }

    private var viewerControls: some View {
        VStack {
            HStack(spacing: 10) {
                Button("Close", systemImage: "xmark") { close() }
                    .labelStyle(.iconOnly)
                    .keyboardShortcut(.cancelAction)
                    .accessibilityLabel("Close photo")

                Spacer()

                Button("Zoom out", systemImage: "minus.magnifyingglass") {
                    setScale(scale - 0.5)
                }
                .labelStyle(.iconOnly)
                .disabled(scale <= PhotoViewerZoom.minimumScale)

                Text("\(Int((scale * 100).rounded()))%")
                    .font(.caption.monospacedDigit().weight(.semibold))
                    .frame(minWidth: 48)

                Button("Actual size", systemImage: "1.magnifyingglass") { resetZoom() }
                    .labelStyle(.iconOnly)
                    .disabled(scale == PhotoViewerZoom.minimumScale && offset == .zero)

                Button("Zoom in", systemImage: "plus.magnifyingglass") {
                    setScale(scale + 0.5)
                }
                .labelStyle(.iconOnly)
                .disabled(scale >= PhotoViewerZoom.maximumScale)
            }
            .buttonStyle(.bordered)
            .buttonBorderShape(.circle)
            .foregroundStyle(.white)
            .padding(16)
            .background(.black.opacity(0.52))

            Spacer()

            Text(viewerInstructions)
                .font(.caption)
                .foregroundStyle(.white.opacity(0.82))
                .padding(.horizontal, 14)
                .padding(.vertical, 8)
                .background(.black.opacity(0.58), in: Capsule())
                .padding(.bottom, 18)
                .allowsHitTesting(false)
        }
    }

    private var viewerInstructions: String {
        #if os(macOS)
        "Pinch to zoom • drag to move • double-click to toggle zoom"
        #else
        "Pinch to zoom • drag to move • double-tap to toggle zoom"
        #endif
    }

    private func toggleZoom() {
        setScale(PhotoViewerZoom.toggledScale(from: scale))
    }

    private func setScale(_ newScale: CGFloat) {
        withAnimation(.snappy) {
            scale = PhotoViewerZoom.clampedScale(newScale)
            settleZoom()
        }
    }

    private func settleZoom() {
        scale = PhotoViewerZoom.clampedScale(scale)
        settledScale = scale
        if scale == PhotoViewerZoom.minimumScale {
            offset = .zero
            settledOffset = .zero
        }
    }

    private func resetZoom() {
        withAnimation(.snappy) {
            scale = PhotoViewerZoom.minimumScale
            settledScale = PhotoViewerZoom.minimumScale
            offset = .zero
            settledOffset = .zero
        }
    }
}

private struct PhotoViewerPresentationModifier: ViewModifier {
    @Binding var isPresented: Bool
    let image: PlatformImage?

    func body(content: Content) -> some View {
        #if os(macOS)
        content.sheet(isPresented: $isPresented) {
            if let image {
                PhotoViewer(image: image, close: { isPresented = false })
                    .frame(minWidth: 900, minHeight: 650)
            }
        }
        #else
        content.fullScreenCover(isPresented: $isPresented) {
            if let image {
                PhotoViewer(image: image, close: { isPresented = false })
            }
        }
        #endif
    }
}

extension View {
    func photoViewer(isPresented: Binding<Bool>, image: PlatformImage?) -> some View {
        modifier(PhotoViewerPresentationModifier(isPresented: isPresented, image: image))
    }

    @ViewBuilder
    func photoViewerExitCommand(_ action: @escaping () -> Void) -> some View {
        #if os(macOS)
        onExitCommand(perform: action)
        #else
        self
        #endif
    }
}

// Use the same zoom surface for every page while keeping actions tied to its message.
struct PhotoGalleryViewer: View {
    @Environment(AppSession.self) private var session
    @Environment(\.dismiss) private var dismiss
    let messages: [ChatMessage]
    let initialPhotoID: String
    let reply: (ChatMessage) -> Void
    let aiReply: (ChatMessage) -> Void
    @State private var selectedIndex: Int?
    @State private var showOriginal = false

    private var index: Int {
        PhotoGalleryLayout.page(after: 0, current: selectedIndex ?? messages.firstIndex(where: { $0.id == initialPhotoID }) ?? 0, count: messages.count)
    }
    private var message: ChatMessage {
        let original = messages[index]
        return session.messages[original.contactId]?.first(where: { $0.id == original.id })
            ?? session.unifiedMessages.first(where: { $0.id == original.id && $0.contactId == original.contactId })
            ?? original
    }

    var body: some View {
        VStack(spacing: 0) {
            if let image = session.messageImages[message.id] {
                PhotoViewer(image: image, close: { dismiss() }, onSwipe: move)
                    .id(message.id)
            } else {
                ZStack(alignment: .topLeading) {
                    Color.black
                    Button("Close", systemImage: "xmark") { dismiss() }.padding()
                    VStack(spacing: 12) {
                        if session.mediaLoadingIDs.contains(message.id) {
                            ProgressView().tint(.white)
                            Text("Loading photo…")
                        } else {
                            Image(systemName: "photo").font(.largeTitle)
                            Button("Retry photo") {
                                let selected = message
                                Task { await session.retryMedia(for: selected) }
                            }
                        }
                    }.frame(maxWidth: .infinity, maxHeight: .infinity)
                }
            }
            details
        }
        .background(.black).foregroundStyle(.white).preferredColorScheme(.dark)
        .task(id: message.id) { [message] in await session.loadMedia(for: message) }
        .photoViewerExitCommand { dismiss() }
    }

    private var details: some View {
        VStack(spacing: 10) {
            ScrollView {
                VStack(alignment: .leading, spacing: 6) {
                    if let context = message.content?.replyContext {
                        Text("\(context.senderName ?? "Reply"): \(context.text ?? "")")
                            .font(.caption).foregroundStyle(.secondary)
                    }
                    if message.contentText != nil {
                        Text(MessageTextLinkifier.attributedString(from: showOriginal ? (message.alternateText ?? message.displayText) : message.displayText))
                            .textSelection(.enabled).frame(maxWidth: .infinity, alignment: .leading)
                    }
                    if let reactions = message.reactions, !reactions.isEmpty {
                        Text(reactions.keys.sorted().map { "\($0) \(reactions[$0]?.count ?? 0)" }.joined(separator: "  "))
                            .font(.caption)
                    }
                }
            }.frame(maxHeight: message.contentText == nil && message.content?.replyContext == nil && (message.reactions?.isEmpty ?? true) ? 0 : 84)
            HStack {
                Button("Previous photo", systemImage: "chevron.left") { move(-1) }
                    .labelStyle(.iconOnly).disabled(index == 0)
                Spacer()
                Text("\(index + 1) of \(messages.count)").monospacedDigit()
                    .accessibilityLabel("Photo \(index + 1) of \(messages.count)")
                Spacer()
                Button("Next photo", systemImage: "chevron.right") { move(1) }
                    .labelStyle(.iconOnly).disabled(index == messages.count - 1)
            }.buttonStyle(.bordered)
            HStack {
                Button("Reply", systemImage: "arrowshape.turn.up.left") {
                    let selected = message
                    dismiss()
                    reply(selected)
                }
                Spacer()
                Text(message.date, format: .dateTime.hour().minute()).font(.caption).foregroundStyle(.secondary)
                Menu("Photo actions", systemImage: "ellipsis.circle") {
                    if message.canTranslate {
                        Button("Translate", systemImage: "translate") {
                            let selected = message
                            Task { await session.translate(selected) }
                        }
                    }
                    if message.alternateText != nil {
                        Button(showOriginal ? "Show translated" : "Show original") { showOriginal.toggle() }
                    }
                    if message.canGenerateAIReply {
                        Button("AI reply", systemImage: "sparkles") {
                            let selected = message
                            dismiss()
                            aiReply(selected)
                        }
                    }
                    Button(session.preferences.isStarred(messageID: message.id, contactID: message.contactId) ? "Unstar" : "Star", systemImage: "star") {
                        session.preferences.toggleStar(messageID: message.id, contactID: message.contactId)
                    }
                    Menu("React") {
                        ForEach(["👍", "❤️", "😂", "😮", "😢", "🙏"], id: \.self) { emoji in
                            Button(emoji) {
                                let selected = message
                                Task { await session.react(to: selected, emoji: emoji) }
                            }
                        }
                        Button("Remove reaction") {
                            let selected = message
                            Task { await session.react(to: selected, emoji: "") }
                        }
                    }
                }.labelStyle(.iconOnly).disabled(session.activeMessageActionIDs.contains(message.id))
            }
        }
        .padding(.horizontal, 18).padding(.vertical, 12)
        .frame(maxWidth: 800).background(.black)
    }

    private func move(_ offset: Int) {
        selectedIndex = PhotoGalleryLayout.page(after: offset, current: index, count: messages.count)
        showOriginal = false
    }
}
