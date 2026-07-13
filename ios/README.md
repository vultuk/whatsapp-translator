# WhatsApp Translator for iOS

Native SwiftUI companion for the WhatsApp Translator backend.

## Generate and open

```bash
cd ios
xcodegen generate
open WhatsAppTranslator.xcodeproj
```

The first-run screen asks for the backend web address and password. These are stored in the iOS Keychain. The client authenticates against `/api/auth`, uses the bearer-protected REST API, and receives live updates over the authenticated `/ws` endpoint.

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

Run the `WhatsAppTranslator` scheme with the `-demo` launch argument to preview populated chats without a backend.
