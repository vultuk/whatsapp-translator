import Intents
import UserNotifications

enum NotificationMessagePresentation {
    static func preparedContent(_ content: UNNotificationContent) -> UNNotificationContent {
        guard let copy = content.mutableCopy() as? UNMutableNotificationContent else { return content }
        let info = content.userInfo
        let isGroup = (info["chatType"] as? String) == "group"
            || (info["contactId"] as? String)?.hasSuffix("@g.us") == true
        let text = (info["messageBody"] as? String) ?? content.body
        let group = (info["conversationName"] as? String)?.trimmingCharacters(in: .whitespacesAndNewlines)
        if isGroup, let group, !group.isEmpty {
            let prefix = "[\(String(group.prefix(100)))] "
            copy.body = text.hasPrefix(prefix) ? text : prefix + text
        } else {
            copy.body = text
        }
        copy.body = String(copy.body.prefix(500))
        return copy
    }

    static func messagingContent(_ input: UNNotificationContent, avatarData: Data?, donate: Bool = true) -> UNNotificationContent {
        let content = preparedContent(input)
        let userInfo = content.userInfo
        guard let contactID = userInfo["contactId"] as? String,
              let senderName = userInfo["senderName"] as? String else {
            return content
        }
        let senderID = (userInfo["senderId"] as? String)?.notificationValue
        let conversationName = (userInfo["conversationName"] as? String)?.notificationValue
        let isGroup = (userInfo["chatType"] as? String) == "group" || contactID.contains("@g.us")
        let body = content.body
        let speakableGroupName = isGroup
            ? conversationName.map(INSpeakableString.init(spokenPhrase:))
            : nil
        let identity = NotificationPersonIdentity.sender(senderID: senderID, senderName: senderName)
        let sender = INPerson(
            personHandle: INPersonHandle(value: identity.handleValue, type: identity.handleType),
            nameComponents: nil,
            displayName: senderName,
            image: avatarData.map(INImage.init(imageData:)),
            contactIdentifier: nil,
            customIdentifier: senderID ?? senderName,
            isContactSuggestion: identity.isContactSuggestion,
            suggestionType: identity.suggestionType
        )
        let intent = INSendMessageIntent(
            recipients: nil,
            outgoingMessageType: .outgoingMessageText,
            content: body,
            speakableGroupName: speakableGroupName,
            conversationIdentifier: contactID,
            serviceName: "Babel Bridge",
            sender: sender,
            attachments: nil
        )

        let interaction = INInteraction(intent: intent, response: nil)
        interaction.direction = .incoming
        if donate { interaction.donate { _ in } }

        do {
            let messagingContent = try content.updating(from: intent)
            return messagingContent
        } catch {
            return content
        }
    }
}

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

private extension String {
    var notificationValue: String? {
        let value = trimmingCharacters(in: .whitespacesAndNewlines)
        return value.isEmpty ? nil : value
    }
}
