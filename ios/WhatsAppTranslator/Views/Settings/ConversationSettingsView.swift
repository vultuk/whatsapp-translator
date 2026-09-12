import SwiftUI

struct ConversationSettingsView: View {
    @Environment(AppSession.self) private var session
    @Environment(\.dismiss) private var dismiss
    let contact: Contact
    @State private var settings = ConversationSettings(languageOverride: nil, translationStyle: nil, sendOriginalFollowUp: false)
    @State private var presentation = ConversationPresentationPreferences.empty
    @State private var isLoading = true
    @State private var hasLoaded = false
    @State private var isSaving = false
    @State private var error: String?
    @State private var showTimezonePicker = ProcessInfo.processInfo.arguments.contains("-demoTimezonePicker")

    var body: some View {
        NavigationStack {
            Form {
                translationSection
                Section("Notifications") { MessageToneSettingsRow(contactID: contact.id) }
                if settings.translationEnabled { VoicePreferenceSection(scope: contact.id) }

                Section {
                    LabeledContent("Nickname") {
                        TextField(
                            "",
                            text: presentationBinding(\.nickname),
                            prompt: Text("Optional")
                        )
                        .multilineTextAlignment(.trailing)
                        .accessibilityLabel("Nickname")
                    }
                    Button {
                        showTimezonePicker = true
                    } label: {
                        LabeledContent {
                            VStack(alignment: .trailing, spacing: 1) {
                                Text(presentation.timezoneIdentifier.map(TimezonePickerView.friendlyName) ?? "Not set")
                                if let identifier = presentation.timezoneIdentifier {
                                    Text(TimezonePickerView.currentTime(in: identifier)).font(.caption2)
                                }
                            }
                            .foregroundStyle(.secondary)
                            Image(systemName: "chevron.right").font(.caption).foregroundStyle(.tertiary)
                        } label: {
                            Text("Contact timezone")
                                .foregroundStyle(.primary)
                        }
                    }
                    .buttonStyle(.plain)
                } header: {
                    Text("Contact")
                } footer: {
                    Text("Nickname and timezone stay on this device. The timezone shows the contact’s current local time in the chat header.")
                        .fixedSize(horizontal: false, vertical: true)
                }

            }
            .platformGroupedFormStyle()
            .platformCompactControlTypography()
            .navigationTitle(session.displayName(for: contact))
            .platformInlineNavigationTitle()
            .disabled(!hasLoaded || isLoading || isSaving)
            .overlay { if isLoading { ProgressView() } }
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }.disabled(isSaving)
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Save") { save() }
                        .fontWeight(.semibold)
                        .disabled(!hasLoaded || isLoading || isSaving)
                }
            }
            .task { await load() }
            .interactiveDismissDisabled(isSaving)
            .alert("Couldn’t save settings", isPresented: errorPresented) {
                Button("OK") { error = nil }
            } message: { Text(error ?? "Please try again.") }
            .sheet(isPresented: $showTimezonePicker) {
                TimezonePickerView(selection: $presentation.timezoneIdentifier)
            }
        }
        #if os(macOS)
        .platformSheetSize(
            minWidth: MacChatLayoutMetrics.settingsSheetMinimumWidth,
            minHeight: MacChatLayoutMetrics.settingsSheetMinimumHeight
        )
        #endif
    }

    private var translationSection: some View {
        Section {
            Toggle("Translate messages", isOn: $settings.translationEnabled)
                .accessibilityIdentifier("conversation-translation-enabled")
            if settings.translationEnabled {
                LabeledContent("Language") {
                    TextField("", text: optionalBinding(\.languageOverride), prompt: Text("Automatic"))
                        .platformWordsInput()
                        .multilineTextAlignment(.trailing)
                        .accessibilityLabel("Language")
                }
                LabeledContent("Style") {
                    TextField("", text: optionalBinding(\.translationStyle), prompt: Text("Optional"))
                        .multilineTextAlignment(.trailing)
                        .accessibilityLabel("Style")
                }
                Toggle("Send original after translation", isOn: $settings.sendOriginalFollowUp)
            }
        } header: {
            Text("Translation")
        } footer: {
            Text(settings.translationEnabled
                 ? "Translates incoming and outgoing messages, captions and voice notes. If selected above, your original follows the translation. This also applies in Messages on every connected device."
                 : "Messages send as written and voice notes use your original recording, without waiting for translation. Off by default for every person and group, including in Messages.")
                .fixedSize(horizontal: false, vertical: true)
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
        do {
            settings = try await session.conversationSettings(for: contact.id)
            hasLoaded = true
        }
        catch { self.error = error.localizedDescription }
        isLoading = false
    }

    private func save() {
        guard hasLoaded, !isLoading, !isSaving else { return }
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
