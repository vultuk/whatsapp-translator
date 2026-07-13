import SwiftUI

struct ConversationView: View {
    @Environment(AppSession.self) private var session
    let contact: Contact
    @State private var draft = ""
    @State private var showSettings = false

    private var messages: [ChatMessage] { session.messages[contact.id] ?? [] }

    var body: some View {
        ZStack {
            ChatWallpaper()
            VStack(spacing: 0) {
                messageTimeline
                ComposerView(
                    text: $draft,
                    isSending: session.sendingContactIDs.contains(contact.id),
                    send: send
                )
            }
        }
        .navigationTitle(contact.displayName)
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .principal) {
                HStack(spacing: 9) {
                    ContactAvatar(contact: contact, url: session.avatarURLs[contact.id], size: 34)
                    Text(contact.displayName)
                        .font(.headline)
                        .lineLimit(1)
                }
            }
            ToolbarItem(placement: .topBarTrailing) {
                Button("Chat settings", systemImage: "ellipsis") { showSettings = true }
            }
        }
        .task {
            draft = session.draftStore.text(for: contact.id)
            await session.loadAvatar(for: contact.id)
            await session.loadMessages(for: contact.id)
        }
        .onChange(of: draft) { _, newValue in
            session.draftStore.save(newValue, for: contact.id)
        }
        .sheet(isPresented: $showSettings) {
            ConversationSettingsView(contact: contact)
        }
    }

    private var messageTimeline: some View {
        ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(spacing: 6) {
                    if session.messageHistoryHasMore[contact.id] == true {
                        Button("Load earlier messages") {
                            Task { await session.loadMessages(for: contact.id, older: true) }
                        }
                        .font(.caption.weight(.semibold))
                        .buttonStyle(.bordered)
                        .padding(.vertical, 10)
                    }

                    ForEach(Array(messages.enumerated()), id: \.element.id) { index, message in
                        if shouldShowDate(before: index) {
                            DatePill(date: message.date)
                                .padding(.vertical, 10)
                        }
                        MessageBubble(message: message)
                            .id(message.id)
                    }
                }
                .padding(.horizontal, 12)
                .padding(.vertical, 14)
            }
            .scrollDismissesKeyboard(.interactively)
            .defaultScrollAnchor(.bottom)
            .onChange(of: messages.count) {
                guard let id = messages.last?.id else { return }
                withAnimation(.snappy) { proxy.scrollTo(id, anchor: .bottom) }
            }
        }
    }

    private func shouldShowDate(before index: Int) -> Bool {
        guard index > 0 else { return true }
        return !Calendar.current.isDate(messages[index - 1].date, inSameDayAs: messages[index].date)
    }

    private func send() {
        let value = draft
        Task {
            if await session.send(text: value, to: contact.id) {
                draft = ""
            }
        }
    }
}

private struct ChatWallpaper: View {
    var body: some View {
        TranslatorTheme.chatBackground
            .overlay {
                Image(systemName: "message.fill")
                    .font(.system(size: 28))
                    .foregroundStyle(.primary.opacity(0.018))
                    .symbolEffect(.pulse, options: .repeating.speed(0.05))
            }
            .ignoresSafeArea()
    }
}

private struct DatePill: View {
    let date: Date
    var body: some View {
        Text(date.formatted(.dateTime.weekday().day().month()))
            .font(.caption.weight(.semibold))
            .foregroundStyle(.secondary)
            .padding(.horizontal, 12)
            .padding(.vertical, 6)
            .translatorGlass(in: Capsule())
    }
}
