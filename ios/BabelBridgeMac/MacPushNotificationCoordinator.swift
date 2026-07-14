import AppKit
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
            intentIdentifiers: [],
            options: [.hiddenPreviewsShowTitle, .hiddenPreviewsShowSubtitle]
        )
    }
}

private struct MacNotificationRouting {
    let contactID: String

    init?(userInfo: [AnyHashable: Any]) {
        guard let contactID = userInfo["contactId"] as? String,
              !contactID.isEmpty else { return nil }
        self.contactID = contactID
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
    private let installationIDKey = "push-notification-installation-id-macos"

    private init() {}

    func activate(for session: AppSession) async {
        self.session = session
        registerMessagingCategory()

        if let pendingContactID {
            self.pendingContactID = nil
            session.openConversation(fromNotification: pendingContactID)
        }

        let center = UNUserNotificationCenter.current()
        let settings = await center.notificationSettings()
        let authorized: Bool
        switch settings.authorizationStatus {
        case .notDetermined:
            authorized = (try? await center.requestAuthorization(options: [.alert, .sound, .badge])) == true
        case .authorized, .provisional, .ephemeral:
            authorized = true
        case .denied:
            authorized = false
        @unknown default:
            authorized = false
        }

        guard authorized else { return }
        NSApplication.shared.registerForRemoteNotifications()
        await registerCurrentTokenIfPossible()
    }

    func registerMessagingCategory() {
        UNUserNotificationCenter.current().setNotificationCategories([MessagingNotificationContract.category])
    }

    func received(deviceToken data: Data) {
        deviceToken = data.map { String(format: "%02x", $0) }.joined()
        Task { await registerCurrentTokenIfPossible() }
    }

    func failedToRegister(error: Error) {
        #if DEBUG
        print("macOS APNs registration failed: \(error.localizedDescription)")
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
            print("macOS notification reply failed: \(error.localizedDescription)")
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

final class MacNotificationAppDelegate: NSObject, NSApplicationDelegate, UNUserNotificationCenterDelegate {
    @MainActor
    func applicationDidFinishLaunching(_ notification: Notification) {
        UNUserNotificationCenter.current().delegate = self
        PushNotificationCoordinator.shared.registerMessagingCategory()
    }

    @MainActor
    func application(
        _ application: NSApplication,
        didRegisterForRemoteNotificationsWithDeviceToken deviceToken: Data
    ) {
        PushNotificationCoordinator.shared.received(deviceToken: deviceToken)
    }

    @MainActor
    func application(
        _ application: NSApplication,
        didFailToRegisterForRemoteNotificationsWithError error: Error
    ) {
        PushNotificationCoordinator.shared.failedToRegister(error: error)
    }

    func application(
        _ application: NSApplication,
        didReceiveRemoteNotification userInfo: [String: Any]
    ) {
        guard let contactID = userInfo["contactId"] as? String else { return }
        Task {
            _ = await BackgroundMessageSynchronizer.shared.sync(contactID: contactID)
            await PushNotificationCoordinator.shared.backgroundCacheDidRefresh()
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
        guard let routing = MacNotificationRouting(
            userInfo: response.notification.request.content.userInfo
        ) else {
            completionHandler()
            return
        }

        if response.actionIdentifier == MessagingNotificationContract.replyActionIdentifier,
           let reply = response as? UNTextInputNotificationResponse {
            let replyText = reply.userText
            let contactID = routing.contactID
            let completion = NotificationCompletion(completionHandler)
            Task { @MainActor in
                _ = await PushNotificationCoordinator.shared.reply(text: replyText, to: contactID)
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
