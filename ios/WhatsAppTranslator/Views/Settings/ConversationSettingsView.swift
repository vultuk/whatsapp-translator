import SwiftUI

struct ConversationSettingsView: View {
    @Environment(AppSession.self) private var session
    @Environment(\.dismiss) private var dismiss
    let contact: Contact
    @State private var settings = ConversationSettings(languageOverride: nil, translationStyle: nil, sendOriginalFollowUp: false)
    @State private var presentation = ConversationPresentationPreferences.empty
    @State private var isLoading = true
    @State private var isSaving = false
    @State private var error: String?
    @State private var showTimezonePicker = ProcessInfo.processInfo.arguments.contains("-demoTimezonePicker")

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    TextField("Nickname", text: presentationBinding(\.nickname))
                    Button {
                        showTimezonePicker = true
                    } label: {
                        HStack {
                            Text("Contact timezone").foregroundStyle(.primary)
                            Spacer()
                            VStack(alignment: .trailing, spacing: 1) {
                                Text(presentation.timezoneIdentifier.map(TimezonePickerView.friendlyName) ?? "Not set")
                                if let identifier = presentation.timezoneIdentifier {
                                    Text(TimezonePickerView.currentTime(in: identifier)).font(.caption2)
                                }
                            }
                            .foregroundStyle(.secondary)
                            Image(systemName: "chevron.right").font(.caption).foregroundStyle(.tertiary)
                        }
                    }
                } header: {
                    Text("Contact")
                } footer: {
                    Text("Nickname and timezone stay on this device. The timezone shows the contact’s current local time in the chat header.")
                }

                Section {
                    TextField("Conversation language", text: optionalBinding(\.languageOverride))
                        .platformWordsInput()
                    TextField("Style, e.g. friendly or formal", text: optionalBinding(\.translationStyle))
                    Toggle("Send original after translation", isOn: $settings.sendOriginalFollowUp)
                } header: {
                    Text("Translation")
                } footer: {
                    Text("When enabled, the translated message sends first, followed immediately by what you originally typed.")
                }
            }
            .navigationTitle(session.displayName(for: contact))
            .platformInlineNavigationTitle()
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
            .sheet(isPresented: $showTimezonePicker) {
                TimezonePickerView(selection: $presentation.timezoneIdentifier)
            }
        }
    }

    private func optionalBinding(_ keyPath: WritableKeyPath<ConversationSettings, String?>) -> Binding<String> {
        Binding(
            get: { settings[keyPath: keyPath] ?? "" },
            set: { settings[keyPath: keyPath] = $0.isEmpty ? nil : $0 }
        )
    }

    private func presentationBinding(_ keyPath: WritableKeyPath<ConversationPresentationPreferences, String?>) -> Binding<String> {
        Binding(
            get: { presentation[keyPath: keyPath] ?? "" },
            set: { presentation[keyPath: keyPath] = $0.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ? nil : $0 }
        )
    }

    private var errorPresented: Binding<Bool> {
        Binding(get: { error != nil }, set: { if !$0 { error = nil } })
    }

    private func load() async {
        presentation = session.preferences.conversationPreferences(for: contact.id)
        do { settings = try await session.conversationSettings(for: contact.id) }
        catch { self.error = error.localizedDescription }
        isLoading = false
    }

    private func save() {
        if let identifier = presentation.timezoneIdentifier, TimeZone(identifier: identifier) == nil {
            error = "Enter a timezone such as Europe/London or Europe/Budapest."
            return
        }
        isSaving = true
        Task {
            session.preferences.setConversationPreferences(presentation, for: contact.id)
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
