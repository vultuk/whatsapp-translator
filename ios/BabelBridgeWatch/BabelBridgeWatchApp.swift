import SwiftUI

@main
struct BabelBridgeWatchApp: App {
    @State private var session = WatchSession()
    @Environment(\.scenePhase) private var scenePhase

    var body: some Scene {
        WindowGroup {
            WatchMessagesView(session: session)
                .tint(.mint)
                .task(id: scenePhase) {
                    guard scenePhase == .active else { return }
                    while !Task.isCancelled {
                        await session.refresh()
                        do { try await Task.sleep(for: .seconds(15)) }
                        catch { return }
                    }
                }
        }
    }
}

private struct WatchMessagesView: View {
    @Bindable var session: WatchSession

    var body: some View {
        NavigationStack {
            List {
                if let error = session.error {
                    Text(error).font(.footnote).foregroundStyle(.secondary)
                }
                if let snapshot = session.snapshot {
                    if snapshot.messages.isEmpty {
                        Text("Your WhatsApp messages will appear here.").foregroundStyle(.secondary)
                    }
                    if let latest = snapshot.messages.last {
                        Button { session.select(latest) } label: {
                            Label("Reply to latest", systemImage: "arrowshape.turn.up.left.fill")
                        }
                        .accessibilityIdentifier("watch-reply-latest")
                    }
                    ForEach(snapshot.messages.reversed()) { message in
                        Button { session.select(message) } label: {
                            VStack(alignment: .leading, spacing: 5) {
                                HStack(alignment: .firstTextBaseline) {
                                    Text(message.conversation).font(.headline).foregroundStyle(.mint)
                                    Spacer(minLength: 2)
                                    if message.isGroup { Image(systemName: "person.2.fill").font(.caption2).foregroundStyle(.secondary) }
                                }
                                if message.isGroup || message.isFromMe {
                                    Text(message.sender).font(.caption2).foregroundStyle(.secondary)
                                }
                                Text(message.text).font(.body).foregroundStyle(.primary)
                                Text(message.date, style: .time).font(.caption2).foregroundStyle(.secondary)
                            }
                            .padding(.vertical, 4)
                        }
                        .buttonStyle(.plain)
                        .accessibilityIdentifier("watch-message-" + message.messageID)
                        .swipeActions(edge: .trailing, allowsFullSwipe: true) {
                            Button { session.select(message) } label: { Label("Reply", systemImage: "arrowshape.turn.up.left") }
                                .tint(.teal)
                        }
                    }
                    Text("Updated \(snapshot.updatedAt, style: .time)")
                        .font(.caption2).foregroundStyle(.secondary)
                } else if session.loading {
                    ProgressView("Loading Messages…")
                } else if session.error == nil {
                    Text("Connecting to your iPhone…").foregroundStyle(.secondary)
                }
                Button { Task { await session.refresh() } } label: { Label("Refresh", systemImage: "arrow.clockwise") }
                    .disabled(session.loading)
            }
            .navigationTitle("Messages")
            .sheet(isPresented: Binding(get: { session.draft != nil }, set: { if !$0 && !session.sending { session.draft = nil } })) {
                WatchReplyView(session: session)
            }
            .alert("Message sent", isPresented: $session.sent) { Button("OK", role: .cancel) {} }
        }
    }
}

private struct WatchReplyView: View {
    @Bindable var session: WatchSession

    var body: some View {
        if let draft = session.draft {
            ScrollView {
                VStack(alignment: .leading, spacing: 12) {
                    Text("Reply to \(draft.target.conversation)").font(.headline).foregroundStyle(.mint)
                    if draft.target.isGroup { Text(draft.target.sender).font(.caption).foregroundStyle(.secondary) }
                    Text(draft.target.text).font(.caption).foregroundStyle(.secondary).lineLimit(3)
                    TextField("Message", text: Binding(get: { session.draft?.text ?? "" }, set: { session.draft?.text = $0 }))
                        .accessibilityIdentifier("watch-reply-text")
                        .disabled(draft.submitted != nil || session.sending)
                    if let error = session.replyError { Text(error).font(.footnote).foregroundStyle(.orange) }
                    Button {
                        Task { await session.send() }
                    } label: {
                        if session.sending { ProgressView() }
                        else { Label(draft.submitted == nil ? "Send" : "Check delivery", systemImage: draft.submitted == nil ? "paperplane.fill" : "arrow.clockwise") }
                    }
                    .accessibilityIdentifier("watch-send")
                    .disabled(session.sending || draft.text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                    Button("Cancel", role: .cancel) { session.draft = nil }
                        .disabled(session.sending)
                }.padding(.horizontal, 4)
            }
            .interactiveDismissDisabled(session.sending)
        }
    }
}
