//! Web server for the WhatsApp Translator frontend.
//!
//! Provides REST API endpoints and WebSocket support for real-time updates.

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Host, Path, Query, State,
    },
    http::{header, StatusCode},
    response::{Html, IntoResponse, Json, Redirect},
    routing::{get, post},
    Form, Router,
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
use crate::mcp::WhatsAppMcpServer;
use crate::oauth::{
    generate_token, AccessToken, AuthorizationCode, AuthorizeRequest, OAuthError,
    OAuthErrorResponse, OAuthMetadata, PendingAuthorization, RefreshToken, RevokeRequest,
    TokenRequest, TokenResponse,
};
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
    pub data_dir: PathBuf,
    pub command_tx: RwLock<Option<mpsc::Sender<BridgeCommand>>>,
    pub translator: Option<Arc<TranslationService>>,
    /// Cache of profile pictures (JID -> ProfilePicture)
    pub avatar_cache: RwLock<HashMap<String, ProfilePicture>>,
    /// Pending profile picture requests (request_id -> sender)
    pub pending_avatar_requests: RwLock<HashMap<i32, oneshot::Sender<Option<String>>>>,
    /// Request ID counter
    pub request_id_counter: AtomicI32,
    /// Password for web interface (None = no password required)
    pub password: Option<String>,
    /// Valid auth tokens (simple session management)
    pub auth_tokens: RwLock<std::collections::HashSet<String>>,
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
    MarkAsRead {
        chat_id: String,
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

/// Translate message request
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslateMessageRequest {
    pub text: String,
    pub message_id: String,
    pub contact_id: String,
}

/// Translate message response
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslateMessageResponse {
    pub success: bool,
    pub translated_text: Option<String>,
    pub source_language: Option<String>,
    pub error: Option<String>,
}

/// AI compose request
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiComposeRequest {
    pub prompt: String,
    /// Optional: the message being replied to (for context)
    pub reply_to_text: Option<String>,
    /// Optional: who sent the message being replied to
    pub reply_to_sender: Option<String>,
    /// Optional: base64 image data if replying to an image
    pub reply_to_image: Option<String>,
    /// Optional: mime type of the image (e.g., "image/jpeg")
    pub reply_to_image_type: Option<String>,
}

/// AI compose response
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiComposeResponse {
    pub success: bool,
    pub message: Option<String>,
    pub error: Option<String>,
    pub cost_usd: Option<f64>,
}

/// Auth check response
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthCheckResponse {
    pub required: bool,
}

/// Auth request
#[derive(Deserialize)]
pub struct AuthRequest {
    pub password: String,
}

/// Auth response
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthResponse {
    pub success: bool,
    pub token: Option<String>,
    pub error: Option<String>,
}

impl AppState {
    pub fn new(
        store: MessageStore,
        web_dir: PathBuf,
        data_dir: PathBuf,
        translator: Option<Arc<TranslationService>>,
        password: Option<String>,
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
            data_dir,
            command_tx: RwLock::new(None),
            translator,
            avatar_cache: RwLock::new(HashMap::new()),
            pending_avatar_requests: RwLock::new(HashMap::new()),
            request_id_counter: AtomicI32::new(1),
            password,
            auth_tokens: RwLock::new(std::collections::HashSet::new()),
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

    /// Broadcast a mark-as-read event (chat was read from another device)
    pub fn broadcast_mark_as_read(&self, chat_id: String) {
        let _ = self
            .broadcast_tx
            .send(WebSocketEvent::MarkAsRead { chat_id });
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
        // OAuth 2.0 routes for MCP authentication
        .route(
            "/.well-known/oauth-authorization-server",
            get(oauth_metadata),
        )
        .route(
            "/.well-known/oauth-protected-resource",
            get(oauth_protected_resource_metadata),
        )
        .route("/oauth/register", post(oauth_register))
        .route("/oauth/authorize", get(oauth_authorize))
        .route("/oauth/approve", post(oauth_approve))
        .route("/oauth/token", post(oauth_token))
        .route("/oauth/revoke", post(oauth_revoke))
        // Auth routes (no auth required)
        .route("/api/auth/check", get(auth_check))
        .route("/api/auth", post(auth_login))
        .route("/api/logout", post(logout))
        // API routes
        .route("/api/status", get(get_status))
        .route("/api/contacts", get(get_contacts))
        .route("/api/contacts/:contact_id/pin", post(toggle_pin))
        .route("/api/messages/:contact_id", get(get_messages))
        .route("/api/avatar/:jid", get(get_avatar))
        .route("/api/qr", get(get_qr))
        .route("/api/send", post(send_message))
        .route("/api/send-image", post(send_image))
        .route("/api/react", post(send_reaction))
        .route("/api/ai-compose", post(ai_compose))
        .route("/api/translate", post(translate_message))
        .route("/api/stats", get(get_stats))
        .route("/api/usage", get(get_global_usage))
        .route("/api/usage/:contact_id", get(get_conversation_usage))
        .route("/api/link-preview", get(get_link_preview))
        // WebSocket
        .route("/ws", get(websocket_handler))
        // MCP (Model Context Protocol) endpoint - HTTP transport
        .route("/mcp", post(mcp_handler))
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

// Auth Handlers

/// Check if authentication is required
async fn auth_check(State(state): State<Arc<AppState>>) -> Json<AuthCheckResponse> {
    Json(AuthCheckResponse {
        required: state.password.is_some(),
    })
}

/// Handle login attempt
async fn auth_login(
    State(state): State<Arc<AppState>>,
    Json(req): Json<AuthRequest>,
) -> impl IntoResponse {
    // If no password is set, auth is not required
    let Some(expected_password) = &state.password else {
        return Json(AuthResponse {
            success: true,
            token: None,
            error: None,
        })
        .into_response();
    };

    // Check password
    if req.password == *expected_password {
        // Generate a simple token (hash of password + timestamp for uniqueness)
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        expected_password.hash(&mut hasher);
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
            .hash(&mut hasher);
        let token = format!("{:x}", hasher.finish());

        // Store the token
        state.auth_tokens.write().await.insert(token.clone());

        info!("User authenticated successfully");
        Json(AuthResponse {
            success: true,
            token: Some(token),
            error: None,
        })
        .into_response()
    } else {
        warn!("Failed authentication attempt");
        Json(AuthResponse {
            success: false,
            token: None,
            error: Some("Invalid password".to_string()),
        })
        .into_response()
    }
}

/// Verify auth token from request header
async fn verify_auth(state: &Arc<AppState>, auth_header: Option<&str>) -> bool {
    // If no password is set, no auth required
    if state.password.is_none() {
        return true;
    }

    // Check for valid token in header
    if let Some(header) = auth_header {
        if let Some(token) = header.strip_prefix("Bearer ") {
            return state.auth_tokens.read().await.contains(token);
        }
    }

    false
}

/// Logout - clear all data and session
async fn logout(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    info!("Logout requested - clearing all data");

    // 1. Clear the message store (contacts, messages, usage)
    if let Err(e) = state.store.clear_all() {
        error!("Failed to clear message store: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "success": false,
                "error": format!("Failed to clear data: {}", e)
            })),
        )
            .into_response();
    }

    // 2. Send logout command to bridge (this will notify WhatsApp and clear the session)
    if let Some(tx) = state.command_tx.read().await.as_ref() {
        if let Err(e) = tx.send(BridgeCommand::Logout).await {
            warn!("Failed to send logout command to bridge: {}", e);
        } else {
            // Give the bridge time to send logout signal to WhatsApp
            // before we delete the session file
            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        }
    }

    // 3. Clear the session database file (cleanup after WhatsApp logout)
    let session_db = state.data_dir.join("session.db");
    if session_db.exists() {
        if let Err(e) = std::fs::remove_file(&session_db) {
            warn!("Failed to remove session database: {}", e);
        }
    }

    // 4. Clear auth tokens
    state.auth_tokens.write().await.clear();

    // 5. Reset connection state
    *state.connected.write().await = false;
    *state.phone.write().await = None;
    *state.name.write().await = None;
    *state.qr_code.write().await = None;

    // 6. Clear avatar cache
    state.avatar_cache.write().await.clear();

    info!("Logout complete - all data cleared");

    Json(serde_json::json!({
        "success": true,
        "message": "Logged out successfully. Please refresh the page."
    }))
    .into_response()
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

/// Toggle pin status for a contact
async fn toggle_pin(
    State(state): State<Arc<AppState>>,
    Path(contact_id): Path<String>,
) -> impl IntoResponse {
    let contact_id = urlencoding::decode(&contact_id)
        .map(|s| s.into_owned())
        .unwrap_or(contact_id);

    match state.store.toggle_pin(&contact_id) {
        Ok(is_pinned) => Json(serde_json::json!({
            "success": true,
            "pinned": is_pinned
        }))
        .into_response(),
        Err(e) => {
            error!("Failed to toggle pin: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to toggle pin").into_response()
        }
    }
}

/// Query parameters for messages pagination
#[derive(Debug, Deserialize)]
struct MessagesQuery {
    /// Maximum number of messages to return (default: 50 for initial load)
    limit: Option<u32>,
    /// Only get messages before this timestamp (for loading older messages)
    before: Option<i64>,
}

/// Response for paginated messages
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MessagesResponse {
    messages: Vec<StoredMessage>,
    has_more: bool,
}

async fn get_messages(
    State(state): State<Arc<AppState>>,
    Path(contact_id): Path<String>,
    Query(params): Query<MessagesQuery>,
) -> impl IntoResponse {
    let contact_id = urlencoding::decode(&contact_id)
        .map(|s| s.into_owned())
        .unwrap_or(contact_id);

    // Default to 50 messages for initial load, unless explicitly requesting all (limit=0)
    let limit = match params.limit {
        Some(0) => None, // 0 means all messages (for backwards compatibility / MCP)
        Some(n) => Some(n),
        None => Some(50), // Default to 50 for lazy loading
    };

    match state
        .store
        .get_messages_paginated(&contact_id, limit, params.before)
    {
        Ok(messages) => {
            // Check if there are more messages (we got a full page)
            let has_more = limit.map(|l| messages.len() >= l as usize).unwrap_or(false);
            Json(MessagesResponse { messages, has_more }).into_response()
        }
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
    // Get contact info for the recipient
    let contact_info = state.store.get_contact(&req.contact_id).ok().flatten();
    let contact_name = contact_info.as_ref().and_then(|c| c.name.clone());
    let contact_phone = contact_info.as_ref().and_then(|c| c.phone.clone());
    let chat_type = contact_info
        .as_ref()
        .and_then(|c| c.contact_type.clone())
        .unwrap_or_else(|| "private".to_string());

    let stored_msg = StoredMessage {
        id: temp_message_id.clone(),
        contact_id: req.contact_id.clone(),
        timestamp,
        is_from_me: true,
        is_forwarded: false,
        sender_name: state.name.read().await.clone(),
        sender_phone: state.phone.read().await.clone(),
        contact_name,
        contact_phone,
        chat_type,
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

    // Update contact's last message time (preserve contact name/phone)
    if let Err(e) = state.store.upsert_contact(
        &stored_msg.contact_id,
        stored_msg.contact_name.as_deref(),
        stored_msg.contact_phone.as_deref(),
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

    // Get contact info for the recipient
    let contact_info = state.store.get_contact(&req.contact_id).ok().flatten();
    let contact_name = contact_info.as_ref().and_then(|c| c.name.clone());
    let contact_phone = contact_info.as_ref().and_then(|c| c.phone.clone());
    let chat_type = contact_info
        .as_ref()
        .and_then(|c| c.contact_type.clone())
        .unwrap_or_else(|| "private".to_string());

    // Store the sent image message locally
    let stored_msg = crate::storage::StoredMessage {
        id: temp_message_id.clone(),
        contact_id: req.contact_id.clone(),
        timestamp,
        is_from_me: true,
        is_forwarded: false,
        sender_name: state.name.read().await.clone(),
        sender_phone: state.phone.read().await.clone(),
        contact_name,
        contact_phone,
        chat_type,
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

    // Update contact's last message time (preserve contact name/phone)
    if let Err(e) = state.store.upsert_contact(
        &stored_msg.contact_id,
        stored_msg.contact_name.as_deref(),
        stored_msg.contact_phone.as_deref(),
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

/// Translate a message manually
async fn translate_message(
    State(state): State<Arc<AppState>>,
    Json(req): Json<TranslateMessageRequest>,
) -> impl IntoResponse {
    // Check if translation service is available
    let translator = match &state.translator {
        Some(t) => t,
        None => {
            return Json(TranslateMessageResponse {
                success: false,
                translated_text: None,
                source_language: None,
                error: Some("Translation service not configured".to_string()),
            })
            .into_response();
        }
    };

    // Call the translation service
    let result = translator.process_text(&req.text).await;

    // Record usage if there was API usage
    if result.usage.input_tokens > 0 {
        if let Err(e) = state.store.record_usage(
            Some(&req.contact_id),
            Some(&req.message_id),
            &result.usage,
            "manual_translate",
        ) {
            warn!("Failed to record translation usage: {}", e);
        }
    }

    // Update the message in the database with the translation
    if result.needs_translation {
        if let Err(e) = state.store.update_message_translation(
            &req.message_id,
            result.translated_text.as_deref(),
            Some(&result.source_language),
        ) {
            warn!("Failed to update message translation in DB: {}", e);
        }
    }

    Json(TranslateMessageResponse {
        success: true,
        translated_text: result.translated_text,
        source_language: Some(result.source_language),
        error: None,
    })
    .into_response()
}

/// AI compose endpoint - generates a message using Claude
async fn ai_compose(
    State(state): State<Arc<AppState>>,
    Json(req): Json<AiComposeRequest>,
) -> impl IntoResponse {
    // Check if translation service is available (it has the API key)
    let translator = match &state.translator {
        Some(t) => t,
        None => {
            return Json(AiComposeResponse {
                success: false,
                message: None,
                error: Some("AI service not configured (missing API key)".to_string()),
                cost_usd: None,
            })
            .into_response();
        }
    };

    // Build reply context if provided
    let reply_context = match (&req.reply_to_sender, &req.reply_to_text) {
        (Some(sender), Some(text)) => Some((sender.as_str(), text.as_str())),
        (None, Some(text)) => Some(("Someone", text.as_str())),
        _ => None,
    };

    // Build image context if provided
    let reply_image = match (&req.reply_to_image_type, &req.reply_to_image) {
        (Some(mime_type), Some(data)) => Some((mime_type.as_str(), data.as_str())),
        _ => None,
    };

    // Call the AI compose method (using Opus 4.5)
    match translator
        .compose_ai_message(&req.prompt, reply_context, reply_image)
        .await
    {
        Ok((message, usage)) => {
            info!(
                "AI composed message ({} chars), cost: ${:.6}",
                message.len(),
                usage.cost_usd
            );

            // Record usage
            if let Err(e) = state.store.record_usage(
                None,
                None,
                &crate::translation::UsageInfo {
                    input_tokens: usage.input_tokens,
                    output_tokens: usage.output_tokens,
                    cost_usd: usage.cost_usd,
                },
                "ai_compose",
            ) {
                warn!("Failed to record AI compose usage: {}", e);
            }

            Json(AiComposeResponse {
                success: true,
                message: Some(message),
                error: None,
                cost_usd: Some(usage.cost_usd),
            })
            .into_response()
        }
        Err(e) => {
            error!("AI compose failed: {}", e);
            Json(AiComposeResponse {
                success: false,
                message: None,
                error: Some(format!("Failed to compose message: {}", e)),
                cost_usd: None,
            })
            .into_response()
        }
    }
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

// ==================== OAuth 2.0 Handlers ====================

/// Get base URL from request (for OAuth metadata)
fn get_base_url(host: &str, is_https: bool) -> String {
    let scheme = if is_https { "https" } else { "http" };
    format!("{}://{}", scheme, host)
}

/// OAuth 2.0 Authorization Server Metadata (RFC 8414)
async fn oauth_metadata(Host(host): Host) -> impl IntoResponse {
    // Assume HTTPS in production (Railway sets this)
    let is_https = !host.contains("localhost") && !host.contains("127.0.0.1");
    let base_url = get_base_url(&host, is_https);

    // Extended metadata with Dynamic Client Registration support
    Json(serde_json::json!({
        "issuer": base_url,
        "authorization_endpoint": format!("{}/oauth/authorize", base_url),
        "token_endpoint": format!("{}/oauth/token", base_url),
        "registration_endpoint": format!("{}/oauth/register", base_url),
        "revocation_endpoint": format!("{}/oauth/revoke", base_url),
        "response_types_supported": ["code"],
        "grant_types_supported": ["authorization_code", "refresh_token"],
        "code_challenge_methods_supported": ["S256"],
        "token_endpoint_auth_methods_supported": ["none"],
        "scopes_supported": ["mcp"],
        // MCP-specific fields
        "service_documentation": format!("{}/docs", base_url),
    }))
}

/// OAuth 2.0 Protected Resource Metadata (RFC 9728)
/// This tells MCP clients which authorization server to use
async fn oauth_protected_resource_metadata(Host(host): Host) -> impl IntoResponse {
    let is_https = !host.contains("localhost") && !host.contains("127.0.0.1");
    let base_url = get_base_url(&host, is_https);

    Json(serde_json::json!({
        "resource": format!("{}/mcp", base_url),
        "authorization_servers": [base_url],
        "scopes_supported": ["mcp"],
        "bearer_methods_supported": ["header"]
    }))
}

/// Dynamic Client Registration request (RFC 7591)
#[derive(Debug, Deserialize)]
struct ClientRegistrationRequest {
    redirect_uris: Vec<String>,
    client_name: Option<String>,
    client_uri: Option<String>,
    scope: Option<String>,
    grant_types: Option<Vec<String>>,
    response_types: Option<Vec<String>>,
    token_endpoint_auth_method: Option<String>,
}

/// Dynamic Client Registration endpoint (RFC 7591)
/// Allows MCP clients like Claude.ai to register before starting OAuth flow
async fn oauth_register(Json(req): Json<ClientRegistrationRequest>) -> impl IntoResponse {
    // Generate a client_id for this registration
    let client_id = format!("client_{}", generate_token()[..16].to_string());

    // For public clients (like Claude.ai), we don't issue a client_secret
    // The client will use PKCE for security instead

    info!(
        "OAuth client registered: {} ({:?}) with redirect_uris: {:?}",
        client_id, req.client_name, req.redirect_uris
    );

    // Return the registration response
    (
        StatusCode::CREATED,
        Json(serde_json::json!({
            "client_id": client_id,
            "client_name": req.client_name,
            "redirect_uris": req.redirect_uris,
            "grant_types": req.grant_types.unwrap_or_else(|| vec!["authorization_code".to_string(), "refresh_token".to_string()]),
            "response_types": req.response_types.unwrap_or_else(|| vec!["code".to_string()]),
            "token_endpoint_auth_method": "none",
            "scope": req.scope.unwrap_or_else(|| "mcp".to_string()),
        })),
    )
}

/// OAuth Authorization endpoint - shows approval page
async fn oauth_authorize(
    State(state): State<Arc<AppState>>,
    Host(host): Host,
    Query(params): Query<AuthorizeRequest>,
) -> impl IntoResponse {
    // Validate request
    if params.response_type != "code" {
        return (
            StatusCode::BAD_REQUEST,
            Html(format!(
                r#"<html><body><h1>Error</h1><p>Unsupported response_type: {}</p></body></html>"#,
                params.response_type
            )),
        )
            .into_response();
    }

    if params.code_challenge_method != "S256" {
        return (
            StatusCode::BAD_REQUEST,
            Html(r#"<html><body><h1>Error</h1><p>Only S256 code_challenge_method is supported (PKCE required)</p></body></html>"#.to_string()),
        )
            .into_response();
    }

    // Generate a session key for this authorization request
    let session_key = generate_token();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    let pending = PendingAuthorization {
        session_key: session_key.clone(),
        client_id: params.client_id.clone(),
        redirect_uri: params.redirect_uri.clone(),
        code_challenge: params.code_challenge.clone(),
        code_challenge_method: params.code_challenge_method.clone(),
        scope: params.scope.clone().unwrap_or_else(|| "mcp".to_string()),
        state: params.state.clone(),
        created_at: now,
        expires_at: now + 600, // 10 minutes
    };

    // Store pending authorization
    if let Err(e) = state.store.oauth_store_pending_auth(&pending) {
        error!("Failed to store pending auth: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Html(
                "<html><body><h1>Error</h1><p>Internal server error</p></body></html>".to_string(),
            ),
        )
            .into_response();
    }

    // Check if password auth is required
    let requires_password = state.password.is_some();

    // Show approval page
    let is_https = !host.contains("localhost") && !host.contains("127.0.0.1");
    let base_url = get_base_url(&host, is_https);

    let html = format!(
        r#"<!DOCTYPE html>
<html>
<head>
    <title>Authorize MCP Client</title>
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <style>
        body {{ font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; 
               max-width: 500px; margin: 50px auto; padding: 20px; background: #f5f5f5; }}
        .card {{ background: white; border-radius: 12px; padding: 30px; box-shadow: 0 2px 10px rgba(0,0,0,0.1); }}
        h1 {{ color: #333; margin-top: 0; }}
        .client-info {{ background: #f0f0f0; padding: 15px; border-radius: 8px; margin: 20px 0; }}
        .scope {{ color: #666; font-size: 0.9em; }}
        .warning {{ color: #e67e22; font-size: 0.9em; margin: 15px 0; }}
        .buttons {{ display: flex; gap: 10px; margin-top: 20px; }}
        button {{ flex: 1; padding: 12px 24px; border: none; border-radius: 8px; font-size: 16px; cursor: pointer; }}
        .approve {{ background: #25D366; color: white; }}
        .approve:hover {{ background: #1da851; }}
        .deny {{ background: #e74c3c; color: white; }}
        .deny:hover {{ background: #c0392b; }}
        input {{ width: 100%; padding: 12px; margin: 10px 0; border: 1px solid #ddd; border-radius: 8px; box-sizing: border-box; }}
        label {{ display: block; margin-top: 15px; color: #666; }}
    </style>
</head>
<body>
    <div class="card">
        <h1>🔐 Authorize MCP Client</h1>
        <p>An application is requesting access to your WhatsApp Translator:</p>
        
        <div class="client-info">
            <strong>Client ID:</strong> {client_id}<br>
            <strong>Redirect URI:</strong> {redirect_uri}
        </div>
        
        <div class="scope">
            <strong>Requested permissions:</strong> {scope}<br>
            This will allow the application to read your contacts, messages, and send messages on your behalf.
        </div>
        
        <p class="warning">⚠️ Only authorize applications you trust!</p>
        
        <form method="POST" action="{base_url}/oauth/approve">
            <input type="hidden" name="session_key" value="{session_key}">
            {password_field}
            <div class="buttons">
                <button type="submit" name="approved" value="true" class="approve">✓ Authorize</button>
                <button type="submit" name="approved" value="false" class="deny">✗ Deny</button>
            </div>
        </form>
    </div>
</body>
</html>"#,
        client_id = html_escape(&params.client_id),
        redirect_uri = html_escape(&params.redirect_uri),
        scope = html_escape(&params.scope.clone().unwrap_or_else(|| "mcp".to_string())),
        base_url = base_url,
        session_key = session_key,
        password_field = if requires_password {
            r#"<label for="password">Enter your password to authorize:</label>
            <input type="password" name="password" id="password" placeholder="Password" required>"#
        } else {
            ""
        }
    );

    Html(html).into_response()
}

/// Simple HTML escaping
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

/// OAuth approval form data
#[derive(Debug, Deserialize)]
struct OAuthApprovalForm {
    session_key: String,
    approved: String,
    password: Option<String>,
}

/// Handle OAuth approval form submission
async fn oauth_approve(
    State(state): State<Arc<AppState>>,
    Form(form): Form<OAuthApprovalForm>,
) -> impl IntoResponse {
    // Get the pending authorization
    let pending = match state.store.oauth_take_pending_auth(&form.session_key) {
        Ok(Some(p)) => p,
        Ok(None) => {
            return (
                StatusCode::BAD_REQUEST,
                Html("<html><body><h1>Error</h1><p>Invalid or expired authorization request</p></body></html>".to_string()),
            )
                .into_response();
        }
        Err(e) => {
            error!("Failed to get pending auth: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Html(
                    "<html><body><h1>Error</h1><p>Internal server error</p></body></html>"
                        .to_string(),
                ),
            )
                .into_response();
        }
    };

    // Check if user denied
    if form.approved != "true" {
        let redirect_url = build_error_redirect(
            &pending.redirect_uri,
            OAuthError::AccessDenied,
            pending.state.as_deref(),
        );
        return Redirect::to(&redirect_url).into_response();
    }

    // Verify password if required
    if let Some(expected_password) = &state.password {
        match &form.password {
            Some(password) if password == expected_password => {}
            _ => {
                let redirect_url = build_error_redirect(
                    &pending.redirect_uri,
                    OAuthError::AccessDenied,
                    pending.state.as_deref(),
                );
                return Redirect::to(&redirect_url).into_response();
            }
        }
    }

    // Generate authorization code
    let code = generate_token();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    let auth_code = AuthorizationCode {
        code: code.clone(),
        client_id: pending.client_id,
        redirect_uri: pending.redirect_uri.clone(),
        code_challenge: pending.code_challenge,
        code_challenge_method: pending.code_challenge_method,
        scope: pending.scope,
        created_at: now,
        expires_at: now + 300, // 5 minutes
        used: false,
    };

    // Store the authorization code
    if let Err(e) = state.store.oauth_store_authorization_code(&auth_code) {
        error!("Failed to store authorization code: {}", e);
        let redirect_url = build_error_redirect(
            &pending.redirect_uri,
            OAuthError::ServerError,
            pending.state.as_deref(),
        );
        return Redirect::to(&redirect_url).into_response();
    }

    info!(
        "OAuth authorization granted for client: {}",
        auth_code.client_id
    );

    // Redirect back to client with authorization code
    let mut redirect_url = format!("{}?code={}", pending.redirect_uri, code);
    if let Some(state_param) = pending.state {
        redirect_url.push_str(&format!("&state={}", urlencoding::encode(&state_param)));
    }

    Redirect::to(&redirect_url).into_response()
}

/// Build error redirect URL
fn build_error_redirect(redirect_uri: &str, error: OAuthError, state: Option<&str>) -> String {
    let mut url = format!(
        "{}?error={}&error_description={}",
        redirect_uri,
        error.as_str(),
        urlencoding::encode(error.description())
    );
    if let Some(s) = state {
        url.push_str(&format!("&state={}", urlencoding::encode(s)));
    }
    url
}

/// OAuth Token endpoint - exchange code for tokens or refresh tokens
async fn oauth_token(
    State(state): State<Arc<AppState>>,
    Form(req): Form<TokenRequest>,
) -> impl IntoResponse {
    match req.grant_type.as_str() {
        "authorization_code" => handle_authorization_code_grant(state, req).await,
        "refresh_token" => handle_refresh_token_grant(state, req).await,
        _ => {
            let error = OAuthErrorResponse::from(OAuthError::UnsupportedGrantType);
            (StatusCode::BAD_REQUEST, Json(error)).into_response()
        }
    }
}

async fn handle_authorization_code_grant(
    state: Arc<AppState>,
    req: TokenRequest,
) -> axum::response::Response {
    // Validate required parameters
    let code = match &req.code {
        Some(c) => c,
        None => {
            let error = OAuthErrorResponse::from(OAuthError::InvalidRequest);
            return (StatusCode::BAD_REQUEST, Json(error)).into_response();
        }
    };

    let code_verifier = match &req.code_verifier {
        Some(v) => v,
        None => {
            let error = OAuthErrorResponse::from(OAuthError::InvalidRequest);
            return (StatusCode::BAD_REQUEST, Json(error)).into_response();
        }
    };

    let redirect_uri = match &req.redirect_uri {
        Some(r) => r,
        None => {
            let error = OAuthErrorResponse::from(OAuthError::InvalidRequest);
            return (StatusCode::BAD_REQUEST, Json(error)).into_response();
        }
    };

    // Get and validate the authorization code
    let auth_code = match state.store.oauth_use_authorization_code(code) {
        Ok(Some(c)) => c,
        Ok(None) => {
            let error = OAuthErrorResponse::from(OAuthError::InvalidGrant);
            return (StatusCode::BAD_REQUEST, Json(error)).into_response();
        }
        Err(e) => {
            error!("Failed to get authorization code: {}", e);
            let error = OAuthErrorResponse::from(OAuthError::ServerError);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(error)).into_response();
        }
    };

    // Verify redirect_uri matches
    if auth_code.redirect_uri != *redirect_uri {
        let error = OAuthErrorResponse::from(OAuthError::InvalidGrant);
        return (StatusCode::BAD_REQUEST, Json(error)).into_response();
    }

    // Verify PKCE
    if !auth_code.verify_pkce(code_verifier) {
        let error = OAuthErrorResponse::from(OAuthError::InvalidGrant);
        return (StatusCode::BAD_REQUEST, Json(error)).into_response();
    }

    // Generate tokens
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    let access_token_str = generate_token();
    let refresh_token_str = generate_token();

    let access_token = AccessToken {
        token: access_token_str.clone(),
        client_id: auth_code.client_id.clone(),
        scope: auth_code.scope.clone(),
        created_at: now,
        expires_at: now + 3600, // 1 hour
    };

    let refresh_token = RefreshToken {
        token: refresh_token_str.clone(),
        client_id: auth_code.client_id.clone(),
        scope: auth_code.scope.clone(),
        created_at: now,
        expires_at: now + 30 * 24 * 3600, // 30 days
    };

    // Store tokens
    if let Err(e) = state.store.oauth_store_access_token(&access_token) {
        error!("Failed to store access token: {}", e);
        let error = OAuthErrorResponse::from(OAuthError::ServerError);
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(error)).into_response();
    }

    if let Err(e) = state.store.oauth_store_refresh_token(&refresh_token) {
        error!("Failed to store refresh token: {}", e);
        let error = OAuthErrorResponse::from(OAuthError::ServerError);
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(error)).into_response();
    }

    info!("OAuth tokens issued for client: {}", auth_code.client_id);

    let response = TokenResponse {
        access_token: access_token_str,
        token_type: "Bearer".to_string(),
        expires_in: 3600,
        refresh_token: refresh_token_str,
        scope: auth_code.scope,
    };

    (
        StatusCode::OK,
        [(header::CACHE_CONTROL, "no-store")],
        Json(response),
    )
        .into_response()
}

async fn handle_refresh_token_grant(
    state: Arc<AppState>,
    req: TokenRequest,
) -> axum::response::Response {
    let refresh_token_str = match &req.refresh_token {
        Some(r) => r,
        None => {
            let error = OAuthErrorResponse::from(OAuthError::InvalidRequest);
            return (StatusCode::BAD_REQUEST, Json(error)).into_response();
        }
    };

    // Validate refresh token
    let refresh_token = match state.store.oauth_get_refresh_token(refresh_token_str) {
        Ok(Some(t)) => t,
        Ok(None) => {
            let error = OAuthErrorResponse::from(OAuthError::InvalidGrant);
            return (StatusCode::BAD_REQUEST, Json(error)).into_response();
        }
        Err(e) => {
            error!("Failed to get refresh token: {}", e);
            let error = OAuthErrorResponse::from(OAuthError::ServerError);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(error)).into_response();
        }
    };

    // Generate new access token
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    let access_token_str = generate_token();

    let access_token = AccessToken {
        token: access_token_str.clone(),
        client_id: refresh_token.client_id.clone(),
        scope: refresh_token.scope.clone(),
        created_at: now,
        expires_at: now + 3600, // 1 hour
    };

    // Store new access token
    if let Err(e) = state.store.oauth_store_access_token(&access_token) {
        error!("Failed to store access token: {}", e);
        let error = OAuthErrorResponse::from(OAuthError::ServerError);
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(error)).into_response();
    }

    info!(
        "OAuth access token refreshed for client: {}",
        refresh_token.client_id
    );

    let response = TokenResponse {
        access_token: access_token_str,
        token_type: "Bearer".to_string(),
        expires_in: 3600,
        refresh_token: refresh_token_str.clone(),
        scope: refresh_token.scope,
    };

    (
        StatusCode::OK,
        [(header::CACHE_CONTROL, "no-store")],
        Json(response),
    )
        .into_response()
}

/// OAuth Token revocation endpoint
async fn oauth_revoke(
    State(state): State<Arc<AppState>>,
    Form(req): Form<RevokeRequest>,
) -> impl IntoResponse {
    // Revoke the token (we don't care if it exists or not per RFC 7009)
    if let Err(e) = state.store.oauth_revoke_token(&req.token) {
        error!("Failed to revoke token: {}", e);
        // Still return 200 per RFC 7009
    }

    StatusCode::OK
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

// MCP (Model Context Protocol) HTTP handler
// Uses Streamable HTTP transport (POST for requests, SSE for responses)

use rmcp::transport::streamable_http_server::{
    session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
};

fn create_mcp_service(
    store: Arc<MessageStore>,
    command_tx: Option<mpsc::Sender<BridgeCommand>>,
) -> StreamableHttpService<WhatsAppMcpServer, LocalSessionManager> {
    let session_manager = Arc::new(LocalSessionManager::default());
    let config = StreamableHttpServerConfig {
        stateful_mode: false, // Stateless mode - simpler, no session management needed
        ..Default::default()
    };

    StreamableHttpService::new(
        move || {
            // Create a new MCP server instance for each request
            Ok(WhatsAppMcpServer::new(store.clone(), command_tx.clone()))
        },
        session_manager,
        config,
    )
}

async fn mcp_handler(
    State(state): State<Arc<AppState>>,
    Host(host): Host,
    request: axum::http::Request<axum::body::Body>,
) -> impl IntoResponse {
    // Check OAuth Bearer token authentication
    let auth_header = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok());

    let is_authenticated = if let Some(header) = auth_header {
        if let Some(token) = header.strip_prefix("Bearer ") {
            // Validate the OAuth access token
            match state.store.oauth_validate_access_token(token) {
                Ok(Some(_)) => {
                    info!("MCP authenticated via OAuth token");
                    true
                }
                Ok(None) => {
                    info!("MCP request with invalid/expired OAuth token");
                    false
                }
                Err(e) => {
                    error!("Failed to validate OAuth token: {}", e);
                    false
                }
            }
        } else {
            false
        }
    } else {
        false
    };

    if !is_authenticated {
        // Build the resource_metadata URL for the WWW-Authenticate header
        let is_https = !host.contains("localhost") && !host.contains("127.0.0.1");
        let base_url = get_base_url(&host, is_https);
        let resource_metadata_url = format!("{}/.well-known/oauth-protected-resource", base_url);

        // Return 401 with WWW-Authenticate header per RFC 6750 and RFC 9728
        // The resource_metadata parameter tells MCP clients where to find OAuth config
        let www_authenticate = format!("Bearer resource_metadata=\"{}\"", resource_metadata_url);

        return (
            StatusCode::UNAUTHORIZED,
            [
                (header::WWW_AUTHENTICATE.as_str(), www_authenticate.as_str()),
                (header::CONTENT_TYPE.as_str(), "application/json"),
            ],
            Json(serde_json::json!({
                "error": "unauthorized",
                "error_description": "OAuth Bearer token required. Complete the OAuth flow to get a token."
            })),
        )
            .into_response();
    }

    // Read the command_tx asynchronously before creating the service
    let command_tx = state.command_tx.read().await.clone();
    let store = Arc::new(state.store.clone());

    let service = create_mcp_service(store, command_tx);
    // StreamableHttpService has an async handle method we can call directly
    service.handle(request).await.into_response()
}
