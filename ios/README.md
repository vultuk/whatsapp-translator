# WhatsApp Translator for iOS

Native SwiftUI companion for the WhatsApp Translator backend.

## Generate and open

```bash
cd ios
xcodegen generate
open WhatsAppTranslator.xcodeproj
```

The first-run screen asks for the backend web address and password. These are stored in the iOS Keychain. The client authenticates against `/api/auth`, uses the bearer-protected REST API, and receives live updates over the authenticated `/ws` endpoint.

After authentication, the app asks for notification permission, registers its
APNs token with the backend, and displays incoming translated messages even when
the app is not open. Tapping a message notification opens the matching chat. A
Debug build registers against the APNs sandbox; an archived Release build uses
the production APNs environment.

Incoming message alerts also request background execution. When iOS grants it,
the app fetches the affected chat and writes a file-protected local cache before
the user opens the app. Cached chats render immediately at launch and refresh
again when the app becomes active. Background execution is best-effort on iOS,
so the foreground refresh remains the fallback if the system throttles a push or
the user has force-quit the app.

Run the `WhatsAppTranslator` scheme with the `-demo` launch argument to preview populated chats without a backend.
