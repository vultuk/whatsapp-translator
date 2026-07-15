import Intents

struct NotificationPersonIdentity {
    let handleValue: String
    let handleType: INPersonHandleType
    let isContactSuggestion: Bool
    let suggestionType: INPersonSuggestionType

    static func sender(senderID: String?, senderName: String) -> Self {
        if let phoneNumber = normalizedPhoneNumber(senderID) {
            return Self(
                handleValue: phoneNumber,
                handleType: .phoneNumber,
                isContactSuggestion: false,
                suggestionType: .none
            )
        }

        let trimmedSenderID = senderID?.trimmingCharacters(in: .whitespacesAndNewlines)
        let handleValue = trimmedSenderID.flatMap { $0.isEmpty ? nil : $0 } ?? senderName
        return Self(
            handleValue: handleValue,
            handleType: .unknown,
            isContactSuggestion: true,
            suggestionType: .instantMessageAddress
        )
    }

    private static func normalizedPhoneNumber(_ senderID: String?) -> String? {
        guard let senderID else { return nil }
        let trimmed = senderID.trimmingCharacters(in: .whitespacesAndNewlines)
        let allowedFormatting = CharacterSet(charactersIn: "+0123456789 ()-.")
        guard !trimmed.isEmpty,
              trimmed.unicodeScalars.allSatisfy(allowedFormatting.contains) else {
            return nil
        }

        let digits = trimmed.filter(\.isNumber)
        guard (7 ... 15).contains(digits.count) else { return nil }
        return "+\(digits)"
    }
}
