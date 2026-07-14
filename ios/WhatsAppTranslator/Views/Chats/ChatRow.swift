import SwiftUI

struct ChatRow: View {
    @Environment(\.translatorPalette) private var palette
    let contact: Contact
    let displayName: String
    let draft: String
    let avatarURL: URL?

    var body: some View {
        HStack(spacing: 13) {
            ContactAvatar(contact: contact, url: avatarURL, size: avatarSize)
            VStack(alignment: .leading, spacing: 5) {
                HStack {
                    Text(displayName)
                        .font(.headline)
                        .lineLimit(1)
                        .truncationMode(.tail)
                        .layoutPriority(1)
                    Spacer(minLength: 8)
                    Text(contact.lastMessageTime.chatListDate)
                        .font(.caption)
                        .foregroundStyle(contact.unreadCount > 0 ? palette.accent : .secondary)
                        .fixedSize(horizontal: true, vertical: false)
                }
                HStack(spacing: 8) {
                    if !draft.isEmpty {
                        Text("Draft: \(draft)")
                            .foregroundStyle(.orange)
                            .italic()
                            .lineLimit(1)
                            .truncationMode(.tail)
                            .layoutPriority(1)
                    } else {
                        Text(contact.lastMessagePreview ?? "No messages yet")
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
                            .truncationMode(.tail)
                            .layoutPriority(1)
                    }
                    Spacer(minLength: 6)
                    if contact.pinnedAt != nil {
                        Image(systemName: "pin.fill")
                            .font(.caption2)
                            .foregroundStyle(.secondary)
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
                .font(.subheadline)
            }
        }
        .padding(.vertical, 7)
        #if os(macOS)
        .frame(minHeight: 58)
        #endif
        .contentShape(Rectangle())
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
