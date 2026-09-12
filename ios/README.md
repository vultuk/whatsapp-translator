# Babel Bridge for Apple platforms

Native SwiftUI clients for the Babel Bridge translation backend. The Xcode
project contains separate iOS and macOS app targets that share the messaging,
translation, caching and API layers while using each platform's native app
lifecycle and interaction patterns.

## Message ringtones

Open **Settings → Notifications → Message ringtone** for the global choice,
or a chat’s **Conversation settings → Notifications → Message ringtone** for
an override. Aurora, Bamboo, Bloom, Droplet, Glass and Orbit are original,
bundled notification tones. Selecting a tone previews it; the separate play
button previews without changing the selection. **Save** commits the choice,
while **Cancel** leaves the saved setting unchanged. Conversation choices include
**Use global ringtone**, **System default**, and **Silent**. Silent keeps visible
notifications and badges without requesting audio.

Choices are stored on the connected server, shared across native and web
clients, and applied to the APNs payload before delivery, including when the app
is closed. Deploy the matching `/api/settings/message-tone` backend before
installing native build 55. Older app builds fall back to the system sound if
they do not contain the selected custom asset. Normal system sound permissions,
silent mode, Focus, and Watch/CarPlay audio policies still apply.

The canonical sound assets and catalog are in `web/public/sounds`; Xcode copies
them into the iOS app, notification extension, and macOS app. Re-render the
original 16-bit mono PCM WAVs with `scripts/generate-message-tones.py`. All clips
are under two seconds. Shared native tests decode and play every bundled clip,
and backend tests verify persistence, inheritance, exact conversation routing,
validation, and silent APNs payloads.

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

## Unified Messages view

Messages is the default tab on iPhone, iPad and Mac. It combines private and group
conversations in chronological order, with a chat label on every message and
cursor-based loading of earlier history. Opening the feed does not mark every
chat as read. Chats retains the individual conversation view and its full media
composer.

Swipe or use a message's Reply action to select the destination for a text reply.
The composer names that chat and keeps a separate draft for each destination.
The backend validates the selected message belongs to the destination, then checks
again after translation: the latest message in that chat receives an ordinary
send; an older message receives a quoted reply. Activity in other chats does not
change the decision. The existing durable send route handles retries.

The feed uses authenticated `GET /api/feed?limit=50&before=…&before_id=…` and
`POST /api/send` with `replyOnlyIfNotLatest: true`. Ordinary chat replies retain
their existing explicit quote behavior. Use `-demo -demoUnifiedFeed` in Xcode's
Run arguments to inspect a synthetic mixed-chat example.

## Compact photo galleries

Consecutive photos from the same sender in the same conversation share a compact
gallery in both Messages and Chats, including photos without WhatsApp album metadata.
A different sender, chat, non-photo message, or calendar day starts a new run.
Search and starred filters retain the original conversation boundaries.

The feed shows at most four thumbnails and a remaining-photo count. Open any tile
to browse every image with swipe or Previous/Next controls, zoom, translated and
original captions, reactions, and actions for the selected photo. Only preview
images load in the timeline; the viewer loads other photos as they are selected.
Use `-demo -demoPhotoGallery` for a synthetic 12-photo group example.

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

After authentication, the app asks for notification, announcement, CarPlay, and Siri
permission, registers its APNs token with the backend, and displays incoming
translated messages even when the app is not open. The notification service
extension presents them as communication notifications with the sender, group
name on a separate line, translated body, and contact avatar. The server carries
the real group recipient count into the communication intent; when that count is
unavailable, a standard alert preserves sender, group, and message as separate fields. Tapping one opens the matching chat; its Reply action
sends through the normal `/api/send` route, so the conversation's translation
and send-original settings are applied. The same action is mirrored to Apple
Watch for dictated or typed replies. A Debug build registers against the APNs
sandbox; an archived Release build uses the production APNs environment.

The Siri intents extension implements sending, searching, and marking messages
read. Those intents provide the voice-first message surface required by CarPlay.
The app also opts its message notification category into announcements and
CarPlay, with the search/read intent handling a tapped CarPlay alert. Existing
installations re-request the full notification option set on activation, so an
upgrade includes CarPlay while preserving the user's existing notification choices.
Apple assigned the managed CarPlay Communication App entitlement to this developer
account on 11 September 2026. It is enabled for `com.vultuk.whatsapptranslator`
and included in the iOS target as `com.apple.developer.carplay-communication`.
The account can enable it separately for other eligible communication apps.

CarPlay opens a native conversation list with unread chats first and up to 12
conversations. Selecting an unread conversation asks Siri to read it; selecting a
read conversation starts a dictated message. The screen shows names and unread
counts, while Siri handles message text and confirmation. Refresh updates the
list, and it refreshes automatically while active. Account setup stays on iPhone.

Tapped alerts resolve their exact delivered notification ID and read its translated
body. An expired alert returns no message instead of reading another conversation.
Siri replies use the selected conversation's exact ID and the existing translation
and durable-send API. Read receipts carry both conversation and message identity,
so the extension can mark the correct chat read without searching recent chats.

To verify on a device, update TestFlight, open Babel Bridge once, enable Show in
CarPlay in its iPhone notification settings, then reconnect to CarPlay. Announce
Notifications and Show Previews settings govern spoken alerts. Test a translated
incoming alert, tap to read, and dictate and confirm a reply to the same chat.
Xcode's iPhone simulator with I/O → External Displays → CarPlay verifies the
conversation screen; `-demo` uses synthetic CarPlay conversations. Siri speech,
real push delivery, and the car's audio routing still require a physical device.

Apple Watch mirrors iPhone notifications. With the iPhone unlocked, Apple routes
the alert to the iPhone. To test Watch delivery, wear and unlock the Watch, lock
the iPhone, and enable this app under Watch → Notifications → Mirror iPhone Alerts
From. Focus and notification settings can still silence alerts. The app cannot
override Apple's routing to alert both devices simultaneously. See
[Apple's notification routing guide](https://support.apple.com/en-ie/108369).

Incoming message alerts also request background execution. When iOS grants it,
the app fetches the affected chat and writes a file-protected local cache before
the user opens the app. Cached chats render immediately at launch and refresh
again when the app becomes active. Background execution is best-effort on iOS,
so the foreground refresh remains the fallback if the system throttles a push or
the user has force-quit the app.

Run either app scheme with the `-demo` launch argument to preview populated
chats without a backend. Add `-demoConversation` to select a populated
conversation immediately.

Add `-demoNotification` to an iOS Debug run to preview a synthetic translated
group alert using the same communication-notification formatter as incoming
pushes. Enter each launch argument on its own row in Xcode. Allow notifications when prompted. It fires after five seconds and
does not contact the backend or send a WhatsApp message.

Add `-demoLiveReactions` alongside `-demo` to preview a reaction arriving after
20 seconds, changing emoji, being removed, and returning without navigation.
The fixture decodes the same live-event payloads as the WebSocket handler.
Reaction state is retained per conversation, target, and actor, so translation
updates, refreshes, history pages, and older event replays cannot erase a newer
reaction. Reaction events remain hidden from message lists.

Live text and caption alerts are persisted on the server until language detection
and any required translation finish. Translation retries and server restarts
retain pending alerts; imported history does not create alerts. Group alerts carry
separate sender, group subtitle, and translated body fields. Their communication
intent uses the actual recipient count from WhatsApp to select the group layout;
older prefixed payloads remain supported.

## Unified feed attachments

The Messages composer supports photo albums (up to 30 photos), videos, files, and translated voice notes on iOS, iPadOS, and macOS through a single **+** menu, without a separate microphone shortcut. Tapping **+** captures the selected message, or the latest message in the feed when no reply is selected, before opening the picker or recorder. The destination and message remain fixed while media is prepared. Cancel the reply to choose a different destination.

Photos use the existing optimization and album progress flow. Videos are prepared as MP4; videos and files are limited to 64 MB per attachment. Captions and voice notes use the captured conversation's translation settings. Sending media preserves any separate text draft.

Deploy the matching backend before updating native clients: `/api/send-media` adds confirmed video/file sends, and `replyOnlyIfNotLatest` now applies to photos, staged albums, and prepared voice notes. The server rechecks the captured message in its destination chat immediately before preparing the send; only older messages receive a quote. The existing duplicate-safe delivery tracking also covers video/file sends.
