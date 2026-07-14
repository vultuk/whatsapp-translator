import SwiftUI

struct AppSettingsView: View {
    @Environment(AppSession.self) private var session
    @Environment(\.dismiss) private var dismiss
    @State private var settings = OpenAISettings(model: nil, reasoningEffort: nil, available: nil)
    @State private var isLoading = true
    @State private var isSaving = false
    @State private var error: String?

    private let models = [
        ("", "App defaults"),
        ("gpt-5.6-sol", "GPT-5.6 Sol · most capable"),
        ("gpt-5.6-terra", "GPT-5.6 Terra · balanced"),
        ("gpt-5.6-luna", "GPT-5.6 Luna · fastest value"),
    ]
    private let efforts = [
        ("", "Task default"), ("none", "None"), ("low", "Low"),
        ("medium", "Medium"), ("high", "High"), ("xhigh", "Extra high"), ("max", "Maximum"),
    ]

    var body: some View {
        NavigationStack {
            Form {
                Section("Connection") {
                    LabeledContent("Server", value: session.configuration?.baseURL.host() ?? "Not configured")
                    LabeledContent("WhatsApp", value: session.backendStatus.connected ? "Connected" : "Disconnected")
                }

                Section {
                    if isLoading {
                        HStack { Spacer(); ProgressView("Loading GPT settings…"); Spacer() }
                    }
                    Picker("GPT model", selection: modelBinding) {
                        ForEach(models, id: \.0) { Text($0.1).tag($0.0) }
                    }
                    .disabled(isLoading || isSaving)
                    Picker("Reasoning", selection: effortBinding) {
                        ForEach(efforts, id: \.0) { Text($0.1).tag($0.0) }
                    }
                    .disabled(isLoading || isSaving)
                } header: {
                    Text("OpenAI")
                } footer: {
                    Text("App defaults preserve the backend’s task-specific model and reasoning choices.")
                }

                Section {
                    Picker("Theme", selection: themeBinding) {
                        ForEach(AppTheme.allCases) { theme in Text(theme.title).tag(theme) }
                    }
                    Picker("Appearance", selection: colorModeBinding) {
                        ForEach(AppColorMode.allCases) { mode in Text(mode.title).tag(mode) }
                    }
                    ThemePreview(theme: session.preferences.theme)
                } header: {
                    Text("Appearance")
                } footer: {
                    Text("Themes change the native chat palette. Appearance can follow the system or stay light or dark.")
                }

                Section {
                    Button("Change server", role: .destructive) {
                        dismiss()
                        session.forgetServer()
                    }
                } footer: {
                    Text("This forgets the address and password on this device. It does not log WhatsApp out or delete backend data.")
                }
            }
            .navigationTitle("Settings")
            .platformInlineNavigationTitle()
            .toolbar {
                #if os(macOS)
                ToolbarItem(placement: .primaryAction) {
                    Button("Save GPT") { save() }
                        .fontWeight(.semibold)
                        .disabled(isLoading || isSaving)
                }
                #else
                ToolbarItem(placement: .cancellationAction) { Button("Close") { dismiss() } }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Save GPT") { save() }
                        .fontWeight(.semibold)
                        .disabled(isLoading || isSaving)
                }
                #endif
            }
            .task { await load() }
            .alert("Couldn’t save settings", isPresented: errorPresented) {
                Button("OK") { error = nil }
            } message: { Text(error ?? "Please try again.") }
        }
    }

    private var modelBinding: Binding<String> {
        Binding(get: { settings.model ?? "" }, set: { settings.model = $0.isEmpty ? nil : $0 })
    }

    private var effortBinding: Binding<String> {
        Binding(get: { settings.reasoningEffort ?? "" }, set: { settings.reasoningEffort = $0.isEmpty ? nil : $0 })
    }

    private var themeBinding: Binding<AppTheme> {
        Binding(get: { session.preferences.theme }, set: { session.preferences.theme = $0 })
    }

    private var colorModeBinding: Binding<AppColorMode> {
        Binding(get: { session.preferences.colorMode }, set: { session.preferences.colorMode = $0 })
    }

    private var errorPresented: Binding<Bool> {
        Binding(get: { error != nil }, set: { if !$0 { error = nil } })
    }

    private func load() async {
        do { settings = try await session.openAISettings() }
        catch { self.error = error.localizedDescription }
        isLoading = false
    }

    private func save() {
        isSaving = true
        Task {
            do {
                try await session.saveOpenAISettings(settings)
                dismiss()
            } catch {
                self.error = error.localizedDescription
                isSaving = false
            }
        }
    }
}

private struct ThemePreview: View {
    let theme: AppTheme

    var body: some View {
        let palette = TranslatorPalette.make(theme)
        VStack(spacing: 8) {
            HStack {
                Text("Incoming message")
                    .padding(9)
                    .background(palette.incomingBubble, in: RoundedRectangle(cornerRadius: 12))
                Spacer()
            }
            HStack {
                Spacer()
                Text("Translated reply")
                    .padding(9)
                    .background(palette.outgoingBubble, in: RoundedRectangle(cornerRadius: 12))
            }
        }
        .font(.caption)
        .padding(12)
        .background(palette.chatBackground, in: RoundedRectangle(cornerRadius: 14))
        .animation(.snappy, value: theme)
    }
}
