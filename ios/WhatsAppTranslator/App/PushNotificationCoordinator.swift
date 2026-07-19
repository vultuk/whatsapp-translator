import Intents
import SwiftUI
import UserNotifications

enum MessagingNotificationContract {
    static let categoryIdentifier = "MESSAGE_CATEGORY"
    static let replyActionIdentifier = "REPLY_ACTION"

    static var category: UNNotificationCategory {
        let reply = UNTextInputNotificationAction(
            identifier: replyActionIdentifier,
            title: "Reply",
            options: [],
            textInputButtonTitle: "Send",
            textInputPlaceholder: "Reply…"
        )
        return UNNotificationCategory(
            identifier: categoryIdentifier,
            actions: [reply],
            intentIdentifiers: [INSendMessageIntentIdentifier, INSearchForMessagesIntentIdentifier],
            options: [
                .allowInCarPlay,
                .allowAnnouncement,
                .hiddenPreviewsShowTitle,
                .hiddenPreviewsShowSubtitle,
            ]
        )
    }
}

struct MessagingNotificationRouting: Sendable {
    let contactID: String
    let messageID: String?
    let senderName: String?
    let conversationName: String?
    let isGroup: Bool

    init?(userInfo: [AnyHashable: Any]) {
        guard let contactID = userInfo["contactId"] as? String,
              !contactID.isEmpty else { return nil }
        self.contactID = contactID
        messageID = userInfo["messageId"] as? String
        senderName = userInfo["senderName"] as? String
        conversationName = userInfo["conversationName"] as? String
        isGroup = (userInfo["chatType"] as? String) == "group" || contactID.contains("@g.us")
    }
}

struct DeliveredMessagingNotification: Equatable, Sendable {
    let identifier: String
    let contactID: String
}

enum MessagingNotificationReadState {
    static func identifiersToRemove(
        for contactID: String,
        from notifications: [DeliveredMessagingNotification]
    ) -> [String] {
        notifications.compactMap { notification in
            notification.contactID == contactID ? notification.identifier : nil
        }
    }
}

private final class NotificationCompletion: @unchecked Sendable {
    private let handler: () -> Void

    init(_ handler: @escaping () -> Void) {
        self.handler = handler
    }

    func finish() {
        handler()
    }
}

@MainActor
final class PushNotificationCoordinator {
    static let shared = PushNotificationCoordinator()

    private weak var session: AppSession?
    private var deviceToken: String?
    private var pendingContactID: String?
    private let installationIDKey = "push-notification-installation-id"

    private init() {}

    func activate(for session: AppSession) async {
        self.session = session
        registerMessagingCategory()
        requestSiriAuthorizationIfNeeded()

        if let pendingContactID {
            self.pendingContactID = nil
            session.openConversation(fromNotification: pendingContactID)
        }

        let center = UNUserNotificationCenter.current()
        let settings = await center.notificationSettings()
        let authorized: Bool
        switch settings.authorizationStatus {
        case .notDetermined:
            authorized = (try? await center.requestAuthorization(
                options: [.alert, .sound, .badge, .announcement]
            )) == true
        case .authorized, .provisional, .ephemeral:
            authorized = true
        case .denied:
            authorized = false
        @unknown default:
            authorized = false
        }

        guard authorized else { return }
        UIApplication.shared.registerForRemoteNotifications()
        await registerCurrentTokenIfPossible()
    }

    func registerMessagingCategory() {
        UNUserNotificationCenter.current().setNotificationCategories([
            MessagingNotificationContract.category,
        ])
    }

    private func requestSiriAuthorizationIfNeeded() {
        guard INPreferences.siriAuthorizationStatus() == .notDetermined else { return }
        INPreferences.requestSiriAuthorization { _ in }
    }

    func received(deviceToken data: Data) {
        deviceToken = data.map { String(format: "%02x", $0) }.joined()
        Task { await registerCurrentTokenIfPossible() }
    }

    func failedToRegister(error: Error) {
        #if DEBUG
        print("APNs registration failed: \(error.localizedDescription)")
        #endif
    }

    func openNotification(contactID: String) {
        if let session {
            session.openConversation(fromNotification: contactID)
        } else {
            pendingContactID = contactID
        }
    }

    func backgroundCacheDidRefresh() async {
        _ = await session?.restoreCachedState()
    }

    func markConversationRead(contactID: String, badgeCount: Int) async {
        let center = UNUserNotificationCenter.current()
        let delivered = await center.deliveredNotifications()
        let messagingNotifications = delivered.compactMap { notification -> DeliveredMessagingNotification? in
            guard let routing = MessagingNotificationRouting(
                userInfo: notification.request.content.userInfo
            ) else { return nil }
            return DeliveredMessagingNotification(
                identifier: notification.request.identifier,
                contactID: routing.contactID
            )
        }
        let identifiers = MessagingNotificationReadState.identifiersToRemove(
            for: contactID,
            from: messagingNotifications
        )
        if !identifiers.isEmpty {
            center.removeDeliveredNotifications(withIdentifiers: identifiers)
        }
        try? await center.setBadgeCount(max(0, badgeCount))
    }

    func reply(text: String, to contactID: String) async -> Bool {
        let clean = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !clean.isEmpty else { return false }

        if let session {
            return await session.send(text: clean, to: contactID)
        }

        guard let configuration = CredentialStore().load() else { return false }
        let sessionConfiguration = URLSessionConfiguration.ephemeral
        sessionConfiguration.timeoutIntervalForRequest = 15
        sessionConfiguration.timeoutIntervalForResource = 30
        let api = APIClient(session: URLSession(configuration: sessionConfiguration))
        await api.configure(configuration)
        do {
            try await api.prepareAuthenticatedRequests()
            _ = try await api.send(contactID: contactID, text: clean)
            _ = await BackgroundMessageSynchronizer.shared.sync(contactID: contactID)
            return true
        } catch {
            #if DEBUG
            print("Notification reply failed: \(error.localizedDescription)")
            #endif
            return false
        }
    }

    private func registerCurrentTokenIfPossible() async {
        guard let deviceToken, let session else { return }
        await session.registerPushDevice(
            token: deviceToken,
            installationID: installationID,
            environment: Self.environment
        )
    }

    private var installationID: String {
        let defaults = UserDefaults.standard
        if let existing = defaults.string(forKey: installationIDKey) {
            return existing
        }
        let created = UUID().uuidString
        defaults.set(created, forKey: installationIDKey)
        return created
    }

    private static var environment: String {
        #if DEBUG
        "sandbox"
        #else
        "production"
        #endif
    }
}

final class NotificationAppDelegate: NSObject, UIApplicationDelegate, UNUserNotificationCenterDelegate {
    func application(
        _ application: UIApplication,
        handleEventsForBackgroundURLSession identifier: String,
        completionHandler: @escaping () -> Void
    ) {
        guard identifier == "com.vultuk.whatsapptranslator.photo-uploads" else {
            completionHandler()
            return
        }
        BackgroundPhotoUploadSession.shared.reconnect(completionHandler: completionHandler)
    }
    @MainActor
    func application(
        _ application: UIApplication,
        didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]? = nil
    ) -> Bool {
        UNUserNotificationCenter.current().delegate = self
        PushNotificationCoordinator.shared.registerMessagingCategory()
        return true
    }

    @MainActor
    func application(
        _ application: UIApplication,
        didRegisterForRemoteNotificationsWithDeviceToken deviceToken: Data
    ) {
        PushNotificationCoordinator.shared.received(deviceToken: deviceToken)
    }

    @MainActor
    func application(
        _ application: UIApplication,
        didFailToRegisterForRemoteNotificationsWithError error: Error
    ) {
        PushNotificationCoordinator.shared.failedToRegister(error: error)
    }

    @MainActor
    func application(
        _ application: UIApplication,
        didReceiveRemoteNotification userInfo: [AnyHashable: Any],
        fetchCompletionHandler completionHandler: @escaping (UIBackgroundFetchResult) -> Void
    ) {
        guard let contactID = userInfo["contactId"] as? String else {
            completionHandler(.noData)
            return
        }

        Task {
            let outcome = await BackgroundMessageSynchronizer.shared.sync(contactID: contactID)
            switch outcome {
            case .newData:
                await PushNotificationCoordinator.shared.backgroundCacheDidRefresh()
                completionHandler(.newData)
            case .noData:
                await PushNotificationCoordinator.shared.backgroundCacheDidRefresh()
                completionHandler(.noData)
            case .failed:
                completionHandler(.failed)
            }
        }
    }

    nonisolated func userNotificationCenter(
        _ center: UNUserNotificationCenter,
        willPresent notification: UNNotification,
        withCompletionHandler completionHandler: @escaping (UNNotificationPresentationOptions) -> Void
    ) {
        completionHandler([.banner, .list, .sound, .badge])
    }

    nonisolated func userNotificationCenter(
        _ center: UNUserNotificationCenter,
        didReceive response: UNNotificationResponse,
        withCompletionHandler completionHandler: @escaping () -> Void
    ) {
        guard let routing = MessagingNotificationRouting(
            userInfo: response.notification.request.content.userInfo
        ) else {
            completionHandler()
            return
        }

        if response.actionIdentifier == MessagingNotificationContract.replyActionIdentifier,
           let reply = response as? UNTextInputNotificationResponse {
            let replyText = reply.userText
            let completion = NotificationCompletion(completionHandler)
            Task { @MainActor in
                _ = await PushNotificationCoordinator.shared.reply(
                    text: replyText,
                    to: routing.contactID
                )
                completion.finish()
            }
            return
        }

        Task { @MainActor in
            PushNotificationCoordinator.shared.openNotification(contactID: routing.contactID)
        }
        completionHandler()
    }
}
