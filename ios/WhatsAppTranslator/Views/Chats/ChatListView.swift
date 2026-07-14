import SwiftUI

struct ChatListView: View {
    @Environment(AppSession.self) private var session
    @Environment(\.translatorPalette) private var palette
    @State private var showSettings = ProcessInfo.processInfo.arguments.contains("-demoSettings")

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
                ToolbarItem(placement: platformTrailingToolbarPlacement) {
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
    private func sidebarContent(
        selection: Binding<String?>,
        searchText: Binding<String>
    ) -> some View {
        #if os(macOS)
        VStack(spacing: 0) {
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

            Divider()
            contactList(selection: selection)
        }
        #else
        contactList(selection: selection)
            .searchable(text: searchText, prompt: "Search chats")
            .refreshable { await session.refresh() }
        #endif
    }

    private func contactList(selection: Binding<String?>) -> some View {
        List(selection: selection) {
            ForEach(session.filteredContacts) { contact in
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
                .listRowBackground(Color.clear)
            }
        }
        .listStyle(.plain)
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
