#if os(macOS)
import SwiftUI

struct MacConversationHeader: View {
    @Environment(\.translatorPalette) private var palette
    let contact: Contact
    let displayName: String
    let avatarURL: URL?
    let localTime: String?
    let usage: UsageSummary?
    let starredOnly: Bool
    let showContactSettings: () -> Void
    let search: () -> Void
    let toggleStarred: () -> Void
    let showCost: () -> Void
    let showConversationSettings: () -> Void

    var body: some View {
        HStack(spacing: 14) {
            Button(action: showContactSettings) {
                HStack(spacing: 11) {
                    ContactAvatar(contact: contact, url: avatarURL, size: 38)
                    VStack(alignment: .leading, spacing: 2) {
                        Text(displayName)
                            .font(.headline)
                            .lineLimit(1)
                            .truncationMode(.tail)
                        Text(subtitle)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
                            .truncationMode(.tail)
                    }
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .help("Conversation settings")
            .layoutPriority(1)

            Spacer(minLength: 12)

            HStack(spacing: 7) {
                headerButton("Search messages", systemImage: "magnifyingglass", action: search)
                headerButton(
                    starredOnly ? "Show all messages" : "Show starred messages",
                    systemImage: starredOnly ? "star.fill" : "star",
                    foreground: starredOnly ? .yellow : .secondary,
                    action: toggleStarred
                )
                Menu {
                    Button("Conversation cost", systemImage: "dollarsign.circle", action: showCost)
                    Button("Conversation settings", systemImage: "slider.horizontal.3", action: showConversationSettings)
                } label: {
                    Image(systemName: "ellipsis")
                        .font(.system(size: 14, weight: .semibold))
                        .frame(width: 32, height: 30)
                        .background(.primary.opacity(0.055), in: RoundedRectangle(cornerRadius: 8))
                        .contentShape(RoundedRectangle(cornerRadius: 8))
                }
                .menuStyle(.borderlessButton)
                .menuIndicator(.hidden)
                .help("More conversation options")
            }
            .fixedSize(horizontal: true, vertical: false)
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 10)
        .background(.regularMaterial)
        .overlay(alignment: .bottom) {
            Divider()
        }
    }

    private var subtitle: String {
        var values = [localTime ?? "Auto-translation on"]
        if let usage {
            values.append(usage.costUsd.formatted(.currency(code: "USD").precision(.fractionLength(4))))
        }
        return values.joined(separator: "  ·  ")
    }

    private func headerButton(
        _ title: String,
        systemImage: String,
        foreground: Color = .secondary,
        action: @escaping () -> Void
    ) -> some View {
        Button(action: action) {
            Image(systemName: systemImage)
                .font(.system(size: 14, weight: .semibold))
                .foregroundStyle(foreground)
                .frame(width: 32, height: 30)
                .background(.primary.opacity(0.055), in: RoundedRectangle(cornerRadius: 8))
                .contentShape(RoundedRectangle(cornerRadius: 8))
        }
        .buttonStyle(.plain)
        .help(title)
        .accessibilityLabel(title)
    }
}
#endif
