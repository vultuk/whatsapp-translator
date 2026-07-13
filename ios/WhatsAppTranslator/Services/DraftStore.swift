import Foundation

struct DraftStore {
    private let defaults: UserDefaults
    private let key = "conversation-drafts-v1"

    init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
    }

    func text(for contactID: String) -> String {
        dictionary()[contactID] ?? ""
    }

    func save(_ text: String, for contactID: String) {
        var drafts = dictionary()
        if text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            drafts.removeValue(forKey: contactID)
        } else {
            drafts[contactID] = text
        }
        defaults.set(drafts, forKey: key)
    }

    private func dictionary() -> [String: String] {
        defaults.dictionary(forKey: key) as? [String: String] ?? [:]
    }
}
