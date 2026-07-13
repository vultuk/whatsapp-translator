import SwiftUI

struct ChatListView: View {
    @Environment(AppSession.self) private var session
    @State private var showSettings = false

    var body: some View {
        @Bindable var session = session

        NavigationSplitView {
            List(selection: $session.selectedContactID) {
                ForEach(session.filteredContacts) { contact in
                    NavigationLink(value: contact.id) {
                        ChatRow(
                            contact: contact,
                            draft: session.draftStore.text(for: contact.id),
                            avatarURL: session.avatarURLs[contact.id]
                        )
                    }
                    .task { await session.loadAvatar(for: contact.id) }
                    .listRowSeparator(.hidden)
                    .listRowBackground(Color.clear)
                }
            }
            .listStyle(.plain)
            .navigationTitle("Chats")
            .searchable(text: $session.searchText, prompt: "Search chats")
            .refreshable { await session.refresh() }
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    Button("Settings", systemImage: "gearshape") { showSettings = true }
                }
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
}

private struct EmptyConversationView: View {
    var body: some View {
        ZStack {
            TranslatorTheme.chatBackground.ignoresSafeArea()
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
