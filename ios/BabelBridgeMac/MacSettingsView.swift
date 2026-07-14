import SwiftUI

struct MacSettingsView: View {
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
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                settingsHeader
                connectionSection
                openAISection
                appearanceSection
                serverSection
            }
            .padding(24)
        }
        .frame(width: 560, height: 620)
        .task { await load() }
        .alert("Couldn’t save settings", isPresented: errorPresented) {
            Button("OK") { error = nil }
        } message: {
            Text(error ?? "Please try again.")
        }
    }

    private var settingsHeader: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text("Babel Bridge Settings")
                .font(.title2.bold())
            Text("Connection, translation intelligence and appearance")
                .foregroundStyle(.secondary)
        }
    }

    private var connectionSection: some View {
        SettingsGroup(title: "Connection", systemImage: "network") {
            Grid(alignment: .leading, horizontalSpacing: 16, verticalSpacing: 10) {
                settingsRow("Server", session.configuration?.baseURL.host() ?? "Not configured")
                settingsRow("WhatsApp", session.backendStatus.connected ? "Connected" : "Disconnected")
            }
        }
    }

    private var openAISection: some View {
        SettingsGroup(title: "OpenAI", systemImage: "sparkles") {
            if isLoading {
                ProgressView("Loading GPT settings…")
                    .controlSize(.small)
            } else {
                Grid(alignment: .leading, horizontalSpacing: 16, verticalSpacing: 12) {
                    GridRow {
                        Text("GPT model")
                            .foregroundStyle(.secondary)
                        Picker("GPT model", selection: modelBinding) {
                            ForEach(models, id: \.0) { Text($0.1).tag($0.0) }
                        }
                        .labelsHidden()
                        .frame(maxWidth: .infinity, alignment: .leading)
                    }
                    GridRow {
                        Text("Reasoning")
                            .foregroundStyle(.secondary)
                        Picker("Reasoning", selection: effortBinding) {
                            ForEach(efforts, id: \.0) { Text($0.1).tag($0.0) }
                        }
                        .labelsHidden()
                        .frame(maxWidth: .infinity, alignment: .leading)
                    }
                }
                .disabled(isSaving)

                Text("App defaults preserve the backend’s task-specific model and reasoning choices.")
                    .font(.caption)
                    .foregroundStyle(.secondary)

                HStack {
                    Spacer()
                    Button {
                        save()
                    } label: {
                        if isSaving {
                            ProgressView()
                                .controlSize(.small)
                        } else {
                            Text("Save GPT Settings")
                        }
                    }
                    .buttonStyle(.borderedProminent)
                    .disabled(isLoading || isSaving)
                }
            }
        }
    }

    private var appearanceSection: some View {
        SettingsGroup(title: "Appearance", systemImage: "paintpalette") {
            Grid(alignment: .leading, horizontalSpacing: 16, verticalSpacing: 12) {
                GridRow {
                    Text("Theme")
                        .foregroundStyle(.secondary)
                    Picker("Theme", selection: themeBinding) {
                        ForEach(AppTheme.allCases) { theme in Text(theme.title).tag(theme) }
                    }
                    .labelsHidden()
                    .frame(maxWidth: .infinity, alignment: .leading)
                }
                GridRow {
                    Text("Appearance")
                        .foregroundStyle(.secondary)
                    Picker("Appearance", selection: colorModeBinding) {
                        ForEach(AppColorMode.allCases) { mode in Text(mode.title).tag(mode) }
                    }
                    .labelsHidden()
                    .frame(maxWidth: .infinity, alignment: .leading)
                }
            }

            MacThemePreview(theme: session.preferences.theme)
        }
    }

    private var serverSection: some View {
        SettingsGroup(title: "Server", systemImage: "server.rack") {
            Text("Changing server forgets the saved address and password on this Mac. It does not log WhatsApp out or delete backend data.")
                .font(.caption)
                .foregroundStyle(.secondary)
            Button("Change Server…", role: .destructive) {
                dismiss()
                session.forgetServer()
            }
        }
    }

    private func settingsRow(_ label: String, _ value: String) -> some View {
        GridRow {
            Text(label)
                .foregroundStyle(.secondary)
            Text(value)
                .textSelection(.enabled)
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
        do {
            settings = try await session.openAISettings()
        } catch {
            self.error = error.localizedDescription
        }
        isLoading = false
    }

    private func save() {
        isSaving = true
        Task {
            do {
                try await session.saveOpenAISettings(settings)
                isSaving = false
            } catch {
                self.error = error.localizedDescription
                isSaving = false
            }
        }
    }
}

private struct SettingsGroup<Content: View>: View {
    let title: String
    let systemImage: String
    @ViewBuilder let content: Content

    var body: some View {
        GroupBox {
            VStack(alignment: .leading, spacing: 14) {
                content
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.vertical, 4)
        } label: {
            Label(title, systemImage: systemImage)
                .font(.headline)
        }
    }
}

private struct MacThemePreview: View {
    let theme: AppTheme

    var body: some View {
        let palette = TranslatorPalette.make(theme)
        VStack(spacing: 8) {
            HStack {
                Text("Incoming message")
                    .padding(8)
                    .background(palette.incomingBubble, in: RoundedRectangle(cornerRadius: 10))
                Spacer()
            }
            HStack {
                Spacer()
                Text("Translated reply")
                    .padding(8)
                    .background(palette.outgoingBubble, in: RoundedRectangle(cornerRadius: 10))
            }
        }
        .font(.caption)
        .padding(12)
        .background(palette.chatBackground, in: RoundedRectangle(cornerRadius: 12))
        .animation(.snappy, value: theme)
    }
}
