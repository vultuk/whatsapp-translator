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
    @State private var usage: UsageSummary?

    private var messages: [ChatMessage] { session.messages[contact.id] ?? [] }
    private var activePhotoSend: PhotoSendProgress? {
        session.photoSendProgress.values
            .filter { $0.contactID == contact.id }
            .sorted { $0.id > $1.id }
            .first
    }
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
                #if os(macOS)
                MacConversationHeader(
                    contact: contact,
                    displayName: session.displayName(for: contact),
                    avatarURL: session.avatarURLs[contact.id],
                    localTime: session.localTimeDescription(for: contact.id),
                    usage: usage,
                    starredOnly: starredOnly,
                    showContactSettings: { showSettings = true },
                    search: { Task { await activateSearch() } },
                    toggleStarred: { Task { await toggleStarredFilter() } },
                    showCost: { showCost = true },
                    showConversationSettings: { showSettings = true }
                )
                #endif
                if showSearch {
                    HStack(spacing: 10) {
                        Image(systemName: "magnifyingglass").foregroundStyle(.secondary)
                        TextField("Search messages", text: $messageSearch)
                            .platformUncapitalizedInput()
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
                if session.loadingCompleteHistoryContactIDs.contains(contact.id) {
                    HStack(spacing: 7) {
                        ProgressView().controlSize(.small)
                        Text("Loading the full conversation…")
                    }
                    .font(.caption.weight(.semibold))
                    .padding(.horizontal, 12)
                    .padding(.vertical, 7)
                    .translatorGlass(in: Capsule())
                    .padding(.top, 8)
                }
                messageTimeline
                if let progress = activePhotoSend {
                    PhotoSendProgressView(progress: progress)
                        .padding(.horizontal, 12)
                        .padding(.vertical, 8)
                }
                ComposerView(
                    text: $draft,
                    reply: replyTarget,
                    isSending: session.sendingContactIDs.contains(contact.id),
                    cancelReply: { replyTarget = nil },
                    sendImages: sendImages,
                    send: send
                )
            }
        }
        .navigationTitle(platformNavigationTitle)
        .task {
            let arguments = ProcessInfo.processInfo.arguments
            guard arguments.contains("-demoPhotoSendProgress") || arguments.contains("-demoPhotoTransferProgress") else { return }
            session.photoSendProgress["demo-photo-send"] = PhotoSendProgress(
                id: "demo-photo-send", contactID: contact.id,
                stage: arguments.contains("-demoPhotoTransferProgress") ? .transferring : .sending,
                completed: 4, total: 10, error: nil
            )
        }
        .platformInlineNavigationTitle()
        .toolbar {
            #if os(iOS)
            ToolbarItem(placement: .principal) {
                Button { showSettings = true } label: {
                    HStack(spacing: 9) {
                        ContactAvatar(contact: contact, url: session.avatarURLs[contact.id], size: 34)
                        VStack(alignment: .leading, spacing: 1) {
                            Text(session.displayName(for: contact))
                                .font(.headline)
                                .lineLimit(1)
                            Text(conversationHeaderSubtitle)
                                .font(.caption2)
                                .foregroundStyle(.secondary)
                                .lineLimit(1)
                        }
                    }
                    .platformCompactControlTypography()
                }
                .buttonStyle(.plain)
                .accessibilityHint("Opens nickname, timezone and translation settings")
            }
            ToolbarItem(placement: platformTrailingToolbarPlacement) {
                Menu("Conversation", systemImage: "ellipsis") {
                    Button("Search messages", systemImage: "magnifyingglass") {
                        Task { await activateSearch() }
                    }
                    .keyboardShortcut("f", modifiers: .command)
                    Button(
                        starredOnly ? "Show all messages" : "Show starred only",
                        systemImage: starredOnly ? "star.fill" : "star"
                    ) {
                        Task { await toggleStarredFilter() }
                    }
                    Divider()
                    Button("Conversation cost", systemImage: "dollarsign.circle") { showCost = true }
                    Button("Conversation settings", systemImage: "slider.horizontal.3") { showSettings = true }
                }
            }
            #endif
        }
        .task {
            draft = ProcessInfo.processInfo.arguments.contains("-demo") ? "" : session.draftStore.text(for: contact.id)
            await session.loadAvatar(for: contact.id)
            await session.loadMessages(for: contact.id)
            usage = try? await session.conversationUsage(for: contact.id)
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
                #if os(macOS)
                VStack(spacing: 6) {
                    messageTimelineContent
                }
                .padding(.horizontal, MacChatLayoutMetrics.timelineHorizontalPadding / 2)
                .padding(.vertical, 14)
                #else
                LazyVStack(spacing: 6) {
                    messageTimelineContent
                }
                .padding(.horizontal, 12)
                .padding(.vertical, 14)
                #endif
            }
            .platformDismissesKeyboard()
            .defaultScrollAnchor(.bottom)
            .onChange(of: messages.count) {
                guard messageSearch.isEmpty, !starredOnly, let id = messages.last?.id else { return }
                withAnimation(.snappy) { proxy.scrollTo(id, anchor: .bottom) }
            }
        }
    }

    private var platformNavigationTitle: String {
        #if os(macOS)
        "Babel Bridge"
        #else
        session.displayName(for: contact)
        #endif
    }

    private var conversationHeaderSubtitle: String {
        session.localTimeDescription(for: contact.id) ?? "Auto-translation on"
    }

    @ViewBuilder
    private var messageTimelineContent: some View {
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
                description: Text(starredOnly ? "Use the actions button under a message to star it." : "Try another search.")
            )
            .padding(.top, 80)
        }

        ForEach(Array(timelineItems.enumerated()), id: \.element.id) { index, item in
            if shouldShowDate(before: index) {
                DatePill(date: item.date)
                    .padding(.vertical, 10)
            }
            switch item {
            case let .message(message):
                messageBubble(message: message)
                    .id(message.id)
                    .task { await loadTimelineMedia(for: [message]) }
            case let .photoAlbum(album):
                messageBubble(message: album.primaryMessage, albumMessages: album.messages)
                    .id(item.id)
                    .task { await loadTimelineMedia(for: album.messages) }
            }
        }
    }

    private var timelineItems: [ConversationTimelineItem] {
        ConversationTimelineBuilder.items(from: visibleMessages)
    }

    private func messageBubble(message: ChatMessage, albumMessages: [ChatMessage] = []) -> some View {
        MessageBubble(
                message: message,
                isStarred: session.preferences.isStarred(messageID: message.id, contactID: contact.id),
                isBusy: session.activeMessageActionIDs.contains(message.id),
                image: session.messageImages[message.id],
                mediaURL: session.messageMediaURLs[message.id],
                mediaIsLoading: session.mediaLoadingIDs.contains(message.id),
                mediaFailed: session.mediaErrorIDs.contains(message.id),
                linkPreviews: message.extractedURLs.compactMap { session.linkPreviews[$0] },
                reply: { replyTarget = session.replyTarget(for: message) },
                translate: { Task { await session.translate(message) } },
                aiReply: { generateAIReply(to: message) },
                toggleStar: { session.preferences.toggleStar(messageID: message.id, contactID: contact.id) },
                react: { emoji in Task { await session.react(to: message, emoji: emoji) } },
                retryMedia: { Task { await session.retryMedia(for: message) } },
                albumMessages: albumMessages,
                albumImages: session.messageImages,
                albumLoadingIDs: session.mediaLoadingIDs,
                albumFailedIDs: session.mediaErrorIDs,
                retryAlbumMedia: { albumMessage in Task { await session.retryMedia(for: albumMessage) } }
            )
    }

    private func loadTimelineMedia(for messages: [ChatMessage]) async {
        for message in messages {
            await session.loadMedia(for: message)
            for url in message.extractedURLs { await session.loadLinkPreview(for: url) }
        }
    }

    private func shouldShowDate(before index: Int) -> Bool {
        guard index > 0 else { return true }
        return !Calendar.current.isDate(timelineItems[index - 1].date, inSameDayAs: timelineItems[index].date)
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

    private func sendImages(_ images: [OutgoingImage], _ caption: String?) -> Bool {
        let reply = replyTarget
        if session.startPhotoSend(images, caption: caption, to: contact.id, reply: reply) {
            replyTarget = nil
            return true
        }
        return false
    }

    private func generateAIReply(to message: ChatMessage) {
        Task {
            guard let suggestion = await session.generateAIReply(to: message) else { return }
            replyTarget = session.replyTarget(for: message)
            draft = suggestion
        }
    }

    private func activateSearch() async {
        await session.loadAllMessages(for: contact.id)
        showSearch = true
    }

    private func toggleStarredFilter() async {
        if starredOnly {
            starredOnly = false
            return
        }
        await session.loadAllMessages(for: contact.id)
        starredOnly = true
    }
}

private struct PhotoSendProgressView: View {
    @Environment(\.translatorPalette) private var palette
    let progress: PhotoSendProgress

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: progress.stage == .complete ? "checkmark.circle.fill" : "photo.stack.fill")
                .font(.title3)
                .foregroundStyle(progress.stage == .failed ? .red : palette.accent)
            VStack(alignment: .leading, spacing: 6) {
                Text(progress.statusText)
                    .font(.subheadline.weight(.semibold))
                    .lineLimit(2)
                ProgressView(value: progress.fractionCompleted)
                    .tint(progress.stage == .failed ? .red : palette.accent)
            }
        }
        .padding(12)
        .translatorGlass(in: RoundedRectangle(cornerRadius: 16, style: .continuous))
        .accessibilityElement(children: .combine)
        .accessibilityLabel(progress.statusText)
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
                    #if os(iOS)
                    .symbolEffect(.pulse, options: .repeating.speed(0.05))
                    #endif
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
                        costRow("Total", usage.costUsd.formatted(.currency(code: "USD").precision(.fractionLength(4))))
                        costRow("Input tokens", usage.inputTokens.formatted())
                        costRow("Cached input", usage.cachedInputTokens.formatted())
                        costRow("Output tokens", usage.outputTokens.formatted())
                    }
                } else if let error {
                    VStack(spacing: 14) {
                        ContentUnavailableView("Couldn’t load cost", systemImage: "exclamationmark.triangle", description: Text(error))
                        Button("Try again", systemImage: "arrow.clockwise") {
                            Task { await load() }
                        }
                        .buttonStyle(.borderedProminent)
                    }
                } else {
                    HStack { Spacer(); ProgressView(); Spacer() }
                }
            }
            .navigationTitle(session.displayName(for: contact))
            .platformInlineNavigationTitle()
            .toolbar { ToolbarItem(placement: .confirmationAction) { Button("Done") { dismiss() } } }
            .task { await load() }
        }
        #if os(macOS)
        .platformSheetSize(
            minWidth: MacChatLayoutMetrics.costSheetMinimumWidth,
            minHeight: MacChatLayoutMetrics.costSheetMinimumHeight
        )
        #endif
    }

    private func load() async {
        error = nil
        do { usage = try await session.conversationUsage(for: contact.id) }
        catch { self.error = error.localizedDescription }
    }

    private func costRow(_ label: String, _ value: String) -> some View {
        LabeledContent(label) {
            Text(value)
                .foregroundStyle(.secondary)
                .monospacedDigit()
        }
    }
}
