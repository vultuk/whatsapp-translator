import Foundation
import Observation
import SwiftUI

enum AppTheme: String, CaseIterable, Codable, Identifiable, Sendable {
    case whatsapp, ocean, sunset, github, dracula, nord, linear, vercel

    var id: String { rawValue }
    var title: String { rawValue.prefix(1).uppercased() + rawValue.dropFirst() }
}

enum AppColorMode: String, CaseIterable, Codable, Identifiable, Sendable {
    case system, light, dark

    var id: String { rawValue }
    var title: String {
        switch self {
        case .system: "Follow system"
        case .light: "Always light"
        case .dark: "Always dark"
        }
    }

    var colorScheme: ColorScheme? {
        switch self {
        case .system: nil
        case .light: .light
        case .dark: .dark
        }
    }
}

struct ConversationPresentationPreferences: Codable, Equatable, Sendable {
    var nickname: String?
    var timezoneIdentifier: String?

    static let empty = ConversationPresentationPreferences(nickname: nil, timezoneIdentifier: nil)
}

@Observable
final class AppPreferencesStore {
    private struct StoredState: Codable {
        var starredMessageIDs: [String: Set<String>] = [:]
        var conversations: [String: ConversationPresentationPreferences] = [:]
        var theme: AppTheme = .whatsapp
        var colorMode: AppColorMode = .system
    }

    private let defaults: UserDefaults
    private let storageKey = "whatsapp-translator-ios-preferences-v1"
    private var starredMessageIDs: [String: Set<String>]
    private var conversations: [String: ConversationPresentationPreferences]

    var theme: AppTheme { didSet { persist() } }
    var colorMode: AppColorMode { didSet { persist() } }

    init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
        let state = defaults.data(forKey: storageKey)
            .flatMap { try? JSONDecoder().decode(StoredState.self, from: $0) }
            ?? StoredState()
        starredMessageIDs = state.starredMessageIDs
        conversations = state.conversations
        theme = state.theme
        colorMode = state.colorMode
    }

    func isStarred(messageID: String, contactID: String) -> Bool {
        starredMessageIDs[contactID]?.contains(messageID) == true
    }

    func toggleStar(messageID: String, contactID: String) {
        var values = starredMessageIDs[contactID, default: []]
        if values.contains(messageID) { values.remove(messageID) } else { values.insert(messageID) }
        starredMessageIDs[contactID] = values
        persist()
    }

    func conversationPreferences(for contactID: String) -> ConversationPresentationPreferences {
        conversations[contactID] ?? .empty
    }

    func setConversationPreferences(_ preferences: ConversationPresentationPreferences, for contactID: String) {
        conversations[contactID] = preferences
        persist()
    }

    func nickname(for contactID: String) -> String? {
        conversations[contactID]?.nickname?.trimmingCharacters(in: .whitespacesAndNewlines).nilIfBlank
    }

    private func persist() {
        let state = StoredState(
            starredMessageIDs: starredMessageIDs,
            conversations: conversations,
            theme: theme,
            colorMode: colorMode
        )
        guard let data = try? JSONEncoder().encode(state) else { return }
        defaults.set(data, forKey: storageKey)
    }
}

private extension String {
    var nilIfBlank: String? { isEmpty ? nil : self }
}
