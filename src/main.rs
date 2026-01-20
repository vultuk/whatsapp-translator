//! WhatsApp Translator - CLI application for connecting to WhatsApp and displaying messages.
//!
//! This application uses a Go bridge (wa-bridge) that implements the WhatsApp Web protocol
//! via the whatsmeow library. Communication happens via JSON-lines over stdio.

mod bridge;
mod cli;
mod display;
mod link_preview;
mod storage;
mod translation;
mod web;

use anyhow::{Context, Result};
use std::sync::Arc;
use tokio::signal;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};
use tracing_subscriber::EnvFilter;

use bridge::{BridgeConfig, BridgeEvent, BridgeProcess, ConnectionState, Message, MessageContent};
use cli::Args;
use display::{
    clear_qr_display, print_connected, print_error, print_info, print_warning, render_qr_code,
    MessageDisplay,
};
use storage::{MessageStore, StoredMessage};
use translation::TranslationService;
use web::AppState;

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse_args();

    // Initialize logging
    init_logging(args.verbose);

    // Determine data directory
    let data_dir = args
        .data_dir
        .clone()
        .or_else(|| bridge::default_data_dir().ok())
        .context("Could not determine data directory")?;

    info!("Using data directory: {:?}", data_dir);

    // Handle logout request
    if args.logout {
        handle_logout(&data_dir).await?;
    }

    // Find bridge binary
    let bridge_path = args
        .bridge_path
        .clone()
        .or_else(|| bridge::find_bridge_binary().ok())
        .context(
            "Could not find wa-bridge binary. Please build it first:\n\
             cd wa-bridge && go build -o wa-bridge .",
        )?;

    info!("Using bridge binary: {:?}", bridge_path);

    // Create bridge configuration
    let config = BridgeConfig {
        binary_path: bridge_path,
        data_dir: data_dir.clone(),
        verbose: args.verbose,
    };

    // Initialize translation service if API key provided
    let translator = args.claude_api_key.as_ref().map(|key| {
        info!("Translation enabled (target: {})", args.default_language);
        Arc::new(TranslationService::new(
            key.clone(),
            args.default_language.clone(),
        ))
    });

    if args.web {
        // Web server mode
        run_web_mode(config, args, data_dir, translator).await
    } else {
        // Terminal mode
        run_terminal_mode(config, args.json, translator).await
    }
}

/// Initialize the tracing subscriber for logging
fn init_logging(verbose: bool) {
    let filter = if verbose {
        EnvFilter::new("debug")
    } else {
        EnvFilter::new("info").add_directive("whatsapp_archiver=info".parse().unwrap())
    };

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .without_time()
        .init();
}

/// Handle logout by removing session data
async fn handle_logout(data_dir: &std::path::Path) -> Result<()> {
    let session_db = data_dir.join("session.db");
    if session_db.exists() {
        tokio::fs::remove_file(&session_db)
            .await
            .context("Failed to remove session database")?;
        print_info("Session cleared. You will need to scan a new QR code.");
    } else {
        print_info("No existing session found.");
    }
    Ok(())
}

/// Run in web server mode
async fn run_web_mode(
    config: BridgeConfig,
    args: Args,
    data_dir: std::path::PathBuf,
    translator: Option<Arc<TranslationService>>,
) -> Result<()> {
    // Initialize message store
    let store = MessageStore::new(&data_dir)?;

    // Find web directory (relative to executable or in project)
    let web_dir = find_web_dir()?;
    info!("Serving web files from: {:?}", web_dir);

    // Create app state with translator and password
    let state = AppState::new(
        store.clone(),
        web_dir,
        data_dir.clone(),
        translator.clone(),
        args.password.clone(),
    );

    // Spawn the web server (once, outside the bridge loop)
    let server_state = state.clone();
    let host = args.host.clone();
    let port = args.port;
    tokio::spawn(async move {
        if let Err(e) = web::start_server(server_state, &host, port).await {
            error!("Web server error: {}", e);
        }
    });

    // Handle Ctrl+C for graceful shutdown
    let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    tokio::spawn(async move {
        signal::ctrl_c().await.expect("Failed to listen for Ctrl+C");
        let _ = shutdown_tx.send(());
    });

    // Bridge restart loop - restarts bridge after logout
    loop {
        // Channel for receiving events from the bridge
        let (event_tx, mut event_rx) = mpsc::channel::<BridgeEvent>(100);

        // Spawn the bridge process
        print_info("Starting WhatsApp bridge...");
        let bridge = match BridgeProcess::spawn(config.clone(), event_tx).await {
            Ok(b) => b,
            Err(e) => {
                print_error(&format!("Failed to start bridge: {}", e));
                tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
                continue;
            }
        };

        // Pass the bridge's command sender to the app state for sending messages
        state.set_command_tx(bridge.command_sender()).await;

        // Event loop for this bridge instance
        let should_exit = loop {
            tokio::select! {
                _ = &mut shutdown_rx => {
                    print_info("Shutting down...");
                    let _ = bridge.shutdown().await;
                    break true; // Exit completely
                }

                event = event_rx.recv() => {
                    match event {
                        Some(event) => {
                            if let Err(e) = handle_web_event(event, &state, &store, translator.as_ref()).await {
                                error!("Error handling event: {}", e);
                            }
                        }
                        None => {
                            // Bridge terminated - check if it was a logout or unexpected
                            info!("Bridge process terminated, restarting...");
                            break false; // Restart bridge
                        }
                    }
                }
            }
        };

        if should_exit {
            break;
        }

        // Small delay before restarting
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    }

    Ok(())
}

/// Handle events in web mode
async fn handle_web_event(
    event: BridgeEvent,
    state: &Arc<AppState>,
    store: &MessageStore,
    translator: Option<&Arc<TranslationService>>,
) -> Result<()> {
    match event {
        BridgeEvent::Qr { data } => {
            debug!("Received QR code data");
            state.set_qr_code(data).await;
        }

        BridgeEvent::Connected { phone, name, .. } => {
            info!("Connected as {} ({})", name, phone);
            state.set_connected(true, Some(phone), Some(name)).await;
        }

        BridgeEvent::ConnectionState { state: conn_state } => match conn_state {
            ConnectionState::Disconnected | ConnectionState::LoggedOut => {
                state.set_connected(false, None, None).await;
            }
            _ => {}
        },

        BridgeEvent::Message(msg) => {
            // Extract unread count before moving msg
            let unread_count = msg.unread_count;
            let is_history = msg.is_history;

            // Process and store the message
            let stored_msg = process_message(msg, translator, Some(store)).await;

            // Update contact with contact_name (not sender_name!)
            // contact_name is the chat name (other person for DMs, group name for groups)
            // sender_name changes based on who sent the message
            store.upsert_contact(
                &stored_msg.contact_id,
                stored_msg.contact_name.as_deref(),
                stored_msg.contact_phone.as_deref(),
                Some(&stored_msg.chat_type),
                stored_msg.timestamp,
            )?;

            // Handle unread counts
            if let Some(unread) = unread_count {
                // History sync message with unread count from WhatsApp - use it directly
                store.set_unread_count(&stored_msg.contact_id, unread)?;
            } else if !stored_msg.is_from_me && !is_history {
                // Live incoming message - increment unread
                store.increment_unread(&stored_msg.contact_id)?;
            }

            // Store message
            store.add_message(&stored_msg)?;

            // Broadcast to WebSocket clients
            state.broadcast_message(stored_msg);
        }

        BridgeEvent::Error { code, message } => {
            error!("Bridge error [{}]: {}", code, message);
        }

        BridgeEvent::Log { level, message } => match level.as_str() {
            "error" => error!("{}", message),
            "warn" => warn!("{}", message),
            "info" => info!("{}", message),
            _ => debug!("{}", message),
        },

        BridgeEvent::LoggedOut { reason } => {
            warn!("Logged out: {}", reason);
            state.set_connected(false, None, None).await;
        }

        BridgeEvent::SendResult {
            request_id,
            success,
            message_id,
            timestamp,
            error,
        } => {
            if success {
                debug!(
                    "Message sent successfully: {:?} at {:?}",
                    message_id, timestamp
                );
            } else {
                error!(
                    "Failed to send message (request {}): {:?}",
                    request_id, error
                );
            }
            // TODO: Could broadcast send result to WebSocket clients for UI updates
        }

        BridgeEvent::ProfilePicture {
            request_id,
            jid: _,
            url,
            id: _,
            error,
        } => {
            if let Some(err) = error {
                debug!("Profile picture error (request {}): {}", request_id, err);
            }
            // Notify the waiting request
            state.handle_profile_picture_response(request_id, url).await;
        }

        BridgeEvent::ChatPresence {
            chat_id,
            user_id,
            state: presence_state,
        } => {
            let state_str = match presence_state {
                bridge::ChatPresenceState::Typing => "typing",
                bridge::ChatPresenceState::Paused => "paused",
                bridge::ChatPresenceState::Recording => "recording",
            };
            // Log at info level so it's always visible
            info!("Chat presence: {} is {} in {}", user_id, state_str, chat_id);
            // Broadcast to WebSocket clients
            state.broadcast_typing(chat_id, user_id, state_str.to_string());
        }

        BridgeEvent::MarkAsRead { chat_id } => {
            // Chat was marked as read from another device (e.g., user's phone)
            info!("Chat marked as read from another device: {}", chat_id);
            store.mark_as_read(&chat_id)?;
            // Broadcast to WebSocket clients so UI updates
            state.broadcast_mark_as_read(chat_id);
        }
    }

    Ok(())
}

/// Process a message, translating if necessary
async fn process_message(
    msg: Message,
    translator: Option<&Arc<TranslationService>>,
    store: Option<&storage::MessageStore>,
) -> StoredMessage {
    let contact_id = msg.chat.jid().to_string();
    let chat_type = match &msg.chat {
        bridge::Chat::Private { .. } => "private",
        bridge::Chat::Group { .. } => "group",
        bridge::Chat::Broadcast { .. } => "broadcast",
        bridge::Chat::Status { .. } => "status",
    };

    // Extract text content for translation (skip history messages)
    let (original_text, translated_text, source_language, is_translated) =
        if let Some(translator) = translator {
            if let Some(text) = extract_text_content(&msg.content) {
                if !msg.is_from_me && !msg.is_history {
                    // Only translate incoming messages (not history sync)
                    let result = translator.process_text(&text).await;

                    // Record usage if we have a store and there was actual API usage
                    if let Some(store) = store {
                        if result.usage.input_tokens > 0 {
                            if let Err(e) = store.record_usage(
                                Some(&contact_id),
                                Some(&msg.id),
                                &result.usage,
                                if result.needs_translation {
                                    "translate_incoming"
                                } else {
                                    "detect_language"
                                },
                            ) {
                                tracing::warn!("Failed to record usage: {}", e);
                            }
                        }
                    }

                    (
                        Some(result.original_text),
                        result.translated_text,
                        Some(result.source_language),
                        result.needs_translation,
                    )
                } else {
                    (Some(text), None, None, false)
                }
            } else {
                (None, None, None, false)
            }
        } else {
            (extract_text_content(&msg.content), None, None, false)
        };

    // Serialize content to JSON
    let content_json = serde_json::to_string(&msg.content).unwrap_or_default();
    let content: Option<serde_json::Value> = serde_json::from_str(&content_json).ok();
    let content_type = msg.content.type_name().to_string();

    // Get contact name and phone from chat info
    // For private chats: this is the other person
    // For groups: this is the group name
    let (contact_name, contact_phone) = match &msg.chat {
        bridge::Chat::Private { name, jid } => {
            let phone = jid.split('@').next().map(|s| s.to_string());
            (name.clone(), phone)
        }
        bridge::Chat::Group { name, .. } => (name.clone(), None),
        bridge::Chat::Broadcast { jid } => {
            let phone = jid.split('@').next().map(|s| s.to_string());
            (
                Some(format!(
                    "Broadcast: {}",
                    phone.as_deref().unwrap_or("Unknown")
                )),
                phone,
            )
        }
        bridge::Chat::Status { .. } => (Some("Status".to_string()), None),
    };

    StoredMessage {
        id: msg.id,
        contact_id,
        timestamp: msg.timestamp.timestamp_millis(),
        is_from_me: msg.is_from_me,
        is_forwarded: msg.is_forwarded,
        sender_name: msg.push_name.or_else(|| msg.from.name.clone()),
        sender_phone: Some(msg.from.phone),
        contact_name,
        contact_phone,
        chat_type: chat_type.to_string(),
        content_type,
        content_json,
        content,
        original_text,
        translated_text,
        source_language,
        is_translated,
    }
}

/// Extract text content from a message
fn extract_text_content(content: &MessageContent) -> Option<String> {
    match content {
        MessageContent::Text { body } => Some(body.clone()),
        MessageContent::Image {
            caption: Some(c), ..
        } => Some(c.clone()),
        MessageContent::Video {
            caption: Some(c), ..
        } => Some(c.clone()),
        MessageContent::Document {
            caption: Some(c), ..
        } => Some(c.clone()),
        _ => None,
    }
}

/// Find the web directory
fn find_web_dir() -> Result<std::path::PathBuf> {
    // Try various locations
    let candidates = [
        std::env::current_dir()?.join("web/public"),
        std::env::current_exe()?
            .parent()
            .unwrap()
            .join("../web/public"),
        std::env::current_exe()?
            .parent()
            .unwrap()
            .join("../../web/public"),
        dirs::data_dir()
            .unwrap_or_default()
            .join("whatsapp-translator/web"),
    ];

    for candidate in candidates {
        if candidate.exists() && candidate.join("index.html").exists() {
            return Ok(candidate);
        }
    }

    anyhow::bail!("Could not find web directory. Expected at ./web/public or near the executable.")
}

/// Main event loop for terminal mode
async fn run_terminal_mode(
    config: BridgeConfig,
    json_output: bool,
    translator: Option<Arc<TranslationService>>,
) -> Result<()> {
    // Channel for receiving events from the bridge
    let (event_tx, mut event_rx) = mpsc::channel::<BridgeEvent>(100);

    // Spawn the bridge process
    print_info("Starting WhatsApp bridge...");
    let bridge = BridgeProcess::spawn(config, event_tx)
        .await
        .context("Failed to start bridge process")?;

    let message_display = MessageDisplay::new();
    let mut connected = false;
    let mut qr_displayed = false;

    // Handle Ctrl+C for graceful shutdown
    let shutdown = async {
        signal::ctrl_c().await.expect("Failed to listen for Ctrl+C");
    };

    tokio::pin!(shutdown);

    loop {
        tokio::select! {
            // Check for shutdown signal
            _ = &mut shutdown => {
                print_info("Shutting down...");
                bridge.shutdown().await?;
                break;
            }

            // Process events from the bridge
            event = event_rx.recv() => {
                match event {
                    Some(event) => {
                        if json_output {
                            // Raw JSON output mode
                            if let Ok(json) = serde_json::to_string(&event) {
                                println!("{}", json);
                            }
                        } else {
                            handle_terminal_event(
                                event,
                                &message_display,
                                &mut connected,
                                &mut qr_displayed,
                                translator.as_ref(),
                            ).await?;
                        }
                    }
                    None => {
                        // Bridge closed the channel
                        if !connected {
                            print_error("Bridge process terminated unexpectedly");
                        }
                        break;
                    }
                }
            }
        }
    }

    Ok(())
}

/// Handle a single bridge event in terminal mode
async fn handle_terminal_event(
    event: BridgeEvent,
    message_display: &MessageDisplay,
    connected: &mut bool,
    qr_displayed: &mut bool,
    translator: Option<&Arc<TranslationService>>,
) -> Result<()> {
    match event {
        BridgeEvent::Qr { data } => {
            debug!("Received QR code data");
            render_qr_code(&data)?;
            *qr_displayed = true;
        }

        BridgeEvent::Connected { phone, name, .. } => {
            if *qr_displayed {
                clear_qr_display()?;
                *qr_displayed = false;
            }
            print_connected(&phone, &name);
            *connected = true;
        }

        BridgeEvent::ConnectionState { state } => {
            debug!("Connection state: {:?}", state);
            match state {
                ConnectionState::Connecting => {
                    print_info("Connecting to WhatsApp...");
                }
                ConnectionState::Connected => {
                    if !*connected {
                        print_info("Connected!");
                    }
                }
                ConnectionState::Disconnected => {
                    print_warning("Disconnected from WhatsApp");
                    *connected = false;
                }
                ConnectionState::Reconnecting => {
                    print_info("Reconnecting...");
                }
                ConnectionState::LoggedOut => {
                    print_warning("Logged out from WhatsApp");
                    *connected = false;
                }
            }
        }

        BridgeEvent::Message(msg) => {
            // Translate if needed
            if let Some(translator) = translator {
                if !msg.is_from_me {
                    if let Some(text) = extract_text_content(&msg.content) {
                        let result = translator.process_text(&text).await;
                        if result.needs_translation {
                            // Display with translation
                            message_display.display_with_translation(
                                &msg,
                                &result.translated_text.unwrap_or(text),
                                &result.source_language,
                            )?;
                            return Ok(());
                        }
                    }
                }
            }
            message_display.display(&msg)?;
        }

        BridgeEvent::Error { code, message } => {
            error!("Bridge error [{}]: {}", code, message);
            print_error(&format!("[{}] {}", code, message));
        }

        BridgeEvent::Log { level, message } => match level.as_str() {
            "error" => error!("{}", message),
            "warn" => warn!("{}", message),
            "info" => info!("{}", message),
            _ => debug!("{}", message),
        },

        BridgeEvent::LoggedOut { reason } => {
            print_warning(&format!("Logged out: {}", reason));
            print_info("Please restart the application to scan a new QR code.");
            *connected = false;
        }

        BridgeEvent::SendResult {
            request_id,
            success,
            message_id,
            timestamp,
            error,
        } => {
            if success {
                debug!(
                    "Message sent successfully: {:?} at {:?}",
                    message_id, timestamp
                );
            } else {
                error!(
                    "Failed to send message (request {}): {:?}",
                    request_id, error
                );
            }
        }

        BridgeEvent::ProfilePicture { .. } => {
            // Profile pictures are only used in web mode
            debug!("Ignoring profile picture event in terminal mode");
        }

        BridgeEvent::ChatPresence { .. } => {
            // Typing indicators are only used in web mode
            debug!("Ignoring chat presence event in terminal mode");
        }

        BridgeEvent::MarkAsRead { .. } => {
            // Mark-as-read events are only used in web mode
            debug!("Ignoring mark-as-read event in terminal mode");
        }
    }

    Ok(())
}

// Implement Serialize for BridgeEvent for JSON output mode
impl serde::Serialize for BridgeEvent {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeMap;

        let mut map = serializer.serialize_map(None)?;

        match self {
            BridgeEvent::Qr { data } => {
                map.serialize_entry("type", "qr")?;
                map.serialize_entry("data", data)?;
            }
            BridgeEvent::Connected {
                phone,
                name,
                platform,
            } => {
                map.serialize_entry("type", "connected")?;
                map.serialize_entry("phone", phone)?;
                map.serialize_entry("name", name)?;
                if let Some(p) = platform {
                    map.serialize_entry("platform", p)?;
                }
            }
            BridgeEvent::ConnectionState { state } => {
                map.serialize_entry("type", "connection_state")?;
                map.serialize_entry("state", state)?;
            }
            BridgeEvent::Message(msg) => {
                map.serialize_entry("type", "message")?;
                map.serialize_entry("message", msg)?;
            }
            BridgeEvent::Error { code, message } => {
                map.serialize_entry("type", "error")?;
                map.serialize_entry("code", code)?;
                map.serialize_entry("message", message)?;
            }
            BridgeEvent::Log { level, message } => {
                map.serialize_entry("type", "log")?;
                map.serialize_entry("level", level)?;
                map.serialize_entry("message", message)?;
            }
            BridgeEvent::LoggedOut { reason } => {
                map.serialize_entry("type", "logged_out")?;
                map.serialize_entry("reason", reason)?;
            }
            BridgeEvent::SendResult {
                request_id,
                success,
                message_id,
                timestamp,
                error,
            } => {
                map.serialize_entry("type", "send_result")?;
                map.serialize_entry("request_id", request_id)?;
                map.serialize_entry("success", success)?;
                if let Some(id) = message_id {
                    map.serialize_entry("message_id", id)?;
                }
                if let Some(ts) = timestamp {
                    map.serialize_entry("timestamp", ts)?;
                }
                if let Some(err) = error {
                    map.serialize_entry("error", err)?;
                }
            }
            BridgeEvent::ProfilePicture {
                request_id,
                jid,
                url,
                id,
                error,
            } => {
                map.serialize_entry("type", "profile_picture")?;
                map.serialize_entry("request_id", request_id)?;
                map.serialize_entry("jid", jid)?;
                if let Some(u) = url {
                    map.serialize_entry("url", u)?;
                }
                if let Some(i) = id {
                    map.serialize_entry("id", i)?;
                }
                if let Some(err) = error {
                    map.serialize_entry("error", err)?;
                }
            }
            BridgeEvent::ChatPresence {
                chat_id,
                user_id,
                state,
            } => {
                map.serialize_entry("type", "chat_presence")?;
                map.serialize_entry("chat_id", chat_id)?;
                map.serialize_entry("user_id", user_id)?;
                let state_str = match state {
                    bridge::ChatPresenceState::Typing => "typing",
                    bridge::ChatPresenceState::Paused => "paused",
                    bridge::ChatPresenceState::Recording => "recording",
                };
                map.serialize_entry("state", state_str)?;
            }
            BridgeEvent::MarkAsRead { chat_id } => {
                map.serialize_entry("type", "mark_as_read")?;
                map.serialize_entry("chat_id", chat_id)?;
            }
        }

        map.end()
    }
}

// Implement Serialize for Message
impl serde::Serialize for bridge::Message {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;

        let mut s = serializer.serialize_struct("Message", 8)?;
        s.serialize_field("id", &self.id)?;
        s.serialize_field("timestamp", &self.timestamp.timestamp())?;
        s.serialize_field("from", &self.from)?;
        s.serialize_field("chat", &self.chat)?;
        s.serialize_field("content", &self.content)?;
        s.serialize_field("is_from_me", &self.is_from_me)?;
        s.serialize_field("is_forwarded", &self.is_forwarded)?;
        s.serialize_field("push_name", &self.push_name)?;
        s.end()
    }
}

// Implement Serialize for Contact
impl serde::Serialize for bridge::Contact {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;

        let mut s = serializer.serialize_struct("Contact", 3)?;
        s.serialize_field("jid", &self.jid)?;
        s.serialize_field("phone", &self.phone)?;
        s.serialize_field("name", &self.name)?;
        s.end()
    }
}

// Implement Serialize for Chat
impl serde::Serialize for bridge::Chat {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeMap;

        let mut map = serializer.serialize_map(None)?;

        match self {
            bridge::Chat::Private { jid, name } => {
                map.serialize_entry("type", "private")?;
                map.serialize_entry("jid", jid)?;
                if let Some(n) = name {
                    map.serialize_entry("name", n)?;
                }
            }
            bridge::Chat::Group {
                jid,
                name,
                participant_count,
            } => {
                map.serialize_entry("type", "group")?;
                map.serialize_entry("jid", jid)?;
                if let Some(n) = name {
                    map.serialize_entry("name", n)?;
                }
                if let Some(c) = participant_count {
                    map.serialize_entry("participant_count", c)?;
                }
            }
            bridge::Chat::Broadcast { jid } => {
                map.serialize_entry("type", "broadcast")?;
                map.serialize_entry("jid", jid)?;
            }
            bridge::Chat::Status { jid } => {
                map.serialize_entry("type", "status")?;
                map.serialize_entry("jid", jid)?;
            }
        }

        map.end()
    }
}

// Implement Serialize for MessageContent
impl serde::Serialize for bridge::MessageContent {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeMap;

        let mut map = serializer.serialize_map(None)?;

        match self {
            bridge::MessageContent::Text { body } => {
                map.serialize_entry("type", "text")?;
                map.serialize_entry("body", body)?;
            }
            bridge::MessageContent::Image {
                caption,
                mime_type,
                file_size,
                file_hash,
                media_data,
            } => {
                map.serialize_entry("type", "image")?;
                map.serialize_entry("mime_type", mime_type)?;
                map.serialize_entry("file_size", file_size)?;
                if let Some(c) = caption {
                    map.serialize_entry("caption", c)?;
                }
                if let Some(h) = file_hash {
                    map.serialize_entry("file_hash", h)?;
                }
                if let Some(data) = media_data {
                    map.serialize_entry("media_data", data)?;
                }
            }
            bridge::MessageContent::Video {
                caption,
                mime_type,
                file_size,
                duration_seconds,
                media_data,
            } => {
                map.serialize_entry("type", "video")?;
                map.serialize_entry("mime_type", mime_type)?;
                map.serialize_entry("file_size", file_size)?;
                if let Some(c) = caption {
                    map.serialize_entry("caption", c)?;
                }
                if let Some(d) = duration_seconds {
                    map.serialize_entry("duration_seconds", d)?;
                }
                if let Some(data) = media_data {
                    map.serialize_entry("media_data", data)?;
                }
            }
            bridge::MessageContent::Audio {
                mime_type,
                file_size,
                duration_seconds,
                is_voice_note,
                media_data,
            } => {
                map.serialize_entry("type", "audio")?;
                map.serialize_entry("mime_type", mime_type)?;
                map.serialize_entry("file_size", file_size)?;
                map.serialize_entry("is_voice_note", is_voice_note)?;
                if let Some(d) = duration_seconds {
                    map.serialize_entry("duration_seconds", d)?;
                }
                if let Some(data) = media_data {
                    map.serialize_entry("media_data", data)?;
                }
            }
            bridge::MessageContent::Document {
                caption,
                mime_type,
                file_name,
                file_size,
                media_data,
            } => {
                map.serialize_entry("type", "document")?;
                map.serialize_entry("mime_type", mime_type)?;
                map.serialize_entry("file_size", file_size)?;
                if let Some(c) = caption {
                    map.serialize_entry("caption", c)?;
                }
                if let Some(n) = file_name {
                    map.serialize_entry("file_name", n)?;
                }
                if let Some(data) = media_data {
                    map.serialize_entry("media_data", data)?;
                }
            }
            bridge::MessageContent::Sticker {
                mime_type,
                is_animated,
                media_data,
            } => {
                map.serialize_entry("type", "sticker")?;
                map.serialize_entry("mime_type", mime_type)?;
                map.serialize_entry("is_animated", is_animated)?;
                if let Some(data) = media_data {
                    map.serialize_entry("media_data", data)?;
                }
            }
            bridge::MessageContent::Location {
                latitude,
                longitude,
                name,
                address,
            } => {
                map.serialize_entry("type", "location")?;
                map.serialize_entry("latitude", latitude)?;
                map.serialize_entry("longitude", longitude)?;
                if let Some(n) = name {
                    map.serialize_entry("name", n)?;
                }
                if let Some(a) = address {
                    map.serialize_entry("address", a)?;
                }
            }
            bridge::MessageContent::Contact {
                display_name,
                vcard,
            } => {
                map.serialize_entry("type", "contact")?;
                map.serialize_entry("display_name", display_name)?;
                map.serialize_entry("vcard", vcard)?;
            }
            bridge::MessageContent::Reaction {
                emoji,
                target_message_id,
            } => {
                map.serialize_entry("type", "reaction")?;
                map.serialize_entry("emoji", emoji)?;
                map.serialize_entry("target_message_id", target_message_id)?;
            }
            bridge::MessageContent::Revoked => {
                map.serialize_entry("type", "revoked")?;
            }
            bridge::MessageContent::Poll { question, options } => {
                map.serialize_entry("type", "poll")?;
                map.serialize_entry("question", question)?;
                map.serialize_entry("options", options)?;
            }
            bridge::MessageContent::Unknown { raw_type } => {
                map.serialize_entry("type", "unknown")?;
                map.serialize_entry("raw_type", raw_type)?;
            }
        }

        map.end()
    }
}
