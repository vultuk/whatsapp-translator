//! Web server for the WhatsApp Translator frontend.
//!
//! Provides REST API endpoints and WebSocket support for real-time updates.

use axum::{
    extract::Request,
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Host, Path, Query, State,
    },
    http::{header, StatusCode},
    middleware::{self, Next},
    response::{Html, IntoResponse, Json, Redirect, Response},
    routing::{delete, get, post},
    Form, Router,
};
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, oneshot, RwLock};
use tower_http::services::ServeDir;
use tracing::{error, info, warn};

use crate::bridge::BridgeCommand;
use crate::mcp::WhatsAppMcpServer;
use crate::oauth::{
    generate_token, AccessToken, AuthorizationCode, AuthorizeRequest, OAuthClientRegistration,
    OAuthError, OAuthErrorResponse, PendingAuthorization, RefreshToken, RevokeRequest,
    TokenRequest, TokenResponse,
};
use crate::storage::{MessageStore, StoredMessage};
use crate::translation::TranslationService;
use tokio::sync::mpsc;

const WEB_AUTH_TOKEN_TTL_SECONDS: i64 = 12 * 60 * 60;
const MAX_IMAGE_BYTES: usize = 16 * 1024 * 1024;
const MAX_IMAGE_BASE64_BYTES: usize = MAX_IMAGE_BYTES.div_ceil(3) * 4;
const SEND_RESULT_TIMEOUT: Duration = Duration::from_secs(30);
const OAUTH_CLEANUP_INTERVAL: Duration = Duration::from_secs(60 * 60);

#[derive(Debug, Clone)]
struct ValidatedImagePayload {
    mime_type: String,
    decoded_size: usize,
}

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
    /// Pending send requests (request_id -> pending reconciliation metadata)
    pub pending_send_requests: RwLock<HashMap<i32, PendingSendRequest>>,
    /// Request ID counter
    pub request_id_counter: AtomicI32,
    /// Password for web interface (None = no password required)
    pub password: Option<String>,
    /// Valid web auth tokens and their expiration timestamps.
    pub auth_tokens: RwLock<HashMap<String, i64>>,
    /// Whether the bridge restart loop should clear WhatsApp session files before respawning.
    pub session_reset_requested: AtomicBool,
}

/// Bridge send result normalized for Rust/web consumers.
#[derive(Debug, Clone)]
pub struct BridgeSendResult {
    pub request_id: i32,
    pub success: bool,
    pub message_id: Option<String>,
    pub timestamp: Option<i64>,
    pub error: Option<String>,
}

/// Error while waiting for the bridge to confirm a send.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendConfirmationError {
    Timeout,
    ChannelClosed,
}

pub struct PendingSendRequest {
    temp_message_id: Option<String>,
    tx: oneshot::Sender<BridgeSendResult>,
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
    SendResult {
        request_id: i32,
        success: bool,
        temp_message_id: Option<String>,
        message_id: Option<String>,
        timestamp: Option<i64>,
        error: Option<String>,
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

#[derive(Serialize)]
struct HealthResponse {
    ok: bool,
    service: &'static str,
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
    /// Sender display name of the replied message (for storage)
    pub reply_to_sender_name: Option<String>,
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
    /// Whether the original draft was confirmed as a second WhatsApp message.
    pub original_follow_up_sent: bool,
    pub original_message_id: Option<String>,
    pub original_timestamp: Option<i64>,
    pub original_follow_up_error: Option<String>,
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
    /// Text preview of the replied message (for storage)
    pub reply_to_text: Option<String>,
    /// Sender display name of the replied message (for storage)
    pub reply_to_sender_name: Option<String>,
}

/// Send image response
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SendImageResponse {
    pub message_id: String,
    pub timestamp: i64,
}

fn validate_image_payload(
    media_data: &str,
    mime_type: &str,
) -> Result<ValidatedImagePayload, String> {
    if media_data.starts_with("data:") {
        return Err("media_data must be raw base64 without a data URL prefix".to_string());
    }

    let mime_type = normalize_image_mime_type(mime_type)?;
    if media_data.len() > MAX_IMAGE_BASE64_BYTES {
        return Err("Image is too large. Maximum decoded size is 16MB.".to_string());
    }

    let decoded = BASE64_STANDARD
        .decode(media_data)
        .map_err(|_| "media_data must be valid base64".to_string())?;
    if decoded.is_empty() {
        return Err("media_data must not decode to an empty image".to_string());
    }
    if decoded.len() > MAX_IMAGE_BYTES {
        return Err("Image is too large. Maximum decoded size is 16MB.".to_string());
    }
    if !image_signature_matches(&decoded, &mime_type) {
        return Err(format!("media_data does not match MIME type {}", mime_type));
    }

    Ok(ValidatedImagePayload {
        mime_type,
        decoded_size: decoded.len(),
    })
}

fn normalize_image_mime_type(mime_type: &str) -> Result<String, String> {
    let normalized = mime_type
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();

    match normalized.as_str() {
        "image/jpeg" | "image/png" | "image/gif" | "image/webp" => Ok(normalized),
        _ => Err("Unsupported image MIME type".to_string()),
    }
}

fn image_signature_matches(decoded: &[u8], mime_type: &str) -> bool {
    match mime_type {
        "image/jpeg" => decoded.starts_with(&[0xff, 0xd8, 0xff]),
        "image/png" => decoded.starts_with(b"\x89PNG\r\n\x1a\n"),
        "image/gif" => decoded.starts_with(b"GIF87a") || decoded.starts_with(b"GIF89a"),
        "image/webp" => {
            decoded.len() >= 12 && decoded.starts_with(b"RIFF") && &decoded[8..12] == b"WEBP"
        }
        _ => false,
    }
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

/// Mark-read request
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarkReadRequest {
    pub message_id: Option<String>,
    pub timestamp: Option<i64>,
    pub sender_jid: Option<String>,
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

/// AI styled reply request - generates a reply that sounds like the user
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiReplyRequest {
    pub contact_id: String,
    pub message_id: String,
}

/// AI styled reply response
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiReplyResponse {
    pub success: bool,
    pub reply_text: Option<String>,
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

/// Conversation settings request/response
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateConversationSettingsRequest {
    /// Override the target language for translations (plain text, e.g., "Spanish")
    pub language_override: Option<String>,
    /// Style instruction for translations (plain text, e.g., "formal", "casual")
    pub translation_style: Option<String>,
    #[serde(default)]
    pub send_original_follow_up: bool,
}

/// Conversation settings response
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationSettingsResponse {
    pub language_override: Option<String>,
    pub translation_style: Option<String>,
    pub send_original_follow_up: bool,
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
            pending_send_requests: RwLock::new(HashMap::new()),
            request_id_counter: AtomicI32::new(1),
            password,
            auth_tokens: RwLock::new(HashMap::new()),
            session_reset_requested: AtomicBool::new(false),
        })
    }

    pub fn request_session_reset_before_bridge_restart(&self) {
        self.session_reset_requested.store(true, Ordering::SeqCst);
    }

    pub fn take_session_reset_request(&self) -> bool {
        self.session_reset_requested.swap(false, Ordering::SeqCst)
    }

    pub fn clear_session_files(&self) -> std::io::Result<()> {
        for filename in ["session.db", "session.db-wal", "session.db-shm"] {
            let path = self.data_dir.join(filename);
            match std::fs::remove_file(&path) {
                Ok(()) => {}
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                Err(err) => return Err(err),
            }
        }

        Ok(())
    }

    pub fn clear_orphaned_session_sidecars(&self) -> std::io::Result<()> {
        if self.data_dir.join("session.db").try_exists()? {
            return Ok(());
        }

        for filename in ["session.db-wal", "session.db-shm"] {
            let path = self.data_dir.join(filename);
            match std::fs::remove_file(&path) {
                Ok(()) => {}
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                Err(err) => return Err(err),
            }
        }

        Ok(())
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

    /// Register a bridge send request so send_result can reconcile it.
    pub async fn register_pending_send(
        &self,
        request_id: i32,
        temp_message_id: Option<String>,
    ) -> oneshot::Receiver<BridgeSendResult> {
        let (tx, rx) = oneshot::channel();
        let mut pending = self.pending_send_requests.write().await;
        pending.insert(
            request_id,
            PendingSendRequest {
                temp_message_id,
                tx,
            },
        );
        rx
    }

    /// Cancel a pending send and remove any temporary optimistic row.
    pub async fn cancel_pending_send(&self, request_id: i32) {
        let pending = self.pending_send_requests.write().await.remove(&request_id);
        if let Some(pending) = pending {
            if let Some(temp_message_id) = pending.temp_message_id {
                if let Err(e) = self.store.delete_message(&temp_message_id) {
                    warn!(
                        "Failed to delete pending message {} after send cancellation: {}",
                        temp_message_id, e
                    );
                }
            }
        }
    }

    /// Wait for a bridge send result with a bounded timeout.
    pub async fn wait_for_send_result(
        &self,
        request_id: i32,
        rx: oneshot::Receiver<BridgeSendResult>,
    ) -> Result<BridgeSendResult, SendConfirmationError> {
        match tokio::time::timeout(SEND_RESULT_TIMEOUT, rx).await {
            Ok(Ok(result)) => Ok(result),
            Ok(Err(_)) => {
                self.cancel_pending_send(request_id).await;
                Err(SendConfirmationError::ChannelClosed)
            }
            Err(_) => {
                self.cancel_pending_send(request_id).await;
                Err(SendConfirmationError::Timeout)
            }
        }
    }

    /// Reconcile a bridge send_result with a temporary stored row and release waiters.
    pub async fn handle_send_result(&self, mut result: BridgeSendResult) {
        result.timestamp = normalize_bridge_timestamp_millis(result.timestamp);
        let pending = self
            .pending_send_requests
            .write()
            .await
            .remove(&result.request_id);

        if let Some(pending) = pending {
            if result.success {
                if let Some(temp_message_id) = pending.temp_message_id.as_deref() {
                    let confirmed_message_id =
                        result.message_id.as_deref().unwrap_or(temp_message_id);
                    if let Err(e) = self.store.replace_message_id(
                        temp_message_id,
                        confirmed_message_id,
                        result.timestamp,
                    ) {
                        warn!(
                            "Failed to reconcile pending message {} to {}: {}",
                            temp_message_id, confirmed_message_id, e
                        );
                    }
                }
            } else if let Some(temp_message_id) = pending.temp_message_id.as_deref() {
                if let Err(e) = self.store.delete_message(temp_message_id) {
                    warn!(
                        "Failed to delete pending message {} after send failure: {}",
                        temp_message_id, e
                    );
                }
            }

            let _ = self.broadcast_tx.send(WebSocketEvent::SendResult {
                request_id: result.request_id,
                success: result.success,
                temp_message_id: pending.temp_message_id.clone(),
                message_id: result.message_id.clone(),
                timestamp: result.timestamp,
                error: result.error.clone(),
            });

            let _ = pending.tx.send(result);
        } else if result.request_id != 0 {
            warn!(
                "Received send_result for unknown request_id {}",
                result.request_id
            );
        }
    }
}

fn normalize_bridge_timestamp_millis(timestamp: Option<i64>) -> Option<i64> {
    timestamp.filter(|value| *value > 0).map(|value| {
        if value < 10_000_000_000 {
            value * 1000
        } else {
            value
        }
    })
}

fn build_outgoing_send_plan(
    translated_or_original: String,
    original_text: &str,
    was_translated: bool,
    send_original_follow_up: bool,
) -> Vec<String> {
    let mut messages = vec![translated_or_original];
    if was_translated && send_original_follow_up {
        messages.push(original_text.to_string());
    }
    messages
}

/// Create the web server router
pub fn create_router(state: Arc<AppState>) -> Router {
    // Serve static files from the web directory
    let serve_dir = ServeDir::new(&state.web_dir);
    let auth_state = state.clone();

    let protected_api = Router::new()
        .route("/api/logout", post(logout))
        .route("/api/status", get(get_status))
        .route("/api/contacts", get(get_contacts))
        .route("/api/contacts/:contact_id/read", post(mark_contact_as_read))
        .route("/api/contacts/:contact_id/pin", post(toggle_pin))
        .route(
            "/api/contacts/:contact_id/settings",
            get(get_conversation_settings).put(update_conversation_settings),
        )
        .route("/api/messages/:contact_id", get(get_messages))
        .route("/api/media/:message_id", get(get_media))
        .route("/api/avatar/:jid", get(get_avatar))
        .route("/api/qr", get(get_qr))
        .route("/api/send", post(send_message))
        .route("/api/send-image", post(send_image))
        .route("/api/react", post(send_reaction))
        .route("/api/ai-compose", post(ai_compose))
        .route("/api/ai-reply", post(ai_reply))
        .route("/api/translate", post(translate_message))
        .route("/api/stats", get(get_stats))
        .route("/api/usage", get(get_global_usage))
        .route("/api/usage/:contact_id", get(get_conversation_usage))
        .route("/api/link-preview", get(get_link_preview))
        .route("/api/oauth/clients", get(list_oauth_clients))
        .route("/api/oauth/clients/:client_id", delete(revoke_oauth_client))
        .route("/api/oauth/revoke-all", post(revoke_all_oauth_clients))
        .route("/ws", get(websocket_handler))
        .route_layer(middleware::from_fn(move |req: Request, next: Next| {
            let state = auth_state.clone();
            async move { require_web_auth(state, req, next).await }
        }));

    Router::new()
        .route("/", get(serve_index))
        .route("/index.html", get(serve_index))
        .route("/api/health", get(get_health))
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
        .merge(protected_api)
        // MCP (Model Context Protocol) endpoint - HTTP transport
        .route("/mcp", post(mcp_handler))
        // Serve static files
        .fallback_service(serve_dir)
        .with_state(state)
}

/// Start the web server
pub async fn start_server(state: Arc<AppState>, host: &str, port: u16) -> anyhow::Result<()> {
    let addr: SocketAddr = format!("{}:{}", host, port).parse()?;
    cleanup_expired_oauth(&state, "startup");
    spawn_oauth_cleanup_task(state.clone());
    let router = create_router(state);

    info!("Web server running at http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, router).await?;

    Ok(())
}

fn cleanup_expired_oauth(state: &Arc<AppState>, reason: &str) {
    if let Err(e) = state.store.oauth_cleanup_expired() {
        warn!(
            "Failed to clean expired OAuth records during {}: {}",
            reason, e
        );
    }
}

fn spawn_oauth_cleanup_task(state: Arc<AppState>) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(OAUTH_CLEANUP_INTERVAL).await;
            cleanup_expired_oauth(&state, "periodic cleanup");
        }
    });
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
        let token = generate_token();
        let expires_at = chrono::Utc::now().timestamp() + WEB_AUTH_TOKEN_TTL_SECONDS;

        state
            .auth_tokens
            .write()
            .await
            .insert(token.clone(), expires_at);

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

async fn require_web_auth(state: Arc<AppState>, req: Request, next: Next) -> Response {
    let auth_header = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .map(str::to_string);
    let websocket_token = if req.uri().path() == "/ws" {
        token_query_param(req.uri().query())
    } else {
        None
    };

    if verify_auth_values(&state, auth_header.as_deref(), websocket_token.as_deref()).await {
        next.run(req).await
    } else {
        StatusCode::UNAUTHORIZED.into_response()
    }
}

async fn verify_auth_values(
    state: &Arc<AppState>,
    auth_header: Option<&str>,
    websocket_token: Option<&str>,
) -> bool {
    if verify_auth_header(state, auth_header).await {
        return true;
    }

    if let Some(token) = websocket_token {
        return verify_auth_token(state, token).await;
    }

    false
}

fn token_query_param(query: Option<&str>) -> Option<String> {
    let query = query?;
    query.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=')?;
        if key == "token" {
            urlencoding::decode(value)
                .ok()
                .map(|token| token.into_owned())
        } else {
            None
        }
    })
}

async fn verify_auth_header(state: &Arc<AppState>, auth_header: Option<&str>) -> bool {
    if state.password.is_none() {
        return true;
    }

    if let Some(header) = auth_header {
        if let Some(token) = header.strip_prefix("Bearer ") {
            return verify_auth_token(state, token).await;
        }
    }

    false
}

async fn verify_auth_token(state: &Arc<AppState>, token: &str) -> bool {
    if state.password.is_none() {
        return true;
    }

    let now = chrono::Utc::now().timestamp();
    let mut tokens = state.auth_tokens.write().await;
    tokens.retain(|_, expires_at| *expires_at > now);

    tokens
        .get(token)
        .map(|expires_at| *expires_at > now)
        .unwrap_or(false)
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

    if let Err(e) = state.store.oauth_clear_all() {
        error!("Failed to clear OAuth tokens: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "success": false,
                "error": format!("Failed to clear OAuth tokens: {}", e)
            })),
        )
            .into_response();
    }

    // 2. Send logout command to bridge (this will notify WhatsApp and stop the bridge).
    // The bridge restart loop clears session files after the process exits and before respawn.
    state.request_session_reset_before_bridge_restart();
    let mut sent_logout_to_bridge = false;
    if let Some(tx) = state.command_tx.read().await.as_ref() {
        if let Err(e) = tx.send(BridgeCommand::Logout).await {
            warn!("Failed to send logout command to bridge: {}", e);
        } else {
            sent_logout_to_bridge = true;
        }
    }

    if !sent_logout_to_bridge {
        if let Err(e) = state.clear_session_files() {
            warn!("Failed to remove session files: {}", e);
        }
        state.take_session_reset_request();
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

async fn get_health() -> Json<HealthResponse> {
    Json(HealthResponse {
        ok: true,
        service: "whatsapp-translator",
    })
}

async fn serve_index(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let index_path = state.web_dir.join("index.html");

    match tokio::fs::read_to_string(&index_path).await {
        Ok(contents) => Html(contents).into_response(),
        Err(err) => {
            error!("Failed to read index.html from {:?}: {}", index_path, err);
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to load web UI").into_response()
        }
    }
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

async fn mark_contact_as_read(
    State(state): State<Arc<AppState>>,
    Path(contact_id): Path<String>,
    Json(req): Json<MarkReadRequest>,
) -> impl IntoResponse {
    let contact_id = urlencoding::decode(&contact_id)
        .map(|s| s.into_owned())
        .unwrap_or(contact_id);

    match state.store.mark_as_read(&contact_id) {
        Ok(()) => {
            if let (Some(message_id), Some(timestamp)) = (req.message_id, req.timestamp) {
                if let Some(tx) = state.command_tx.read().await.as_ref() {
                    let cmd = BridgeCommand::MarkRead {
                        to: contact_id.clone(),
                        message_id,
                        timestamp,
                        sender_jid: req.sender_jid,
                    };

                    if let Err(e) = tx.send(cmd).await {
                        warn!("Failed to send mark-read command to bridge: {}", e);
                    }
                }
            }

            Json(serde_json::json!({
                "success": true
            }))
            .into_response()
        }
        Err(e) => {
            error!("Failed to mark contact as read: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "success": false,
                    "error": "Failed to mark conversation as read"
                })),
            )
                .into_response()
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

/// Get conversation settings for a contact
async fn get_conversation_settings(
    State(state): State<Arc<AppState>>,
    Path(contact_id): Path<String>,
) -> impl IntoResponse {
    let contact_id = urlencoding::decode(&contact_id)
        .map(|s| s.into_owned())
        .unwrap_or(contact_id);

    match state.store.get_conversation_settings(&contact_id) {
        Ok(settings) => Json(ConversationSettingsResponse {
            language_override: settings.language_override,
            translation_style: settings.translation_style,
            send_original_follow_up: settings.send_original_follow_up,
        })
        .into_response(),
        Err(e) => {
            error!("Failed to get conversation settings: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to get conversation settings",
            )
                .into_response()
        }
    }
}

/// Update conversation settings for a contact
async fn update_conversation_settings(
    State(state): State<Arc<AppState>>,
    Path(contact_id): Path<String>,
    Json(req): Json<UpdateConversationSettingsRequest>,
) -> impl IntoResponse {
    let contact_id = urlencoding::decode(&contact_id)
        .map(|s| s.into_owned())
        .unwrap_or(contact_id);

    // Convert empty strings to None
    let settings = crate::storage::ConversationSettings {
        language_override: req.language_override.filter(|s| !s.trim().is_empty()),
        translation_style: req.translation_style.filter(|s| !s.trim().is_empty()),
        send_original_follow_up: req.send_original_follow_up,
    };

    match state
        .store
        .update_conversation_settings(&contact_id, &settings)
    {
        Ok(()) => Json(serde_json::json!({
            "success": true,
            "languageOverride": settings.language_override,
            "translationStyle": settings.translation_style,
            "sendOriginalFollowUp": settings.send_original_follow_up
        }))
        .into_response(),
        Err(e) => {
            error!("Failed to update conversation settings: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "success": false,
                    "error": format!("Failed to update settings: {}", e)
                })),
            )
                .into_response()
        }
    }
}

/// Query parameters for messages pagination
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MessagesQuery {
    /// Maximum number of messages to return (default: 30 for initial load)
    limit: Option<u32>,
    /// Only get messages before this timestamp (for loading older messages)
    before: Option<i64>,
    /// Tie-breaker message ID for stable pagination when timestamps match
    before_id: Option<String>,
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

    // Default to 30 messages for initial load, unless explicitly requesting all (limit=0)
    let limit = match params.limit {
        Some(0) => None, // 0 means all messages (for backwards compatibility / MCP)
        Some(n) => Some(n),
        None => Some(30), // Default to 30 for lazy loading
    };
    let storage_limit = limit.map(|l| l.saturating_add(1));

    // Strip media_data from messages to reduce payload (media loaded on demand via /api/media)
    match state.store.get_messages_paginated(
        &contact_id,
        storage_limit,
        params.before,
        params.before_id.as_deref(),
        true,
    ) {
        Ok(mut messages) => {
            let has_more = limit.map(|l| messages.len() > l as usize).unwrap_or(false);
            if has_more {
                messages.remove(0);
            }
            Json(MessagesResponse { messages, has_more }).into_response()
        }
        Err(e) => {
            error!("Failed to get messages: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to get messages").into_response()
        }
    }
}

/// Get media data for a specific message (lazy loaded)
async fn get_media(
    State(state): State<Arc<AppState>>,
    Path(message_id): Path<String>,
) -> impl IntoResponse {
    let message_id = urlencoding::decode(&message_id)
        .map(|s| s.into_owned())
        .unwrap_or(message_id);

    match state.store.get_message_media(&message_id) {
        Ok(Some((media_data, mime_type))) => {
            // Return the base64 media data and mime type
            Json(serde_json::json!({
                "media_data": media_data,
                "mime_type": mime_type
            }))
            .into_response()
        }
        Ok(None) => (StatusCode::NOT_FOUND, "Media not found").into_response(),
        Err(e) => {
            error!("Failed to get media: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to get media").into_response()
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

    let settings = state
        .store
        .get_conversation_settings(&req.contact_id)
        .unwrap_or_default();

    // Determine the text to send - translate if needed based on conversation settings or language
    let (text_to_send, _original_text, was_translated, target_language) =
        if let Some(translator) = &state.translator {
            // Determine target language: settings override > auto-detected > none
            let target_lang = if let Some(ref lang_override) = settings.language_override {
                // User explicitly set a language override - ALWAYS use it
                Some(lang_override.clone())
            } else {
                // Fall back to auto-detected conversation language
                state
                    .store
                    .get_conversation_language(&req.contact_id, 10)
                    .ok()
                    .flatten()
            };

            if let Some(conv_lang) = target_lang {
                info!(
                    "Target language for {} is {} (override: {})",
                    req.contact_id,
                    conv_lang,
                    settings.language_override.is_some()
                );

                // If there's a language override, always translate (even English -> other)
                // Otherwise, use the normal translate_to which skips if already in target
                let force_translate = settings.language_override.is_some();

                match translator
                    .translate_outgoing(&req.text, &conv_lang, force_translate)
                    .await
                {
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
            } else {
                // No target language set or detected
                (req.text.clone(), None, false, None)
            }
        } else {
            (req.text.clone(), None, false, None)
        };

    let send_plan = build_outgoing_send_plan(
        text_to_send,
        &req.text,
        was_translated,
        settings.send_original_follow_up,
    );
    let text_to_send = send_plan[0].clone();

    let request_id = state.next_request_id();
    let timestamp = chrono::Utc::now().timestamp_millis();
    let temp_message_id = format!("pending_{}_{}", request_id, timestamp);

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
        content_json: serde_json::json!({
            "type": "text",
            "body": req.text.clone(),
            "showTranslatedPrimary": was_translated && settings.send_original_follow_up,
            "reply_context": req.reply_to.as_ref().map(|reply_to| serde_json::json!({
                "messageId": reply_to,
                "senderName": req.reply_to_sender_name.clone().unwrap_or_else(|| {
                    req.reply_to_sender.clone().unwrap_or_else(|| "Unknown".to_string())
                }),
                "text": req.reply_to_text.clone().unwrap_or_default()
            }))
        })
        .to_string(),
        content: Some(serde_json::json!({
            "type": "text",
            "body": req.text.clone(),
            "showTranslatedPrimary": was_translated && settings.send_original_follow_up,
            "reply_context": req.reply_to.as_ref().map(|reply_to| serde_json::json!({
                "messageId": reply_to,
                "senderName": req.reply_to_sender_name.clone().unwrap_or_else(|| {
                    req.reply_to_sender.clone().unwrap_or_else(|| "Unknown".to_string())
                }),
                "text": req.reply_to_text.clone().unwrap_or_default()
            }))
        })),
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

    if let Err(e) = state.store.add_message(&stored_msg) {
        error!("Failed to store sent message: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": format!("Failed to store pending message: {}", e)
            })),
        )
            .into_response();
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
        let _ = state.store.delete_message(&temp_message_id);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": format!("Failed to update contact: {}", e)
            })),
        )
            .into_response();
    }

    let rx = state
        .register_pending_send(request_id, Some(temp_message_id.clone()))
        .await;

    let cmd = BridgeCommand::Send {
        request_id: Some(request_id),
        to: req.contact_id.clone(),
        text: text_to_send.clone(),
        reply_to: req.reply_to.clone(),
        reply_to_sender: req.reply_to_sender.clone(),
        reply_to_text: req.reply_to_text.clone(),
    };

    if let Err(e) = state.send_bridge_command(cmd).await {
        error!("Failed to send message: {}", e);
        state.cancel_pending_send(request_id).await;
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": format!("Failed to send message: {}", e)
            })),
        )
            .into_response();
    }

    let send_result = match state.wait_for_send_result(request_id, rx).await {
        Ok(result) => result,
        Err(SendConfirmationError::Timeout) => {
            return (
                StatusCode::GATEWAY_TIMEOUT,
                Json(serde_json::json!({
                    "error": "Timed out waiting for WhatsApp send confirmation"
                })),
            )
                .into_response();
        }
        Err(SendConfirmationError::ChannelClosed) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({
                    "error": "WhatsApp send confirmation channel closed"
                })),
            )
                .into_response();
        }
    };

    if !send_result.success {
        return (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({
                "error": send_result.error.unwrap_or_else(|| "WhatsApp rejected the message".to_string())
            })),
        )
            .into_response();
    }

    let confirmed_message_id = send_result.message_id.unwrap_or(temp_message_id);
    let confirmed_timestamp = send_result.timestamp.unwrap_or(timestamp);

    let (
        original_follow_up_sent,
        original_message_id,
        original_timestamp,
        original_follow_up_error,
    ) = if let Some(original_text) = send_plan.get(1) {
        match send_original_follow_up(&state, &req, &stored_msg, original_text).await {
            Ok((message_id, timestamp)) => (true, Some(message_id), Some(timestamp), None),
            Err(error) => {
                warn!(
                    "Translated message sent to {}, but original follow-up failed: {}",
                    req.contact_id, error
                );
                (false, None, None, Some(error))
            }
        }
    } else {
        (false, None, None, None)
    };

    Json(SendMessageResponse {
        message_id: confirmed_message_id,
        timestamp: confirmed_timestamp,
        is_translated: was_translated,
        translated_text: if was_translated {
            Some(text_to_send)
        } else {
            None
        },
        source_language: target_language,
        original_follow_up_sent,
        original_message_id,
        original_timestamp,
        original_follow_up_error,
    })
    .into_response()
}

async fn send_original_follow_up(
    state: &Arc<AppState>,
    req: &SendMessageRequest,
    primary_message: &StoredMessage,
    original_text: &str,
) -> Result<(String, i64), String> {
    let request_id = state.next_request_id();
    let timestamp = chrono::Utc::now()
        .timestamp_millis()
        .max(primary_message.timestamp.saturating_add(1));
    let temp_message_id = format!("pending_original_{}_{}", request_id, timestamp);
    let content = serde_json::json!({
        "type": "text",
        "body": original_text,
    });
    let stored_msg = StoredMessage {
        id: temp_message_id.clone(),
        contact_id: req.contact_id.clone(),
        timestamp,
        is_from_me: true,
        is_forwarded: false,
        sender_name: primary_message.sender_name.clone(),
        sender_phone: primary_message.sender_phone.clone(),
        contact_name: primary_message.contact_name.clone(),
        contact_phone: primary_message.contact_phone.clone(),
        chat_type: primary_message.chat_type.clone(),
        content_type: "Text".to_string(),
        content_json: content.to_string(),
        content: Some(content),
        original_text: None,
        translated_text: None,
        source_language: None,
        is_translated: false,
    };

    state
        .store
        .add_message(&stored_msg)
        .map_err(|error| format!("Failed to store original follow-up: {}", error))?;

    if let Err(error) = state.store.upsert_contact(
        &stored_msg.contact_id,
        stored_msg.contact_name.as_deref(),
        stored_msg.contact_phone.as_deref(),
        Some(&stored_msg.chat_type),
        stored_msg.timestamp,
    ) {
        let _ = state.store.delete_message(&temp_message_id);
        return Err(format!(
            "Failed to update contact for original follow-up: {}",
            error
        ));
    }

    let rx = state
        .register_pending_send(request_id, Some(temp_message_id.clone()))
        .await;
    let command = BridgeCommand::Send {
        request_id: Some(request_id),
        to: req.contact_id.clone(),
        text: original_text.to_string(),
        reply_to: None,
        reply_to_sender: None,
        reply_to_text: None,
    };

    if let Err(error) = state.send_bridge_command(command).await {
        state.cancel_pending_send(request_id).await;
        return Err(format!("Failed to send original follow-up: {}", error));
    }

    let send_result =
        state
            .wait_for_send_result(request_id, rx)
            .await
            .map_err(|error| match error {
                SendConfirmationError::Timeout => {
                    "Timed out waiting for original follow-up confirmation".to_string()
                }
                SendConfirmationError::ChannelClosed => {
                    "Original follow-up confirmation channel closed".to_string()
                }
            })?;

    if !send_result.success {
        return Err(send_result
            .error
            .unwrap_or_else(|| "WhatsApp rejected the original follow-up".to_string()));
    }

    Ok((
        send_result.message_id.unwrap_or(temp_message_id),
        send_result.timestamp.unwrap_or(timestamp),
    ))
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

    let validated_image = match validate_image_payload(&req.media_data, &req.mime_type) {
        Ok(payload) => payload,
        Err(error) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": error
                })),
            )
                .into_response();
        }
    };
    let validated_mime_type = validated_image.mime_type.clone();
    let validated_decoded_size = validated_image.decoded_size;

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

    let request_id = state.next_request_id();
    let timestamp = chrono::Utc::now().timestamp_millis();
    let temp_message_id = format!("pending_img_{}_{}", request_id, timestamp);

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
            "mime_type": validated_mime_type.clone(),
            "caption": req.caption,
            "media_data": req.media_data,
            "file_size": validated_decoded_size,
            "reply_context": req.reply_to.as_ref().map(|reply_to| serde_json::json!({
                "messageId": reply_to,
                "senderName": req.reply_to_sender_name.clone().unwrap_or_else(|| {
                    req.reply_to_sender.clone().unwrap_or_else(|| "Unknown".to_string())
                }),
                "text": req.reply_to_text.clone().unwrap_or_default()
            }))
        })
        .to_string(),
        content: Some(serde_json::json!({
            "type": "image",
            "mime_type": validated_mime_type.clone(),
            "caption": req.caption,
            "media_data": req.media_data,
            "file_size": validated_decoded_size,
            "reply_context": req.reply_to.as_ref().map(|reply_to| serde_json::json!({
                "messageId": reply_to,
                "senderName": req.reply_to_sender_name.clone().unwrap_or_else(|| {
                    req.reply_to_sender.clone().unwrap_or_else(|| "Unknown".to_string())
                }),
                "text": req.reply_to_text.clone().unwrap_or_default()
            }))
        })),
        original_text: None,
        translated_text: None,
        source_language: None,
        is_translated: false,
    };

    // Store the message
    if let Err(e) = state.store.add_message(&stored_msg) {
        error!("Failed to store sent image: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": format!("Failed to store pending image: {}", e)
            })),
        )
            .into_response();
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
        let _ = state.store.delete_message(&temp_message_id);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": format!("Failed to update contact: {}", e)
            })),
        )
            .into_response();
    }

    let rx = state
        .register_pending_send(request_id, Some(temp_message_id.clone()))
        .await;

    let cmd = BridgeCommand::SendImage {
        request_id: Some(request_id),
        to: req.contact_id.clone(),
        media_data: req.media_data.clone(),
        mime_type: validated_mime_type,
        caption: req.caption.clone(),
        reply_to: req.reply_to.clone(),
        reply_to_sender: req.reply_to_sender.clone(),
        reply_to_text: req.reply_to_text.clone(),
    };

    if let Err(e) = state.send_bridge_command(cmd).await {
        error!("Failed to send image: {}", e);
        state.cancel_pending_send(request_id).await;
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": format!("Failed to send image: {}", e)
            })),
        )
            .into_response();
    }

    let send_result = match state.wait_for_send_result(request_id, rx).await {
        Ok(result) => result,
        Err(SendConfirmationError::Timeout) => {
            return (
                StatusCode::GATEWAY_TIMEOUT,
                Json(serde_json::json!({
                    "error": "Timed out waiting for WhatsApp image send confirmation"
                })),
            )
                .into_response();
        }
        Err(SendConfirmationError::ChannelClosed) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({
                    "error": "WhatsApp image send confirmation channel closed"
                })),
            )
                .into_response();
        }
    };

    if !send_result.success {
        return (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({
                "error": send_result.error.unwrap_or_else(|| "WhatsApp rejected the image".to_string())
            })),
        )
            .into_response();
    }

    let confirmed_message_id = send_result.message_id.unwrap_or(temp_message_id);
    let confirmed_timestamp = send_result.timestamp.unwrap_or(timestamp);

    Json(SendImageResponse {
        message_id: confirmed_message_id,
        timestamp: confirmed_timestamp,
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

    let request_id = state.next_request_id();
    let rx = state.register_pending_send(request_id, None).await;

    // Send the reaction via bridge
    let cmd = BridgeCommand::SendReaction {
        request_id: Some(request_id),
        to: req.contact_id.clone(),
        message_id: req.message_id.clone(),
        sender_jid: req.sender_jid.clone(),
        emoji: req.emoji.clone(),
    };

    if let Err(e) = state.send_bridge_command(cmd).await {
        error!("Failed to send reaction: {}", e);
        state.cancel_pending_send(request_id).await;
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": format!("Failed to send reaction: {}", e)
            })),
        )
            .into_response();
    }

    let send_result = match state.wait_for_send_result(request_id, rx).await {
        Ok(result) => result,
        Err(SendConfirmationError::Timeout) => {
            return (
                StatusCode::GATEWAY_TIMEOUT,
                Json(serde_json::json!({
                    "error": "Timed out waiting for WhatsApp reaction confirmation"
                })),
            )
                .into_response();
        }
        Err(SendConfirmationError::ChannelClosed) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({
                    "error": "WhatsApp reaction confirmation channel closed"
                })),
            )
                .into_response();
        }
    };

    if !send_result.success {
        return (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({
                "error": send_result.error.unwrap_or_else(|| "WhatsApp rejected the reaction".to_string())
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
    // Get conversation settings for this contact
    let settings = state
        .store
        .get_conversation_settings(&req.contact_id)
        .unwrap_or_default();

    // Call the translation service with conversation settings
    let result = match translator
        .process_text(
            &req.text,
            settings.language_override.as_deref(),
            settings.translation_style.as_deref(),
        )
        .await
    {
        Ok(result) => result,
        Err(e) => {
            warn!("Manual translation failed for {}: {}", req.message_id, e);
            return (
                StatusCode::BAD_GATEWAY,
                Json(TranslateMessageResponse {
                    success: false,
                    translated_text: None,
                    source_language: None,
                    error: Some(format!("Translation failed: {}", e)),
                }),
            )
                .into_response();
        }
    };

    // Record usage if there was API usage
    if result.usage.input_tokens > 0 {
        if let Err(e) = state.store.record_usage(
            Some(&req.contact_id),
            Some(&req.message_id),
            &result.usage,
            if result.needs_translation {
                "manual_translate"
            } else {
                "detect_language"
            },
        ) {
            warn!("Failed to record translation usage: {}", e);
        }
    }

    if !result.needs_translation {
        return Json(TranslateMessageResponse {
            success: false,
            translated_text: None,
            source_language: Some(result.source_language.clone()),
            error: Some(format!(
                "Message already appears to be in {}",
                result.source_language
            )),
        })
        .into_response();
    }

    let translated_text = match result.translated_text {
        Some(text) if !text.trim().is_empty() => text,
        _ => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(TranslateMessageResponse {
                    success: false,
                    translated_text: None,
                    source_language: Some(result.source_language),
                    error: Some("Translation service returned no translated text".to_string()),
                }),
            )
                .into_response();
        }
    };

    // Update the message in the database with the translation
    if let Err(e) = state.store.update_message_translation(
        &req.message_id,
        Some(&translated_text),
        Some(&result.source_language),
    ) {
        warn!("Failed to update message translation in DB: {}", e);
    }

    Json(TranslateMessageResponse {
        success: true,
        translated_text: Some(translated_text),
        source_language: Some(result.source_language),
        error: None,
    })
    .into_response()
}

/// AI compose endpoint - generates a message using OpenAI
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

    // Call the AI compose method
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
                    cached_input_tokens: usage.cached_input_tokens,
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

/// AI styled reply endpoint - generates a reply that sounds like the user
async fn ai_reply(
    State(state): State<Arc<AppState>>,
    Json(req): Json<AiReplyRequest>,
) -> impl IntoResponse {
    // Check if translation service is available (it has the API key)
    let translator = match &state.translator {
        Some(t) => t,
        None => {
            return Json(AiReplyResponse {
                success: false,
                reply_text: None,
                error: Some("AI service not configured (missing API key)".to_string()),
                cost_usd: None,
            })
            .into_response();
        }
    };

    // Get the message being replied to
    let message = match state.store.get_message_by_id(&req.message_id) {
        Ok(Some(m)) => m,
        Ok(None) => {
            return Json(AiReplyResponse {
                success: false,
                reply_text: None,
                error: Some("Message not found".to_string()),
                cost_usd: None,
            })
            .into_response();
        }
        Err(e) => {
            error!("Failed to get message: {}", e);
            return Json(AiReplyResponse {
                success: false,
                reply_text: None,
                error: Some(format!("Failed to get message: {}", e)),
                cost_usd: None,
            })
            .into_response();
        }
    };

    // Get recent conversation for context (last 20 messages)
    let recent_conversation = match state.store.get_recent_messages(&req.contact_id, 20) {
        Ok(msgs) => msgs,
        Err(e) => {
            warn!("Failed to get recent messages: {}", e);
            vec![]
        }
    };

    // Create style analyzer
    let api_key = translator.get_api_key();
    let detection_model = translator.get_detection_model();
    let style_analyzer = crate::style_analyzer::StyleAnalyzer::new(api_key, detection_model);

    // Get or create global style profile
    let (global_style, global_usage) = match style_analyzer
        .get_or_create_profile(&state.store, None)
        .await
    {
        Ok(result) => result,
        Err(e) => {
            error!("Failed to get global style profile: {}", e);
            return Json(AiReplyResponse {
                success: false,
                reply_text: None,
                error: Some(format!("Failed to analyze writing style: {}", e)),
                cost_usd: None,
            })
            .into_response();
        }
    };

    // Get or create per-contact style profile
    let (contact_style, contact_usage) = match style_analyzer
        .get_or_create_profile(&state.store, Some(&req.contact_id))
        .await
    {
        Ok(result) => result,
        Err(e) => {
            warn!("Failed to get contact style profile: {}", e);
            // Continue without contact-specific style
            (
                crate::storage::StyleProfile {
                    contact_id: req.contact_id.clone(),
                    profile_text: "No contact-specific style data available.".to_string(),
                    sample_messages: vec![],
                    message_count: 0,
                    updated_at: 0,
                },
                None,
            )
        }
    };

    // Get examples of user's messages to this contact (more examples = better style matching)
    let my_examples = match state
        .store
        .get_outgoing_messages_for_style(Some(&req.contact_id), 20)
    {
        Ok(msgs) => msgs,
        Err(e) => {
            warn!("Failed to get example messages: {}", e);
            vec![]
        }
    };

    // Generate the styled reply
    match translator
        .compose_styled_reply(
            &message,
            &recent_conversation,
            &global_style,
            Some(&contact_style),
            &my_examples,
        )
        .await
    {
        Ok((reply_text, usage)) => {
            // Calculate total cost (including any style analysis)
            let mut total_input_tokens = usage.input_tokens as i64;
            let mut total_cached_input_tokens = usage.cached_input_tokens as i64;
            let mut total_output_tokens = usage.output_tokens as i64;
            let mut total_cost = usage.cost_usd;
            if let Some(gu) = &global_usage {
                total_input_tokens += gu.input_tokens as i64;
                total_cached_input_tokens += gu.cached_input_tokens as i64;
                total_output_tokens += gu.output_tokens as i64;
                total_cost += gu.cost_usd;
            }
            if let Some(cu) = &contact_usage {
                total_input_tokens += cu.input_tokens as i64;
                total_cached_input_tokens += cu.cached_input_tokens as i64;
                total_output_tokens += cu.output_tokens as i64;
                total_cost += cu.cost_usd;
            }

            info!(
                "AI reply generated ({} chars), cost: ${:.6}",
                reply_text.len(),
                total_cost
            );

            // Record usage
            if let Err(e) = state.store.record_usage(
                Some(&req.contact_id),
                Some(&req.message_id),
                &crate::translation::UsageInfo {
                    input_tokens: total_input_tokens as u32,
                    cached_input_tokens: total_cached_input_tokens as u32,
                    output_tokens: total_output_tokens as u32,
                    cost_usd: total_cost,
                },
                "ai_styled_reply",
            ) {
                warn!("Failed to record AI reply usage: {}", e);
            }

            Json(AiReplyResponse {
                success: true,
                reply_text: Some(reply_text),
                error: None,
                cost_usd: Some(total_cost),
            })
            .into_response()
        }
        Err(e) => {
            error!("AI reply generation failed: {}", e);
            Json(AiReplyResponse {
                success: false,
                reply_text: None,
                error: Some(format!("Failed to generate reply: {}", e)),
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
            "cachedInputTokens": usage.cached_input_tokens,
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
            "cachedInputTokens": usage.cached_input_tokens,
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

    if let Err(e) = link_preview::validate_link_preview_url(&url).await {
        return Json(link_preview::LinkPreview::error(
            url,
            format!("Blocked link preview URL: {}", e),
        ))
        .into_response();
    }

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

fn request_host_is_loopback(host: &str) -> bool {
    let normalized = if let Some(rest) = host.strip_prefix('[') {
        rest.split(']').next().unwrap_or(host)
    } else {
        host.split(':').next().unwrap_or(host)
    };

    matches!(normalized, "localhost" | "127.0.0.1" | "::1")
}

fn now_unix_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

fn normalize_oauth_scope(scope: Option<&str>) -> Result<String, String> {
    let scope = scope.unwrap_or("mcp").trim();
    if scope.is_empty() {
        return Ok("mcp".to_string());
    }

    let scopes: Vec<&str> = scope.split_whitespace().collect();
    if scopes.iter().all(|entry| *entry == "mcp") {
        Ok("mcp".to_string())
    } else {
        Err("Only the mcp OAuth scope is supported".to_string())
    }
}

fn validate_oauth_redirect_uri(redirect_uri: &str) -> Result<(), String> {
    let parsed = reqwest::Url::parse(redirect_uri)
        .map_err(|_| "redirect_uri must be an absolute URL".to_string())?;

    if parsed.fragment().is_some() {
        return Err("redirect_uri must not contain a fragment".to_string());
    }

    if !matches!(parsed.scheme(), "http" | "https") {
        return Err("redirect_uri must use loopback http or https".to_string());
    }

    let host = parsed
        .host_str()
        .ok_or_else(|| "redirect_uri must include a loopback host".to_string())?;
    if matches!(host, "localhost" | "127.0.0.1" | "::1") {
        Ok(())
    } else {
        Err("redirect_uri host must be localhost, 127.0.0.1, or ::1".to_string())
    }
}

fn validate_oauth_client_request(req: &ClientRegistrationRequest) -> Result<String, String> {
    if req.redirect_uris.is_empty() {
        return Err("redirect_uris must include at least one loopback URI".to_string());
    }

    for redirect_uri in &req.redirect_uris {
        validate_oauth_redirect_uri(redirect_uri)?;
    }

    if let Some(grant_types) = &req.grant_types {
        let allowed = ["authorization_code", "refresh_token"];
        if grant_types.is_empty()
            || grant_types
                .iter()
                .any(|grant| !allowed.contains(&grant.as_str()))
            || !grant_types
                .iter()
                .any(|grant| grant == "authorization_code")
        {
            return Err(
                "grant_types must contain authorization_code and may contain refresh_token"
                    .to_string(),
            );
        }
    }

    if let Some(response_types) = &req.response_types {
        if response_types.is_empty() || response_types.iter().any(|response| response != "code") {
            return Err("Only the code response type is supported".to_string());
        }
    }

    if let Some(method) = &req.token_endpoint_auth_method {
        if method != "none" {
            return Err(
                "Only public OAuth clients with token_endpoint_auth_method=none are supported"
                    .to_string(),
            );
        }
    }

    normalize_oauth_scope(req.scope.as_deref())
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
/// Allows public MCP clients to register before starting OAuth flow
async fn oauth_register(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ClientRegistrationRequest>,
) -> impl IntoResponse {
    let scope = match validate_oauth_client_request(&req) {
        Ok(scope) => scope,
        Err(error) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "invalid_client_metadata",
                    "error_description": error
                })),
            )
                .into_response();
        }
    };

    // Generate a client_id for this registration
    let client_id = format!("client_{}", &generate_token()[..16]);
    let now = now_unix_seconds();

    // For public clients, we don't issue a client_secret
    // The client will use PKCE for security instead
    let client = OAuthClientRegistration {
        client_id: client_id.clone(),
        client_name: req.client_name.clone(),
        redirect_uris: req.redirect_uris.clone(),
        scope: scope.clone(),
        created_at: now,
    };

    if let Err(e) = state.store.oauth_store_client(&client) {
        error!("Failed to store OAuth client registration: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": "server_error",
                "error_description": "Failed to store OAuth client registration"
            })),
        )
            .into_response();
    }

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
            "scope": scope,
        })),
    )
        .into_response()
}

/// OAuth Authorization endpoint - shows approval page
async fn oauth_authorize(
    State(state): State<Arc<AppState>>,
    Host(host): Host,
    Query(params): Query<AuthorizeRequest>,
) -> impl IntoResponse {
    if state.password.is_none() && !request_host_is_loopback(&host) {
        return (
            StatusCode::FORBIDDEN,
            Html("<html><body><h1>Error</h1><p>WA_PASSWORD is required before OAuth clients can be approved on a non-loopback host.</p></body></html>".to_string()),
        )
            .into_response();
    }

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

    let client = match state.store.oauth_get_client(&params.client_id) {
        Ok(Some(client)) => client,
        Ok(None) => {
            return (
                StatusCode::BAD_REQUEST,
                Html(
                    "<html><body><h1>Error</h1><p>Unknown OAuth client. Register the client before authorizing.</p></body></html>"
                        .to_string(),
                ),
            )
                .into_response();
        }
        Err(e) => {
            error!("Failed to look up OAuth client: {}", e);
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

    if !client
        .redirect_uris
        .iter()
        .any(|redirect_uri| redirect_uri == &params.redirect_uri)
    {
        return (
            StatusCode::BAD_REQUEST,
            Html("<html><body><h1>Error</h1><p>redirect_uri is not registered for this OAuth client.</p></body></html>".to_string()),
        )
            .into_response();
    }

    let requested_scope = match normalize_oauth_scope(params.scope.as_deref()) {
        Ok(scope) => scope,
        Err(error) => {
            return (
                StatusCode::BAD_REQUEST,
                Html(format!(
                    "<html><body><h1>Error</h1><p>{}</p></body></html>",
                    html_escape(&error)
                )),
            )
                .into_response();
        }
    };

    if requested_scope != client.scope {
        return (
            StatusCode::BAD_REQUEST,
            Html("<html><body><h1>Error</h1><p>Requested scope is not registered for this OAuth client.</p></body></html>".to_string()),
        )
            .into_response();
    }

    // Generate a session key for this authorization request
    let session_key = generate_token();
    let now = now_unix_seconds();

    let pending = PendingAuthorization {
        session_key: session_key.clone(),
        client_id: params.client_id.clone(),
        redirect_uri: params.redirect_uri.clone(),
        code_challenge: params.code_challenge.clone(),
        code_challenge_method: params.code_challenge_method.clone(),
        scope: requested_scope.clone(),
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
            <strong>Client:</strong> {client_name}<br>
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
        client_name = html_escape(
            client
                .client_name
                .as_deref()
                .unwrap_or("Unnamed MCP client")
        ),
        client_id = html_escape(&params.client_id),
        redirect_uri = html_escape(&params.redirect_uri),
        scope = html_escape(&requested_scope),
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

    if let Some(client_id) = &req.client_id {
        if client_id != &auth_code.client_id {
            let error = OAuthErrorResponse::from(OAuthError::InvalidClient);
            return (StatusCode::BAD_REQUEST, Json(error)).into_response();
        }
    }

    let client = match state.store.oauth_get_client(&auth_code.client_id) {
        Ok(Some(client)) => client,
        Ok(None) => {
            let error = OAuthErrorResponse::from(OAuthError::InvalidClient);
            return (StatusCode::BAD_REQUEST, Json(error)).into_response();
        }
        Err(e) => {
            error!("Failed to look up OAuth client: {}", e);
            let error = OAuthErrorResponse::from(OAuthError::ServerError);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(error)).into_response();
        }
    };

    if !client
        .redirect_uris
        .iter()
        .any(|registered| registered == redirect_uri)
    {
        let error = OAuthErrorResponse::from(OAuthError::InvalidGrant);
        return (StatusCode::BAD_REQUEST, Json(error)).into_response();
    }

    // Verify PKCE
    if !auth_code.verify_pkce(code_verifier) {
        let error = OAuthErrorResponse::from(OAuthError::InvalidGrant);
        return (StatusCode::BAD_REQUEST, Json(error)).into_response();
    }

    // Generate tokens
    let now = now_unix_seconds();

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

    if let Some(client_id) = &req.client_id {
        if client_id != &refresh_token.client_id {
            let error = OAuthErrorResponse::from(OAuthError::InvalidClient);
            return (StatusCode::BAD_REQUEST, Json(error)).into_response();
        }
    }

    // Generate new access token
    let now = now_unix_seconds();

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

async fn list_oauth_clients(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    match state.store.oauth_list_clients() {
        Ok(clients) => Json(serde_json::json!({ "clients": clients })).into_response(),
        Err(e) => {
            error!("Failed to list OAuth clients: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": format!("Failed to list OAuth clients: {}", e)
                })),
            )
                .into_response()
        }
    }
}

async fn revoke_oauth_client(
    State(state): State<Arc<AppState>>,
    Path(client_id): Path<String>,
) -> impl IntoResponse {
    match state.store.oauth_revoke_client(&client_id) {
        Ok(revoked) => Json(serde_json::json!({
            "success": true,
            "clientId": client_id,
            "revoked": revoked
        }))
        .into_response(),
        Err(e) => {
            error!("Failed to revoke OAuth client {}: {}", client_id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": format!("Failed to revoke OAuth client: {}", e)
                })),
            )
                .into_response()
        }
    }
}

async fn revoke_all_oauth_clients(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    match state.store.oauth_clear_all() {
        Ok(()) => Json(serde_json::json!({ "success": true })).into_response(),
        Err(e) => {
            error!("Failed to revoke all OAuth clients: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": format!("Failed to revoke OAuth clients: {}", e)
                })),
            )
                .into_response()
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

// MCP (Model Context Protocol) HTTP handler
// Uses Streamable HTTP transport (POST for requests, SSE for responses)

use rmcp::transport::streamable_http_server::{
    session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
};

fn create_mcp_service(
    state: Arc<AppState>,
) -> StreamableHttpService<WhatsAppMcpServer, LocalSessionManager> {
    let session_manager = Arc::new(LocalSessionManager::default());
    let config = StreamableHttpServerConfig {
        stateful_mode: false, // Stateless mode - simpler, no session management needed
        ..Default::default()
    };

    StreamableHttpService::new(
        move || {
            // Create a new MCP server instance for each request
            Ok(WhatsAppMcpServer::new(state.clone()))
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

    let service = create_mcp_service(state);
    // StreamableHttpService has an async handle method we can call directly
    service.handle(request).await.into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request as HttpRequest;
    use std::path::PathBuf;
    use tower::ServiceExt;

    fn test_state(password: Option<&str>) -> (Arc<AppState>, PathBuf) {
        let data_dir = std::env::temp_dir().join(format!(
            "whatsapp-translator-web-test-{}",
            uuid::Uuid::new_v4()
        ));
        let store = MessageStore::new(&data_dir).expect("test message store");
        let web_dir = std::env::current_dir()
            .expect("current dir")
            .join("web/public");
        let state = AppState::new(
            store,
            web_dir,
            data_dir.clone(),
            None,
            password.map(str::to_string),
        );
        (state, data_dir)
    }

    fn empty_request(uri: &str) -> HttpRequest<Body> {
        HttpRequest::builder()
            .uri(uri)
            .body(Body::empty())
            .expect("request")
    }

    fn test_outgoing_message(id: &str, timestamp: i64) -> StoredMessage {
        let content = serde_json::json!({
            "type": "text",
            "body": "hello",
        });

        StoredMessage {
            id: id.to_string(),
            contact_id: "chat@example.test".to_string(),
            timestamp,
            is_from_me: true,
            is_forwarded: false,
            sender_name: Some("Me".to_string()),
            sender_phone: Some("123".to_string()),
            contact_name: Some("Test Chat".to_string()),
            contact_phone: None,
            chat_type: "private".to_string(),
            content_type: "Text".to_string(),
            content_json: content.to_string(),
            content: Some(content),
            original_text: None,
            translated_text: None,
            source_language: None,
            is_translated: false,
        }
    }

    #[test]
    fn outgoing_send_plan_puts_translation_before_original_follow_up() {
        assert_eq!(
            build_outgoing_send_plan("Hola a todos".to_string(), "Hello everyone", true, true,),
            vec!["Hola a todos", "Hello everyone"]
        );
        assert_eq!(
            build_outgoing_send_plan("Hello everyone".to_string(), "Hello everyone", false, true,),
            vec!["Hello everyone"]
        );
    }

    #[test]
    fn clear_orphaned_session_sidecars_removes_sidecars_without_session_db() {
        let (state, data_dir) = test_state(None);
        std::fs::write(data_dir.join("session.db-wal"), b"wal").expect("write wal");
        std::fs::write(data_dir.join("session.db-shm"), b"shm").expect("write shm");

        state
            .clear_orphaned_session_sidecars()
            .expect("clear sidecars");

        assert!(!data_dir.join("session.db-wal").exists());
        assert!(!data_dir.join("session.db-shm").exists());

        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[test]
    fn clear_orphaned_session_sidecars_keeps_sidecars_with_session_db() {
        let (state, data_dir) = test_state(None);
        std::fs::write(data_dir.join("session.db"), b"session").expect("write session");
        std::fs::write(data_dir.join("session.db-wal"), b"wal").expect("write wal");
        std::fs::write(data_dir.join("session.db-shm"), b"shm").expect("write shm");

        state
            .clear_orphaned_session_sidecars()
            .expect("clear sidecars");

        assert!(data_dir.join("session.db").exists());
        assert!(data_dir.join("session.db-wal").exists());
        assert!(data_dir.join("session.db-shm").exists());

        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[tokio::test]
    async fn protected_api_allows_requests_when_password_is_not_configured() {
        let (state, data_dir) = test_state(None);
        let response = create_router(state)
            .oneshot(empty_request("/api/status"))
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);
        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[tokio::test]
    async fn protected_api_rejects_missing_token_when_password_is_configured() {
        let (state, data_dir) = test_state(Some("secret"));
        let response = create_router(state)
            .oneshot(empty_request("/api/status"))
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[tokio::test]
    async fn health_check_stays_public_when_password_is_configured() {
        let (state, data_dir) = test_state(Some("secret"));
        let response = create_router(state)
            .oneshot(empty_request("/api/health"))
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body bytes");
        let payload: serde_json::Value = serde_json::from_slice(&body).expect("health json");
        assert_eq!(payload["ok"], true);
        assert_eq!(payload["service"], "whatsapp-translator");

        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[tokio::test]
    async fn protected_api_accepts_valid_bearer_token() {
        let (state, data_dir) = test_state(Some("secret"));
        let token = generate_token();
        state
            .auth_tokens
            .write()
            .await
            .insert(token.clone(), chrono::Utc::now().timestamp() + 60);

        let request = HttpRequest::builder()
            .uri("/api/status")
            .header(header::AUTHORIZATION, format!("Bearer {}", token))
            .body(Body::empty())
            .expect("request");

        let response = create_router(state)
            .oneshot(request)
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);
        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[tokio::test]
    async fn websocket_route_rejects_missing_token_before_upgrade() {
        let (state, data_dir) = test_state(Some("secret"));
        let response = create_router(state)
            .oneshot(empty_request("/ws"))
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[tokio::test]
    async fn logout_route_rejects_missing_token_when_password_is_configured() {
        let (state, data_dir) = test_state(Some("secret"));
        let request = HttpRequest::builder()
            .method("POST")
            .uri("/api/logout")
            .body(Body::empty())
            .expect("request");

        let response = create_router(state)
            .oneshot(request)
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[tokio::test]
    async fn logout_clears_messages_oauth_clients_and_ui_tokens() {
        let (state, data_dir) = test_state(Some("secret"));
        for filename in ["session.db", "session.db-wal", "session.db-shm"] {
            std::fs::write(data_dir.join(filename), b"session").expect("write session file");
        }
        state
            .store
            .upsert_contact(
                "chat@example.test",
                Some("Test Chat"),
                None,
                Some("private"),
                1_700_000_000_000,
            )
            .expect("insert contact");
        state
            .store
            .add_message(&test_outgoing_message("msg_1", 1_700_000_000_000))
            .expect("insert message");

        let client = OAuthClientRegistration {
            client_id: "client_test".to_string(),
            client_name: Some("Local MCP Client".to_string()),
            redirect_uris: vec!["http://127.0.0.1:8787/callback".to_string()],
            scope: "mcp".to_string(),
            created_at: 1_700_000_000,
        };
        state
            .store
            .oauth_store_client(&client)
            .expect("store oauth client");
        state
            .store
            .oauth_store_access_token(&AccessToken {
                token: "access-token".to_string(),
                client_id: client.client_id.clone(),
                scope: "mcp".to_string(),
                created_at: 1_700_000_000,
                expires_at: i64::MAX,
            })
            .expect("store access token");
        state
            .store
            .oauth_store_refresh_token(&RefreshToken {
                token: "refresh-token".to_string(),
                client_id: client.client_id.clone(),
                scope: "mcp".to_string(),
                created_at: 1_700_000_000,
                expires_at: i64::MAX,
            })
            .expect("store refresh token");

        let ui_token = generate_token();
        state
            .auth_tokens
            .write()
            .await
            .insert(ui_token.clone(), chrono::Utc::now().timestamp() + 60);

        let request = HttpRequest::builder()
            .method("POST")
            .uri("/api/logout")
            .header(header::AUTHORIZATION, format!("Bearer {}", ui_token))
            .body(Body::empty())
            .expect("request");

        let response = create_router(state.clone())
            .oneshot(request)
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);
        assert!(state.store.get_contacts().expect("contacts").is_empty());
        assert!(state
            .store
            .get_messages_paginated("chat@example.test", None, None, None, true)
            .expect("messages")
            .is_empty());
        assert!(state
            .store
            .oauth_list_clients()
            .expect("oauth clients")
            .is_empty());
        assert!(state
            .store
            .oauth_validate_access_token("access-token")
            .expect("access token")
            .is_none());
        assert!(state
            .store
            .oauth_get_refresh_token("refresh-token")
            .expect("refresh token")
            .is_none());
        assert!(state.auth_tokens.read().await.is_empty());
        for filename in ["session.db", "session.db-wal", "session.db-shm"] {
            assert!(
                !data_dir.join(filename).exists(),
                "{filename} should be removed"
            );
        }
        assert!(!state.take_session_reset_request());

        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[tokio::test]
    async fn logout_defers_session_file_cleanup_until_bridge_restart_when_bridge_is_running() {
        let (state, data_dir) = test_state(Some("secret"));
        let (command_tx, mut command_rx) = mpsc::channel(1);
        state.set_command_tx(command_tx).await;

        let ui_token = generate_token();
        state
            .auth_tokens
            .write()
            .await
            .insert(ui_token.clone(), chrono::Utc::now().timestamp() + 60);

        let request = HttpRequest::builder()
            .method("POST")
            .uri("/api/logout")
            .header(header::AUTHORIZATION, format!("Bearer {}", ui_token))
            .body(Body::empty())
            .expect("request");

        let response = create_router(state.clone())
            .oneshot(request)
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);
        assert!(matches!(command_rx.try_recv(), Ok(BridgeCommand::Logout)));
        assert!(state.take_session_reset_request());

        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[test]
    fn oauth_redirect_validation_allows_only_loopback_http_callbacks() {
        assert!(validate_oauth_redirect_uri("http://127.0.0.1:8787/callback").is_ok());
        assert!(validate_oauth_redirect_uri("http://localhost/callback").is_ok());
        assert!(validate_oauth_redirect_uri("https://example.com/callback").is_err());
        assert!(validate_oauth_redirect_uri("file:///tmp/callback").is_err());
        assert!(validate_oauth_redirect_uri("http://127.0.0.1/callback#fragment").is_err());
    }

    #[test]
    fn request_host_loopback_detection_is_exact() {
        assert!(request_host_is_loopback("localhost:3000"));
        assert!(request_host_is_loopback("127.0.0.1:3000"));
        assert!(request_host_is_loopback("[::1]:3000"));
        assert!(!request_host_is_loopback("localhost.example.com"));
        assert!(!request_host_is_loopback("translator.example.com"));
    }

    #[tokio::test]
    async fn oauth_registration_rejects_non_loopback_redirects() {
        let (state, data_dir) = test_state(None);
        let request = HttpRequest::builder()
            .method("POST")
            .uri("/oauth/register")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                serde_json::json!({
                    "redirect_uris": ["https://example.com/callback"],
                    "client_name": "Remote Client"
                })
                .to_string(),
            ))
            .expect("request");

        let response = create_router(state)
            .oneshot(request)
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[tokio::test]
    async fn oauth_registration_persists_loopback_client() {
        let (state, data_dir) = test_state(None);
        let request = HttpRequest::builder()
            .method("POST")
            .uri("/oauth/register")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                serde_json::json!({
                    "redirect_uris": ["http://127.0.0.1:8787/callback"],
                    "client_name": "Local MCP Client"
                })
                .to_string(),
            ))
            .expect("request");

        let response = create_router(state.clone())
            .oneshot(request)
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::CREATED);
        let clients = state.store.oauth_list_clients().expect("clients");
        assert_eq!(clients.len(), 1);
        assert_eq!(clients[0].client_name.as_deref(), Some("Local MCP Client"));
        assert_eq!(
            clients[0].redirect_uris,
            vec!["http://127.0.0.1:8787/callback".to_string()]
        );

        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[tokio::test]
    async fn send_result_reconciles_pending_message_and_releases_waiter() {
        let (state, data_dir) = test_state(None);
        state
            .store
            .upsert_contact(
                "chat@example.test",
                Some("Test Chat"),
                None,
                Some("private"),
                1_700_000_000_000,
            )
            .expect("insert contact");
        state
            .store
            .add_message(&test_outgoing_message("pending_1", 1_700_000_000_000))
            .expect("insert pending");

        let rx = state
            .register_pending_send(42, Some("pending_1".to_string()))
            .await;
        state
            .handle_send_result(BridgeSendResult {
                request_id: 42,
                success: true,
                message_id: Some("real_1".to_string()),
                timestamp: Some(1_700_000_005),
                error: None,
            })
            .await;

        let result = rx.await.expect("send result");
        assert!(result.success);
        assert_eq!(result.message_id.as_deref(), Some("real_1"));
        assert_eq!(result.timestamp, Some(1_700_000_005_000));

        let messages = state
            .store
            .get_messages_paginated("chat@example.test", None, None, None, true)
            .expect("messages");
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].id, "real_1");
        assert_eq!(messages[0].timestamp, 1_700_000_005_000);

        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[tokio::test]
    async fn failed_send_result_deletes_pending_message_and_releases_waiter() {
        let (state, data_dir) = test_state(None);
        state
            .store
            .upsert_contact(
                "chat@example.test",
                Some("Test Chat"),
                None,
                Some("private"),
                1_700_000_000_000,
            )
            .expect("insert contact");
        state
            .store
            .add_message(&test_outgoing_message("pending_1", 1_700_000_000_000))
            .expect("insert pending");

        let rx = state
            .register_pending_send(43, Some("pending_1".to_string()))
            .await;
        state
            .handle_send_result(BridgeSendResult {
                request_id: 43,
                success: false,
                message_id: None,
                timestamp: None,
                error: Some("rate limited".to_string()),
            })
            .await;

        let result = rx.await.expect("send result");
        assert!(!result.success);
        assert_eq!(result.error.as_deref(), Some("rate limited"));

        let messages = state
            .store
            .get_messages_paginated("chat@example.test", None, None, None, true)
            .expect("messages");
        assert!(messages.is_empty());

        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[test]
    fn image_payload_validation_accepts_valid_png() {
        let png_base64 = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+/p9sAAAAASUVORK5CYII=";
        let payload =
            validate_image_payload(png_base64, "image/png; charset=binary").expect("valid png");

        assert_eq!(payload.mime_type, "image/png");
        assert!(payload.decoded_size > 0);
    }

    #[test]
    fn image_payload_validation_rejects_invalid_base64() {
        let error = validate_image_payload("not valid base64", "image/png").unwrap_err();
        assert_eq!(error, "media_data must be valid base64");
    }

    #[test]
    fn image_payload_validation_rejects_mime_mismatch() {
        let png_base64 = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+/p9sAAAAASUVORK5CYII=";
        let error = validate_image_payload(png_base64, "image/jpeg").unwrap_err();

        assert_eq!(error, "media_data does not match MIME type image/jpeg");
    }

    #[test]
    fn image_payload_validation_rejects_data_url_prefix() {
        let error =
            validate_image_payload("data:image/png;base64,iVBORw0KGgo=", "image/png").unwrap_err();

        assert_eq!(
            error,
            "media_data must be raw base64 without a data URL prefix"
        );
    }

    #[test]
    fn image_payload_validation_rejects_oversized_encoded_payload() {
        let oversized = "A".repeat(MAX_IMAGE_BASE64_BYTES + 1);
        let error = validate_image_payload(&oversized, "image/png").unwrap_err();

        assert_eq!(error, "Image is too large. Maximum decoded size is 16MB.");
    }
}
