import SwiftUI

struct MessageToneSettingsRow: View {
    @Environment(AppSession.self) private var session
    var contactID: String? = nil
    @State private var settings: MessageToneSettings?
    @State private var showingPicker = false

    var body: some View {
        Button { showingPicker = true } label: {
            HStack(spacing: 12) {
                Label("Message ringtone", systemImage: "bell.badge")
                    .foregroundStyle(.primary)
                Spacer(minLength: 8)
                Text(summary).foregroundStyle(.secondary).multilineTextAlignment(.trailing)
                Image(systemName: "chevron.right").font(.caption).foregroundStyle(.tertiary)
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier(contactID == nil ? "global-message-ringtone" : "conversation-message-ringtone")
        .sheet(isPresented: $showingPicker) {
            MessageTonePickerView(contactID: contactID) { settings = $0 }
        }
        .task(id: session.messageToneRevision) {
            settings = try? await session.messageToneSettings(contactID: contactID)
        }
    }

    private var summary: String {
        guard let settings else { return "Choose…" }
        if contactID != nil && settings.tone == nil { return "Global · \(settings.globalTone.title)" }
        return settings.effectiveTone.title
    }
}

struct MessageTonePickerView: View {
    @Environment(AppSession.self) private var session
    @Environment(\.dismiss) private var dismiss
    let contactID: String?
    var onSave: (MessageToneSettings) -> Void = { _ in }
    @State private var settings: MessageToneSettings?
    @State private var selection: MessageTone?
    @State private var loading = true
    @State private var saving = false
    @State private var error: String?
    @State private var preview = MessageTonePreviewPlayer()

    var body: some View {
        NavigationStack {
            Form {
                if let settings {
                    if contactID != nil {
                        Section {
                            toneRow(nil, title: "Use global ringtone", detail: "Currently \(settings.globalTone.title)", symbol: "arrow.triangle.branch")
                        } footer: {
                            Text("Follows your global choice whenever it changes.")
                        }
                    }
                    Section {
                        ForEach(MessageTone.customTones) { tone in
                            toneRow(tone, title: tone.title, detail: tone.detail, symbol: tone.symbol)
                        }
                    } header: {
                        Text("Babel Bridge tones")
                    } footer: {
                        Text("Tap a tone to listen. Your choice applies on every device connected to this server.")
                    }
                    Section {
                        ForEach([MessageTone.default, .silent]) { tone in
                            toneRow(tone, title: tone.title, detail: tone.detail, symbol: tone.symbol)
                        }
                    }
                }
                if let error {
                    Section {
                        Text(error).foregroundStyle(.red)
                        if settings == nil { Button("Try again") { Task { await load() } } }
                    }
                }
            }
            .platformGroupedFormStyle()
            .navigationTitle(contactID == nil ? "Global ringtone" : "Conversation ringtone")
            .platformInlineNavigationTitle()
            .disabled(loading || saving)
            .overlay { if loading { ProgressView("Loading ringtones…") } }
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }.disabled(saving)
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button(saving ? "Saving…" : "Save") { save() }
                        .fontWeight(.semibold)
                        .disabled(loading || saving || settings == nil || selection == settings?.tone)
                }
            }
            .task { await load() }
            .onDisappear { preview.stop() }
            .interactiveDismissDisabled(saving)
        }
        #if os(macOS)
        .frame(minWidth: 480, idealWidth: 520, minHeight: 570, idealHeight: 650)
        #endif
    }

    private func toneRow(_ tone: MessageTone?, title: String, detail: String, symbol: String) -> some View {
        HStack(spacing: 10) {
            Button {
                selection = tone
                play(tone ?? settings?.globalTone ?? .default)
            } label: {
                HStack(spacing: 12) {
                    Image(systemName: symbol)
                        .font(.title3)
                        .foregroundStyle(.tint)
                        .frame(width: 34, height: 38)
                    VStack(alignment: .leading, spacing: 3) {
                        Text(title).font(.body.weight(.medium)).foregroundStyle(.primary)
                        Text(detail).font(.caption).foregroundStyle(.secondary)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                    Spacer(minLength: 0)
                    Image(systemName: selection == tone ? "checkmark.circle.fill" : "circle")
                        .foregroundStyle(selection == tone ? Color.accentColor : Color.secondary.opacity(0.35))
                }
                .frame(minHeight: 50)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel(title)
            .accessibilityValue(selection == tone ? "Selected" : "Not selected")
            .accessibilityIdentifier("message-tone-\(tone?.rawValue ?? "global")")
            let effectiveTone = tone ?? settings?.globalTone ?? .default
            if effectiveTone.filename != nil {
                Button {
                    if preview.playingTone == effectiveTone { preview.stop() }
                    else { play(effectiveTone) }
                } label: {
                    Image(systemName: preview.playingTone == effectiveTone ? "stop.fill" : "play.fill")
                        .frame(width: 40, height: 44)
                }
                .buttonStyle(.borderless)
                .accessibilityLabel("\(preview.playingTone == effectiveTone ? "Stop" : "Preview") \(title)")
            }
        }
    }

    private func play(_ tone: MessageTone) {
        do { try preview.play(tone); error = nil }
        catch { self.error = error.localizedDescription }
    }

    private func load() async {
        loading = true
        defer { loading = false }
        do {
            let result = try await session.messageToneSettings(contactID: contactID)
            settings = result
            selection = result.tone
            error = nil
        } catch { self.error = error.localizedDescription }
    }

    private func save() {
        preview.stop()
        saving = true
        Task {
            defer { saving = false }
            do {
                let saved = try await session.saveMessageTone(selection, contactID: contactID)
                onSave(saved)
                dismiss()
            } catch { self.error = error.localizedDescription }
        }
    }
}
