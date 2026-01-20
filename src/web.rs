//! Web server for the WhatsApp Translator frontend.
//!
//! Provides REST API endpoints and WebSocket support for real-time updates.

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, State,
    },
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::ServeDir;
use tracing::{error, info};

use crate::bridge::BridgeCommand;
use crate::storage::{MessageStore, StoredMessage};
use crate::translation::TranslationService;
use tokio::sync::mpsc;

/// Shared application state
pub struct AppState {
    pub store: MessageStore,
    pub connected: RwLock<bool>,
    pub phone: RwLock<Option<String>>,
    pub name: RwLock<Option<String>>,
    pub qr_code: RwLock<Option<String>>,
    pub broadcast_tx: broadcast::Sender<WebSocketEvent>,
    pub web_dir: PathBuf,
    pub command_tx: RwLock<Option<mpsc::Sender<BridgeCommand>>>,
    pub translator: Option<Arc<TranslationService>>,
}

/// Events sent to WebSocket clients
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WebSocketEvent {
    Status {
        connected: bool,
        phone: Option<String>,
        name: Option<String>,
    },
    Qr {
        data: String,
    },
    Connected {
        phone: String,
        name: String,
    },
    Disconnected,
    Message {
        message: StoredMessage,
    },
    Error {
        error: String,
    },
}

/// API status response
#[derive(Serialize)]
struct StatusResponse {
    connected: bool,
    phone: Option<String>,
    name: Option<String>,
}

/// API QR response
#[derive(Serialize)]
struct QrResponse {
    qr: Option<String>,
}

/// Send message request
#[derive(Deserialize)]
pub struct SendMessageRequest {
    #[serde(rename = "contactId")]
    pub contact_id: String,
    pub text: String,
}

/// Send message response
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SendMessageResponse {
    pub message_id: String,
    pub timestamp: i64,
    /// Whether the message was translated before sending
    pub is_translated: bool,
    /// The translated text that was actually sent (if translated)
    pub translated_text: Option<String>,
    /// The target language (if translated)
    pub source_language: Option<String>,
}

impl AppState {
    pub fn new(
        store: MessageStore,
        web_dir: PathBuf,
        translator: Option<Arc<TranslationService>>,
    ) -> Arc<Self> {
        let (broadcast_tx, _) = broadcast::channel(100);

        Arc::new(Self {
            store,
            connected: RwLock::new(false),
            phone: RwLock::new(None),
            name: RwLock::new(None),
            qr_code: RwLock::new(None),
            broadcast_tx,
            web_dir,
            command_tx: RwLock::new(None),
            translator,
        })
    }

    /// Set the bridge command sender
    pub async fn set_command_tx(&self, tx: mpsc::Sender<BridgeCommand>) {
        *self.command_tx.write().await = Some(tx);
    }

    /// Send a command to the bridge
    pub async fn send_bridge_command(&self, cmd: BridgeCommand) -> Result<(), String> {
        let tx = self.command_tx.read().await;
        if let Some(tx) = tx.as_ref() {
            tx.send(cmd).await.map_err(|e| e.to_string())
        } else {
            Err("Bridge not connected".to_string())
        }
    }

    /// Update connection status
    pub async fn set_connected(
        &self,
        connected: bool,
        phone: Option<String>,
        name: Option<String>,
    ) {
        *self.connected.write().await = connected;
        *self.phone.write().await = phone.clone();
        *self.name.write().await = name.clone();

        if connected {
            *self.qr_code.write().await = None;
            let _ = self.broadcast_tx.send(WebSocketEvent::Connected {
                phone: phone.unwrap_or_default(),
                name: name.unwrap_or_default(),
            });
        } else {
            let _ = self.broadcast_tx.send(WebSocketEvent::Disconnected);
        }
    }

    /// Set QR code
    pub async fn set_qr_code(&self, qr: String) {
        *self.qr_code.write().await = Some(qr.clone());
        let _ = self.broadcast_tx.send(WebSocketEvent::Qr { data: qr });
    }

    /// Broadcast a new message
    pub fn broadcast_message(&self, message: StoredMessage) {
        let _ = self.broadcast_tx.send(WebSocketEvent::Message { message });
    }
}

/// Create the web server router
pub fn create_router(state: Arc<AppState>) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // Serve static files from the web directory
    let serve_dir = ServeDir::new(&state.web_dir);

    Router::new()
        // API routes
        .route("/api/status", get(get_status))
        .route("/api/contacts", get(get_contacts))
        .route("/api/messages/:contact_id", get(get_messages))
        .route("/api/qr", get(get_qr))
        .route("/api/send", post(send_message))
        .route("/api/stats", get(get_stats))
        // WebSocket
        .route("/ws", get(websocket_handler))
        // Serve static files
        .fallback_service(serve_dir)
        .layer(cors)
        .with_state(state)
}

/// Start the web server
pub async fn start_server(state: Arc<AppState>, host: &str, port: u16) -> anyhow::Result<()> {
    let addr: SocketAddr = format!("{}:{}", host, port).parse()?;
    let router = create_router(state);

    info!("Web server running at http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, router).await?;

    Ok(())
}

// API Handlers

async fn get_status(State(state): State<Arc<AppState>>) -> Json<StatusResponse> {
    Json(StatusResponse {
        connected: *state.connected.read().await,
        phone: state.phone.read().await.clone(),
        name: state.name.read().await.clone(),
    })
}

async fn get_contacts(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    match state.store.get_contacts() {
        Ok(contacts) => Json(contacts).into_response(),
        Err(e) => {
            error!("Failed to get contacts: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to get contacts").into_response()
        }
    }
}

async fn get_messages(
    State(state): State<Arc<AppState>>,
    Path(contact_id): Path<String>,
) -> impl IntoResponse {
    let contact_id = urlencoding::decode(&contact_id)
        .map(|s| s.into_owned())
        .unwrap_or(contact_id);

    match state.store.get_messages(&contact_id) {
        Ok(messages) => Json(messages).into_response(),
        Err(e) => {
            error!("Failed to get messages: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to get messages").into_response()
        }
    }
}

async fn get_qr(State(state): State<Arc<AppState>>) -> Json<QrResponse> {
    Json(QrResponse {
        qr: state.qr_code.read().await.clone(),
    })
}

async fn send_message(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SendMessageRequest>,
) -> impl IntoResponse {
    // Validate input
    if req.contact_id.is_empty() || req.text.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "contact_id and text are required"
            })),
        )
            .into_response();
    }

    // Check if connected
    if !*state.connected.read().await {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": "Not connected to WhatsApp"
            })),
        )
            .into_response();
    }

    // Determine the text to send - translate if needed based on conversation language
    let (text_to_send, _original_text, was_translated, target_language) =
        if let Some(translator) = &state.translator {
            // Check what language the contact has been using
            match state.store.get_conversation_language(&req.contact_id, 10) {
                Ok(Some(conv_lang)) => {
                    // Contact uses a specific language - translate our message to it
                    info!(
                        "Conversation language for {} is {}",
                        req.contact_id, conv_lang
                    );
                    match translator.translate_to(&req.text, &conv_lang).await {
                        Ok(translated) => {
                            if translated != req.text {
                                info!("Translated outgoing message to {}", conv_lang);
                                (translated, Some(req.text.clone()), true, Some(conv_lang))
                            } else {
                                (req.text.clone(), None, false, None)
                            }
                        }
                        Err(e) => {
                            error!("Failed to translate outgoing message: {}", e);
                            (req.text.clone(), None, false, None)
                        }
                    }
                }
                Ok(None) => {
                    // No conversation history or language detected
                    (req.text.clone(), None, false, None)
                }
                Err(e) => {
                    error!("Failed to get conversation language: {}", e);
                    (req.text.clone(), None, false, None)
                }
            }
        } else {
            (req.text.clone(), None, false, None)
        };

    // Send the message via bridge
    let cmd = BridgeCommand::Send {
        request_id: None, // We don't track request IDs for now, response is fire-and-forget
        to: req.contact_id.clone(),
        text: text_to_send.clone(),
    };

    if let Err(e) = state.send_bridge_command(cmd).await {
        error!("Failed to send message: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": format!("Failed to send message: {}", e)
            })),
        )
            .into_response();
    }

    // Generate a temporary message ID and timestamp for immediate response
    // The actual message ID will come back via the bridge's send_result event
    let timestamp = chrono::Utc::now().timestamp_millis();
    let temp_message_id = format!("pending_{}", timestamp);

    // Store the sent message locally
    // For outgoing translated messages:
    // - content.body = what user typed (English) - THIS IS DISPLAYED
    // - original_text = what user typed (English) - same as content for consistency
    // - translated_text = what was actually sent (foreign language) - SHOWN IN TOOLTIP
    // - source_language = the language we translated TO (e.g., "French")
    let stored_msg = StoredMessage {
        id: temp_message_id.clone(),
        contact_id: req.contact_id.clone(),
        timestamp,
        is_from_me: true,
        is_forwarded: false,
        sender_name: state.name.read().await.clone(),
        sender_phone: state.phone.read().await.clone(),
        chat_type: "private".to_string(), // Default to private, could be improved
        content_type: "Text".to_string(),
        // Store English (what user typed) as the content for display
        content_json: serde_json::json!({"type": "text", "body": req.text.clone()}).to_string(),
        content: Some(serde_json::json!({"type": "text", "body": req.text.clone()})),
        original_text: if was_translated {
            Some(req.text.clone())
        } else {
            None
        },
        // Store the translated text (what was actually sent) for the tooltip
        translated_text: if was_translated {
            Some(text_to_send.clone())
        } else {
            None
        },
        source_language: target_language.clone(), // The language we translated TO
        is_translated: was_translated,
    };

    // Store the message (don't broadcast - frontend already displays it optimistically)
    if let Err(e) = state.store.add_message(&stored_msg) {
        error!("Failed to store sent message: {}", e);
    }

    // Update contact's last message time
    if let Err(e) = state.store.upsert_contact(
        &stored_msg.contact_id,
        None,
        None,
        Some(&stored_msg.chat_type),
        stored_msg.timestamp,
    ) {
        error!("Failed to update contact: {}", e);
    }

    // Note: We don't broadcast sent messages - the frontend displays them immediately.
    // The message is stored in the DB so it will appear when the conversation is reloaded.

    Json(SendMessageResponse {
        message_id: temp_message_id,
        timestamp,
        is_translated: was_translated,
        translated_text: if was_translated {
            Some(text_to_send)
        } else {
            None
        },
        source_language: target_language,
    })
    .into_response()
}

async fn get_stats(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    match state.store.get_stats() {
        Ok((messages, contacts)) => Json(serde_json::json!({
            "messageCount": messages,
            "contactCount": contacts,
        }))
        .into_response(),
        Err(e) => {
            error!("Failed to get stats: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to get stats").into_response()
        }
    }
}

// WebSocket handler

async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_websocket(socket, state))
}

async fn handle_websocket(socket: WebSocket, state: Arc<AppState>) {
    let (mut sender, mut receiver) = socket.split();

    // Subscribe to broadcast events
    let mut rx = state.broadcast_tx.subscribe();

    // Send current status
    let status = WebSocketEvent::Status {
        connected: *state.connected.read().await,
        phone: state.phone.read().await.clone(),
        name: state.name.read().await.clone(),
    };

    if let Ok(json) = serde_json::to_string(&status) {
        let _ = sender.send(Message::Text(json)).await;
    }

    // Send current QR if available
    if let Some(qr) = state.qr_code.read().await.clone() {
        let event = WebSocketEvent::Qr { data: qr };
        if let Ok(json) = serde_json::to_string(&event) {
            let _ = sender.send(Message::Text(json)).await;
        }
    }

    // Handle incoming messages and broadcast events
    loop {
        tokio::select! {
            // Broadcast events to client
            Ok(event) = rx.recv() => {
                if let Ok(json) = serde_json::to_string(&event) {
                    if sender.send(Message::Text(json)).await.is_err() {
                        break;
                    }
                }
            }

            // Handle client messages (for future use)
            msg = receiver.next() => {
                match msg {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(Message::Ping(data))) => {
                        let _ = sender.send(Message::Pong(data)).await;
                    }
                    _ => {}
                }
            }
        }
    }
}
