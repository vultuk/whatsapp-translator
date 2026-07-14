import SwiftUI

@main
struct WhatsAppTranslatorApp: App {
    @UIApplicationDelegateAdaptor(NotificationAppDelegate.self) private var notificationAppDelegate
    @Environment(\.scenePhase) private var scenePhase
    @State private var session = AppSession()

    var body: some Scene {
        WindowGroup {
            RootView()
                .environment(session)
                .environment(\.translatorPalette, TranslatorPalette.make(session.preferences.theme))
                .tint(TranslatorPalette.make(session.preferences.theme).accent)
                .preferredColorScheme(session.preferences.colorMode.colorScheme)
                .task { await session.start() }
                .onChange(of: scenePhase) { _, phase in
                    guard phase == .active else { return }
                    Task { await session.becameActive() }
                }
        }
    }
}
