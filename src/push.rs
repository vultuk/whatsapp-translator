use anyhow::{anyhow, bail, Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use serde::Serialize;
use serde_json::{json, Value};

use crate::storage::{PushDevice, StoredMessage};

const SANDBOX_APNS_HOST: &str = "https://api.sandbox.push.apple.com";
const PRODUCTION_APNS_HOST: &str = "https://api.push.apple.com";
const MESSAGE_CATEGORY: &str = "MESSAGE_CATEGORY";

pub struct ApnsClient {
    client: reqwest::Client,
    key_id: String,
    team_id: String,
    bundle_id: String,
    encoding_key: EncodingKey,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApnsSendOutcome {
    Delivered,
    RemoveDevice,
}

#[derive(Debug, Clone)]
pub struct PushNotification {
    payload: Value,
}

#[derive(Serialize)]
struct ProviderTokenClaims<'a> {
    iss: &'a str,
    iat: i64,
}

impl ApnsClient {
    pub fn from_env() -> Result<Option<Self>> {
        let key_id = std::env::var("APNS_KEY_ID").ok();
        let team_id = std::env::var("APNS_TEAM_ID").ok();
        let bundle_id = std::env::var("APNS_BUNDLE_ID").ok();
        let private_key_base64 = std::env::var("APNS_PRIVATE_KEY_BASE64").ok();

        if key_id.is_none()
            && team_id.is_none()
            && bundle_id.is_none()
            && private_key_base64.is_none()
        {
            return Ok(None);
        }

        let key_id = key_id.ok_or_else(|| anyhow!("APNS_KEY_ID is required"))?;
        let team_id = team_id.ok_or_else(|| anyhow!("APNS_TEAM_ID is required"))?;
        let bundle_id = bundle_id.ok_or_else(|| anyhow!("APNS_BUNDLE_ID is required"))?;
        let private_key = BASE64_STANDARD
            .decode(
                private_key_base64.ok_or_else(|| anyhow!("APNS_PRIVATE_KEY_BASE64 is required"))?,
            )
            .context("APNS_PRIVATE_KEY_BASE64 is not valid base64")?;
        let encoding_key = EncodingKey::from_ec_pem(&private_key)
            .context("APNS private key is not a valid EC PKCS#8 key")?;

        Ok(Some(Self {
            client: reqwest::Client::builder()
                .http2_prior_knowledge()
                .http2_adaptive_window(true)
                .build()
                .context("Failed to create APNs HTTP client")?,
            key_id,
            team_id,
            bundle_id,
            encoding_key,
        }))
    }

    pub async fn send(
        &self,
        device: &PushDevice,
        notification: &PushNotification,
    ) -> Result<ApnsSendOutcome> {
        let host = match device.environment.as_str() {
            "sandbox" => SANDBOX_APNS_HOST,
            "production" => PRODUCTION_APNS_HOST,
            other => bail!("Unsupported APNs environment: {other}"),
        };
        let mut header = Header::new(Algorithm::ES256);
        header.kid = Some(self.key_id.clone());
        let provider_token = encode(
            &header,
            &ProviderTokenClaims {
                iss: &self.team_id,
                iat: chrono::Utc::now().timestamp(),
            },
            &self.encoding_key,
        )
        .context("Failed to sign APNs provider token")?;

        let response = self
            .client
            .post(format!("{host}/3/device/{}", device.token))
            .bearer_auth(provider_token)
            .header("apns-topic", &self.bundle_id)
            .header("apns-push-type", "alert")
            .header("apns-priority", "10")
            .header("apns-expiration", "0")
            .header("apns-id", uuid::Uuid::new_v4().to_string())
            .json(&notification.payload)
            .send()
            .await
            .map_err(|error| {
                let is_connect = error.is_connect();
                let is_timeout = error.is_timeout();
                let is_request = error.is_request();
                anyhow!(
                    "APNs request failed (connect={is_connect}, timeout={is_timeout}, request={is_request}): {}",
                    error.without_url()
                )
            })?;

        if response.status().is_success() {
            return Ok(ApnsSendOutcome::Delivered);
        }

        let status = response.status();
        let reason = response
            .json::<Value>()
            .await
            .ok()
            .and_then(|body| {
                body.get("reason")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
            .unwrap_or_else(|| "Unknown APNs error".to_string());
        if status == reqwest::StatusCode::GONE
            || reason == "BadDeviceToken"
            || reason == "DeviceTokenNotForTopic"
            || reason == "Unregistered"
        {
            return Ok(ApnsSendOutcome::RemoveDevice);
        }

        bail!("APNs returned {status}: {reason}")
    }
}

impl PushNotification {
    pub fn test() -> Self {
        Self {
            payload: json!({
                "aps": {
                    "alert": {
                        "title": "WhatsApp Translator",
                        "body": "Notifications are ready on this iPhone."
                    },
                    "sound": "default"
                },
                "notificationTest": true
            }),
        }
    }

    pub fn from_message(message: &StoredMessage, badge: i32) -> Self {
        Self::from_message_with_avatar(message, badge, None)
    }

    pub fn from_message_with_avatar(
        message: &StoredMessage,
        badge: i32,
        avatar_url: Option<&str>,
    ) -> Self {
        Self::from_message_with_context(message, badge, avatar_url, None)
    }

    pub fn from_message_with_context(
        message: &StoredMessage,
        badge: i32,
        avatar_url: Option<&str>,
        reaction_target: Option<&StoredMessage>,
    ) -> Self {
        let is_group = message.chat_type == "group";
        let conversation_name = message
            .contact_name
            .as_deref()
            .or(message.contact_phone.as_deref())
            .unwrap_or("WhatsApp chat");
        let sender_name = message
            .sender_name
            .as_deref()
            .or(message.sender_phone.as_deref())
            .or(message.contact_name.as_deref())
            .or(message.contact_phone.as_deref())
            .unwrap_or("New WhatsApp message");
        let title = truncate(
            if is_group {
                sender_name
            } else {
                conversation_name
            },
            100,
        );
        let subtitle = if is_group {
            Some(truncate(conversation_name, 100))
        } else {
            None
        };
        let body = truncate(&notification_body(message, reaction_target), 500);

        let mut alert = json!({
            "title": title,
            "body": body.clone(),
        });
        if let Some(subtitle) = subtitle {
            alert["subtitle"] = Value::String(subtitle);
        }

        let mut payload = json!({
            "aps": {
                "alert": alert,
                "sound": "default",
                "badge": badge,
                "thread-id": message.contact_id,
                "content-available": 1,
                "mutable-content": 1,
                "category": MESSAGE_CATEGORY,
                "summary-arg": conversation_name,
            },
            "contactId": message.contact_id,
            "messageId": message.id,
            "senderId": message.sender_phone,
            "senderName": sender_name,
            "conversationName": conversation_name,
            "chatType": message.chat_type,
            "messageBody": body,
        });
        if let Some(avatar_url) = avatar_url.filter(|value| !value.trim().is_empty()) {
            payload["avatarUrl"] = Value::String(avatar_url.to_string());
        }
        Self { payload }
    }
}

fn notification_body(message: &StoredMessage, reaction_target: Option<&StoredMessage>) -> String {
    if let Some(text) = message
        .translated_text
        .as_deref()
        .filter(|text| !text.trim().is_empty())
    {
        return text.to_string();
    }
    if let Some(text) = message
        .original_text
        .as_deref()
        .filter(|text| !text.trim().is_empty())
    {
        return text.to_string();
    }

    let content = message
        .content
        .clone()
        .or_else(|| serde_json::from_str(&message.content_json).ok());
    if let Some(text) = content.as_ref().and_then(|content| {
        content
            .get("body")
            .or_else(|| content.get("caption"))
            .and_then(Value::as_str)
    }) {
        if !text.trim().is_empty() {
            return text.to_string();
        }
    }

    match message.content_type.to_ascii_lowercase().as_str() {
        "reaction" => {
            let emoji = content
                .as_ref()
                .and_then(|content| content.get("emoji"))
                .and_then(Value::as_str)
                .filter(|emoji| !emoji.trim().is_empty());
            let target = reaction_target
                .map(|target| truncate(&notification_body(target, None), 100))
                .filter(|text| text != "New message");
            match (emoji, target) {
                (Some(emoji), Some(target)) => {
                    format!("Reacted {emoji} to \u{201c}{target}\u{201d}")
                }
                (None, Some(target)) => format!("Reacted to \u{201c}{target}\u{201d}"),
                (Some(emoji), None) => format!("Reacted {emoji} to a message"),
                (None, None) => "Reacted to a message".to_string(),
            }
        }
        "image" => "Photo".to_string(),
        "video" => "Video".to_string(),
        "audio" | "voice note" => "Voice message".to_string(),
        "document" => "Document".to_string(),
        "sticker" => "Sticker".to_string(),
        "location" => "Location".to_string(),
        "contact" => "Contact".to_string(),
        "poll" => "Poll".to_string(),
        "revoked" => "This message was deleted".to_string(),
        _ => "New message".to_string(),
    }
}

fn truncate(value: &str, maximum_characters: usize) -> String {
    let mut characters = value.chars();
    let truncated: String = characters.by_ref().take(maximum_characters).collect();
    if characters.next().is_some() {
        format!("{truncated}…")
    } else {
        truncated
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translated_message_payload_contains_chat_navigation_and_badge() {
        let content = json!({"type": "text", "body": "Szia"});
        let message = StoredMessage {
            id: "message-1".to_string(),
            contact_id: "family@g.us".to_string(),
            timestamp: 1_700_000_000_000,
            is_from_me: false,
            is_forwarded: false,
            sender_name: Some("Virág".to_string()),
            sender_phone: None,
            contact_name: Some("The Skinners".to_string()),
            contact_phone: None,
            chat_type: "group".to_string(),
            content_type: "Text".to_string(),
            content_json: content.to_string(),
            content: Some(content),
            original_text: Some("Szia".to_string()),
            translated_text: Some("Hello".to_string()),
            source_language: Some("Hungarian".to_string()),
            is_translated: true,
        };

        let notification = PushNotification::from_message_with_avatar(
            &message,
            7,
            Some("https://cdn.example.com/avatar.jpg"),
        );

        assert_eq!(notification.payload["aps"]["alert"]["title"], "Virág");
        assert_eq!(
            notification.payload["aps"]["alert"]["subtitle"],
            "The Skinners"
        );
        assert_eq!(notification.payload["aps"]["alert"]["body"], "Hello");
        assert_eq!(notification.payload["aps"]["badge"], 7);
        assert_eq!(notification.payload["aps"]["content-available"], 1);
        assert_eq!(notification.payload["aps"]["mutable-content"], 1);
        assert_eq!(notification.payload["aps"]["category"], "MESSAGE_CATEGORY");
        assert_eq!(notification.payload["aps"]["thread-id"], "family@g.us");
        assert_eq!(notification.payload["contactId"], "family@g.us");
        assert_eq!(notification.payload["messageId"], "message-1");
        assert_eq!(notification.payload["senderName"], "Virág");
        assert_eq!(notification.payload["conversationName"], "The Skinners");
        assert_eq!(notification.payload["chatType"], "group");
        assert_eq!(notification.payload["messageBody"], "Hello");
        assert_eq!(
            notification.payload["avatarUrl"],
            "https://cdn.example.com/avatar.jpg"
        );
    }

    #[test]
    fn reaction_payload_describes_the_reaction() {
        let content = json!({
            "type": "reaction",
            "emoji": "😂",
            "target_message_id": "message-1"
        });
        let message = StoredMessage {
            id: "reaction-1".to_string(),
            contact_id: "family@g.us".to_string(),
            timestamp: 1_700_000_000_000,
            is_from_me: false,
            is_forwarded: false,
            sender_name: Some("Eileen".to_string()),
            sender_phone: Some("447700900123".to_string()),
            contact_name: Some("The Skinners".to_string()),
            contact_phone: None,
            chat_type: "group".to_string(),
            content_type: "Reaction".to_string(),
            content_json: content.to_string(),
            content: Some(content),
            original_text: None,
            translated_text: None,
            source_language: None,
            is_translated: false,
        };

        let target_content = json!({"type": "text", "body": "Mother... slow down!"});
        let target = StoredMessage {
            id: "message-1".to_string(),
            content_type: "Text".to_string(),
            content_json: target_content.to_string(),
            content: Some(target_content),
            ..message.clone()
        };
        let notification =
            PushNotification::from_message_with_context(&message, 1, None, Some(&target));

        assert_eq!(
            notification.payload["aps"]["alert"]["body"],
            "Reacted 😂 to “Mother... slow down!”"
        );
    }

    #[test]
    fn test_payload_is_visible_and_does_not_open_a_chat() {
        let notification = PushNotification::test();

        assert_eq!(
            notification.payload["aps"]["alert"]["title"],
            "WhatsApp Translator"
        );
        assert_eq!(notification.payload["aps"]["sound"], "default");
        assert_eq!(notification.payload["notificationTest"], true);
        assert!(notification.payload.get("contactId").is_none());
    }
}
