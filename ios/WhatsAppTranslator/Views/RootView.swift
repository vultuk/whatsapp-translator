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
    @Environment(\.translatorPalette) private var palette
    @State private var selected: ChatMessage?
    @State private var drafts: [String: String] = [:]
    @State private var sending = false

    var body: some View {
        VStack(spacing: 0) {
            Group {
                if session.mainTab == .messages {
                    UnifiedMessagesView(selected: $selected, drafts: $drafts, sending: $sending)
                } else {
                    ChatListView()
                }
            }
            Divider()
            HStack(spacing: 0) {
                tab("Messages", symbol: "text.bubble.fill", value: .messages)
                tab("Chats", symbol: "person.2.fill", value: .chats)
            }
            .padding(.top, 9)
            .padding(.bottom, 7)
            .background(.regularMaterial)
        }
    }

    private func tab(_ title: String, symbol: String, value: AppSession.MainTab) -> some View {
        Button {
            session.mainTab = value
        } label: {
            VStack(spacing: 3) {
                Image(systemName: symbol).font(.system(size: 20))
                Text(title).font(.caption2.weight(.semibold))
            }
            .foregroundStyle(session.mainTab == value ? palette.deepAccent : .secondary)
            .frame(maxWidth: .infinity)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityAddTraits(session.mainTab == value ? .isSelected : [])
    }
}

private struct UnifiedMessagesView: View {
    @Environment(AppSession.self) private var session
    @Environment(\.translatorPalette) private var palette
    @Binding var selected: ChatMessage?
    @Binding var drafts: [String: String]
    @Binding var sending: Bool
    @State private var showSettings = false
    @State private var atBottom = true
    @FocusState private var composerFocused: Bool

    private var draft: Binding<String> {
        Binding(get: { selected.map { drafts[$0.contactId] ?? "" } ?? "" }, set: { value in
            if let selected { drafts[selected.contactId] = value }
        })
    }

    var body: some View {
        NavigationStack {
            ZStack {
                ChatWallpaper()
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
                    .defaultScrollAnchor(.bottom)
                    .refreshable { await session.loadFeed() }
                    .onChange(of: session.unifiedMessages.last?.id) { _, _ in
                        if atBottom { withAnimation { proxy.scrollTo("feed-bottom", anchor: .bottom) } }
                    }
                }
            }
            .navigationTitle("Messages")
            .toolbar {
                ToolbarItemGroup(placement: .primaryAction) {
                    #if os(macOS)
                    Button("Refresh messages", systemImage: "arrow.clockwise") {
                        Task { await session.loadFeed() }
                    }
                    .keyboardShortcut("r", modifiers: .command)
                    .help("Refresh messages")
                    SettingsLink { Label("Settings", systemImage: "gearshape") }
                    #else
                    Button("Settings", systemImage: "gearshape") { showSettings = true }
                    #endif
                }
            }
            .safeAreaInset(edge: .bottom, spacing: 0) { composer }
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
        selected = message
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
                    if let suggestion = await session.generateAIReply(to: message), selected?.id == message.id, !sending {
                        drafts[message.contactId] = suggestion
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
            if let selected {
                HStack(alignment: .top, spacing: 9) {
                    RoundedRectangle(cornerRadius: 2).fill(palette.accent).frame(width: 3)
                    VStack(alignment: .leading, spacing: 3) {
                        Text("To: \(name(selected))").font(.subheadline.weight(.semibold)).foregroundStyle(palette.deepAccent)
                        Text(session.feedReplyNeedsQuote(selected) ? "Reply with quote" : "Send as a normal message")
                            .font(.caption).foregroundStyle(.secondary)
                        Text(selected.displayText).font(.caption).lineLimit(1).foregroundStyle(.secondary)
                    }
                    Spacer(minLength: 0)
                    Button("Cancel reply", systemImage: "xmark.circle.fill") { self.selected = nil; composerFocused = false }
                        .labelStyle(.iconOnly).foregroundStyle(.secondary).disabled(sending)
                }.fixedSize(horizontal: false, vertical: true)
                HStack(alignment: .bottom, spacing: 10) {
                    TextField("Message", text: draft, axis: .vertical)
                        .lineLimit(1...5)
                        .textFieldStyle(.plain)
                        .padding(10)
                        .background(palette.incomingBubble, in: RoundedRectangle(cornerRadius: 20))
                        .focused($composerFocused)
                        .disabled(sending)
                    Button(action: send) {
                        Group {
                            if sending { ProgressView().tint(.white) }
                            else { Image(systemName: "arrow.up").font(.system(size: 19, weight: .semibold)) }
                        }.frame(width: 42, height: 42).foregroundStyle(.white).background(palette.accent, in: Circle())
                    }
                    .buttonStyle(.plain)
                    .accessibilityLabel("Send to \(name(selected))")
                    .disabled(sending || draft.wrappedValue.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                }
            } else {
                Label("Swipe a message to reply to its chat", systemImage: "arrowshape.turn.up.left")
                    .font(.subheadline).foregroundStyle(.secondary)
                    .frame(maxWidth: .infinity).padding(.vertical, 9)
            }
        }
        .padding(.horizontal, 14).padding(.vertical, 10)
        .background(.regularMaterial)
    }

    private func send() {
        guard let target = selected, !sending else { return }
        let text = drafts[target.contactId] ?? ""
        sending = true
        Task {
            let sent = await session.send(text: text, to: target.contactId, reply: session.replyTarget(for: target), replyOnlyIfNotLatest: true)
            sending = false
            if sent {
                drafts[target.contactId] = ""
                selected = nil
                composerFocused = false
            }
        }
    }
}
