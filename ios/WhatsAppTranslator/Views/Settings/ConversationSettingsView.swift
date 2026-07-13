import SwiftUI

struct ConversationSettingsView: View {
    @Environment(AppSession.self) private var session
    @Environment(\.dismiss) private var dismiss
    let contact: Contact
    @State private var settings = ConversationSettings(languageOverride: nil, translationStyle: nil, sendOriginalFollowUp: false)
    @State private var isLoading = true
    @State private var isSaving = false
    @State private var error: String?

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    TextField("Conversation language", text: optionalBinding(\.languageOverride))
                        .textInputAutocapitalization(.words)
                    TextField("Style, e.g. friendly or formal", text: optionalBinding(\.translationStyle))
                    Toggle("Send original after translation", isOn: $settings.sendOriginalFollowUp)
                } header: {
                    Text("Translation")
                } footer: {
                    Text("When enabled, the translated message sends first, followed immediately by what you originally typed.")
                }
            }
            .navigationTitle(contact.displayName)
            .navigationBarTitleDisplayMode(.inline)
            .disabled(isLoading || isSaving)
            .overlay { if isLoading { ProgressView() } }
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Save") { save() }
                        .fontWeight(.semibold)
                }
            }
            .task { await load() }
            .alert("Couldn’t save settings", isPresented: errorPresented) {
                Button("OK") { error = nil }
            } message: { Text(error ?? "Please try again.") }
        }
    }

    private func optionalBinding(_ keyPath: WritableKeyPath<ConversationSettings, String?>) -> Binding<String> {
        Binding(
            get: { settings[keyPath: keyPath] ?? "" },
            set: { settings[keyPath: keyPath] = $0.isEmpty ? nil : $0 }
        )
    }

    private var errorPresented: Binding<Bool> {
        Binding(get: { error != nil }, set: { if !$0 { error = nil } })
    }

    private func load() async {
        do { settings = try await session.conversationSettings(for: contact.id) }
        catch { self.error = error.localizedDescription }
        isLoading = false
    }

    private func save() {
        isSaving = true
        Task {
            do {
                try await session.saveConversationSettings(settings, for: contact.id)
                dismiss()
            } catch {
                self.error = error.localizedDescription
                isSaving = false
            }
        }
    }
}
