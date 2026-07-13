import SwiftUI

struct ConversationView: View {
    @Environment(AppSession.self) private var session
    @Environment(\.translatorPalette) private var palette
    let contact: Contact
    @State private var draft = ""
    @State private var replyTarget: MessageReplyTarget?
    @State private var showSettings = ProcessInfo.processInfo.arguments.contains("-demoConversationSettings")
    @State private var showCost = ProcessInfo.processInfo.arguments.contains("-demoCost")
    @State private var showSearch = ProcessInfo.processInfo.arguments.contains("-demoSearch")
    @State private var messageSearch = ""
    @State private var starredOnly = false

    private var messages: [ChatMessage] { session.messages[contact.id] ?? [] }
    private var visibleMessages: [ChatMessage] {
        let query = messageSearch.trimmingCharacters(in: .whitespacesAndNewlines)
        return messages.filter { message in
            let starMatches = !starredOnly || session.preferences.isStarred(messageID: message.id, contactID: contact.id)
            let searchMatches = query.isEmpty
                || message.displayText.localizedCaseInsensitiveContains(query)
                || (message.alternateText?.localizedCaseInsensitiveContains(query) ?? false)
                || (message.senderName?.localizedCaseInsensitiveContains(query) ?? false)
            return starMatches && searchMatches
        }
    }

    var body: some View {
        ZStack {
            ChatWallpaper()
            VStack(spacing: 0) {
                if showSearch {
                    HStack(spacing: 10) {
                        Image(systemName: "magnifyingglass").foregroundStyle(.secondary)
                        TextField("Search messages", text: $messageSearch)
                            .textInputAutocapitalization(.never)
                        Button("Close search", systemImage: "xmark.circle.fill") {
                            messageSearch = ""
                            showSearch = false
                        }
                        .labelStyle(.iconOnly)
                        .foregroundStyle(.secondary)
                    }
                    .padding(.horizontal, 14)
                    .padding(.vertical, 10)
                    .translatorGlass(in: RoundedRectangle(cornerRadius: 18, style: .continuous))
                    .padding(.horizontal, 12)
                    .padding(.top, 8)
                }
                if starredOnly {
                    FilterPill(title: "Starred messages", systemImage: "star.fill") {
                        starredOnly = false
                    }
                    .padding(.top, 8)
                }
                messageTimeline
                ComposerView(
                    text: $draft,
                    reply: replyTarget,
                    isSending: session.sendingContactIDs.contains(contact.id),
                    cancelReply: { replyTarget = nil },
                    sendImage: sendImage,
                    send: send
                )
            }
        }
        .navigationTitle(session.displayName(for: contact))
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .principal) {
                HStack(spacing: 9) {
                    ContactAvatar(contact: contact, url: session.avatarURLs[contact.id], size: 34)
                    VStack(alignment: .leading, spacing: 1) {
                        Text(session.displayName(for: contact))
                            .font(.headline)
                            .lineLimit(1)
                        if let localTime = session.localTimeDescription(for: contact.id) {
                            Text(localTime)
                                .font(.caption2)
                                .foregroundStyle(.secondary)
                        }
                    }
                }
            }
            ToolbarItem(placement: .topBarTrailing) {
                Menu("Conversation", systemImage: "ellipsis") {
                    Button("Search messages", systemImage: "magnifyingglass") { showSearch = true }
                    Button(starredOnly ? "Show all messages" : "Show starred only", systemImage: starredOnly ? "text.bubble" : "star") {
                        starredOnly.toggle()
                    }
                    Divider()
                    Button("Conversation cost", systemImage: "dollarsign.circle") { showCost = true }
                    Button("Conversation settings", systemImage: "slider.horizontal.3") { showSettings = true }
                }
            }
        }
        .task {
            draft = ProcessInfo.processInfo.arguments.contains("-demo") ? "" : session.draftStore.text(for: contact.id)
            await session.loadAvatar(for: contact.id)
            await session.loadMessages(for: contact.id)
        }
        .onChange(of: draft) { _, newValue in
            session.draftStore.save(newValue, for: contact.id)
        }
        .sheet(isPresented: $showSettings) {
            ConversationSettingsView(contact: contact)
        }
        .sheet(isPresented: $showCost) {
            ConversationCostView(contact: contact)
        }
    }

    private var messageTimeline: some View {
        ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(spacing: 6) {
                    if session.messageHistoryHasMore[contact.id] == true, messageSearch.isEmpty, !starredOnly {
                        Button("Load earlier messages") {
                            Task { await session.loadMessages(for: contact.id, older: true) }
                        }
                        .font(.caption.weight(.semibold))
                        .buttonStyle(.bordered)
                        .padding(.vertical, 10)
                    }

                    if visibleMessages.isEmpty {
                        ContentUnavailableView(
                            starredOnly ? "No starred messages" : "No messages found",
                            systemImage: starredOnly ? "star" : "magnifyingglass",
                            description: Text(starredOnly ? "Long-press a message to star it." : "Try another search.")
                        )
                        .padding(.top, 80)
                    }

                    ForEach(Array(visibleMessages.enumerated()), id: \.element.id) { index, message in
                        if shouldShowDate(before: index) {
                            DatePill(date: message.date)
                                .padding(.vertical, 10)
                        }
                        MessageBubble(
                            message: message,
                            isStarred: session.preferences.isStarred(messageID: message.id, contactID: contact.id),
                            isBusy: session.activeMessageActionIDs.contains(message.id),
                            image: session.messageImages[message.id],
                            linkPreview: message.extractedURLs.first.flatMap { session.linkPreviews[$0] },
                            reply: { replyTarget = session.replyTarget(for: message) },
                            translate: { Task { await session.translate(message) } },
                            aiReply: { generateAIReply(to: message) },
                            toggleStar: { session.preferences.toggleStar(messageID: message.id, contactID: contact.id) },
                            react: { emoji in Task { await session.react(to: message, emoji: emoji) } }
                        )
                        .id(message.id)
                        .task {
                            await session.loadMedia(for: message)
                            if let url = message.extractedURLs.first { await session.loadLinkPreview(for: url) }
                        }
                    }
                }
                .padding(.horizontal, 12)
                .padding(.vertical, 14)
            }
            .scrollDismissesKeyboard(.interactively)
            .defaultScrollAnchor(.bottom)
            .onChange(of: messages.count) {
                guard messageSearch.isEmpty, !starredOnly, let id = messages.last?.id else { return }
                withAnimation(.snappy) { proxy.scrollTo(id, anchor: .bottom) }
            }
        }
    }

    private func shouldShowDate(before index: Int) -> Bool {
        guard index > 0 else { return true }
        return !Calendar.current.isDate(visibleMessages[index - 1].date, inSameDayAs: visibleMessages[index].date)
    }

    private func send() {
        let value = draft
        let reply = replyTarget
        Task {
            if await session.send(text: value, to: contact.id, reply: reply) {
                draft = ""
                replyTarget = nil
            }
        }
    }

    private func sendImage(_ data: Data, _ mimeType: String) {
        let reply = replyTarget
        Task {
            if await session.sendImage(data: data, mimeType: mimeType, to: contact.id, reply: reply) {
                replyTarget = nil
            }
        }
    }

    private func generateAIReply(to message: ChatMessage) {
        Task {
            guard let suggestion = await session.generateAIReply(to: message) else { return }
            replyTarget = session.replyTarget(for: message)
            draft = suggestion
        }
    }
}

private struct ChatWallpaper: View {
    @Environment(\.translatorPalette) private var palette
    var body: some View {
        palette.chatBackground
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

private struct FilterPill: View {
    let title: String
    let systemImage: String
    let clear: () -> Void

    var body: some View {
        HStack(spacing: 7) {
            Image(systemName: systemImage)
            Text(title)
            Button("Clear filter", systemImage: "xmark.circle.fill", action: clear)
                .labelStyle(.iconOnly)
        }
        .font(.caption.weight(.semibold))
        .padding(.horizontal, 12)
        .padding(.vertical, 7)
        .translatorGlass(in: Capsule())
    }
}

private struct ConversationCostView: View {
    @Environment(AppSession.self) private var session
    @Environment(\.dismiss) private var dismiss
    let contact: Contact
    @State private var usage: UsageSummary?
    @State private var error: String?

    var body: some View {
        NavigationStack {
            List {
                if let usage {
                    Section("Conversation cost") {
                        LabeledContent("Total", value: usage.costUsd.formatted(.currency(code: "USD").precision(.fractionLength(4))))
                        LabeledContent("Input tokens", value: usage.inputTokens.formatted())
                        LabeledContent("Cached input", value: usage.cachedInputTokens.formatted())
                        LabeledContent("Output tokens", value: usage.outputTokens.formatted())
                    }
                } else if let error {
                    ContentUnavailableView("Couldn’t load cost", systemImage: "exclamationmark.triangle", description: Text(error))
                } else {
                    HStack { Spacer(); ProgressView(); Spacer() }
                }
            }
            .navigationTitle(session.displayName(for: contact))
            .navigationBarTitleDisplayMode(.inline)
            .toolbar { ToolbarItem(placement: .confirmationAction) { Button("Done") { dismiss() } } }
            .task {
                do { usage = try await session.conversationUsage(for: contact.id) }
                catch { self.error = error.localizedDescription }
            }
        }
    }
}
