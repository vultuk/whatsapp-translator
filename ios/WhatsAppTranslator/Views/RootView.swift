import SwiftUI

struct RootView: View {
    @Environment(AppSession.self) private var session
    #if DEBUG && os(macOS)
    @State private var showsDemoIcons = ProcessInfo.processInfo.arguments.contains("-demo")
        && ProcessInfo.processInfo.arguments.contains("-demoAppIcons")
    #endif

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
                Group {
                    if session.requiresWhatsAppLink {
                        WhatsAppLinkView()
                    } else {
                        MainMessagesView()
                            .safeAreaInset(edge: .top, spacing: 0) {
                                if !session.backendStatus.connected {
                                    Label("Reconnecting to WhatsApp…", systemImage: "arrow.triangle.2.circlepath")
                                        .font(.footnote)
                                        .padding(10)
                                        .frame(maxWidth: .infinity)
                                        .background(.regularMaterial)
                                }
                            }
                    }
                }
                .task { await session.monitorWhatsAppConnection() }
            }
        }
        .animation(.snappy, value: session.phase)
        .alert(session.errorTitle, isPresented: errorPresented) {
            Button("OK") { session.errorMessage = nil }
        } message: {
            Text(session.errorMessage ?? "Something went wrong.")
        }
        #if DEBUG && os(macOS)
        .sheet(isPresented: $showsDemoIcons) {
            NavigationStack {
                AppIconPickerView()
                    .toolbar {
                        ToolbarItem(placement: .confirmationAction) {
                            Button("Done") { showsDemoIcons = false }
                        }
                    }
            }
            .frame(width: 560, height: 650)
        }
        #endif
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

struct TopicFilterBar: View {
    @Environment(AppSession.self) private var session
    let contactID: String?
    @Binding var selection: String?
    @State private var showManagement = false

    private var topics: [ChatTopic] { session.topics(for: contactID) }
    private var selected: ChatTopic? { topics.first { $0.id == selection } }
    private var pending: Int { session.topicCatalog.pendingCount(contactID: contactID) }

    var body: some View {
        VStack(alignment: .leading, spacing: 5) {
            HStack(spacing: 10) {
                Menu {
                    Button { selection = nil } label: {
                        Label("All messages", systemImage: selection == nil ? "checkmark" : "text.bubble")
                    }
                    if !topics.isEmpty {
                        Divider()
                        ForEach(topics) { topic in
                            Button { selection = topic.id } label: {
                                Label(topic.title,
                                      systemImage: selection == topic.id ? "checkmark" : "number")
                            }
                        }
                    }
                    Divider()
                    Button("Manage topics", systemImage: "slider.horizontal.3") { showManagement = true }
                } label: {
                    #if os(macOS)
                    Label(selected?.title ?? "All messages", systemImage: "line.3.horizontal.decrease.circle")
                    #else
                    HStack(spacing: 6) {
                        Image(systemName: "line.3.horizontal.decrease.circle")
                        VStack(alignment: .leading, spacing: 1) {
                            Text(selected?.title ?? "All messages").font(.subheadline.weight(.semibold)).lineLimit(1)
                        }
                        Image(systemName: "chevron.down").font(.caption2)
                    }
                    #endif
                }
                .accessibilityIdentifier("topic-filter")
                Spacer(minLength: 4)
                if pending > 0 {
                    ProgressView().controlSize(.mini)
                    Text("Organising \(pending)").font(.caption2).foregroundStyle(.secondary)
                }
                Button("Topics", systemImage: "sparkles") { showManagement = true }
                    .font(.caption.weight(.semibold))
                    .accessibilityIdentifier("manage-topics")
            }
            if let error = session.topicCatalogError {
                HStack {
                    Text(error).font(.caption2).foregroundStyle(.secondary)
                    Button("Retry") { Task { await session.loadTopics() } }.font(.caption)
                }
            }
        }
        .padding(.horizontal, 14).padding(.vertical, 8)
        .background(.regularMaterial)
        .sheet(isPresented: $showManagement) { TopicManagementView(contactID: contactID) }
    }
}

struct TopicPageControls: View {
    @Environment(AppSession.self) private var session
    let topicID: String
    var body: some View {
        VStack(spacing: 8) {
            if session.topicLoading.contains(topicID) { ProgressView("Loading topic…").font(.caption) }
            if let error = session.topicPages[topicID]?.error {
                Text(error).font(.caption).foregroundStyle(.secondary)
                Button("Retry") { Task { await session.loadTopicMessages(topicID) } }
            }
            if session.topicPages[topicID]?.hasMore == true {
                Button("Load earlier topic messages") { Task { await session.loadTopicMessages(topicID, older: true) } }
                    .disabled(session.topicLoading.contains(topicID))
                    .font(.caption.weight(.semibold)).buttonStyle(.bordered)
            }
        }
    }
}

struct TopicManagementView: View {
    @Environment(AppSession.self) private var session
    @Environment(\.dismiss) private var dismiss
    let contactID: String?
    @State private var saving: Set<String> = []
    @State private var error: String?
    @State private var importing = false
    @State private var importPreview: TopicImportSummary?
    @State private var showImportConfirmation = false
    @State private var importStatus: String?
    private var contacts: [Contact] {
        session.contacts.filter { !$0.isUpdates && (contactID == nil || $0.id == contactID) }
            .sorted { session.displayName(for: $0).localizedStandardCompare(session.displayName(for: $1)) == .orderedAscending }
    }
    var body: some View {
        NavigationStack {
            Form {
                Section {
                    Text("Follow one discussion at a time in Chats and Messages. Topics always stay separate by chat.")
                        .font(.subheadline)
                }
                Section {
                    ForEach(contacts) { contact in
                        VStack(alignment: .leading, spacing: 6) {
                            Toggle(session.displayName(for: contact), isOn: Binding(
                                get: { session.topicSetting(for: contact.id).enabled },
                                set: { setEnabled($0, for: contact.id) }
                            ))
                            .disabled(importing || saving.contains(contact.id) || (!session.topicCatalog.available && !session.topicSetting(for: contact.id).enabled))
                            .accessibilityIdentifier("topics-enabled-\(contact.id)")
                            if saving.contains(contact.id) { ProgressView().controlSize(.small) }
                            let status = session.topicSetting(for: contact.id)
                            if status.pendingCount > 0 { Text("Organising \(status.pendingCount) messages…").font(.caption).foregroundStyle(.secondary) }
                            if status.failedCount > 0 {
                                HStack {
                                    Text("\(status.failedCount) messages couldn’t be organised.").font(.caption)
                                    Button("Retry") { setEnabled(true, for: contact.id) }.disabled(saving.contains(contact.id))
                                }
                            }
                        }
                    }
                } header: { Text("Organise by topic") } footer: {
                    Text("Off by default. Enabling sends the latest 200 text messages and captions to your configured AI, then processes new messages and edits. This adds AI usage. Existing topics are saved; everyone else continues using their normal WhatsApp chat.")
                }
                Section {
                    Button {
                        importing = true
                        error = nil
                        Task {
                            defer { importing = false }
                            do {
                                importPreview = try await session.topicImportPreview()
                                if importPreview?.messageCount == 0 { importStatus = "No unorganised text messages or captions from the last 7 days are stored on this server." }
                                else { showImportConfirmation = true }
                            } catch { self.error = error.localizedDescription }
                        }
                    } label: {
                        Label("Organise last 7 days from all chats", systemImage: "tray.and.arrow.down")
                    }
                    .accessibilityIdentifier("import-recent-topics")
                    .disabled(importing || !saving.isEmpty || !session.topicCatalog.available)
                    if importing { ProgressView("Preparing import…") }
                    if let importStatus { Text(importStatus).font(.caption).foregroundStyle(.secondary) }
                } header: { Text("Initial import") } footer: {
                    Text("Uses text and captions already stored on the server. Skips messages already organised. Enables topics for the included chats so new messages stay organised, with topics kept separate by chat.")
                }
                if !session.topicCatalog.available {
                    Text("OpenAI is unavailable on this server. Configure it before enabling topics.").font(.caption).foregroundStyle(.secondary)
                }
                if let error { Text(error).foregroundStyle(.red).font(.caption) }
                if let error = session.topicCatalogError {
                    Text(error).font(.caption)
                    Button("Retry loading topics") { Task { await session.loadTopics() } }
                }
            }
            .platformGroupedFormStyle()
            .navigationTitle("Topics")
            .platformInlineNavigationTitle()
            .toolbar { ToolbarItem(placement: .confirmationAction) { Button("Done") { dismiss() }.disabled(importing || !saving.isEmpty) } }
            .task { await session.loadTopics() }
            .interactiveDismissDisabled(importing || !saving.isEmpty)
            .confirmationDialog("Organise the last 7 days?", isPresented: $showImportConfirmation, titleVisibility: .visible) {
                Button("Start import") {
                    importing = true
                    Task {
                        defer { importing = false }
                        do {
                            let result = try await session.importRecentTopics()
                            importStatus = "Queued \(result.messageCount) messages across \(result.chatCount) chats. Topics will appear as processing finishes."
                        } catch { self.error = error.localizedDescription }
                    }
                }
            } message: {
                Text("\(importPreview?.messageCount ?? 0) messages across \(importPreview?.chatCount ?? 0) chats will be sent to your configured AI. This adds AI usage and enables topics for those chats.")
            }
        }
        #if os(macOS)
        .frame(minWidth: 440, minHeight: 400)
        #endif
    }
    private func setEnabled(_ enabled: Bool, for id: String) {
        saving.insert(id)
        error = nil
        Task {
            defer { saving.remove(id) }
            do { try await session.setTopicsEnabled(enabled, contactID: id) }
            catch { self.error = error.localizedDescription }
        }
    }
}

private struct UnifiedMessagesView: View {
    @Environment(AppSession.self) private var session
    @Environment(\.translatorPalette) private var palette
    @Binding var replyDraft: UnifiedReplyDraft
    @Binding var sending: Bool
    @State private var showSettings = false
    @State private var settingsContact: Contact?
    @State private var selectedTopicID: String?
    private var displayedMessages: [ChatMessage] {
        if let selectedTopicID { return session.topicPages[selectedTopicID]?.messages ?? [] }
        return session.unifiedMessages
    }
    @State private var atBottom = true
    @State private var composerFocused = false
    @State private var imagePaste = ImagePasteController()

    private var draft: Binding<String> {
        Binding(get: { replyDraft.text }, set: { value in
            guard !sending else { return }
            replyDraft.updateText(value, latestMessage: displayedMessages.last)
        })
    }

    var body: some View {
        GeometryReader { geometry in
            messagesContent(bottomSafeArea: geometry.safeAreaInsets.bottom)
        }
        .modifier(PastedImagePresentation(controller: imagePaste))
    }

    private func messagesContent(bottomSafeArea: CGFloat) -> some View {
        let items = ConversationTimelineBuilder.items(from: displayedMessages)
        return NavigationStack {
            ZStack {
                ChatWallpaper()
                    .blur(radius: replyDraft.isFocused ? 8 : 0)
                ScrollViewReader { proxy in
                    ScrollView {
                        LazyVStack(spacing: 6) {
                            if let selectedTopicID { TopicPageControls(topicID: selectedTopicID) }
                            if selectedTopicID == nil, session.feedHasMore {
                                Button("Load earlier messages") {
                                    let anchor = session.unifiedMessages.first?.id
                                    Task {
                                        await session.loadFeed(older: true)
                                        if let anchor { proxy.scrollTo(anchor, anchor: .top) }
                                    }
                                }.disabled(session.feedLoading)
                            }
                            if selectedTopicID == nil, session.feedLoading { ProgressView().padding() }
                            if selectedTopicID == nil, let error = session.feedError {
                                VStack(spacing: 8) {
                                    Text(error).font(.caption).foregroundStyle(.secondary)
                                    Button("Retry") { Task { await session.loadFeed() } }
                                }.padding()
                            }
                            if displayedMessages.isEmpty && !session.feedLoading && session.feedError == nil {
                                ContentUnavailableView(selectedTopicID == nil ? "All your messages, together" : "No messages in this topic", systemImage: "text.bubble", description: Text(selectedTopicID == nil ? "Messages from your chats will appear here. Swipe a message to choose where your reply goes." : "New messages will appear here after they’re organised. All messages remains available."))
                            }
                            ForEach(Array(items.enumerated()), id: \.element.id) { index, item in
                                let message = item.primaryMessage
                                let startsConversation = index == 0
                                    || items[index - 1].primaryMessage.contactId != message.contactId
                                    || !Calendar.current.isDate(items[index - 1].date, inSameDayAs: message.date)
                                VStack(alignment: .leading, spacing: 5) {
                                    if startsConversation {
                                        source(message)
                                            .padding(.top, index == 0 ? 0 : 10)
                                    }
                                    bubble(message, albumMessages: item.messages.count > 1 ? item.messages : [])
                                }
                                .id(item.id)
                                .task(id: item.messages.map(\.id)) {
                                    for photo in item.messages.prefix(PhotoGalleryLayout.previewLimit) {
                                        await session.loadMedia(for: photo)
                                        for url in photo.extractedURLs { await session.loadLinkPreview(for: url) }
                                    }
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
                    .refreshable {
                        await session.loadTopics()
                        if let selectedTopicID { await session.loadTopicMessages(selectedTopicID) }
                        else { await session.loadFeed() }
                    }
                    .onChange(of: displayedMessages.last?.id) { _, _ in
                        if atBottom { withAnimation { proxy.scrollTo("feed-bottom", anchor: .bottom) } }
                    }
                }
                .blur(radius: replyDraft.isFocused ? 8 : 0)
                .opacity(replyDraft.isFocused ? 0.35 : 1)
                .allowsHitTesting(!replyDraft.isFocused)
                .accessibilityHidden(replyDraft.isFocused)
                if replyDraft.isFocused, let selected = replyDraft.selected {
                    FocusedReplyOverlay(destination: name(selected), isSending: sending, cancel: cancelReply) {
                        bubble(session.unifiedMessages.first(where: { $0.id == selected.id && $0.contactId == selected.contactId }) ?? selected)
                    }
                }
            }
            .navigationTitle("Messages")
            .platformInlineNavigationTitle()
            .platformChatNavigationBackground()
            .toolbar { MainNavigationToolbar(showSettings: $showSettings) }
            .safeAreaInset(edge: .top, spacing: 0) {
                TopicFilterBar(contactID: nil, selection: $selectedTopicID)
            }
            .safeAreaInset(edge: .bottom, spacing: 0) {
                composer.padding(.bottom, ComposerLayout.bottomAdjustment(for: bottomSafeArea))
            }
            .sheet(isPresented: $showSettings) { AppSettingsView() }
            .sheet(item: $settingsContact) { ConversationSettingsView(contact: $0) }
        }
        .task {
            if !session.feedHasLoaded { await session.loadFeed() }
            await session.loadTopics()
        }
        .task(id: selectedTopicID) {
            if let selectedTopicID { await session.loadTopicMessages(selectedTopicID) }
        }
        .onChange(of: selectedTopicID) { _, _ in cancelReply() }
        .onChange(of: session.topicCatalog.topics) { _, topics in
            if let selectedTopicID, !session.topics().contains(where: { $0.id == selectedTopicID }) { self.selectedTopicID = nil }
        }
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
        let contact = session.contacts.first(where: { $0.id == message.contactId }) ?? Contact(
            id: message.contactId, name: name(message), phone: message.contactPhone,
            type: message.chatType.lowercased(), lastMessageTime: message.timestamp,
            unreadCount: 0, pinnedAt: nil, lastMessagePreview: nil
        )
        return Button {
            session.selectedContactID = message.contactId
            session.mainTab = .chats
        } label: {
            HStack(spacing: 7) {
                ContactAvatar(contact: contact, url: session.avatarURLs[message.contactId], size: 30)
                    .accessibilityHidden(true)
                Text(name(message)).fontWeight(.semibold).lineLimit(1)
                Image(systemName: "chevron.right").font(.system(size: 9, weight: .bold))
                Spacer(minLength: 8)
                Text(message.date, format: .dateTime.day().month(.abbreviated)).foregroundStyle(.secondary)
            }
            .font(.caption)
            .foregroundStyle(palette.deepAccent)
            .padding(.horizontal, 9)
            .padding(.vertical, 5)
            .contentShape(Rectangle())
        }
        .contextMenu {
            Button("Conversation settings", systemImage: "slider.horizontal.3") { settingsContact = contact }
        }
        .buttonStyle(.plain)
        .accessibilityLabel("Open chat: \(name(message))")
        .task { await session.loadAvatar(for: message.contactId) }
    }

    private func select(_ message: ChatMessage) {
        guard !sending else { return }
        replyDraft.select(message)
        composerFocused = true
    }

    private func bubble(_ message: ChatMessage, albumMessages: [ChatMessage] = []) -> some View {
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
            albumMessages: albumMessages, albumImages: session.messageImages,
            albumLoadingIDs: session.mediaLoadingIDs, albumFailedIDs: session.mediaErrorIDs,
            retryAlbumMedia: { message in Task { await session.retryMedia(for: message) } },
            albumReply: select,
            albumAIReply: { photo in
                select(photo)
                Task {
                    if let suggestion = await session.generateAIReply(to: photo), replyDraft.selected?.id == photo.id, !sending {
                        replyDraft.drafts[photo.contactId] = suggestion
                    }
                }
            }
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
            } else if replyDraft.selected == nil && displayedMessages.isEmpty {
                Text("Waiting for messages")
                    .font(.caption).foregroundStyle(.secondary)
            }
            if let progress = session.photoSendProgress.values.max(by: { $0.startedAt < $1.startedAt }) {
                PhotoSendProgressView(progress: progress)
            }
            ComposerGlassGroup {
                HStack(alignment: .bottom, spacing: 7) {
                    UnifiedMediaControls(
                        disabled: sending || session.sendingContactIDs.contains((replyDraft.selected ?? displayedMessages.last)?.contactId ?? "") || (replyDraft.selected == nil && displayedMessages.isEmpty),
                        begin: {
                            guard let target = replyDraft.beginAttachment(latestMessage: displayedMessages.last) else { return nil }
                            composerFocused = false
                            return UnifiedMediaContext(message: target, reply: session.replyTarget(for: target), destination: name(target))
                        },
                        onSent: { target in replyDraft.finishMediaSending(to: target); composerFocused = false }
                    )
                    MessageComposerTextInput(text: draft, focus: $composerFocused, allowsImagePaste: !sending && !imagePaste.isLoading) { providers in
                        guard !sending,
                              let target = replyDraft.beginAttachment(latestMessage: displayedMessages.last),
                              !session.sendingContactIDs.contains(target.contactId) else { return }
                        composerFocused = false
                        let reply = session.replyTarget(for: target)
                        imagePaste.begin(providers, reply: reply, destination: name(target)) { images, caption in
                            guard session.startPhotoSend(images, caption: caption, to: target.contactId, reply: reply, replyOnlyIfNotLatest: true) else { return false }
                            replyDraft.finishMediaSending(to: target)
                            composerFocused = false
                            return true
                        }
                    }
                        .textFieldStyle(.plain)
                        .padding(.horizontal, 16)
                        .padding(.vertical, 12)
                        .frame(minHeight: 46)
                        .translatorGlassControl(in: RoundedRectangle(cornerRadius: 24))
                        .disabled(sending || (replyDraft.selected == nil && displayedMessages.isEmpty))
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
