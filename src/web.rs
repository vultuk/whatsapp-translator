//! Web server for the WhatsApp Translator frontend.
//!
//! Provides REST API endpoints and WebSocket support for real-time updates.

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, Query, State,
    },
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Arc;
use tokio::sync::{broadcast, oneshot, RwLock};
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::ServeDir;
use tracing::{error, info, warn};

use crate::bridge::BridgeCommand;
use crate::storage::{MessageStore, StoredMessage};
use crate::translation::TranslationService;
use tokio::sync::mpsc;

/// Profile picture cache entry
#[derive(Debug, Clone)]
pub struct ProfilePicture {
    pub url: Option<String>,
    pub fetched_at: i64,
}

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
    /// Cache of profile pictures (JID -> ProfilePicture)
    pub avatar_cache: RwLock<HashMap<String, ProfilePicture>>,
    /// Pending profile picture requests (request_id -> sender)
    pub pending_avatar_requests: RwLock<HashMap<i32, oneshot::Sender<Option<String>>>>,
    /// Request ID counter
    pub request_id_counter: AtomicI32,
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
    Typing {
        chat_id: String,
        user_id: String,
        state: String, // "typing", "paused", or "recording"
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
#[serde(rename_all = "camelCase")]
pub struct SendMessageRequest {
    pub contact_id: String,
    pub text: String,
    /// Message ID to reply to (optional)
    pub reply_to: Option<String>,
    /// Sender JID of the replied message (optional)
    pub reply_to_sender: Option<String>,
    /// Text preview of the replied message (for storage)
    pub reply_to_text: Option<String>,
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

/// Send image request
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendImageRequest {
    pub contact_id: String,
    /// Base64 encoded image data
    pub media_data: String,
    pub mime_type: String,
    pub caption: Option<String>,
    /// Message ID to reply to (optional)
    pub reply_to: Option<String>,
    /// Sender JID of the replied message (optional)
    pub reply_to_sender: Option<String>,
}

/// Send image response
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SendImageResponse {
    pub message_id: String,
    pub timestamp: i64,
}

/// Send reaction request
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendReactionRequest {
    pub contact_id: String,
    pub message_id: String,
    pub sender_jid: Option<String>,
    pub emoji: String,
}

/// Send reaction response
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SendReactionResponse {
    pub success: bool,
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
            avatar_cache: RwLock::new(HashMap::new()),
            pending_avatar_requests: RwLock::new(HashMap::new()),
            request_id_counter: AtomicI32::new(1),
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

    /// Broadcast a typing indicator
    pub fn broadcast_typing(&self, chat_id: String, user_id: String, state: String) {
        tracing::info!(
            "Broadcasting typing event: chat={}, user={}, state={}",
            chat_id,
            user_id,
            state
        );
        let result = self.broadcast_tx.send(WebSocketEvent::Typing {
            chat_id,
            user_id,
            state,
        });
        if let Err(e) = result {
            tracing::warn!("Failed to broadcast typing event: {}", e);
        }
    }

    /// Get next request ID
    pub fn next_request_id(&self) -> i32 {
        self.request_id_counter.fetch_add(1, Ordering::SeqCst)
    }

    /// Request a profile picture and wait for the response
    pub async fn get_profile_picture(&self, jid: &str) -> Option<String> {
        // Check cache first (valid for 1 hour)
        let now = chrono::Utc::now().timestamp();
        {
            let cache = self.avatar_cache.read().await;
            if let Some(cached) = cache.get(jid) {
                if now - cached.fetched_at < 3600 {
                    return cached.url.clone();
                }
            }
        }

        // Not in cache or expired, request from bridge
        let request_id = self.next_request_id();
        let (tx, rx) = oneshot::channel();

        // Register pending request
        {
            let mut pending = self.pending_avatar_requests.write().await;
            pending.insert(request_id, tx);
        }

        // Send command to bridge
        let cmd = BridgeCommand::GetProfilePicture {
            request_id,
            to: jid.to_string(),
        };

        if let Err(e) = self.send_bridge_command(cmd).await {
            error!("Failed to request profile picture: {}", e);
            // Clean up pending request
            let mut pending = self.pending_avatar_requests.write().await;
            pending.remove(&request_id);
            return None;
        }

        // Wait for response with timeout
        match tokio::time::timeout(std::time::Duration::from_secs(10), rx).await {
            Ok(Ok(url)) => {
                // Cache the result
                let mut cache = self.avatar_cache.write().await;
                cache.insert(
                    jid.to_string(),
                    ProfilePicture {
                        url: url.clone(),
                        fetched_at: now,
                    },
                );
                url
            }
            _ => {
                // Timeout or error, clean up
                let mut pending = self.pending_avatar_requests.write().await;
                pending.remove(&request_id);
                None
            }
        }
    }

    /// Handle profile picture response from bridge
    pub async fn handle_profile_picture_response(&self, request_id: i32, url: Option<String>) {
        let mut pending = self.pending_avatar_requests.write().await;
        if let Some(tx) = pending.remove(&request_id) {
            let _ = tx.send(url);
        }
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
        .route("/api/avatar/:jid", get(get_avatar))
        .route("/api/qr", get(get_qr))
        .route("/api/send", post(send_message))
        .route("/api/send-image", post(send_image))
        .route("/api/react", post(send_reaction))
        .route("/api/stats", get(get_stats))
        .route("/api/usage", get(get_global_usage))
        .route("/api/usage/:contact_id", get(get_conversation_usage))
        .route("/api/link-preview", get(get_link_preview))
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

/// Avatar response
#[derive(Serialize)]
struct AvatarResponse {
    url: Option<String>,
}

async fn get_avatar(
    State(state): State<Arc<AppState>>,
    Path(jid): Path<String>,
) -> impl IntoResponse {
    let jid = urlencoding::decode(&jid)
        .map(|s| s.into_owned())
        .unwrap_or(jid);

    // Check if connected
    if !*state.connected.read().await {
        return Json(AvatarResponse { url: None }).into_response();
    }

    let url = state.get_profile_picture(&jid).await;
    Json(AvatarResponse { url }).into_response()
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
                        Ok((translated, usage)) => {
                            // Record usage if there was actual API usage
                            if usage.input_tokens > 0 {
                                if let Err(e) = state.store.record_usage(
                                    Some(&req.contact_id),
                                    None, // No message ID for outgoing yet
                                    &usage,
                                    "translate_outgoing",
                                ) {
                                    warn!("Failed to record usage: {}", e);
                                }
                            }

                            if translated != req.text {
                                info!(
                                    "Translated outgoing message to {} (cost: ${:.6})",
                                    conv_lang, usage.cost_usd
                                );
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
        reply_to: req.reply_to.clone(),
        reply_to_sender: req.reply_to_sender.clone(),
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

async fn send_image(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SendImageRequest>,
) -> impl IntoResponse {
    // Validate input
    if req.contact_id.is_empty() || req.media_data.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "contact_id and media_data are required"
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

    // Send the image via bridge
    let cmd = BridgeCommand::SendImage {
        request_id: None,
        to: req.contact_id.clone(),
        media_data: req.media_data.clone(),
        mime_type: req.mime_type.clone(),
        caption: req.caption.clone(),
        reply_to: req.reply_to.clone(),
        reply_to_sender: req.reply_to_sender.clone(),
    };

    if let Err(e) = state.send_bridge_command(cmd).await {
        error!("Failed to send image: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": format!("Failed to send image: {}", e)
            })),
        )
            .into_response();
    }

    // Generate a temporary message ID and timestamp for immediate response
    let timestamp = chrono::Utc::now().timestamp_millis();
    let temp_message_id = format!("pending_img_{}", timestamp);

    // Store the sent image message locally
    let stored_msg = crate::storage::StoredMessage {
        id: temp_message_id.clone(),
        contact_id: req.contact_id.clone(),
        timestamp,
        is_from_me: true,
        is_forwarded: false,
        sender_name: state.name.read().await.clone(),
        sender_phone: state.phone.read().await.clone(),
        chat_type: "private".to_string(),
        content_type: "Image".to_string(),
        content_json: serde_json::json!({
            "type": "image",
            "mime_type": req.mime_type,
            "caption": req.caption,
            "media_data": req.media_data
        })
        .to_string(),
        content: Some(serde_json::json!({
            "type": "image",
            "mime_type": req.mime_type,
            "caption": req.caption,
            "media_data": req.media_data
        })),
        original_text: None,
        translated_text: None,
        source_language: None,
        is_translated: false,
    };

    // Store the message
    if let Err(e) = state.store.add_message(&stored_msg) {
        error!("Failed to store sent image: {}", e);
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

    Json(SendImageResponse {
        message_id: temp_message_id,
        timestamp,
    })
    .into_response()
}

async fn send_reaction(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SendReactionRequest>,
) -> impl IntoResponse {
    // Validate input
    if req.contact_id.is_empty() || req.message_id.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "contact_id and message_id are required"
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

    // Send the reaction via bridge
    let cmd = BridgeCommand::SendReaction {
        request_id: None,
        to: req.contact_id.clone(),
        message_id: req.message_id.clone(),
        sender_jid: req.sender_jid.clone(),
        emoji: req.emoji.clone(),
    };

    if let Err(e) = state.send_bridge_command(cmd).await {
        error!("Failed to send reaction: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": format!("Failed to send reaction: {}", e)
            })),
        )
            .into_response();
    }

    Json(SendReactionResponse { success: true }).into_response()
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

/// Get global translation usage/cost
async fn get_global_usage(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    match state.store.get_global_usage() {
        Ok(usage) => Json(serde_json::json!({
            "inputTokens": usage.input_tokens,
            "outputTokens": usage.output_tokens,
            "costUsd": usage.cost_usd,
        }))
        .into_response(),
        Err(e) => {
            error!("Failed to get global usage: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to get usage").into_response()
        }
    }
}

/// Get translation usage/cost for a specific conversation
async fn get_conversation_usage(
    State(state): State<Arc<AppState>>,
    Path(contact_id): Path<String>,
) -> impl IntoResponse {
    let contact_id = urlencoding::decode(&contact_id)
        .map(|s| s.into_owned())
        .unwrap_or(contact_id);

    match state.store.get_conversation_usage(&contact_id) {
        Ok(usage) => Json(serde_json::json!({
            "inputTokens": usage.input_tokens,
            "outputTokens": usage.output_tokens,
            "costUsd": usage.cost_usd,
        }))
        .into_response(),
        Err(e) => {
            error!("Failed to get conversation usage: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to get usage").into_response()
        }
    }
}

/// Query parameters for link preview
#[derive(Deserialize)]
struct LinkPreviewQuery {
    url: String,
}

/// Get link preview for a URL (with caching)
async fn get_link_preview(
    State(state): State<Arc<AppState>>,
    Query(query): Query<LinkPreviewQuery>,
) -> impl IntoResponse {
    use crate::link_preview;

    let url = query.url;

    // Cache duration: 24 hours for successful fetches, 1 hour for errors
    let cache_duration = 24 * 60 * 60; // 24 hours

    // Check cache first
    match state.store.get_link_preview(&url, cache_duration) {
        Ok(Some(preview)) => {
            return Json(preview).into_response();
        }
        Ok(None) => {
            // Not in cache, need to fetch
        }
        Err(e) => {
            warn!("Failed to check link preview cache: {}", e);
            // Continue to fetch
        }
    }

    // Fetch the preview
    match link_preview::fetch_link_preview(&url).await {
        Ok(preview) => {
            // Cache the result
            if let Err(e) = state.store.save_link_preview(&preview) {
                warn!("Failed to cache link preview: {}", e);
            }
            Json(preview).into_response()
        }
        Err(e) => {
            error!("Failed to fetch link preview for {}: {}", url, e);
            // Return error preview
            let error_preview =
                link_preview::LinkPreview::error(url, format!("Failed to fetch: {}", e));
            // Cache the error for a shorter duration (by saving it)
            let _ = state.store.save_link_preview(&error_preview);
            Json(error_preview).into_response()
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
