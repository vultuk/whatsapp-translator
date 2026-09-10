import Foundation
import Intents
@preconcurrency import UserNotifications

final class NotificationService: UNNotificationServiceExtension, @unchecked Sendable {
    private var contentHandler: ((UNNotificationContent) -> Void)?
    private var fallbackContent: UNNotificationContent?
    private let completionLock = NSLock()

    override func didReceive(
        _ request: UNNotificationRequest,
        withContentHandler contentHandler: @escaping (UNNotificationContent) -> Void
    ) {
        let content = NotificationMessagePresentation.preparedContent(request.content)
        completionLock.lock()
        self.contentHandler = contentHandler
        fallbackContent = content
        completionLock.unlock()
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
        finish(with: NotificationMessagePresentation.messagingContent(content, avatarData: avatarData))
    }

    private func finish(with content: UNNotificationContent) {
        completionLock.lock()
        let handler = contentHandler
        contentHandler = nil
        completionLock.unlock()
        handler?(content)
    }

    override func serviceExtensionTimeWillExpire() {
        completionLock.lock()
        let fallback = fallbackContent
        completionLock.unlock()
        if let fallbackContent = fallback {
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
