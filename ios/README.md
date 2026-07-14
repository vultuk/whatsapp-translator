# Babel Bridge for Apple platforms

Native SwiftUI clients for the Babel Bridge translation backend. The Xcode
project contains separate iOS and macOS app targets that share the messaging,
translation, caching and API layers while using each platform's native app
lifecycle and interaction patterns.

## Generate and open

```bash
cd ios
xcodegen generate
open WhatsAppTranslator.xcodeproj
```

Use the `WhatsAppTranslator` scheme for iPhone and iPad, or the
`BabelBridgeMac` scheme for the native Mac app. The Mac target is built with the
macOS SDK and SwiftUI/AppKit; it is not Mac Catalyst or an iPad compatibility
build.

The first-run screen asks for the backend web address and password. These are
stored in the system Keychain. The client authenticates against `/api/auth`,
uses the bearer-protected REST API, and receives live updates over the
authenticated `/ws` endpoint.

## Native Mac app

The Mac app provides a resizable two-column conversation window, native menu
bar and Settings scene, Command-R refresh, Command-F conversation search,
compact desktop message bubbles, drag-to-reply, context menus and inline media.
It shares the same App Store bundle ID and backend configuration as the iOS app.

macOS registers its own APNs token and notification category. Incoming message
notifications can open the matching conversation or send an inline reply
through the normal translation-aware `/api/send` route. The sandboxed target
has outbound network, communication-notification and shared-Keychain
entitlements.

After authentication, the app asks for notification, announcement, and Siri
permission, registers its APNs token with the backend, and displays incoming
translated messages even when the app is not open. The notification service
extension presents them as communication notifications with the sender, group
name, and contact avatar. Tapping one opens the matching chat; its Reply action
sends through the normal `/api/send` route, so the conversation's translation
and send-original settings are applied. The same action is mirrored to Apple
Watch for dictated or typed replies. A Debug build registers against the APNs
sandbox; an archived Release build uses the production APNs environment.

The Siri intents extension implements sending, searching, and marking messages
read. Those intents provide the voice-first message surface required by CarPlay.
The app also opts its message notification category into announcements and
CarPlay, but Apple must approve and assign the managed CarPlay Communication App
entitlement before the full CarPlay app surface can be signed and distributed.

Incoming message alerts also request background execution. When iOS grants it,
the app fetches the affected chat and writes a file-protected local cache before
the user opens the app. Cached chats render immediately at launch and refresh
again when the app becomes active. Background execution is best-effort on iOS,
so the foreground refresh remains the fallback if the system throttles a push or
the user has force-quit the app.

Run either app scheme with the `-demo` launch argument to preview populated
chats without a backend. Add `-demoConversation` to select a populated
conversation immediately.
