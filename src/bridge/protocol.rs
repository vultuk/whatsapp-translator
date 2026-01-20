//! JSON protocol types for communication between Rust CLI and Go bridge.
//!
//! The Go bridge sends JSON-line messages to stdout, and receives commands via stdin.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Events sent from Go bridge to Rust CLI (via stdout)
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BridgeEvent {
    /// QR code data for pairing
    Qr { data: String },

    /// Successfully connected to WhatsApp
    Connected {
        phone: String,
        name: String,
        platform: Option<String>,
    },

    /// Connection state changed
    ConnectionState { state: ConnectionState },

    /// Received a message
    Message(Message),

    /// Result of a send message request
    SendResult {
        request_id: i32,
        success: bool,
        message_id: Option<String>,
        timestamp: Option<i64>,
        error: Option<String>,
    },

    /// Profile picture response
    ProfilePicture {
        request_id: i32,
        jid: String,
        url: Option<String>,
        id: Option<String>,
        error: Option<String>,
    },

    /// Chat presence (typing/recording indicator)
    ChatPresence {
        chat_id: String,
        user_id: String,
        state: ChatPresenceState,
    },

    /// Error occurred
    Error { code: String, message: String },

    /// Informational log message
    Log { level: String, message: String },

    /// Session logged out (need to re-scan QR)
    LoggedOut { reason: String },
}

/// Connection states
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionState {
    Connecting,
    Connected,
    Disconnected,
    Reconnecting,
    LoggedOut,
}

/// Chat presence states (typing indicators)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChatPresenceState {
    /// User is typing a message
    Typing,
    /// User stopped typing
    Paused,
    /// User is recording audio
    Recording,
}

/// A WhatsApp message with full metadata
#[derive(Debug, Clone, Deserialize)]
pub struct Message {
    /// Unique message ID
    pub id: String,

    /// Timestamp when the message was sent
    #[serde(with = "chrono::serde::ts_seconds")]
    pub timestamp: DateTime<Utc>,

    /// Information about the sender
    pub from: Contact,

    /// Chat information (private or group)
    pub chat: Chat,

    /// Message content
    pub content: MessageContent,

    /// Whether this message was sent by us
    pub is_from_me: bool,

    /// Whether this is a forwarded message
    pub is_forwarded: bool,

    /// Push name (display name set by sender)
    pub push_name: Option<String>,
}

/// Contact information
#[derive(Debug, Clone, Deserialize)]
pub struct Contact {
    /// JID (WhatsApp ID) - e.g., "1234567890@s.whatsapp.net"
    pub jid: String,

    /// Phone number (extracted from JID)
    pub phone: String,

    /// Display name if known
    pub name: Option<String>,
}

/// Chat information
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Chat {
    /// Private one-on-one chat
    Private { jid: String, name: Option<String> },

    /// Group chat
    Group {
        jid: String,
        name: Option<String>,
        /// Number of participants
        participant_count: Option<u32>,
    },

    /// Broadcast list
    Broadcast { jid: String },

    /// Status/Stories
    Status { jid: String },
}

/// Message content types
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MessageContent {
    /// Plain text message
    Text { body: String },

    /// Image message
    Image {
        caption: Option<String>,
        mime_type: String,
        file_size: u64,
        /// SHA256 hash of the file
        file_hash: Option<String>,
        /// Base64 encoded image data
        media_data: Option<String>,
    },

    /// Video message
    Video {
        caption: Option<String>,
        mime_type: String,
        file_size: u64,
        duration_seconds: Option<u32>,
        /// Base64 encoded video data
        media_data: Option<String>,
    },

    /// Audio message (including voice notes)
    Audio {
        mime_type: String,
        file_size: u64,
        duration_seconds: Option<u32>,
        is_voice_note: bool,
        /// Base64 encoded audio data
        media_data: Option<String>,
    },

    /// Document/file message
    Document {
        caption: Option<String>,
        mime_type: String,
        file_name: Option<String>,
        file_size: u64,
        /// Base64 encoded document data
        media_data: Option<String>,
    },

    /// Sticker message
    Sticker {
        mime_type: String,
        is_animated: bool,
        /// Base64 encoded sticker data
        media_data: Option<String>,
    },

    /// Location message
    Location {
        latitude: f64,
        longitude: f64,
        name: Option<String>,
        address: Option<String>,
    },

    /// Contact card
    Contact { display_name: String, vcard: String },

    /// Reaction to another message
    Reaction {
        emoji: String,
        target_message_id: String,
    },

    /// Message was deleted/revoked
    Revoked,

    /// Poll message
    Poll {
        question: String,
        options: Vec<String>,
    },

    /// Unknown or unsupported message type
    Unknown { raw_type: String },
}

/// Commands sent from Rust CLI to Go bridge (via stdin)
/// These will be used in future phases for sending messages, etc.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BridgeCommand {
    /// Send a text message
    Send {
        #[serde(skip_serializing_if = "Option::is_none")]
        request_id: Option<i32>,
        to: String,
        text: String,
        /// Message ID to reply to (optional)
        #[serde(skip_serializing_if = "Option::is_none")]
        reply_to: Option<String>,
        /// Sender JID of the replied message (optional)
        #[serde(skip_serializing_if = "Option::is_none")]
        reply_to_sender: Option<String>,
    },

    /// Send an image message
    SendImage {
        #[serde(skip_serializing_if = "Option::is_none")]
        request_id: Option<i32>,
        to: String,
        /// Base64 encoded image data
        media_data: String,
        mime_type: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        caption: Option<String>,
        /// Message ID to reply to (optional)
        #[serde(skip_serializing_if = "Option::is_none")]
        reply_to: Option<String>,
        /// Sender JID of the replied message (optional)
        #[serde(skip_serializing_if = "Option::is_none")]
        reply_to_sender: Option<String>,
    },

    /// Send a reaction to a message
    SendReaction {
        #[serde(skip_serializing_if = "Option::is_none")]
        request_id: Option<i32>,
        /// Chat JID
        to: String,
        /// Target message ID to react to
        message_id: String,
        /// Sender JID of the target message (for groups)
        #[serde(skip_serializing_if = "Option::is_none")]
        sender_jid: Option<String>,
        /// Reaction emoji (empty string to remove reaction)
        emoji: String,
    },

    /// Get profile picture for a JID
    GetProfilePicture { request_id: i32, to: String },

    /// Disconnect and exit
    Disconnect,

    /// Request logout (clears session)
    Logout,
}

impl Chat {
    /// Get the JID of the chat
    pub fn jid(&self) -> &str {
        match self {
            Chat::Private { jid, .. } => jid,
            Chat::Group { jid, .. } => jid,
            Chat::Broadcast { jid } => jid,
            Chat::Status { jid } => jid,
        }
    }

    /// Check if this is a group chat
    pub fn is_group(&self) -> bool {
        matches!(self, Chat::Group { .. })
    }

    /// Get the display name of the chat
    pub fn display_name(&self) -> String {
        match self {
            Chat::Private { name, jid } => name.clone().unwrap_or_else(|| extract_phone(jid)),
            Chat::Group { name, jid, .. } => name.clone().unwrap_or_else(|| extract_phone(jid)),
            Chat::Broadcast { jid } => format!("Broadcast: {}", extract_phone(jid)),
            Chat::Status { .. } => "Status".to_string(),
        }
    }
}

impl Contact {
    /// Get display name, falling back to phone number
    pub fn display_name(&self) -> &str {
        self.name.as_deref().unwrap_or(&self.phone)
    }
}

impl MessageContent {
    /// Get a short description of the content type
    pub fn type_name(&self) -> &'static str {
        match self {
            MessageContent::Text { .. } => "Text",
            MessageContent::Image { .. } => "Image",
            MessageContent::Video { .. } => "Video",
            MessageContent::Audio {
                is_voice_note: true,
                ..
            } => "Voice Note",
            MessageContent::Audio { .. } => "Audio",
            MessageContent::Document { .. } => "Document",
            MessageContent::Sticker { .. } => "Sticker",
            MessageContent::Location { .. } => "Location",
            MessageContent::Contact { .. } => "Contact",
            MessageContent::Reaction { .. } => "Reaction",
            MessageContent::Revoked => "Deleted Message",
            MessageContent::Poll { .. } => "Poll",
            MessageContent::Unknown { .. } => "Unknown",
        }
    }
}

/// Extract phone number from JID
fn extract_phone(jid: &str) -> String {
    jid.split('@').next().unwrap_or(jid).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_qr_event() {
        let json = r#"{"type": "qr", "data": "2@ABC123"}"#;
        let event: BridgeEvent = serde_json::from_str(json).unwrap();
        assert!(matches!(event, BridgeEvent::Qr { data } if data == "2@ABC123"));
    }

    #[test]
    fn test_parse_text_message() {
        let json = r#"{
            "type": "message",
            "id": "msg123",
            "timestamp": 1705689600,
            "from": {"jid": "1234567890@s.whatsapp.net", "phone": "1234567890", "name": "John"},
            "chat": {"type": "private", "jid": "1234567890@s.whatsapp.net"},
            "content": {"type": "text", "body": "Hello!"},
            "is_from_me": false,
            "is_forwarded": false,
            "push_name": "John"
        }"#;
        let event: BridgeEvent = serde_json::from_str(json).unwrap();
        assert!(matches!(event, BridgeEvent::Message(_)));
    }
}
