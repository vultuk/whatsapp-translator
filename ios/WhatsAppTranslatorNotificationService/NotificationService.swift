import Foundation
import Intents
@preconcurrency import UserNotifications

final class NotificationService: UNNotificationServiceExtension, @unchecked Sendable {
    private var contentHandler: ((UNNotificationContent) -> Void)?
    private var fallbackContent: UNNotificationContent?

    override func didReceive(
        _ request: UNNotificationRequest,
        withContentHandler contentHandler: @escaping (UNNotificationContent) -> Void
    ) {
        self.contentHandler = contentHandler
        fallbackContent = request.content

        let content = request.content
        let userInfo = content.userInfo
        guard let contactID = userInfo["contactId"] as? String,
              let senderName = userInfo["senderName"] as? String,
              !contactID.isEmpty,
              !senderName.isEmpty else {
            finish(with: content)
            return
        }

        guard let avatarURLString = (userInfo["avatarUrl"] as? String)?.nilIfBlank,
              let avatarURL = URL(string: avatarURLString) else {
            deliverMessagingContent(content, avatarData: nil)
            return
        }

        var request = URLRequest(url: avatarURL)
        request.timeoutInterval = 8
        URLSession.shared.dataTask(with: request) { [weak self] data, response, _ in
            let validData: Data?
            if let http = response as? HTTPURLResponse,
               (200 ..< 300).contains(http.statusCode),
               let data,
               data.count <= 2_000_000 {
                validData = data
            } else {
                validData = nil
            }
            self?.deliverMessagingContent(content, avatarData: validData)
        }.resume()
    }

    private func deliverMessagingContent(_ content: UNNotificationContent, avatarData: Data?) {
        let userInfo = content.userInfo
        guard let contactID = userInfo["contactId"] as? String,
              let senderName = userInfo["senderName"] as? String else {
            finish(with: content)
            return
        }
        let senderID = (userInfo["senderId"] as? String)?.nilIfBlank ?? senderName
        let conversationName = (userInfo["conversationName"] as? String)?.nilIfBlank
        let isGroup = (userInfo["chatType"] as? String) == "group" || contactID.contains("@g.us")
        let body = (userInfo["messageBody"] as? String)?.nilIfBlank ?? content.body
        let speakableGroupName = isGroup
            ? conversationName.map(INSpeakableString.init(spokenPhrase:))
            : nil
        let sender = INPerson(
            personHandle: INPersonHandle(value: senderID, type: .unknown),
            nameComponents: nil,
            displayName: senderName,
            image: avatarData.map(INImage.init(imageData:)),
            contactIdentifier: nil,
            customIdentifier: senderID,
            isContactSuggestion: true,
            suggestionType: .instantMessageAddress
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
        interaction.donate { _ in }

        do {
            let messagingContent = try content.updating(from: intent)
            fallbackContent = messagingContent
            finish(with: messagingContent)
        } catch {
            finish(with: content)
        }
    }

    private func finish(with content: UNNotificationContent) {
        let handler = contentHandler
        contentHandler = nil
        handler?(content)
    }

    override func serviceExtensionTimeWillExpire() {
        if let fallbackContent {
            finish(with: fallbackContent)
        }
    }
}

private extension String {
    var nilIfBlank: String? {
        let value = trimmingCharacters(in: .whitespacesAndNewlines)
        return value.isEmpty ? nil : value
    }
}
