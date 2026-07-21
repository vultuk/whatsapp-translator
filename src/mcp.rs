//! MCP (Model Context Protocol) server implementation for WhatsApp.
//!
//! The MCP surface is deliberately split into read-only discovery/context tools and
//! explicitly mutating tools. Text sends use a two-step prepare/send flow so the
//! exact recipient and translated text are fixed before WhatsApp is touched.

use crate::bridge::BridgeCommand;
use crate::storage::{StoredContact, StoredMessage};
use crate::web::{AppState, SendConfirmationError};
use rmcp::{
    model::{
        CallToolRequestParam, CallToolResult, Implementation, ListToolsResult,
        PaginatedRequestParam, ServerCapabilities, ServerInfo, Tool, ToolAnnotations,
    },
    service::RequestContext,
    ErrorData as McpError, RoleServer, ServerHandler,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use tracing::{info, warn};

const DEFAULT_LIMIT: usize = 50;
const MAX_LIMIT: usize = 200;
const PREPARATION_TTL_MILLIS: i64 = 10 * 60 * 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct McpPermissions {
    pub read: bool,
    pub send: bool,
}

impl McpPermissions {
    pub fn from_scope(scope: &str) -> Self {
        let scopes: Vec<&str> = scope.split_whitespace().collect();
        let legacy_full_access = scopes.contains(&"mcp");
        Self {
            read: legacy_full_access || scopes.contains(&"whatsapp.read"),
            send: legacy_full_access || scopes.contains(&"whatsapp.send"),
        }
    }
}

#[derive(Clone)]
pub struct WhatsAppMcpServer {
    state: Arc<AppState>,
    permissions: McpPermissions,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ContactInfo {
    id: String,
    name: Option<String>,
    phone: Option<String>,
    #[serde(rename = "type")]
    contact_type: Option<String>,
    unread_count: i32,
    is_pinned: bool,
    last_message_time: i64,
    last_message_time_iso: Option<String>,
    last_message_preview: Option<String>,
}

impl From<StoredContact> for ContactInfo {
    fn from(contact: StoredContact) -> Self {
        Self {
            id: contact.id,
            name: contact.name,
            phone: contact.phone,
            contact_type: contact.contact_type,
            unread_count: contact.unread_count,
            is_pinned: contact.pinned_at.is_some(),
            last_message_time: contact.last_message_time,
            last_message_time_iso: timestamp_millis_to_iso(contact.last_message_time),
            last_message_preview: contact.last_message_preview,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct MessageInfo {
    id: String,
    contact_id: String,
    timestamp: i64,
    timestamp_iso: Option<String>,
    is_from_me: bool,
    sender_name: Option<String>,
    sender_phone: Option<String>,
    text: Option<String>,
    original_text: Option<String>,
    translated_text: Option<String>,
    content_type: String,
    delivery_status: Option<String>,
    reply_context: Option<Value>,
}

impl From<StoredMessage> for MessageInfo {
    fn from(message: StoredMessage) -> Self {
        let original_text = message.original_text.clone().or_else(|| {
            message.content.as_ref().and_then(|content| {
                content
                    .get("body")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .or_else(|| {
                        content
                            .get("caption")
                            .and_then(Value::as_str)
                            .map(str::to_string)
                    })
            })
        });
        let text = if message.is_translated && !message.is_from_me {
            message.translated_text.clone()
        } else {
            original_text.clone()
        };
        let reply_context = message
            .content
            .as_ref()
            .and_then(|content| content.get("reply_context"))
            .filter(|value| !value.is_null())
            .cloned();

        Self {
            id: message.id,
            contact_id: message.contact_id,
            timestamp: message.timestamp,
            timestamp_iso: timestamp_millis_to_iso(message.timestamp),
            is_from_me: message.is_from_me,
            sender_name: message.sender_name,
            sender_phone: message.sender_phone,
            text,
            original_text,
            translated_text: message.translated_text,
            content_type: message.content_type,
            delivery_status: message.delivery_status,
            reply_context,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TranslationMode {
    Auto,
    Never,
    Required,
}

impl TranslationMode {
    fn parse(value: Option<&str>) -> Result<Self, McpError> {
        match value.unwrap_or("auto") {
            "auto" => Ok(Self::Auto),
            "never" => Ok(Self::Never),
            "required" => Ok(Self::Required),
            _ => Err(McpError::invalid_params(
                "translation_mode must be auto, never, or required",
                None,
            )),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Never => "never",
            Self::Required => "required",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedMcpMessage {
    pub token: String,
    pub contact_id: String,
    pub contact_name: Option<String>,
    pub original_text: String,
    pub final_text: String,
    pub translated: bool,
    pub target_language: Option<String>,
    pub translation_mode: String,
    pub reply_to_message_id: Option<String>,
    pub reply_to_sender: Option<String>,
    pub reply_to_text: Option<String>,
    pub created_at: i64,
    pub expires_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpIdempotencyRecord {
    pub operation_key: String,
    pub status: String,
    pub result: Option<Value>,
    pub expires_at: i64,
}

fn timestamp_millis_to_iso(timestamp: i64) -> Option<String> {
    chrono::DateTime::from_timestamp_millis(timestamp).map(|value| value.to_rfc3339())
}

fn argument_limit(args: &Value) -> usize {
    args.get("limit")
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .unwrap_or(DEFAULT_LIMIT)
        .clamp(1, MAX_LIMIT)
}

fn resolve_prepared_translation(
    original_text: &str,
    target_language: Option<&str>,
    mode: TranslationMode,
    translation_result: Result<String, String>,
) -> Result<(String, bool, Option<String>), String> {
    if mode == TranslationMode::Never {
        return Ok((original_text.to_string(), false, None));
    }

    let Some(target_language) = target_language.filter(|value| !value.trim().is_empty()) else {
        return if mode == TranslationMode::Required {
            Err("Translation was required but no target language could be determined. No message was sent or prepared.".to_string())
        } else {
            Ok((original_text.to_string(), false, None))
        };
    };

    match translation_result {
        Ok(translated_text) if !translated_text.trim().is_empty() => Ok((
            translated_text.clone(),
            translated_text != original_text,
            Some(target_language.to_string()),
        )),
        Ok(_) => Err(format!(
            "Translation to {target_language} returned no text. No message was sent or prepared."
        )),
        Err(error) => Err(format!(
            "Translation to {target_language} failed: {error}. No message was sent or prepared."
        )),
    }
}

impl WhatsAppMcpServer {
    pub fn new(state: Arc<AppState>, permissions: McpPermissions) -> Self {
        Self { state, permissions }
    }

    fn tool(
        name: &'static str,
        title: &'static str,
        description: &'static str,
        input_schema: Value,
        output_schema: Value,
        annotations: ToolAnnotations,
    ) -> Tool {
        let mut tool = Tool::new(
            name,
            description,
            input_schema.as_object().expect("tool input schema").clone(),
        );
        tool.title = Some(title.to_string());
        tool.output_schema = Some(Arc::new(
            output_schema
                .as_object()
                .expect("tool output schema")
                .clone(),
        ));
        tool.annotate(annotations)
    }

    fn read_annotations(title: &str) -> ToolAnnotations {
        ToolAnnotations::with_title(title)
            .read_only(true)
            .destructive(false)
            .idempotent(true)
            .open_world(true)
    }

    fn write_annotations(title: &str, idempotent: bool) -> ToolAnnotations {
        ToolAnnotations::with_title(title)
            .read_only(false)
            .destructive(false)
            .idempotent(idempotent)
            .open_world(true)
    }

    fn object_output(properties: Value, required: &[&str]) -> Value {
        json!({
            "type": "object",
            "properties": properties,
            "required": required,
            "additionalProperties": true
        })
    }

    fn get_status_tool() -> Tool {
        Self::tool(
            "get_status",
            "Get WhatsApp status",
            "Check whether WhatsApp and translation are available before reading or sending.",
            json!({"type":"object","properties":{},"additionalProperties":false}),
            Self::object_output(
                json!({
                    "connected":{"type":"boolean"},
                    "accountName":{"type":["string","null"]},
                    "accountPhone":{"type":["string","null"]},
                    "translationConfigured":{"type":"boolean"},
                    "contactCount":{"type":"integer"},
                    "messageCount":{"type":"integer"}
                }),
                &[
                    "connected",
                    "translationConfigured",
                    "contactCount",
                    "messageCount",
                ],
            ),
            Self::read_annotations("Get WhatsApp status"),
        )
    }

    fn contacts_input_schema(require_query: bool) -> Value {
        let mut schema = json!({
            "type":"object",
            "properties":{
                "query":{"type":"string","description":"Case-insensitive name, phone, or JID search"},
                "contact_type":{"type":"string","enum":["all","private","group"],"default":"all"},
                "unread_only":{"type":"boolean","default":false},
                "cursor":{"type":"integer","minimum":0,"description":"Offset returned by the previous page"},
                "limit":{"type":"integer","minimum":1,"maximum":200,"default":50}
            },
            "additionalProperties":false
        });
        if require_query {
            schema["required"] = json!(["query"]);
        }
        schema
    }

    fn contacts_output_schema() -> Value {
        Self::object_output(
            json!({
                "contacts":{"type":"array","items":{"type":"object"}},
                "nextCursor":{"type":["integer","null"]},
                "totalMatched":{"type":"integer"}
            }),
            &["contacts", "totalMatched"],
        )
    }

    fn list_contacts_tool() -> Tool {
        Self::tool(
            "list_contacts",
            "List WhatsApp conversations",
            "List recent WhatsApp contacts and groups with last activity, preview, unread count, pin state, and pagination.",
            Self::contacts_input_schema(false),
            Self::contacts_output_schema(),
            Self::read_annotations("List WhatsApp conversations"),
        )
    }

    fn search_contacts_tool() -> Tool {
        Self::tool(
            "search_contacts",
            "Search WhatsApp contacts",
            "Resolve a person or group by case-insensitive name, phone number, or WhatsApp JID before reading or preparing a message.",
            Self::contacts_input_schema(true),
            Self::contacts_output_schema(),
            Self::read_annotations("Search WhatsApp contacts"),
        )
    }

    fn read_messages_tool() -> Tool {
        Self::tool(
            "read_messages",
            "Read WhatsApp messages",
            "Read a chronological page of messages with reply context, delivery state, translated text, and an older-page cursor.",
            json!({
                "type":"object",
                "properties":{
                    "contact_id":{"type":"string"},
                    "limit":{"type":"integer","minimum":1,"maximum":200,"default":50},
                    "before_timestamp":{"type":"integer","description":"Unix timestamp in milliseconds"},
                    "before_message_id":{"type":"string"},
                    "direction":{"type":"string","enum":["all","incoming","outgoing"],"default":"all"},
                    "content_type":{"type":"string","description":"Optional case-insensitive content type filter"}
                },
                "required":["contact_id"],
                "additionalProperties":false
            }),
            Self::object_output(
                json!({
                    "contact":{"type":"object"},
                    "messages":{"type":"array","items":{"type":"object"}},
                    "nextCursor":{"type":["object","null"]},
                    "hasMore":{"type":"boolean"}
                }),
                &["contact", "messages", "hasMore"],
            ),
            Self::read_annotations("Read WhatsApp messages"),
        )
    }

    fn search_messages_tool() -> Tool {
        Self::tool(
            "search_messages",
            "Search WhatsApp messages",
            "Search stored message text across all conversations or within one contact, optionally constrained by time and direction.",
            json!({
                "type":"object",
                "properties":{
                    "query":{"type":"string"},
                    "contact_id":{"type":"string"},
                    "after_timestamp":{"type":"integer"},
                    "before_timestamp":{"type":"integer"},
                    "direction":{"type":"string","enum":["all","incoming","outgoing"],"default":"all"},
                    "limit":{"type":"integer","minimum":1,"maximum":200,"default":50}
                },
                "required":["query"],
                "additionalProperties":false
            }),
            Self::object_output(
                json!({
                    "query":{"type":"string"},
                    "messages":{"type":"array","items":{"type":"object"}},
                    "truncated":{"type":"boolean"}
                }),
                &["query", "messages", "truncated"],
            ),
            Self::read_annotations("Search WhatsApp messages"),
        )
    }

    fn prepare_message_tool() -> Tool {
        Self::tool(
            "prepare_message",
            "Prepare a WhatsApp message",
            "Resolve the recipient and final translated text without sending. Returns a short-lived token required by send_message or reply_to_message. Translation failures never fall back to English.",
            json!({
                "type":"object",
                "properties":{
                    "contact_id":{"type":"string"},
                    "text":{"type":"string","minLength":1},
                    "translation_mode":{"type":"string","enum":["auto","never","required"],"default":"auto"},
                    "target_language":{"type":"string"},
                    "reply_to_message_id":{"type":"string"}
                },
                "required":["contact_id","text"],
                "additionalProperties":false
            }),
            Self::object_output(
                json!({
                    "preparationToken":{"type":"string"},
                    "expiresAt":{"type":"integer"},
                    "recipient":{"type":"object"},
                    "originalText":{"type":"string"},
                    "finalText":{"type":"string"},
                    "translated":{"type":"boolean"},
                    "targetLanguage":{"type":["string","null"]},
                    "replyToMessageId":{"type":["string","null"]}
                }),
                &["preparationToken", "expiresAt", "recipient", "originalText", "finalText", "translated"],
            ),
            Self::write_annotations("Prepare a WhatsApp message", false),
        )
    }

    fn send_message_tool() -> Tool {
        Self::prepared_send_tool(
            "send_message",
            "Send prepared WhatsApp message",
            "Send an exact message previously returned by prepare_message. Retries with the same idempotency key never send twice.",
        )
    }

    fn reply_to_message_tool() -> Tool {
        Self::prepared_send_tool(
            "reply_to_message",
            "Send prepared WhatsApp reply",
            "Send a prepared reply. The preparation must include reply_to_message_id, fixing both reply target and final text before sending.",
        )
    }

    fn prepared_send_tool(
        name: &'static str,
        title: &'static str,
        description: &'static str,
    ) -> Tool {
        Self::tool(
            name,
            title,
            description,
            json!({
                "type":"object",
                "properties":{
                    "preparation_token":{"type":"string"},
                    "idempotency_key":{"type":"string","minLength":8,"maxLength":200}
                },
                "required":["preparation_token","idempotency_key"],
                "additionalProperties":false
            }),
            Self::object_output(
                json!({
                    "success":{"type":"boolean"},
                    "messageId":{"type":"string"},
                    "timestamp":{"type":"integer"},
                    "recipient":{"type":"object"},
                    "sentText":{"type":"string"},
                    "translated":{"type":"boolean"},
                    "targetLanguage":{"type":["string","null"]},
                    "replyToMessageId":{"type":["string","null"]}
                }),
                &[
                    "success",
                    "messageId",
                    "timestamp",
                    "recipient",
                    "sentText",
                    "translated",
                ],
            ),
            Self::write_annotations(title, false),
        )
    }

    fn react_to_message_tool() -> Tool {
        Self::tool(
            "react_to_message",
            "React to WhatsApp message",
            "Add, replace, or remove a reaction on a known message. Use an empty emoji to remove your reaction.",
            json!({
                "type":"object",
                "properties":{
                    "contact_id":{"type":"string"},
                    "message_id":{"type":"string"},
                    "emoji":{"type":"string","maxLength":16},
                    "sender_jid":{"type":"string"},
                    "idempotency_key":{"type":"string","minLength":8,"maxLength":200}
                },
                "required":["contact_id","message_id","emoji","idempotency_key"],
                "additionalProperties":false
            }),
            Self::object_output(
                json!({"success":{"type":"boolean"},"messageId":{"type":"string"},"emoji":{"type":"string"}}),
                &["success", "messageId", "emoji"],
            ),
            Self::write_annotations("React to WhatsApp message", false),
        )
    }

    fn mark_conversation_read_tool() -> Tool {
        Self::tool(
            "mark_conversation_read",
            "Mark WhatsApp conversation read",
            "Clear the local unread count and optionally send a WhatsApp read receipt for a specific stored message.",
            json!({
                "type":"object",
                "properties":{
                    "contact_id":{"type":"string"},
                    "message_id":{"type":"string"}
                },
                "required":["contact_id"],
                "additionalProperties":false
            }),
            Self::object_output(
                json!({"success":{"type":"boolean"},"contactId":{"type":"string"},"receiptSent":{"type":"boolean"}}),
                &["success", "contactId", "receiptSent"],
            ),
            Self::write_annotations("Mark WhatsApp conversation read", true),
        )
    }

    fn require_read(&self) -> Result<(), McpError> {
        if self.permissions.read {
            Ok(())
        } else {
            Err(McpError::invalid_params(
                "This OAuth token does not include whatsapp.read",
                None,
            ))
        }
    }

    fn require_send(&self) -> Result<(), McpError> {
        if self.permissions.send {
            Ok(())
        } else {
            Err(McpError::invalid_params(
                "This OAuth token does not include whatsapp.send",
                None,
            ))
        }
    }

    async fn handle_get_status(&self) -> Result<CallToolResult, McpError> {
        self.require_read()?;
        let (message_count, contact_count) = self
            .state
            .store
            .get_stats()
            .map_err(|error| McpError::internal_error(error.to_string(), None))?;
        Ok(CallToolResult::structured(json!({
            "connected": *self.state.connected.read().await,
            "accountName": self.state.name.read().await.clone(),
            "accountPhone": self.state.phone.read().await.clone(),
            "translationConfigured": self.state.translator.is_some(),
            "contactCount": contact_count,
            "messageCount": message_count,
            "checkedAt": chrono::Utc::now().to_rfc3339(),
        })))
    }

    async fn handle_contacts(
        &self,
        args: Value,
        require_query: bool,
    ) -> Result<CallToolResult, McpError> {
        self.require_read()?;
        let query = args
            .get("query")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_lowercase();
        if require_query && query.is_empty() {
            return Err(McpError::invalid_params("query is required", None));
        }
        let contact_type = args
            .get("contact_type")
            .and_then(Value::as_str)
            .unwrap_or("all");
        let unread_only = args
            .get("unread_only")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let cursor = args.get("cursor").and_then(Value::as_u64).unwrap_or(0) as usize;
        let limit = argument_limit(&args);

        let contacts = self
            .state
            .store
            .get_contacts()
            .map_err(|error| McpError::internal_error(error.to_string(), None))?;
        let filtered: Vec<StoredContact> = contacts
            .into_iter()
            .filter(|contact| {
                (contact_type == "all" || contact.contact_type.as_deref() == Some(contact_type))
                    && (!unread_only || contact.unread_count > 0)
                    && (query.is_empty()
                        || contact.id.to_lowercase().contains(&query)
                        || contact
                            .name
                            .as_deref()
                            .is_some_and(|value| value.to_lowercase().contains(&query))
                        || contact
                            .phone
                            .as_deref()
                            .is_some_and(|value| value.to_lowercase().contains(&query)))
            })
            .collect();
        let total_matched = filtered.len();
        let contacts: Vec<ContactInfo> = filtered
            .into_iter()
            .skip(cursor)
            .take(limit)
            .map(ContactInfo::from)
            .collect();
        let next_cursor =
            (cursor + contacts.len() < total_matched).then_some(cursor + contacts.len());

        Ok(CallToolResult::structured(json!({
            "contacts": contacts,
            "nextCursor": next_cursor,
            "totalMatched": total_matched,
        })))
    }

    async fn handle_read_messages(&self, args: Value) -> Result<CallToolResult, McpError> {
        self.require_read()?;
        let contact_id = required_string(&args, "contact_id")?;
        let contact = self
            .state
            .store
            .get_contact(contact_id)
            .mcp()?
            .ok_or_else(|| McpError::invalid_params("Unknown contact_id", None))?;
        let limit = argument_limit(&args);
        let before_timestamp = args.get("before_timestamp").and_then(Value::as_i64);
        let before_message_id = args.get("before_message_id").and_then(Value::as_str);
        let direction = args
            .get("direction")
            .and_then(Value::as_str)
            .unwrap_or("all");
        let content_type = args
            .get("content_type")
            .and_then(Value::as_str)
            .map(str::to_lowercase);
        let fetch_limit = ((limit + 1) * 4).min(MAX_LIMIT * 4) as u32;
        let mut messages: Vec<StoredMessage> = self
            .state
            .store
            .get_messages_paginated(
                contact_id,
                Some(fetch_limit),
                before_timestamp,
                before_message_id,
                true,
            )
            .mcp()?
            .into_iter()
            .filter(|message| match direction {
                "incoming" => !message.is_from_me,
                "outgoing" => message.is_from_me,
                _ => true,
            })
            .filter(|message| {
                content_type
                    .as_ref()
                    .is_none_or(|value| message.content_type.to_lowercase() == *value)
            })
            .collect();
        let has_more = messages.len() > limit;
        if has_more {
            let remove_count = messages.len() - limit;
            messages.drain(0..remove_count);
        }
        let next_cursor = if has_more {
            messages.first().map(
                |message| json!({"beforeTimestamp":message.timestamp,"beforeMessageId":message.id}),
            )
        } else {
            None
        };
        let messages: Vec<MessageInfo> = messages.into_iter().map(MessageInfo::from).collect();

        Ok(CallToolResult::structured(json!({
            "contact": ContactInfo::from(contact),
            "messages": messages,
            "nextCursor": next_cursor,
            "hasMore": has_more,
        })))
    }

    async fn handle_search_messages(&self, args: Value) -> Result<CallToolResult, McpError> {
        self.require_read()?;
        let query = required_string(&args, "query")?.trim().to_lowercase();
        if query.is_empty() {
            return Err(McpError::invalid_params("query must not be empty", None));
        }
        let contact_filter = args.get("contact_id").and_then(Value::as_str);
        let after = args.get("after_timestamp").and_then(Value::as_i64);
        let before = args.get("before_timestamp").and_then(Value::as_i64);
        let direction = args
            .get("direction")
            .and_then(Value::as_str)
            .unwrap_or("all");
        let limit = argument_limit(&args);
        let contacts = if let Some(contact_id) = contact_filter {
            vec![self
                .state
                .store
                .get_contact(contact_id)
                .mcp()?
                .ok_or_else(|| McpError::invalid_params("Unknown contact_id", None))?]
        } else {
            self.state.store.get_contacts().mcp()?
        };

        let mut matches = Vec::new();
        for contact in contacts {
            for message in self.state.store.get_messages(&contact.id).mcp()? {
                if after.is_some_and(|value| message.timestamp < value)
                    || before.is_some_and(|value| message.timestamp >= value)
                    || (direction == "incoming" && message.is_from_me)
                    || (direction == "outgoing" && !message.is_from_me)
                {
                    continue;
                }
                let searchable = format!(
                    "{} {} {} {}",
                    message.original_text.as_deref().unwrap_or_default(),
                    message.translated_text.as_deref().unwrap_or_default(),
                    message
                        .content
                        .as_ref()
                        .and_then(|content| content.get("body"))
                        .and_then(Value::as_str)
                        .unwrap_or_default(),
                    message.sender_name.as_deref().unwrap_or_default()
                )
                .to_lowercase();
                if searchable.contains(&query) {
                    matches.push(message);
                }
            }
        }
        matches.sort_by(|left, right| {
            right
                .timestamp
                .cmp(&left.timestamp)
                .then_with(|| right.id.cmp(&left.id))
        });
        let truncated = matches.len() > limit;
        matches.truncate(limit);
        let messages: Vec<MessageInfo> = matches.into_iter().map(MessageInfo::from).collect();
        Ok(CallToolResult::structured(json!({
            "query": query,
            "messages": messages,
            "truncated": truncated,
        })))
    }

    async fn handle_prepare_message(&self, args: Value) -> Result<CallToolResult, McpError> {
        self.require_read()?;
        let contact_id = required_string(&args, "contact_id")?;
        let original_text = required_string(&args, "text")?.trim();
        if original_text.is_empty() {
            return Err(McpError::invalid_params("text must not be empty", None));
        }
        let mode = TranslationMode::parse(args.get("translation_mode").and_then(Value::as_str))?;
        let contact = self
            .state
            .store
            .get_contact(contact_id)
            .mcp()?
            .ok_or_else(|| McpError::invalid_params("Unknown contact_id", None))?;
        let settings = self
            .state
            .store
            .get_conversation_settings(contact_id)
            .unwrap_or_default();
        let target_language = if mode == TranslationMode::Never {
            None
        } else {
            args.get("target_language")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .or(settings.language_override)
                .or_else(|| {
                    self.state
                        .store
                        .get_conversation_language(contact_id, 10)
                        .ok()
                        .flatten()
                })
        };

        let translation_result = if let Some(target) = target_language.as_deref() {
            let translator = self.state.translator.as_ref().ok_or_else(|| {
                McpError::internal_error(
                    format!(
                        "Translation to {target} is needed, but translation is not configured. No message was sent or prepared."
                    ),
                    None,
                )
            })?;
            match translator.translate_to(original_text, target).await {
                Ok((translated, usage)) => {
                    if usage.input_tokens > 0 {
                        if let Err(error) = self.state.store.record_usage(
                            Some(contact_id),
                            None,
                            &usage,
                            "prepare_outgoing_mcp",
                        ) {
                            warn!("Failed to record MCP preparation usage: {error}");
                        }
                    }
                    Ok(translated)
                }
                Err(error) => Err(error.to_string()),
            }
        } else {
            Ok(original_text.to_string())
        };
        let (final_text, translated, target_language) = resolve_prepared_translation(
            original_text,
            target_language.as_deref(),
            mode,
            translation_result,
        )
        .map_err(|error| McpError::internal_error(error, None))?;

        let reply_to_message_id = args
            .get("reply_to_message_id")
            .and_then(Value::as_str)
            .map(str::to_string);
        let (reply_to_sender, reply_to_text) = if let Some(message_id) = &reply_to_message_id {
            let message = self
                .state
                .store
                .get_message_by_id(message_id)
                .mcp()?
                .ok_or_else(|| McpError::invalid_params("Unknown reply_to_message_id", None))?;
            if message.contact_id != contact_id {
                return Err(McpError::invalid_params(
                    "reply_to_message_id belongs to another conversation",
                    None,
                ));
            }
            let info = MessageInfo::from(message.clone());
            (message.sender_phone, info.text)
        } else {
            (None, None)
        };
        let now = chrono::Utc::now().timestamp_millis();
        let token = uuid::Uuid::new_v4().to_string();
        let prepared = PreparedMcpMessage {
            token: token.clone(),
            contact_id: contact_id.to_string(),
            contact_name: contact.name.clone(),
            original_text: original_text.to_string(),
            final_text: final_text.clone(),
            translated,
            target_language: target_language.clone(),
            translation_mode: mode.as_str().to_string(),
            reply_to_message_id: reply_to_message_id.clone(),
            reply_to_sender,
            reply_to_text,
            created_at: now,
            expires_at: now + PREPARATION_TTL_MILLIS,
        };
        let prepared_value = serde_json::to_value(&prepared)
            .map_err(|error| McpError::internal_error(error.to_string(), None))?;
        let mut preparations = self.state.mcp_prepared_messages.write().await;
        preparations.retain(|_, value| {
            value
                .get("expiresAt")
                .and_then(Value::as_i64)
                .is_some_and(|expires_at| expires_at > now)
        });
        preparations.insert(token.clone(), prepared_value);

        Ok(CallToolResult::structured(json!({
            "preparationToken": token,
            "expiresAt": prepared.expires_at,
            "recipient": {"id":contact.id,"name":contact.name,"phone":contact.phone,"type":contact.contact_type},
            "originalText": prepared.original_text,
            "finalText": final_text,
            "translated": translated,
            "targetLanguage": target_language,
            "translationMode": prepared.translation_mode,
            "replyToMessageId": reply_to_message_id,
        })))
    }

    async fn handle_prepared_send(
        &self,
        args: Value,
        require_reply: bool,
    ) -> Result<CallToolResult, McpError> {
        self.require_send()?;
        let preparation_token = required_string(&args, "preparation_token")?;
        let idempotency_key = required_string(&args, "idempotency_key")?;
        if idempotency_key.len() < 8 || idempotency_key.len() > 200 {
            return Err(McpError::invalid_params(
                "idempotency_key must contain 8 to 200 characters",
                None,
            ));
        }
        let operation_key = format!("send:{preparation_token}");
        if let Some(existing) = self
            .idempotency_result(idempotency_key, &operation_key)
            .await?
        {
            return Ok(existing);
        }
        let prepared_value = self
            .state
            .mcp_prepared_messages
            .read()
            .await
            .get(preparation_token)
            .cloned()
            .ok_or_else(|| {
                McpError::invalid_params(
                    "Unknown, expired, or already-used preparation_token",
                    None,
                )
            })?;
        let prepared: PreparedMcpMessage = serde_json::from_value(prepared_value)
            .map_err(|error| McpError::internal_error(error.to_string(), None))?;
        if prepared.expires_at <= chrono::Utc::now().timestamp_millis() {
            return Err(McpError::invalid_params(
                "preparation_token has expired",
                None,
            ));
        }
        if require_reply && prepared.reply_to_message_id.is_none() {
            return Err(McpError::invalid_params(
                "reply_to_message requires a preparation created with reply_to_message_id",
                None,
            ));
        }
        if !self
            .begin_idempotent_operation(idempotency_key, &operation_key)
            .await
        {
            return self
                .idempotency_result(idempotency_key, &operation_key)
                .await?
                .ok_or_else(|| McpError::internal_error("Idempotency state changed", None));
        }
        self.state
            .mcp_prepared_messages
            .write()
            .await
            .remove(preparation_token);
        match self.send_prepared_message(&prepared).await {
            Ok(result) => {
                self.finish_idempotent_operation(idempotency_key, "completed", result.clone())
                    .await;
                Ok(CallToolResult::structured(result))
            }
            Err(error) => {
                let result = json!({"success":false,"error":error.to_string()});
                self.finish_idempotent_operation(idempotency_key, "failed", result)
                    .await;
                Err(error)
            }
        }
    }

    async fn send_prepared_message(
        &self,
        prepared: &PreparedMcpMessage,
    ) -> Result<Value, McpError> {
        if !*self.state.connected.read().await {
            return Err(McpError::internal_error("Not connected to WhatsApp", None));
        }
        let contact = self
            .state
            .store
            .get_contact(&prepared.contact_id)
            .mcp()?
            .ok_or_else(|| McpError::invalid_params("Recipient no longer exists", None))?;
        let request_id = self.state.next_request_id();
        let timestamp = chrono::Utc::now().timestamp_millis();
        let temp_message_id = format!("mcp_pending_{request_id}_{timestamp}");
        let reply_context = prepared.reply_to_message_id.as_ref().map(|message_id| {
            json!({
                "messageId": message_id,
                "senderName": prepared.reply_to_sender.clone().unwrap_or_else(|| "Unknown".to_string()),
                "text": prepared.reply_to_text.clone().unwrap_or_default(),
            })
        });
        let content = json!({
            "type":"text",
            "body":prepared.original_text,
            "reply_context":reply_context,
        });
        let stored_message = StoredMessage {
            id: temp_message_id.clone(),
            contact_id: prepared.contact_id.clone(),
            timestamp,
            is_from_me: true,
            is_forwarded: false,
            sender_name: self.state.name.read().await.clone(),
            sender_phone: self.state.phone.read().await.clone(),
            contact_name: contact.name.clone(),
            contact_phone: contact.phone.clone(),
            chat_type: contact
                .contact_type
                .clone()
                .unwrap_or_else(|| "private".to_string()),
            content_type: "Text".to_string(),
            content_json: content.to_string(),
            content: Some(content),
            original_text: prepared.translated.then(|| prepared.original_text.clone()),
            translated_text: prepared.translated.then(|| prepared.final_text.clone()),
            source_language: prepared.target_language.clone(),
            is_translated: prepared.translated,
            delivery_status: Some("sent".to_string()),
        };
        self.state
            .store
            .add_message(&stored_message)
            .map_err(|error| McpError::internal_error(error.to_string(), None))?;
        if let Err(error) = self.state.store.upsert_contact(
            &prepared.contact_id,
            contact.name.as_deref(),
            contact.phone.as_deref(),
            contact.contact_type.as_deref(),
            timestamp,
        ) {
            let _ = self.state.store.delete_message(&temp_message_id);
            return Err(McpError::internal_error(error.to_string(), None));
        }
        let receiver = self
            .state
            .register_pending_send(request_id, Some(temp_message_id.clone()))
            .await;
        let command = BridgeCommand::Send {
            request_id: Some(request_id),
            to: prepared.contact_id.clone(),
            text: prepared.final_text.clone(),
            reply_to: prepared.reply_to_message_id.clone(),
            reply_to_sender: prepared.reply_to_sender.clone(),
            reply_to_text: prepared.reply_to_text.clone(),
        };
        if let Err(error) = self.state.send_bridge_command(command).await {
            self.state.cancel_pending_send(request_id).await;
            let _ = self.state.store.delete_message(&temp_message_id);
            return Err(McpError::internal_error(
                format!("Failed to send message: {error}"),
                None,
            ));
        }
        let send_result = self
            .state
            .wait_for_send_result(request_id, receiver)
            .await
            .map_err(|error| match error {
                SendConfirmationError::Timeout => McpError::internal_error(
                    "Timed out waiting for WhatsApp confirmation; the idempotency key is locked to prevent a duplicate send",
                    None,
                ),
                SendConfirmationError::ChannelClosed => McpError::internal_error(
                    "WhatsApp confirmation channel closed; the idempotency key is locked to prevent a duplicate send",
                    None,
                ),
            })?;
        if !send_result.success {
            return Err(McpError::internal_error(
                send_result
                    .error
                    .unwrap_or_else(|| "WhatsApp rejected the message".to_string()),
                None,
            ));
        }
        let message_id = send_result.message_id.unwrap_or(temp_message_id);
        let timestamp = send_result.timestamp.unwrap_or(timestamp);
        info!(
            "MCP sent prepared message {message_id} to {}",
            prepared.contact_id
        );
        Ok(json!({
            "success":true,
            "messageId":message_id,
            "timestamp":timestamp,
            "timestampIso":timestamp_millis_to_iso(timestamp),
            "recipient":{"id":prepared.contact_id,"name":prepared.contact_name},
            "sentText":prepared.final_text,
            "translated":prepared.translated,
            "targetLanguage":prepared.target_language,
            "replyToMessageId":prepared.reply_to_message_id,
        }))
    }

    async fn handle_reaction(&self, args: Value) -> Result<CallToolResult, McpError> {
        self.require_send()?;
        let contact_id = required_string(&args, "contact_id")?;
        let message_id = required_string(&args, "message_id")?;
        let emoji = args
            .get("emoji")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if emoji.chars().count() > 16 {
            return Err(McpError::invalid_params("emoji is too long", None));
        }
        let idempotency_key = required_string(&args, "idempotency_key")?;
        let operation_key = format!("reaction:{contact_id}:{message_id}:{emoji}");
        if let Some(existing) = self
            .idempotency_result(idempotency_key, &operation_key)
            .await?
        {
            return Ok(existing);
        }
        let message = self
            .state
            .store
            .get_message_by_id(message_id)
            .mcp()?
            .ok_or_else(|| McpError::invalid_params("Unknown message_id", None))?;
        if message.contact_id != contact_id {
            return Err(McpError::invalid_params(
                "message_id belongs to another conversation",
                None,
            ));
        }
        if !*self.state.connected.read().await {
            return Err(McpError::internal_error("Not connected to WhatsApp", None));
        }
        if !self
            .begin_idempotent_operation(idempotency_key, &operation_key)
            .await
        {
            return self
                .idempotency_result(idempotency_key, &operation_key)
                .await?
                .ok_or_else(|| McpError::internal_error("Idempotency state changed", None));
        }
        let request_id = self.state.next_request_id();
        let receiver = self.state.register_pending_send(request_id, None).await;
        let command = BridgeCommand::SendReaction {
            request_id: Some(request_id),
            to: contact_id.to_string(),
            message_id: message_id.to_string(),
            sender_jid: args
                .get("sender_jid")
                .and_then(Value::as_str)
                .map(str::to_string)
                .or(message.sender_phone),
            emoji: emoji.to_string(),
        };
        let outcome = async {
            if let Err(error) = self.state.send_bridge_command(command).await {
                self.state.cancel_pending_send(request_id).await;
                return Err(McpError::internal_error(error, None));
            }
            let result = self
                .state
                .wait_for_send_result(request_id, receiver)
                .await
                .map_err(|error| match error {
                    SendConfirmationError::Timeout => McpError::internal_error(
                        "Timed out waiting for reaction confirmation",
                        None,
                    ),
                    SendConfirmationError::ChannelClosed => {
                        McpError::internal_error("Reaction confirmation channel closed", None)
                    }
                })?;
            if result.success {
                Ok(json!({"success":true,"messageId":message_id,"emoji":emoji}))
            } else {
                Err(McpError::internal_error(
                    result
                        .error
                        .unwrap_or_else(|| "WhatsApp rejected the reaction".to_string()),
                    None,
                ))
            }
        }
        .await;
        match outcome {
            Ok(result) => {
                self.finish_idempotent_operation(idempotency_key, "completed", result.clone())
                    .await;
                Ok(CallToolResult::structured(result))
            }
            Err(error) => {
                self.finish_idempotent_operation(
                    idempotency_key,
                    "failed",
                    json!({"success":false,"error":error.to_string()}),
                )
                .await;
                Err(error)
            }
        }
    }

    async fn handle_mark_read(&self, args: Value) -> Result<CallToolResult, McpError> {
        self.require_send()?;
        let contact_id = required_string(&args, "contact_id")?;
        self.state
            .store
            .get_contact(contact_id)
            .mcp()?
            .ok_or_else(|| McpError::invalid_params("Unknown contact_id", None))?;
        self.state.store.mark_as_read(contact_id).mcp()?;
        let mut receipt_sent = false;
        if let Some(message_id) = args.get("message_id").and_then(Value::as_str) {
            let message = self
                .state
                .store
                .get_message_by_id(message_id)
                .mcp()?
                .ok_or_else(|| McpError::invalid_params("Unknown message_id", None))?;
            if message.contact_id != contact_id {
                return Err(McpError::invalid_params(
                    "message_id belongs to another conversation",
                    None,
                ));
            }
            self.state
                .send_bridge_command(BridgeCommand::MarkRead {
                    to: contact_id.to_string(),
                    message_id: message_id.to_string(),
                    timestamp: message.timestamp.div_euclid(1000),
                    sender_jid: message.sender_phone,
                })
                .await
                .map_err(|error| McpError::internal_error(error, None))?;
            receipt_sent = true;
        }
        Ok(CallToolResult::structured(json!({
            "success":true,
            "contactId":contact_id,
            "receiptSent":receipt_sent,
        })))
    }

    async fn idempotency_result(
        &self,
        idempotency_key: &str,
        operation_key: &str,
    ) -> Result<Option<CallToolResult>, McpError> {
        let records = self.state.mcp_idempotency_results.read().await;
        let Some(value) = records.get(idempotency_key) else {
            return Ok(None);
        };
        let record: McpIdempotencyRecord = serde_json::from_value(value.clone())
            .map_err(|error| McpError::internal_error(error.to_string(), None))?;
        if record.expires_at <= chrono::Utc::now().timestamp_millis() {
            return Ok(None);
        }
        if record.operation_key != operation_key {
            return Err(McpError::invalid_params(
                "idempotency_key was already used for a different operation",
                None,
            ));
        }
        match record.status.as_str() {
            "completed" => Ok(record.result.map(CallToolResult::structured)),
            "failed" => Err(McpError::internal_error(
                record
                    .result
                    .as_ref()
                    .and_then(|value| value.get("error"))
                    .and_then(Value::as_str)
                    .unwrap_or("The previous attempt failed; use a new preparation and idempotency key")
                    .to_string(),
                None,
            )),
            _ => Err(McpError::internal_error(
                "An operation with this idempotency_key is already in progress; it will not be sent again",
                None,
            )),
        }
    }

    async fn begin_idempotent_operation(&self, idempotency_key: &str, operation_key: &str) -> bool {
        let record = McpIdempotencyRecord {
            operation_key: operation_key.to_string(),
            status: "pending".to_string(),
            result: None,
            expires_at: chrono::Utc::now().timestamp_millis() + 24 * 60 * 60 * 1000,
        };
        let mut records = self.state.mcp_idempotency_results.write().await;
        let now = chrono::Utc::now().timestamp_millis();
        records.retain(|_, value| {
            value
                .get("expiresAt")
                .and_then(Value::as_i64)
                .is_some_and(|expires_at| expires_at > now)
        });
        if records.contains_key(idempotency_key) {
            false
        } else {
            records.insert(
                idempotency_key.to_string(),
                serde_json::to_value(record).expect("serialize idempotency record"),
            );
            true
        }
    }

    async fn finish_idempotent_operation(
        &self,
        idempotency_key: &str,
        status: &str,
        result: Value,
    ) {
        let mut records = self.state.mcp_idempotency_results.write().await;
        if let Some(value) = records.get_mut(idempotency_key) {
            if let Ok(mut record) = serde_json::from_value::<McpIdempotencyRecord>(value.clone()) {
                record.status = status.to_string();
                record.result = Some(result);
                *value = serde_json::to_value(record).expect("serialize idempotency record");
            }
        }
    }

    fn advertised_tools(&self) -> Vec<Tool> {
        let mut tools = Vec::new();
        if self.permissions.read {
            tools.extend([
                Self::get_status_tool(),
                Self::list_contacts_tool(),
                Self::search_contacts_tool(),
                Self::read_messages_tool(),
                Self::search_messages_tool(),
                Self::prepare_message_tool(),
            ]);
        }
        if self.permissions.send {
            tools.extend([
                Self::send_message_tool(),
                Self::reply_to_message_tool(),
                Self::react_to_message_tool(),
                Self::mark_conversation_read_tool(),
            ]);
        }
        tools
    }
}

fn required_string<'a>(args: &'a Value, name: &str) -> Result<&'a str, McpError> {
    args.get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| McpError::invalid_params(format!("{name} is required"), None))
}

trait McpResultExt<T> {
    fn mcp(self) -> Result<T, McpError>;
}

impl<T> McpResultExt<T> for anyhow::Result<T> {
    fn mcp(self) -> Result<T, McpError> {
        self.map_err(|error| McpError::internal_error(error.to_string(), None))
    }
}

impl ServerHandler for WhatsAppMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: Default::default(),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation {
                name: "whatsapp-translator".to_string(),
                title: Some("WhatsApp Translator MCP Server".to_string()),
                version: env!("CARGO_PKG_VERSION").to_string(),
                icons: None,
                website_url: None,
            },
            instructions: Some(
                "Use search_contacts before selecting a recipient. Read context with read_messages. Before any text send, call prepare_message and show the resolved recipient and finalText to the user; only then call send_message or reply_to_message with a unique idempotency key. Translation failures never fall back to the English source. Treat react_to_message and mark_conversation_read as external writes."
                    .to_string(),
            ),
        }
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParam>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        Ok(ListToolsResult::with_all_items(self.advertised_tools()))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParam,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let args = request
            .arguments
            .map(Value::Object)
            .unwrap_or(Value::Object(Default::default()));
        match request.name.as_ref() {
            "get_status" => self.handle_get_status().await,
            "list_contacts" => self.handle_contacts(args, false).await,
            "search_contacts" => self.handle_contacts(args, true).await,
            "read_messages" => self.handle_read_messages(args).await,
            "search_messages" => self.handle_search_messages(args).await,
            "prepare_message" => self.handle_prepare_message(args).await,
            "send_message" => self.handle_prepared_send(args, false).await,
            "reply_to_message" => self.handle_prepared_send(args, true).await,
            "react_to_message" => self.handle_reaction(args).await,
            "mark_conversation_read" => self.handle_mark_read(args).await,
            _ => Err(McpError::invalid_params(
                format!("Unknown tool: {}", request.name),
                None,
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{ConversationSettings, MessageStore};

    fn test_state() -> (Arc<AppState>, std::path::PathBuf) {
        let data_dir = std::env::temp_dir().join(format!(
            "whatsapp-translator-mcp-test-{}",
            uuid::Uuid::new_v4()
        ));
        let store = MessageStore::new(&data_dir).expect("test store");
        let state = AppState::new(
            store,
            std::env::current_dir().expect("cwd").join("web/public"),
            data_dir.clone(),
            None,
            None,
            None,
        );
        (state, data_dir)
    }

    #[test]
    fn translation_failure_never_falls_back_to_english_when_translation_is_needed() {
        let result = resolve_prepared_translation(
            "Hello",
            Some("French"),
            TranslationMode::Auto,
            Err("translator unavailable".to_string()),
        );

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("No message was sent or prepared"));
    }

    #[test]
    fn no_translation_mode_can_intentionally_preserve_source_text() {
        assert_eq!(
            resolve_prepared_translation(
                "Hello",
                Some("French"),
                TranslationMode::Never,
                Err("translator unavailable".to_string()),
            )
            .expect("translation disabled"),
            ("Hello".to_string(), false, None)
        );
    }

    #[test]
    fn read_tools_are_annotated_read_only_and_send_is_not() {
        let list = WhatsAppMcpServer::list_contacts_tool();
        let read = WhatsAppMcpServer::read_messages_tool();
        let send = WhatsAppMcpServer::send_message_tool();

        assert_eq!(
            list.annotations
                .as_ref()
                .and_then(|value| value.read_only_hint),
            Some(true)
        );
        assert_eq!(
            read.annotations
                .as_ref()
                .and_then(|value| value.read_only_hint),
            Some(true)
        );
        assert_eq!(
            send.annotations
                .as_ref()
                .and_then(|value| value.read_only_hint),
            Some(false)
        );
        assert_eq!(
            send.annotations
                .as_ref()
                .and_then(|value| value.idempotent_hint),
            Some(false)
        );
    }

    #[test]
    fn scopes_control_advertised_tools() {
        assert_eq!(
            McpPermissions::from_scope("whatsapp.read"),
            McpPermissions {
                read: true,
                send: false
            }
        );
        assert_eq!(
            McpPermissions::from_scope("whatsapp.read whatsapp.send"),
            McpPermissions {
                read: true,
                send: true
            }
        );
        assert_eq!(
            McpPermissions::from_scope("mcp"),
            McpPermissions {
                read: true,
                send: true
            }
        );
    }

    #[test]
    fn read_only_scope_hides_external_write_tools() {
        let (state, data_dir) = test_state();
        let server = WhatsAppMcpServer::new(
            state,
            McpPermissions {
                read: true,
                send: false,
            },
        );
        let names: Vec<String> = server
            .advertised_tools()
            .into_iter()
            .map(|tool| tool.name.into_owned())
            .collect();

        assert!(names.iter().any(|name| name == "search_contacts"));
        assert!(names.iter().any(|name| name == "prepare_message"));
        assert!(!names.iter().any(|name| name == "send_message"));
        assert!(!names.iter().any(|name| name == "react_to_message"));
        std::fs::remove_dir_all(data_dir).expect("remove test data");
    }

    #[tokio::test]
    async fn search_contacts_returns_rich_structured_results() {
        let (state, data_dir) = test_state();
        state
            .store
            .upsert_contact(
                "447700900123@s.whatsapp.net",
                Some("Lee Example"),
                Some("447700900123"),
                Some("private"),
                1_700_000_000_000,
            )
            .expect("insert contact");
        let server = WhatsAppMcpServer::new(
            state,
            McpPermissions {
                read: true,
                send: false,
            },
        );

        let result = server
            .handle_contacts(json!({"query":"lee"}), true)
            .await
            .expect("search contacts");
        let payload = result.structured_content.expect("structured output");
        assert_eq!(payload["totalMatched"], 1);
        assert_eq!(payload["contacts"][0]["name"], "Lee Example");
        assert_eq!(
            payload["contacts"][0]["lastMessageTime"],
            1_700_000_000_000_i64
        );
        std::fs::remove_dir_all(data_dir).expect("remove test data");
    }

    #[tokio::test]
    async fn required_conversation_translation_without_a_translator_prepares_nothing() {
        let (state, data_dir) = test_state();
        let contact_id = "33612345678@s.whatsapp.net";
        state
            .store
            .upsert_contact(
                contact_id,
                Some("French Contact"),
                Some("33612345678"),
                Some("private"),
                1_700_000_000_000,
            )
            .expect("insert contact");
        state
            .store
            .update_conversation_settings(
                contact_id,
                &ConversationSettings {
                    language_override: Some("French".to_string()),
                    translation_style: None,
                    send_original_follow_up: false,
                },
            )
            .expect("settings");
        let server = WhatsAppMcpServer::new(
            state.clone(),
            McpPermissions {
                read: true,
                send: true,
            },
        );

        let error = server
            .handle_prepare_message(json!({
                "contact_id":contact_id,
                "text":"Hello",
                "translation_mode":"auto"
            }))
            .await
            .expect_err("translation must fail closed");
        assert!(error
            .to_string()
            .contains("No message was sent or prepared"));
        assert!(state.mcp_prepared_messages.read().await.is_empty());
        std::fs::remove_dir_all(data_dir).expect("remove test data");
    }

    #[tokio::test]
    async fn choosing_reply_for_a_non_reply_preparation_does_not_consume_it() {
        let (state, data_dir) = test_state();
        let token = "prepared-token";
        let prepared = PreparedMcpMessage {
            token: token.to_string(),
            contact_id: "chat@example.test".to_string(),
            contact_name: Some("Test".to_string()),
            original_text: "Hello".to_string(),
            final_text: "Hello".to_string(),
            translated: false,
            target_language: None,
            translation_mode: "never".to_string(),
            reply_to_message_id: None,
            reply_to_sender: None,
            reply_to_text: None,
            created_at: chrono::Utc::now().timestamp_millis(),
            expires_at: chrono::Utc::now().timestamp_millis() + PREPARATION_TTL_MILLIS,
        };
        state.mcp_prepared_messages.write().await.insert(
            token.to_string(),
            serde_json::to_value(prepared).expect("serialize preparation"),
        );
        let server = WhatsAppMcpServer::new(
            state.clone(),
            McpPermissions {
                read: true,
                send: true,
            },
        );

        server
            .handle_prepared_send(
                json!({"preparation_token":token,"idempotency_key":"reply-test-key"}),
                true,
            )
            .await
            .expect_err("not a reply preparation");
        assert!(state.mcp_prepared_messages.read().await.contains_key(token));
        std::fs::remove_dir_all(data_dir).expect("remove test data");
    }

    #[tokio::test]
    async fn idempotency_claim_is_atomic() {
        let (state, data_dir) = test_state();
        let server = WhatsAppMcpServer::new(
            state,
            McpPermissions {
                read: true,
                send: true,
            },
        );
        assert!(
            server
                .begin_idempotent_operation("unique-key", "send:one")
                .await
        );
        assert!(
            !server
                .begin_idempotent_operation("unique-key", "send:one")
                .await
        );
        std::fs::remove_dir_all(data_dir).expect("remove test data");
    }
}
