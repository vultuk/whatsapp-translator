import SwiftUI

@MainActor
enum MessageReactionChoices {
    static let quick = ["👍", "❤️", "😂", "😮", "🙏"]
    static let all = quick + ["😢", "✅", "🥰", "😍", "😊", "😁", "😅", "🤣", "😎", "🤔", "🙌", "👏", "🔥", "🎉", "💯", "💪", "🤝", "😴", "👀", "🤗", "😇", "😬", "🤷", "💔", "✨", "🥳", "😡", "😭", "👋", "🫶", "🤞", "👌", "🚀", "🎂", "☕", "🌷"]
    private static var icons: [String: Image] = [:]

    static func icon(_ emoji: String) -> Image {
        if let cached = icons[emoji] { return cached }
        let renderer = ImageRenderer(content: Text(emoji).font(.system(size: 28)).frame(width: 36, height: 36))
        renderer.scale = 3
        #if os(iOS)
        let image = renderer.uiImage.map { Image(uiImage: $0).renderingMode(.original) }
        #else
        let image = renderer.nsImage.map { Image(nsImage: $0).renderingMode(.original) }
        #endif
        let result = image ?? Image(systemName: "face.smiling")
        icons[emoji] = result
        return result
    }

    static func isSingleEmoji(_ value: String) -> Bool {
        let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
        guard trimmed.count == 1, let character = trimmed.first else { return false }
        let scalars = character.unicodeScalars
        if scalars.count == 1, let scalar = scalars.first, scalar.isASCII { return false }
        return scalars.contains { $0.properties.isEmoji }
    }
}

struct MessageReactionPicker: View {
    let selected: String?
    let choose: (String) -> Void
    @Environment(\.dismiss) private var dismiss
    @State private var customEmoji = ""

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(spacing: 20) {
                    LazyVGrid(columns: [GridItem(.adaptive(minimum: 46), spacing: 8)], spacing: 8) {
                        ForEach(MessageReactionChoices.all, id: \.self) { emoji in
                            Button {
                                select(emoji == selected ? "" : emoji)
                            } label: {
                                Text(emoji)
                                    .font(.system(size: 30))
                                    .frame(maxWidth: .infinity, minHeight: 46)
                                    .background(selected == emoji ? Color.accentColor.opacity(0.18) : .clear, in: RoundedRectangle(cornerRadius: 12))
                            }
                            .buttonStyle(.plain)
                            .accessibilityLabel("React with \(emoji)")
                            .accessibilityAddTraits(selected == emoji ? .isSelected : [])
                        }
                    }
                    HStack {
                        TextField("Other emoji", text: $customEmoji)
                            .textFieldStyle(.roundedBorder)
                            .accessibilityIdentifier("custom-reaction-emoji")
                            .onSubmit { if MessageReactionChoices.isSingleEmoji(customEmoji) { select(customEmoji.trimmingCharacters(in: .whitespacesAndNewlines)) } }
                        Button("React") { select(customEmoji.trimmingCharacters(in: .whitespacesAndNewlines)) }
                            .disabled(!MessageReactionChoices.isSingleEmoji(customEmoji))
                    }
                    if selected != nil {
                        Button("Remove reaction", systemImage: "minus.circle") { select("") }
                    }
                }
                .padding(20)
            }
            .navigationTitle("React to message")
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
            }
        }
        #if os(iOS)
        .presentationDetents([.medium, .large])
        .presentationDragIndicator(.visible)
        #else
        .frame(width: 420, height: 430)
        #endif
    }

    private func select(_ emoji: String) {
        choose(emoji)
        dismiss()
    }
}
