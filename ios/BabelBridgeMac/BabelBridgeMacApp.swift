import AppKit
import SwiftUI

@main
struct BabelBridgeMacApp: App {
    @NSApplicationDelegateAdaptor(MacNotificationAppDelegate.self) private var notificationAppDelegate
    @Environment(\.scenePhase) private var scenePhase
    @State private var session = AppSession()

    var body: some Scene {
        WindowGroup("Babel Bridge") {
            RootView()
                .environment(session)
                .environment(\.translatorPalette, TranslatorPalette.make(session.preferences.theme))
                .tint(TranslatorPalette.make(session.preferences.theme).accent)
                .preferredColorScheme(session.preferences.colorMode.colorScheme)
                .frame(
                    minWidth: MacChatLayoutMetrics.minimumWindowWidth,
                    minHeight: MacChatLayoutMetrics.minimumWindowHeight
                )
                .task { await session.start() }
                .onChange(of: scenePhase) { _, phase in
                    guard phase == .active else { return }
                    Task { await session.becameActive() }
                }
        }
        .defaultSize(
            width: MacChatLayoutMetrics.defaultWindowWidth,
            height: MacChatLayoutMetrics.defaultWindowHeight
        )
        .windowToolbarStyle(.unifiedCompact)
        .commands {
            SidebarCommands()
            CommandGroup(replacing: .help) {
                Link("Babel Bridge on GitHub", destination: URL(string: "https://github.com/vultuk/whatsapp-translator")!)
                Divider()
                Link("Report an Issue", destination: URL(string: "https://github.com/vultuk/whatsapp-translator/issues")!)
            }
        }

        Settings {
            MacSettingsView()
                .environment(session)
                .environment(\.translatorPalette, TranslatorPalette.make(session.preferences.theme))
                .tint(TranslatorPalette.make(session.preferences.theme).accent)
                .preferredColorScheme(session.preferences.colorMode.colorScheme)
        }
        .windowResizability(.contentSize)
    }

}
