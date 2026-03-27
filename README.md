# WhatsApp Translator

Small WhatsApp Web client with a local web UI, message history, translation, AI replies, and MCP access.

## Features

- Connect a WhatsApp account through QR login
- View chats and messages in a local web UI
- Send text, images, replies, and reactions
- Translate incoming and outgoing messages with OpenAI
- Generate AI-composed messages and AI replies in your writing style
- Per-chat translation settings
- Local SQLite storage for messages, usage, and session data
- MCP endpoint with OAuth support

## Required Environment Variables

Minimum:

- `WA_WEB=true`

Required for AI features:

- `OPENAI_API_KEY`

Optional:

- `WA_OPENAI_DETECTION_MODEL` default: `gpt-5.4-nano`
- `WA_OPENAI_TRANSLATION_MODEL` default: `gpt-5.4-mini`
- `WA_OPENAI_HIGH_END_MODEL` default: `gpt-5.4`
- `WA_DEFAULT_LANGUAGE` default: `English`
- `WA_PASSWORD` password for the web UI
- `WA_HOST` default: `0.0.0.0`
- `WA_PORT` default: `3000`
- `WA_DATA_DIR` data directory for `session.db` and `messages.db`
- `WA_BRIDGE_PATH` path to the `wa-bridge` binary
- `WA_VERBOSE=true` enable verbose logs
- `WA_LOGOUT=true` clear the WhatsApp session on startup

## Run Locally

Prerequisites:

- Rust
- Go

Run:

```bash
export WA_WEB=true
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
export WA_BRIDGE_PATH="$PWD/wa-bridge/wa-bridge"
export OPENAI_API_KEY=your_key_here

cargo run --release
```

Then open:

```text
http://localhost:3000
```

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

Do not commit those files or any `.env` file with real secrets.
