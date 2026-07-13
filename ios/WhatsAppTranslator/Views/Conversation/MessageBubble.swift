import SwiftUI

struct MessageBubble: View {
    @Environment(\.translatorPalette) private var palette
    let message: ChatMessage
    let isStarred: Bool
    let isBusy: Bool
    let image: UIImage?
    let linkPreview: LinkPreview?
    let reply: () -> Void
    let translate: () -> Void
    let aiReply: () -> Void
    let toggleStar: () -> Void
    let react: (String) -> Void
    @State private var showAlternate = false

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

                messageContent

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
                        Image(systemName: "character.bubble")
                        Text(showAlternate ? "Original" : "Translated")
                    }
                    if isBusy { ProgressView().controlSize(.mini) }
                    Spacer(minLength: 4)
                    if isStarred { Image(systemName: "star.fill").foregroundStyle(.yellow) }
                    Text(message.date.formatted(date: .omitted, time: .shortened))
                    if message.isFromMe { Image(systemName: "checkmark.done") }
                }
                .font(.caption2)
                .foregroundStyle(.secondary)
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
            .onTapGesture {
                guard message.alternateText != nil else { return }
                withAnimation(.snappy) { showAlternate.toggle() }
            }
            .contextMenu { actionMenu }
            if !message.isFromMe { Spacer(minLength: 52) }
        }
        .frame(maxWidth: .infinity)
    }

    @ViewBuilder
    private var messageContent: some View {
        if message.isImage {
            if let image {
                Image(uiImage: image)
                    .resizable()
                    .scaledToFit()
                    .frame(maxWidth: 280, maxHeight: 280)
                    .clipShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
            } else {
                ZStack {
                    RoundedRectangle(cornerRadius: 12, style: .continuous)
                        .fill(.primary.opacity(0.06))
                        .frame(width: 220, height: 150)
                    ProgressView()
                }
            }
            if let caption = message.content?.caption, !caption.isEmpty {
                Text(caption).font(.body)
            }
        } else {
            Text(showAlternate ? (message.alternateText ?? message.displayText) : message.displayText)
                .font(.body)
                .textSelection(.enabled)
        }

        if let preview = linkPreview, preview.error == nil {
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
    }
}
