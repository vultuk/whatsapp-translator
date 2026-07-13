import SwiftUI

struct ChatRow: View {
    let contact: Contact
    let draft: String
    let avatarURL: URL?

    var body: some View {
        HStack(spacing: 13) {
            ContactAvatar(contact: contact, url: avatarURL)
            VStack(alignment: .leading, spacing: 5) {
                HStack {
                    Text(contact.displayName)
                        .font(.headline)
                        .lineLimit(1)
                    Spacer(minLength: 8)
                    Text(contact.lastMessageTime.chatListDate)
                        .font(.caption)
                        .foregroundStyle(contact.unreadCount > 0 ? TranslatorTheme.green : .secondary)
                }
                HStack(spacing: 8) {
                    if !draft.isEmpty {
                        Text("Draft: \(draft)")
                            .foregroundStyle(.orange)
                            .italic()
                    } else {
                        Text(contact.lastMessagePreview ?? "No messages yet")
                            .foregroundStyle(.secondary)
                    }
                    Spacer(minLength: 6)
                    if contact.unreadCount > 0 {
                        Text("\(contact.unreadCount)")
                            .font(.caption2.bold())
                            .foregroundStyle(.white)
                            .padding(.horizontal, 7)
                            .padding(.vertical, 4)
                            .background(TranslatorTheme.green, in: Capsule())
                    }
                }
                .font(.subheadline)
                .lineLimit(1)
            }
        }
        .padding(.vertical, 7)
        .contentShape(Rectangle())
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
