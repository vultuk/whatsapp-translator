import SwiftUI

struct RootView: View {
    @Environment(AppSession.self) private var session

    var body: some View {
        Group {
            switch session.phase {
            case .restoring:
                LaunchView(label: "Restoring your translator…")
            case .connecting:
                LaunchView(label: "Connecting securely…")
            case .needsConfiguration:
                ConnectionSetupView()
            case .ready:
                MainMessagesView()
            }
        }
        .animation(.snappy, value: session.phase)
        .alert(session.errorTitle, isPresented: errorPresented) {
            Button("OK") { session.errorMessage = nil }
        } message: {
            Text(session.errorMessage ?? "Something went wrong.")
        }
    }

    private var errorPresented: Binding<Bool> {
        Binding(
            get: { session.errorMessage != nil },
            set: { if !$0 { session.errorMessage = nil } }
        )
    }
}

private struct LaunchView: View {
    let label: String

    var body: some View {
        ZStack {
            TranslatorBackdrop()
            VStack(spacing: 18) {
                TranslatorMark(size: 70)
                ProgressView()
                    .controlSize(.large)
                Text(label)
                    .font(.headline)
                    .foregroundStyle(.secondary)
            }
        }
    }
}

private struct MainMessagesView: View {
    @Environment(AppSession.self) private var session
    @State private var replyDraft = UnifiedReplyDraft()
    @State private var sending = false

    var body: some View {
        Group {
            if session.mainTab == .messages {
                UnifiedMessagesView(replyDraft: $replyDraft, sending: $sending)
            } else {
                ChatListView()
            }
        }
    }
}

struct UnifiedReplyDraft {
    var selected: ChatMessage?
    var isFocused = false
    var drafts: [String: String] = [:]

    var text: String { selected.map { drafts[$0.contactId] ?? "" } ?? "" }

    mutating func updateText(_ value: String, latestMessage: ChatMessage?) {
        if selected == nil {
            guard !value.isEmpty, let latestMessage else { return }
            selected = latestMessage
            isFocused = true
        }
        if let selected { drafts[selected.contactId] = value }
    }

    @discardableResult
    mutating func beginAttachment(latestMessage: ChatMessage?) -> ChatMessage? {
        if selected == nil { selected = latestMessage }
        isFocused = selected != nil
        return selected
    }

    mutating func finishMediaSending(to target: ChatMessage) {
        // A separate photo caption or recording must not erase a text draft.
        if selected?.id == target.id { cancelSelection() }
    }

    mutating func select(_ message: ChatMessage) {
        selected = message
        isFocused = true
    }

    mutating func cancelSelection() {
        selected = nil
        isFocused = false
    }

    mutating func finishSending(to target: ChatMessage) {
        drafts[target.contactId] = ""
        if selected?.id == target.id { cancelSelection() }
    }
}

struct MainNavigationToolbar: ToolbarContent {
    @Environment(AppSession.self) private var session
    @Binding var showSettings: Bool

    var body: some ToolbarContent {
        ToolbarItemGroup(placement: .primaryAction) {
            Button("Messages", systemImage: session.mainTab == .messages ? "text.bubble.fill" : "text.bubble") {
                session.mainTab = .messages
            }
            .accessibilityAddTraits(session.mainTab == .messages ? .isSelected : [])
            .help("Messages")
            Button("Chats", systemImage: session.mainTab == .chats ? "person.2.fill" : "person.2") {
                session.mainTab = .chats
            }
            .accessibilityAddTraits(session.mainTab == .chats ? .isSelected : [])
            .help("Chats")
            #if os(macOS)
            SettingsLink { Label("Settings", systemImage: "gearshape") }
                .help("Settings")
            #else
            Button("Settings", systemImage: "gearshape") { showSettings = true }
            #endif
        }
    }
}

private struct UnifiedMessagesView: View {
    @Environment(AppSession.self) private var session
    @Environment(\.translatorPalette) private var palette
    @Binding var replyDraft: UnifiedReplyDraft
    @Binding var sending: Bool
    @State private var showSettings = false
    @State private var atBottom = true
    @FocusState private var composerFocused: Bool

    private var draft: Binding<String> {
        Binding(get: { replyDraft.text }, set: { value in
            guard !sending else { return }
            replyDraft.updateText(value, latestMessage: session.unifiedMessages.last)
        })
    }

    var body: some View {
        GeometryReader { geometry in
            messagesContent(bottomSafeArea: geometry.safeAreaInsets.bottom)
        }
    }

    private func messagesContent(bottomSafeArea: CGFloat) -> some View {
        NavigationStack {
            ZStack {
                ChatWallpaper()
                    .blur(radius: replyDraft.isFocused ? 8 : 0)
                ScrollViewReader { proxy in
                    ScrollView {
                        LazyVStack(spacing: 12) {
                            if session.feedHasMore {
                                Button("Load earlier messages") {
                                    let anchor = session.unifiedMessages.first?.id
                                    Task {
                                        await session.loadFeed(older: true)
                                        if let anchor { proxy.scrollTo(anchor, anchor: .top) }
                                    }
                                }.disabled(session.feedLoading)
                            }
                            if session.feedLoading { ProgressView().padding() }
                            if let error = session.feedError {
                                VStack(spacing: 8) {
                                    Text(error).font(.caption).foregroundStyle(.secondary)
                                    Button("Retry") { Task { await session.loadFeed() } }
                                }.padding()
                            }
                            if session.unifiedMessages.isEmpty && !session.feedLoading && session.feedError == nil {
                                ContentUnavailableView("All your messages, together", systemImage: "text.bubble", description: Text("Messages from your chats will appear here. Swipe a message to choose where your reply goes."))
                            }
                            ForEach(session.unifiedMessages) { message in
                                VStack(alignment: .leading, spacing: 5) {
                                    source(message)
                                    bubble(message)
                                }
                                .id(message.id)
                                .task {
                                    await session.loadMedia(for: message)
                                    for url in message.extractedURLs { await session.loadLinkPreview(for: url) }
                                }
                            }
                            Color.clear.frame(height: 1).id("feed-bottom")
                                .onAppear { atBottom = true }
                                .onDisappear { atBottom = false }
                        }
                        .padding(.horizontal, 12)
                        .padding(.vertical, 14)
                        .frame(maxWidth: 900)
                        .frame(maxWidth: .infinity)
                    }
                    #if os(iOS)
                    // Keep scrolled messages behind the native navigation-bar material.
                    .scrollClipDisabled()
                    #endif
                    .platformDismissesKeyboard()
                    .platformSwipeDownDismissesKeyboard()
                    .defaultScrollAnchor(.bottom)
                    .refreshable { await session.loadFeed() }
                    .onChange(of: session.unifiedMessages.last?.id) { _, _ in
                        if atBottom { withAnimation { proxy.scrollTo("feed-bottom", anchor: .bottom) } }
                    }
                }
                .blur(radius: replyDraft.isFocused ? 8 : 0)
                .opacity(replyDraft.isFocused ? 0.35 : 1)
                .allowsHitTesting(!replyDraft.isFocused)
                .accessibilityHidden(replyDraft.isFocused)
                if replyDraft.isFocused, let selected = replyDraft.selected {
                    FocusedReplyOverlay(destination: name(selected), isSending: sending, cancel: cancelReply) {
                        bubble(selected)
                    }
                }
            }
            .navigationTitle("Messages")
            .platformChatNavigationBackground()
            .toolbar { MainNavigationToolbar(showSettings: $showSettings) }
            .safeAreaInset(edge: .bottom, spacing: 0) {
                composer.padding(.bottom, ComposerLayout.bottomAdjustment(for: bottomSafeArea))
            }
            .sheet(isPresented: $showSettings) { AppSettingsView() }
        }
        .task { if !session.feedHasLoaded { await session.loadFeed() } }
        .onChange(of: session.mainTab) { _, tab in
            if tab != .messages { composerFocused = false }
        }
    }

    private func name(_ message: ChatMessage) -> String {
        if let contact = session.contacts.first(where: { $0.id == message.contactId }) {
            return session.displayName(for: contact)
        }
        return message.contactName ?? message.contactPhone ?? message.contactId
    }

    private func source(_ message: ChatMessage) -> some View {
        Button {
            session.selectedContactID = message.contactId
            session.mainTab = .chats
        } label: {
            HStack(spacing: 7) {
                Image(systemName: message.chatType == "Group" || message.contactId.hasSuffix("@g.us") ? "person.2.fill" : "person.fill")
                Text(name(message)).fontWeight(.semibold).lineLimit(1)
                Image(systemName: "chevron.right").font(.system(size: 9, weight: .bold))
                Spacer(minLength: 8)
                Text(message.date, format: .dateTime.day().month(.abbreviated)).foregroundStyle(.secondary)
            }
            .font(.caption)
            .foregroundStyle(palette.deepAccent)
            .padding(.horizontal, 9)
            .padding(.vertical, 5)
        }
        .buttonStyle(.plain)
        .accessibilityLabel("Open chat: \(name(message))")
    }

    private func select(_ message: ChatMessage) {
        guard !sending else { return }
        replyDraft.select(message)
        composerFocused = true
    }

    private func bubble(_ message: ChatMessage) -> some View {
        MessageBubble(
            message: message,
            isStarred: session.preferences.isStarred(messageID: message.id, contactID: message.contactId),
            isBusy: session.activeMessageActionIDs.contains(message.id),
            image: session.messageImages[message.id],
            mediaURL: session.messageMediaURLs[message.id],
            mediaIsLoading: session.mediaLoadingIDs.contains(message.id),
            mediaFailed: session.mediaErrorIDs.contains(message.id),
            linkPreviews: message.extractedURLs.compactMap { session.linkPreviews[$0] },
            reply: { select(message) },
            translate: { Task { await session.translate(message) } },
            aiReply: {
                select(message)
                Task {
                    if let suggestion = await session.generateAIReply(to: message), replyDraft.selected?.id == message.id, !sending {
                        replyDraft.drafts[message.contactId] = suggestion
                    }
                }
            },
            toggleStar: { session.preferences.toggleStar(messageID: message.id, contactID: message.contactId) },
            react: { emoji in Task { await session.react(to: message, emoji: emoji) } },
            retryMedia: { Task { await session.retryMedia(for: message) } },
            albumMessages: [], albumImages: [:], albumLoadingIDs: [], albumFailedIDs: [],
            retryAlbumMedia: { message in Task { await session.retryMedia(for: message) } }
        )
    }

    private var composer: some View {
        VStack(alignment: .leading, spacing: 8) {
            if let selected = replyDraft.selected, !replyDraft.isFocused {
                HStack(alignment: .top, spacing: 9) {
                    RoundedRectangle(cornerRadius: 2).fill(palette.accent).frame(width: 3)
                    VStack(alignment: .leading, spacing: 3) {
                        Text("To: \(name(selected))").font(.subheadline.weight(.semibold)).foregroundStyle(palette.deepAccent)
                        Text(session.feedReplyNeedsQuote(selected) ? "Reply with quote" : "Send as a normal message")
                            .font(.caption).foregroundStyle(.secondary)
                        Text(selected.displayText).font(.caption).lineLimit(1).foregroundStyle(.secondary)
                    }
                    Spacer(minLength: 0)
                    Button("Cancel reply", systemImage: "xmark.circle.fill", action: cancelReply)
                        .labelStyle(.iconOnly).foregroundStyle(.secondary).disabled(sending)
                }.fixedSize(horizontal: false, vertical: true)
                    .padding(12)
                    .translatorGlass(in: RoundedRectangle(cornerRadius: 20))
            } else if replyDraft.selected == nil && session.unifiedMessages.isEmpty {
                Text("Waiting for messages")
                    .font(.caption).foregroundStyle(.secondary)
            }
            if let progress = session.photoSendProgress.values.max(by: { $0.startedAt < $1.startedAt }) {
                PhotoSendProgressView(progress: progress)
            }
            ComposerGlassGroup {
                HStack(alignment: .bottom, spacing: 7) {
                    UnifiedMediaControls(
                        disabled: sending || session.sendingContactIDs.contains((replyDraft.selected ?? session.unifiedMessages.last)?.contactId ?? "") || (replyDraft.selected == nil && session.unifiedMessages.isEmpty),
                        begin: {
                            guard let target = replyDraft.beginAttachment(latestMessage: session.unifiedMessages.last) else { return nil }
                            composerFocused = false
                            return UnifiedMediaContext(message: target, reply: session.replyTarget(for: target), destination: name(target))
                        },
                        onSent: { target in replyDraft.finishMediaSending(to: target); composerFocused = false }
                    )
                    TextField("Message", text: draft, axis: .vertical)
                        .lineLimit(1...5)
                        .textFieldStyle(.plain)
                        .padding(.horizontal, 16)
                        .padding(.vertical, 12)
                        .frame(minHeight: 46)
                        .translatorGlassControl(in: RoundedRectangle(cornerRadius: 24))
                        .focused($composerFocused)
                        .disabled(sending || (replyDraft.selected == nil && session.unifiedMessages.isEmpty))
                    Button(action: send) {
                        Group {
                            if sending { ProgressView().tint(.white) }
                            else { Image(systemName: "arrow.up").font(.system(size: 19, weight: .semibold)) }
                        }.frame(width: 46, height: 46).foregroundStyle(.white)
                            .contentShape(Circle())
                            .translatorGlassControl(in: Circle(), tint: palette.accent)
                    }
                    .buttonStyle(.plain)
                    .accessibilityLabel(replyDraft.selected.map { "Send to \(name($0))" } ?? "Send message")
                    .disabled(sending || replyDraft.selected == nil || draft.wrappedValue.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                }
            }
        }
        .frame(maxWidth: 900)
        .padding(.horizontal, 14).padding(.vertical, 10)
        .frame(maxWidth: .infinity)
    }

    private func cancelReply() {
        replyDraft.cancelSelection()
        composerFocused = false
    }

    private func send() {
        guard let target = replyDraft.selected, !sending else { return }
        let text = replyDraft.drafts[target.contactId] ?? ""
        sending = true
        Task {
            let sent = await session.send(text: text, to: target.contactId, reply: session.replyTarget(for: target), replyOnlyIfNotLatest: true)
            sending = false
            if sent {
                replyDraft.finishSending(to: target)
                composerFocused = false
            }
        }
    }
}
