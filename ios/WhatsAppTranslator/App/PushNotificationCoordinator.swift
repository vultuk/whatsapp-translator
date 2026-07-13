import SwiftUI
import UserNotifications

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
        UIApplication.shared.registerForRemoteNotifications()
        await registerCurrentTokenIfPossible()
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
    @MainActor
    func application(
        _ application: UIApplication,
        didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]? = nil
    ) -> Bool {
        UNUserNotificationCenter.current().delegate = self
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
        let contactID = response.notification.request.content.userInfo["contactId"] as? String
        if let contactID {
            Task { @MainActor in
                PushNotificationCoordinator.shared.openNotification(contactID: contactID)
            }
        }
        completionHandler()
    }
}
