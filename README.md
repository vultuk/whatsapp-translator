# WhatsApp Translator

Small WhatsApp Web client with a local web UI, message history, translation, AI replies, and MCP access.

## Deploy

Railway is the easiest hosted option for this app because it supports long-running services and a persistent volume for WhatsApp session data.

[![Deploy on Railway](https://railway.com/button.svg)](https://railway.com/deploy/tGnsUG?referralCode=ov3_G4&utm_medium=integration&utm_source=template&utm_campaign=generic)

## Features

- Connect a WhatsApp account through QR login
- View chats and messages in a local web UI
- Send text, images, replies, and reactions
- Translate incoming and outgoing messages with OpenAI
- Listen to translated voice notes and record, preview, and send translated WhatsApp audio
- Generate AI-composed messages and AI replies in your writing style
- Per-chat translation settings, including an option to send the translation followed by the original text
- Local SQLite storage for messages, usage, and session data
- MCP endpoint with OAuth support

## Required Environment Variables

Minimum:

- `WA_WEB=true`
- `WA_PASSWORD` (required for hosted/network-accessible operation)

Required for AI features:

- `OPENAI_API_KEY`

Optional:

- `WA_OPENAI_DETECTION_MODEL` default: `gpt-6-astra`
- `WA_OPENAI_TRANSLATION_MODEL` default: `gpt-6-astra`
- `WA_OPENAI_HIGH_END_MODEL` default: `gpt-6-astra`
- `WA_DEFAULT_LANGUAGE` default: `English`
- `WA_ALLOW_LOCAL_NO_AUTH=true` permits password-free development only with an explicit loopback `WA_HOST`
- `WA_HOST` default: `0.0.0.0`
- `WA_PORT` default: `3000`
- `WA_DATA_DIR` data directory for `session.db` and `messages.db`
- `WA_BRIDGE_PATH` path to the `wa-bridge` binary
- `WA_VERBOSE=true` enable verbose logs
- `WA_LOGOUT=true` clear the WhatsApp session on startup

Text AI defaults to GPT-6 Astra with low reasoning. The shared OpenAI settings override environment model choices; selecting Astra with no reasoning selection uses low. Speech recognition and speech synthesis retain their dedicated audio models.

Incoming text and media captions translate into `WA_DEFAULT_LANGUAGE`. Outgoing text, captions, and voice use a conversation override first, then the replied-to incoming message's language, then the predominant recent incoming language. Unknown chat language is resolved from recent incoming text before sending. Language-neutral messages stay unchanged. Translation failures prevent outgoing sends and incoming jobs retry with backoff; opening a conversation queues previously undetected text and captions.

Required for native iOS push notifications:

- `APNS_KEY_ID` Apple push notification key ID
- `APNS_TEAM_ID` Apple Developer team ID
- `APNS_BUNDLE_ID` iOS app bundle identifier
- `APNS_PRIVATE_KEY_BASE64` base64-encoded contents of the APNs `.p8` private key

The APNs key is a production secret. Keep the downloaded `.p8` file outside the
repository and provide it to the deployed service only through secret environment
variables. The authenticated `POST /api/push/test` endpoint sends a test alert to
all currently registered iPhones.

Hosted deploy recommendation:

- `WA_PASSWORD` is required on non-loopback hosts; startup fails without it
- attach a persistent volume at `/data`
- if `WA_PORT` is not set, the app will use Railway's `PORT`
- MCP OAuth is intended for local MCP clients you explicitly approve. Dynamic
  client registration only accepts loopback `http://localhost`, `http://127.0.0.1`,
  or `http://[::1]` redirect URIs, and authorization requires an exact registered
  redirect match.

## Run Locally

Prerequisites:

- Rust
- Go
- FFmpeg (required for voice-note conversion and audio integration tests; included in Docker)

Run:

```bash
export WA_WEB=true
export WA_HOST=127.0.0.1
export WA_ALLOW_LOCAL_NO_AUTH=true
export OPENAI_API_KEY=your_key_here

cargo run --release
```

Notes:

- If `go` is installed, `cargo` will try to build `wa-bridge` automatically.
- If not, build it manually:

```bash
cd wa-bridge
go build -o wa-bridge .
cd ..

export WA_WEB=true
export WA_HOST=127.0.0.1
export WA_ALLOW_LOCAL_NO_AUTH=true
export WA_BRIDGE_PATH="$PWD/wa-bridge/wa-bridge"
export OPENAI_API_KEY=your_key_here

cargo run --release
```

Then open:

```text
http://localhost:3000
```

The supported backend is the Rust app (`cargo run --release`). The `web/`
package is only for frontend tests and Storybook previews; it does not run a
separate Node API server.

## Translated voice notes

Incoming voice notes are transcribed, translated into your default language, and
spoken by an AI-generated voice. The original remains available beside the
translation. Older recordings can be translated on demand.

Use the microphone button in a conversation to record up to three minutes,
listen to the translated preview, then send it as a WhatsApp voice note. Set the
conversation language first if it has not yet been detected. The existing
original-follow-up setting also applies to recordings. Translation failures do
not send the original as a fallback. Prepared recordings expire after 15 minutes;
if a send reports an uncertain delivery, check the conversation before recording
and sending again.

Choose Automatic, Masculine, Feminine, or Neutral in app settings for your
outgoing voice, or conversation settings for a contact's translated voice.
Automatic approximately matches acoustic pitch and falls back to Neutral when
uncertain; it does not clone the speaker or identify their gender. Each choice
has an audible sample. Changing a setting applies to the next prepared recording.

Voice processing requires microphone permission, FFmpeg, and an OpenAI key with
access to `gpt-4o-mini-transcribe` and `gpt-4o-mini-tts`. Browser recording requires
HTTPS or localhost. Audio and transcripts are sent to OpenAI for processing;
cached recordings are stored in the private application database. Audio-service
charges are not currently included in the app's text-model usage totals.

## MCP access

The Streamable HTTP endpoint is `/mcp`. OAuth permissions are split into:

- `whatsapp.read` — status, conversation discovery, message history/search,
  and message preparation
- `whatsapp.send` — sending prepared messages and replies, reactions, and read
  receipts; this scope requires `whatsapp.read`

If a client does not request a scope, it receives read-only access. The legacy
`mcp` scope remains accepted for previously registered clients and grants both
read and send access.

The read workflow exposes `get_status`, `list_contacts`, `search_contacts`,
`read_messages`, `search_messages`, and `prepare_message`. External writes are
separate: `send_message`, `reply_to_message`, `react_to_message`, and
`mark_conversation_read`.

Text sends are deliberately two-step:

1. Call `prepare_message` to resolve the exact recipient, translation mode,
   target language, final text, and optional reply target.
2. Show that result to the user, then pass its short-lived preparation token
   and a unique idempotency key to `send_message` or `reply_to_message`.

When a conversation requires translation, preparation fails closed if the
translator is unavailable, errors, or returns empty text. The English source is
never sent as a fallback. Use `translation_mode: "never"` only when sending the
provided text unchanged is intentional.

## Deploy on Railway

1. Click the Railway button above.
2. Attach a persistent volume mounted at `/data`.
3. Set:
   - `OPENAI_API_KEY` if you want AI features
   - `WA_PASSWORD` (required)
4. Deploy and open the generated Railway domain.

The repo includes [railway.toml](/Users/vultuk/Development/Personal/whatsapp-translator/railway.toml) for Dockerfile-based deploys and unauthenticated `/api/health` health checks.

## Run With Docker

Build:

```bash
docker build -t whatsapp-translator .
```

Run:

```bash
docker run --rm -it \
  -p 3000:3000 \
  -v whatsapp-translator-data:/data \
  -e WA_WEB=true \
  -e WA_PASSWORD=choose_a_strong_password \
  -e OPENAI_API_KEY=your_key_here \
  whatsapp-translator
```

Then open:

```text
http://localhost:3000
```

## Data

The app stores local state in the data directory, including:

- `session.db` for WhatsApp session state
- `messages.db` for chats, settings, and usage
- OAuth client registrations and bearer/refresh tokens for approved MCP clients

Treat the data directory as secret material. OAuth tokens in `messages.db` are
bearer credentials for local MCP clients; protect the volume, set `WA_PASSWORD`
on reachable deployments, and use logout or the protected OAuth client revocation
API to revoke access. Do not commit those files or any `.env` file with real
secrets.

## Delivery and connection recovery

Incoming text is stored immediately, then translated by two background workers.
The persistent queue holds up to 1,000 pending jobs and retries failed jobs up to
three attempts. Queue overflow and exhausted retries retain the original message;
manual translation remains available. Completed translations update the existing
message without creating another unread message.

Native and browser clients reconnect with bounded backoff and refresh the open
conversation to recover messages missed while disconnected. Outgoing actions use
persisted idempotency keys. A lost response or server restart retains the same
operation identity; retrying that operation does not blindly send another message.
Pending sends remain visible as uncertain until confirmation arrives. A late
WhatsApp acknowledgement reconciles the stored message and operation result.
Check an uncertain conversation before composing a new send. These protections
cannot prove delivery when WhatsApp never returns an acknowledgement.

Keep the backend data volume and client app/browser storage intact for recovery.
Logout clears server recovery records. MCP send claims and outcomes also persist
across backend restarts. Deploy the updated backend before distributing the new
native clients. Network-accessible startup requires a nonblank password; explicit
password-free development binds only to loopback. Password verification allows
15 attempts per minute across the single-user service, including OAuth approval.
