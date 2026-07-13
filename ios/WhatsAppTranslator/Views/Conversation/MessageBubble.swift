import SwiftUI

struct MessageBubble: View {
    @Environment(\.translatorPalette) private var palette
    let message: ChatMessage
    let isStarred: Bool
    let isBusy: Bool
    let image: UIImage?
    let mediaURL: URL?
    let mediaIsLoading: Bool
    let mediaFailed: Bool
    let linkPreviews: [LinkPreview]
    let reply: () -> Void
    let translate: () -> Void
    let aiReply: () -> Void
    let toggleStar: () -> Void
    let react: (String) -> Void
    let retryMedia: () -> Void
    @State private var showAlternate = false
    @State private var showActions = false

    var body: some View {
        HStack(alignment: .bottom, spacing: 8) {
            if message.isFromMe { Spacer(minLength: 52) }
            VStack(alignment: .leading, spacing: 6) {
                if !message.isFromMe, message.chatType == "group", let sender = message.senderName {
                    Text(sender)
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(palette.deepAccent)
                }
                if let reply = message.content?.replyContext {
                    VStack(alignment: .leading, spacing: 2) {
                        Text(reply.senderName ?? "Reply")
                            .font(.caption.weight(.semibold))
                        Text(reply.text ?? "")
                            .font(.caption)
                            .lineLimit(2)
                    }
                    .foregroundStyle(.secondary)
                    .padding(8)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(.primary.opacity(0.06), in: RoundedRectangle(cornerRadius: 9))
                }

                RichMessageContentView(
                    message: message,
                    displayText: showAlternate ? (message.alternateText ?? message.displayText) : message.displayText,
                    image: image,
                    mediaURL: mediaURL,
                    isLoading: mediaIsLoading,
                    failed: mediaFailed,
                    retry: retryMedia
                )

                ForEach(linkPreviews, id: \.url) { preview in
                    LinkPreviewCard(preview: preview)
                }

                if let reactions = message.reactions, !reactions.isEmpty {
                    HStack(spacing: 4) {
                        ForEach(reactions.keys.sorted(), id: \.self) { emoji in
                            Text("\(emoji) \(reactions[emoji]?.count ?? 0)")
                                .font(.caption2.weight(.medium))
                                .padding(.horizontal, 7)
                                .padding(.vertical, 3)
                                .background(.ultraThinMaterial, in: Capsule())
                        }
                    }
                }

                HStack(spacing: 4) {
                    if message.isTranslated {
                        Button(showAlternate ? "Original" : "Translated", systemImage: "character.bubble") {
                            withAnimation(.snappy) { showAlternate.toggle() }
                        }
                        .buttonStyle(.plain)
                    }
                    if isBusy { ProgressView().controlSize(.mini) }
                    Spacer(minLength: 4)
                    if isStarred { Image(systemName: "star.fill").foregroundStyle(.yellow) }
                    Text(message.date.formatted(date: .omitted, time: .shortened))
                    if message.isFromMe { Image(systemName: "checkmark.done") }
                    Button(showActions ? "Hide" : "Actions", systemImage: "ellipsis.circle") {
                        withAnimation(.snappy) { showActions.toggle() }
                    }
                    .buttonStyle(.plain)
                    .accessibilityHint("Reply, translate, star or react")
                }
                .font(.caption2)
                .foregroundStyle(.secondary)

                if showActions {
                    visibleActionStrip
                        .transition(.move(edge: .bottom).combined(with: .opacity))
                }
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 9)
            .background(
                message.isFromMe ? palette.outgoingBubble : palette.incomingBubble,
                in: UnevenRoundedRectangle(
                    topLeadingRadius: message.isFromMe ? 17 : 4,
                    bottomLeadingRadius: 17,
                    bottomTrailingRadius: 17,
                    topTrailingRadius: message.isFromMe ? 4 : 17,
                    style: .continuous
                )
            )
            .shadow(color: .black.opacity(0.06), radius: 1, y: 1)
            .contextMenu { actionMenu }
            if !message.isFromMe { Spacer(minLength: 52) }
        }
        .frame(maxWidth: .infinity)
        .task {
            if ProcessInfo.processInfo.arguments.contains("-demoActions"), message.id == "4" {
                showActions = true
            }
        }
    }

    private var visibleActionStrip: some View {
        HStack(spacing: 10) {
            actionButton("Reply", systemImage: "arrowshape.turn.up.left", action: reply)
            if message.canTranslate {
                actionButton("Translate", systemImage: "character.bubble", action: translate)
            }
            if message.canGenerateAIReply {
                actionButton("AI reply", systemImage: "sparkles", action: aiReply)
            }
            actionButton(isStarred ? "Unstar" : "Star", systemImage: isStarred ? "star.fill" : "star", action: toggleStar)
            Menu {
                ForEach(["👍", "❤️", "😂", "😮", "😢", "🙏"], id: \.self) { emoji in
                    Button(emoji) { react(emoji) }
                }
            } label: {
                actionLabel("React", systemImage: "face.smiling")
            }
        }
        .font(.caption2.weight(.medium))
        .foregroundStyle(palette.deepAccent)
        .padding(.top, 2)
    }

    private func actionButton(_ title: String, systemImage: String, action: @escaping () -> Void) -> some View {
        Button {
            action()
            withAnimation(.snappy) { showActions = false }
        } label: {
            actionLabel(title, systemImage: systemImage)
        }
        .buttonStyle(.plain)
        .accessibilityLabel(title)
    }

    private func actionLabel(_ title: String, systemImage: String) -> some View {
        VStack(spacing: 2) {
            Image(systemName: systemImage).font(.body)
            Text(title).lineLimit(1)
        }
        .frame(minWidth: 42)
    }

    @ViewBuilder
    private var actionMenu: some View {
        Button("Reply", systemImage: "arrowshape.turn.up.left", action: reply)
        if message.canTranslate {
            Button("Translate", systemImage: "character.bubble", action: translate)
        }
        if message.canGenerateAIReply {
            Button("AI reply", systemImage: "sparkles", action: aiReply)
        }
        Button(isStarred ? "Remove star" : "Star", systemImage: isStarred ? "star.slash" : "star", action: toggleStar)
        Menu("React", systemImage: "face.smiling") {
            ForEach(["👍", "❤️", "😂", "😮", "😢", "🙏"], id: \.self) { emoji in
                Button(emoji) { react(emoji) }
            }
        }
        if message.alternateText != nil {
            Divider()
            Button(showAlternate ? "Show translated" : "Show original", systemImage: "arrow.left.arrow.right") {
                showAlternate.toggle()
            }
        }
    }
}

private struct LinkPreviewCard: View {
    @Environment(\.translatorPalette) private var palette
    let preview: LinkPreview

    var body: some View {
        if preview.error == nil {
            Link(destination: preview.url) {
                HStack(spacing: 10) {
                    if let imageURL = preview.imageURL {
                        AsyncImage(url: imageURL) { image in
                            image.resizable().scaledToFill()
                        } placeholder: {
                            Color.primary.opacity(0.05)
                        }
                        .frame(width: 64, height: 64)
                        .clipShape(RoundedRectangle(cornerRadius: 9))
                    }
                    VStack(alignment: .leading, spacing: 3) {
                        Text(preview.siteName ?? preview.url.host() ?? "Link")
                            .font(.caption2.weight(.semibold))
                            .foregroundStyle(palette.deepAccent)
                        Text(preview.title ?? preview.url.absoluteString)
                            .font(.caption.weight(.semibold))
                            .foregroundStyle(.primary)
                            .lineLimit(2)
                        if let description = preview.description {
                            Text(description)
                                .font(.caption2)
                                .foregroundStyle(.secondary)
                                .lineLimit(2)
                        }
                    }
                }
                .padding(8)
                .frame(maxWidth: 300, alignment: .leading)
                .background(.primary.opacity(0.06), in: RoundedRectangle(cornerRadius: 11))
            }
            .buttonStyle(.plain)
        }
    }
}
