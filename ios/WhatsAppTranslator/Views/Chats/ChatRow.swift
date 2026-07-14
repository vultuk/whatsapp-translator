import SwiftUI

struct ChatRow: View {
    @Environment(\.translatorPalette) private var palette
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize
    let contact: Contact
    let displayName: String
    let draft: String
    let avatarURL: URL?

    var body: some View {
        Group {
            if NativeLayoutPolicy.usesStackedChatRow(for: dynamicTypeSize) {
                accessibilityLayout
            } else {
                standardLayout
            }
        }
        .padding(.vertical, 7)
        #if os(macOS)
        .frame(minHeight: 58)
        #endif
        .contentShape(Rectangle())
    }

    private var standardLayout: some View {
        HStack(spacing: 13) {
            ContactAvatar(contact: contact, url: avatarURL, size: avatarSize)
            VStack(alignment: .leading, spacing: 5) {
                HStack {
                    Text(displayName)
                        .font(.headline)
                        .foregroundStyle(Color.platformPrimaryLabel)
                        .lineLimit(1)
                        .truncationMode(.tail)
                        .layoutPriority(1)
                    Spacer(minLength: 8)
                    Text(contact.lastMessageTime.chatListDate)
                        .font(.caption)
                        .foregroundStyle(contact.unreadCount > 0 ? palette.accent : Color.platformSecondaryLabel)
                        .fixedSize(horizontal: true, vertical: false)
                }
                HStack(spacing: 8) {
                    previewText(lineLimit: 1)
                    Spacer(minLength: 6)
                    statusIndicators
                }
                .font(.subheadline)
            }
        }
    }

    private var accessibilityLayout: some View {
        HStack(alignment: .top, spacing: 12) {
            ContactAvatar(contact: contact, url: avatarURL, size: 46)
            VStack(alignment: .leading, spacing: 7) {
                Text(displayName)
                    .font(.headline)
                    .foregroundStyle(Color.platformPrimaryLabel)
                    .lineLimit(2)
                    .fixedSize(horizontal: false, vertical: true)
                previewText(lineLimit: 2)
                    .font(.subheadline)
                HStack(spacing: 9) {
                    Text(contact.lastMessageTime.chatListDate)
                        .foregroundStyle(contact.unreadCount > 0 ? palette.accent : Color.platformSecondaryLabel)
                    statusIndicators
                }
                .font(.caption)
                .platformCompactControlTypography()
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    @ViewBuilder
    private func previewText(lineLimit: Int) -> some View {
        if !draft.isEmpty {
            Text("Draft: \(draft)")
                .foregroundStyle(.orange)
                .italic()
                .lineLimit(lineLimit)
                .truncationMode(.tail)
                .layoutPriority(1)
        } else {
            Text(contact.lastMessagePreview ?? "No messages yet")
                .foregroundStyle(Color.platformSecondaryLabel)
                .lineLimit(lineLimit)
                .truncationMode(.tail)
                .layoutPriority(1)
        }
    }

    @ViewBuilder
    private var statusIndicators: some View {
        if contact.pinnedAt != nil {
            Image(systemName: "pin.fill")
                .font(.caption2)
                .foregroundStyle(Color.platformSecondaryLabel)
        }
        if contact.unreadCount > 0 {
            Text("\(contact.unreadCount)")
                .font(.caption2.bold())
                .foregroundStyle(.white)
                .padding(.horizontal, 7)
                .padding(.vertical, 4)
                .background(palette.accent, in: Capsule())
                .fixedSize(horizontal: true, vertical: false)
        }
    }

    private var avatarSize: CGFloat {
        #if os(macOS)
        44
        #else
        52
        #endif
    }

}

private extension Int64 {
    var chatListDate: String {
        let date = Date(timeIntervalSince1970: TimeInterval(self) / 1_000)
        if Calendar.current.isDateInToday(date) {
            return date.formatted(date: .omitted, time: .shortened)
        }
        if Calendar.current.isDateInYesterday(date) { return "Yesterday" }
        return date.formatted(.dateTime.day().month(.abbreviated))
    }
}
