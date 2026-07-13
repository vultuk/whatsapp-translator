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
                    Picker("GPT model", selection: modelBinding) {
                        ForEach(models, id: \.0) { Text($0.1).tag($0.0) }
                    }
                    Picker("Reasoning", selection: effortBinding) {
                        ForEach(efforts, id: \.0) { Text($0.1).tag($0.0) }
                    }
                } header: {
                    Text("OpenAI")
                } footer: {
                    Text("App defaults preserve the backend’s task-specific model and reasoning choices.")
                }

                Section {
                    Button("Change server", role: .destructive) {
                        dismiss()
                        session.forgetServer()
                    }
                } footer: {
                    Text("This forgets the address and password on this iPhone. It does not log WhatsApp out or delete backend data.")
                }
            }
            .navigationTitle("Settings")
            .navigationBarTitleDisplayMode(.inline)
            .disabled(isLoading || isSaving)
            .overlay { if isLoading { ProgressView() } }
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Save") { save() }.fontWeight(.semibold)
                }
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
