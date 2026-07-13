import SwiftUI

struct MessageBubble: View {
    let message: ChatMessage
    @State private var showAlternate = false

    var body: some View {
        HStack(alignment: .bottom, spacing: 8) {
            if message.isFromMe { Spacer(minLength: 52) }
            VStack(alignment: .leading, spacing: 5) {
                if !message.isFromMe, message.chatType == "group", let sender = message.senderName {
                    Text(sender)
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(TranslatorTheme.deepGreen)
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
                Text(showAlternate ? (message.alternateText ?? message.displayText) : message.displayText)
                    .font(.body)
                    .textSelection(.enabled)
                HStack(spacing: 4) {
                    if message.isTranslated {
                        Image(systemName: "character.bubble")
                        Text(showAlternate ? "Original" : "Translated")
                    }
                    Spacer(minLength: 4)
                    Text(message.date.formatted(date: .omitted, time: .shortened))
                    if message.isFromMe { Image(systemName: "checkmark.done") }
                }
                .font(.caption2)
                .foregroundStyle(.secondary)
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 9)
            .background(
                message.isFromMe ? TranslatorTheme.outgoingBubble : TranslatorTheme.incomingBubble,
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
            if !message.isFromMe { Spacer(minLength: 52) }
        }
        .frame(maxWidth: .infinity)
    }
}
