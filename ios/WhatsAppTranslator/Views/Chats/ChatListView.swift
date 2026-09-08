import SwiftUI

struct ChatListView: View {
    @Environment(AppSession.self) private var session
    @Environment(\.translatorPalette) private var palette
    @State private var showSettings = ProcessInfo.processInfo.arguments.contains("-demoSettings")

    @State private var filter: ChatFilter = .all

    var body: some View {
        @Bindable var session = session

        NavigationSplitView {
            sidebarContent(selection: $session.selectedContactID, searchText: $session.searchText)
            #if os(macOS)
            .navigationSplitViewColumnWidth(
                min: MacChatLayoutMetrics.minimumSidebarWidth,
                ideal: MacChatLayoutMetrics.idealSidebarWidth,
                max: MacChatLayoutMetrics.maximumSidebarWidth
            )
            #endif
            .navigationTitle("Chats")
            .toolbar {
                #if os(macOS)
                ToolbarItemGroup(placement: .primaryAction) {
                    updatesToolbarButton
                    Button("Refresh chats", systemImage: "arrow.clockwise") {
                        Task { await session.refresh() }
                    }
                    .keyboardShortcut("r", modifiers: .command)
                    .labelStyle(.iconOnly)
                    .help("Refresh chats")
                    SettingsLink {
                        Label("Settings", systemImage: "gearshape")
                    }
                    .labelStyle(.iconOnly)
                    .help("Settings")
                }
                #else
                ToolbarItemGroup(placement: platformTrailingToolbarPlacement) {
                    updatesToolbarButton
                    Button("Settings", systemImage: "gearshape") { showSettings = true }
                }
                #endif
            }
            .overlay {
                if session.contacts.isEmpty {
                    ContentUnavailableView(
                        "No chats yet",
                        systemImage: "message",
                        description: Text("Conversations will appear when your translator receives them.")
                    )
                }
            }
        } detail: {
            if let id = session.selectedContactID,
               let contact = session.contacts.first(where: { $0.id == id }) {
                ConversationView(contact: contact)
                    .id(id)
            } else {
                EmptyConversationView()
            }
        }
        .navigationSplitViewStyle(.balanced)
        .sheet(isPresented: $showSettings) {
            AppSettingsView()
        }
    }

    @ViewBuilder
    private var updatesToolbarButton: some View {
        if let updates = session.updatesContact {
            Button {
                session.selectedContactID = updates.id
            } label: {
                UpdatesToolbarIcon(unreadCount: updates.unreadCount)
            }
            .accessibilityLabel("Updates")
            .accessibilityValue(
                updates.unreadCount == 0
                    ? "No unread updates"
                    : "\(updates.unreadCount) unread"
            )
            .help("Updates")
        }
    }

    @ViewBuilder
    private func sidebarContent(
        selection: Binding<String?>,
        searchText: Binding<String>
    ) -> some View {
        #if os(macOS)
        VStack(spacing: 0) {
            HStack {
                Text("Chats").font(.largeTitle.bold())
                Spacer()
            }
            .padding(.horizontal, 18)
            .padding(.top, 16)
            .padding(.bottom, 4)
            HStack(spacing: 8) {
                Image(systemName: "magnifyingglass")
                    .foregroundStyle(.secondary)
                TextField("Search chats", text: searchText)
                    .textFieldStyle(.plain)
                if !searchText.wrappedValue.isEmpty {
                    Button("Clear search", systemImage: "xmark.circle.fill") {
                        searchText.wrappedValue = ""
                    }
                    .labelStyle(.iconOnly)
                    .buttonStyle(.plain)
                    .foregroundStyle(.secondary)
                }
            }
            .padding(.horizontal, 11)
            .padding(.vertical, 8)
            .background(.primary.opacity(0.055), in: RoundedRectangle(cornerRadius: 9))
            .padding(.horizontal, 10)
            .padding(.vertical, 9)

            filterBar
            contactList(selection: selection)
        }
        #else
        VStack(spacing: 0) {
            filterBar
            contactList(selection: selection)
        }
            .searchable(text: searchText, prompt: "Search chats")
            .refreshable { await session.refresh() }
        #endif
    }

    private var filterBar: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 8) {
                ForEach(ChatFilter.allCases, id: \.self) { item in
                    Button { filter = item } label: {
                        Text(item.rawValue)
                            .font(.subheadline.weight(.semibold))
                            .padding(.horizontal, 16)
                            .padding(.vertical, 8)
                            .foregroundStyle(filter == item ? palette.deepAccent : Color.secondary)
                            .background(filter == item ? palette.accent.opacity(0.14) : Color.primary.opacity(0.045), in: Capsule())
                            .frame(minHeight: 44)
                    }
                    .buttonStyle(.plain)
                    .accessibilityAddTraits(filter == item ? .isSelected : [])
                }
            }
            .padding(.horizontal, 14)
        }
        .padding(.bottom, 6)
    }

    private func contactList(selection: Binding<String?>) -> some View {
        List(selection: selection) {
            ForEach(session.filteredContacts.filter { filter.includes($0) }) { contact in
                NavigationLink(value: contact.id) {
                    ChatRow(
                        contact: contact,
                        displayName: session.displayName(for: contact),
                        draft: session.draftStore.text(for: contact.id),
                        avatarURL: session.avatarURLs[contact.id]
                    )
                }
                .swipeActions(edge: .leading, allowsFullSwipe: false) {
                    Button {
                        Task { await session.togglePin(contact) }
                    } label: {
                        Label(
                            contact.pinnedAt == nil ? "Pin" : "Unpin",
                            systemImage: contact.pinnedAt == nil ? "pin" : "pin.slash"
                        )
                    }
                    .tint(palette.accent)
                }
                .contextMenu {
                    Button(
                        contact.pinnedAt == nil ? "Pin conversation" : "Unpin conversation",
                        systemImage: contact.pinnedAt == nil ? "pin" : "pin.slash"
                    ) {
                        Task { await session.togglePin(contact) }
                    }
                }
                .task { await session.loadAvatar(for: contact.id) }
                .listRowSeparator(.hidden)
                .listRowBackground(
                    session.selectedContactID == contact.id
                        ? palette.accent.opacity(0.14)
                        : Color.clear
                )
            }
        }
        .listStyle(.plain)
        .overlay {
            if !session.contacts.isEmpty && session.filteredContacts.filter({ filter.includes($0) }).isEmpty {
                ContentUnavailableView("No matching chats", systemImage: "bubble.left.and.bubble.right", description: Text("Try another filter or search."))
            }
        }
    }
}

private struct UpdatesToolbarIcon: View {
    @Environment(\.translatorPalette) private var palette
    let unreadCount: Int

    var body: some View {
        ZStack(alignment: .topTrailing) {
            Image(systemName: unreadCount > 0 ? "bell.fill" : "bell")
                .frame(width: 28, height: 28)

            if unreadCount > 0 {
                Text(unreadCount > 99 ? "99+" : "\(unreadCount)")
                    .font(.system(size: 9, weight: .bold, design: .rounded))
                    .foregroundStyle(.white)
                    .padding(.horizontal, 4)
                    .frame(minWidth: 16, minHeight: 16)
                    .background(palette.accent, in: Capsule())
                    .offset(x: 7, y: -5)
            }
        }
        .padding(.trailing, unreadCount > 0 ? 7 : 0)
    }
}

private struct EmptyConversationView: View {
    @Environment(\.translatorPalette) private var palette

    var body: some View {
        ZStack {
            palette.chatBackground.ignoresSafeArea()
            VStack(spacing: 18) {
                TranslatorMark(size: 78)
                Text("Choose a chat")
                    .font(.largeTitle.bold())
                Text("Incoming messages translate automatically.\nYour replies send in the conversation’s language.")
                    .multilineTextAlignment(.center)
                    .foregroundStyle(.secondary)
            }
            .padding(32)
        }
    }
}

enum ChatFilter: String, CaseIterable {
    case all = "All"
    case unread = "Unread"
    case groups = "Groups"

    func includes(_ contact: Contact) -> Bool {
        switch self {
        case .all: true
        case .unread: contact.unreadCount > 0
        case .groups: contact.isGroup
        }
    }
}
