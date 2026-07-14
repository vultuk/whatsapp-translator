import SwiftUI

#if canImport(UIKit)
import UIKit
#elseif canImport(AppKit)
import AppKit
#endif

enum MessageSwipeReply {
    static let triggerDistance: CGFloat = 56
    static let maximumReveal: CGFloat = 72

    static func offset(translation: CGSize) -> CGFloat {
        guard translation.width > 0,
              abs(translation.width) > abs(translation.height) else { return 0 }
        return min(translation.width, maximumReveal)
    }

    static func shouldReply(translation: CGSize) -> Bool {
        offset(translation: translation) >= triggerDistance
    }
}

struct MessageBubble: View {
    @Environment(\.translatorPalette) private var palette
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize
    let message: ChatMessage
    let isStarred: Bool
    let isBusy: Bool
    let image: PlatformImage?
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
    @State private var demoSwipeOffset: CGFloat = 0
    @GestureState private var swipeTranslation: CGSize = .zero

    private var swipeOffset: CGFloat {
        max(demoSwipeOffset, MessageSwipeReply.offset(translation: swipeTranslation))
    }

    var body: some View {
        HStack(alignment: .bottom, spacing: 8) {
            if message.isFromMe { Spacer(minLength: bubbleEdgeInset) }
            ZStack(alignment: .leading) {
                Image(systemName: "arrowshape.turn.up.left.fill")
                    .font(.system(size: 17, weight: .semibold))
                    .foregroundStyle(palette.deepAccent)
                    .frame(width: 34, height: 34)
                    .background(.ultraThinMaterial, in: Circle())
                    .offset(x: 6)
                    .scaleEffect(0.75 + min(swipeOffset / MessageSwipeReply.triggerDistance, 1) * 0.25)
                    .opacity(min(swipeOffset / 32, 1))
                    .accessibilityHidden(true)

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

                    messageMetadata

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
                .offset(x: swipeOffset)
            }
            #if os(macOS)
            .frame(
                maxWidth: MacChatLayoutMetrics.maximumBubbleWidth,
                alignment: message.isFromMe ? .trailing : .leading
            )
            #endif
            .contentShape(Rectangle())
            .simultaneousGesture(
                DragGesture(minimumDistance: 12)
                    .updating($swipeTranslation) { value, state, _ in
                        state = value.translation
                    }
                    .onEnded { value in
                        guard MessageSwipeReply.shouldReply(translation: value.translation) else { return }
                        performReplyFeedback()
                        reply()
                    }
            )
            .accessibilityAction(named: "Reply") { reply() }
            if !message.isFromMe { Spacer(minLength: bubbleEdgeInset) }
        }
        .frame(maxWidth: .infinity)
        .task {
            if ProcessInfo.processInfo.arguments.contains("-demoActions"), message.id == "4" {
                showActions = true
            }
            if ProcessInfo.processInfo.arguments.contains("-demoSwipeReplyReveal"), message.id == "4" {
                demoSwipeOffset = MessageSwipeReply.triggerDistance
            }
        }
    }

    private func performReplyFeedback() {
        #if canImport(UIKit)
        UIImpactFeedbackGenerator(style: .light).impactOccurred()
        #elseif canImport(AppKit)
        NSHapticFeedbackManager.defaultPerformer.perform(.alignment, performanceTime: .now)
        #endif
    }

    private var messageMetadata: some View {
        HStack(spacing: 6) {
            if message.isTranslated {
                Button {
                    withAnimation(.snappy) { showAlternate.toggle() }
                } label: {
                    if usesCompactMessageChrome {
                        Image(systemName: "character.bubble")
                    } else {
                        Label(showAlternate ? "Original" : "Translated", systemImage: "character.bubble")
                            .lineLimit(1)
                    }
                }
                .buttonStyle(.plain)
                .accessibilityLabel(showAlternate ? "Show translated message" : "Show original message")
            }
            if isBusy { ProgressView().controlSize(.mini) }
            #if os(macOS)
            Spacer().frame(width: 8)
            #else
            Spacer(minLength: 8)
            #endif
            if isStarred { Image(systemName: "star.fill").foregroundStyle(.yellow) }
            Text(message.date.formatted(date: .omitted, time: .shortened))
                .fixedSize(horizontal: true, vertical: false)
            if message.isFromMe {
                Image(systemName: "checkmark")
                    .fixedSize(horizontal: true, vertical: false)
            }
            Button {
                withAnimation(.snappy) { showActions.toggle() }
            } label: {
                Image(systemName: showActions ? "xmark.circle.fill" : "ellipsis.circle")
            }
            .buttonStyle(.plain)
            .help(showActions ? "Hide message actions" : "Show message actions")
            .accessibilityLabel(showActions ? "Hide message actions" : "Show message actions")
            .accessibilityHint("Reply, translate, star or react")
        }
        .font(.caption2)
        .foregroundStyle(.secondary)
        .platformCompactControlTypography()
    }

    @ViewBuilder
    private var visibleActionStrip: some View {
        #if os(macOS)
        HStack(spacing: 6) {
            compactActionButton("Reply", systemImage: "arrowshape.turn.up.left", action: reply)
            if message.canTranslate {
                compactActionButton("Translate", systemImage: "character.bubble", action: translate)
            }
            if message.canGenerateAIReply {
                compactActionButton("AI reply", systemImage: "sparkles", action: aiReply)
            }
            compactActionButton(
                isStarred ? "Unstar" : "Star",
                systemImage: isStarred ? "star.fill" : "star",
                action: toggleStar
            )
            Menu {
                ForEach(["👍", "❤️", "😂", "😮", "😢", "🙏"], id: \.self) { emoji in
                    Button(emoji) { react(emoji) }
                }
            } label: {
                compactActionLabel(systemImage: "face.smiling")
            }
            .menuStyle(.borderlessButton)
            .menuIndicator(.hidden)
            .help("React")
            .accessibilityLabel("React")
        }
        .foregroundStyle(palette.deepAccent)
        .padding(.top, 2)
        #else
        if usesCompactMessageChrome {
            compactMobileActionStrip
        } else {
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
        #endif
    }

    #if os(macOS)
    private func compactActionButton(
        _ title: String,
        systemImage: String,
        action: @escaping () -> Void
    ) -> some View {
        Button {
            action()
            withAnimation(.snappy) { showActions = false }
        } label: {
            compactActionLabel(systemImage: systemImage)
        }
        .buttonStyle(.plain)
        .help(title)
        .accessibilityLabel(title)
    }

    private func compactActionLabel(systemImage: String) -> some View {
        Image(systemName: systemImage)
            .font(.system(size: 13, weight: .semibold))
            .frame(width: 28, height: 26)
            .background(.primary.opacity(0.065), in: RoundedRectangle(cornerRadius: 7))
            .contentShape(RoundedRectangle(cornerRadius: 7))
    }
    #endif

    #if os(iOS)
    private var compactMobileActionStrip: some View {
        HStack(spacing: 8) {
            compactMobileActionButton("Reply", systemImage: "arrowshape.turn.up.left", action: reply)
            if message.canTranslate {
                compactMobileActionButton("Translate", systemImage: "character.bubble", action: translate)
            }
            if message.canGenerateAIReply {
                compactMobileActionButton("AI reply", systemImage: "sparkles", action: aiReply)
            }
            compactMobileActionButton(
                isStarred ? "Unstar" : "Star",
                systemImage: isStarred ? "star.fill" : "star",
                action: toggleStar
            )
            Menu {
                ForEach(["👍", "❤️", "😂", "😮", "😢", "🙏"], id: \.self) { emoji in
                    Button(emoji) { react(emoji) }
                }
            } label: {
                compactMobileActionLabel(systemImage: "face.smiling")
            }
            .accessibilityLabel("React")
        }
        .foregroundStyle(palette.deepAccent)
        .padding(.top, 2)
    }

    private func compactMobileActionButton(
        _ title: String,
        systemImage: String,
        action: @escaping () -> Void
    ) -> some View {
        Button {
            action()
            withAnimation(.snappy) { showActions = false }
        } label: {
            compactMobileActionLabel(systemImage: systemImage)
        }
        .buttonStyle(.plain)
        .accessibilityLabel(title)
    }

    private func compactMobileActionLabel(systemImage: String) -> some View {
        Image(systemName: systemImage)
            .font(.system(size: 17, weight: .semibold))
            .frame(width: 40, height: 38)
            .background(.primary.opacity(0.065), in: RoundedRectangle(cornerRadius: 10))
            .contentShape(RoundedRectangle(cornerRadius: 10))
    }
    #endif

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

    private var usesCompactMessageChrome: Bool {
        NativeLayoutPolicy.usesCompactMessageChrome(for: dynamicTypeSize)
    }

    private var bubbleEdgeInset: CGFloat {
        usesCompactMessageChrome ? 18 : 52
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
                            .truncationMode(.tail)
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
