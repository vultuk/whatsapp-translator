//! MCP (Model Context Protocol) server implementation for WhatsApp.
//!
//! Exposes WhatsApp functionality to external LLMs via the MCP protocol.

use crate::storage::{MessageStore, StoredContact, StoredMessage};
use crate::translation::TranslationService;
use rmcp::{
    model::{
        CallToolRequestParam, CallToolResult, Content, Implementation, ListToolsResult,
        PaginatedRequestParam, ServerCapabilities, ServerInfo, Tool,
    },
    service::RequestContext,
    ErrorData as McpError, RoleServer, ServerHandler,
};
use serde::Serialize;
use serde_json::json;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

use crate::bridge::BridgeCommand;

/// WhatsApp MCP Server handler
#[derive(Clone)]
pub struct WhatsAppMcpServer {
    store: Arc<MessageStore>,
    command_tx: Option<mpsc::Sender<BridgeCommand>>,
    translator: Option<Arc<TranslationService>>,
}

/// Contact information returned by the API
#[derive(Debug, Serialize)]
pub struct ContactInfo {
    pub id: String,
    pub name: Option<String>,
    pub phone: Option<String>,
    #[serde(rename = "type")]
    pub contact_type: Option<String>,
    pub unread_count: i32,
    pub is_pinned: bool,
}

impl From<StoredContact> for ContactInfo {
    fn from(c: StoredContact) -> Self {
        Self {
            id: c.id,
            name: c.name,
            phone: c.phone,
            contact_type: c.contact_type,
            unread_count: c.unread_count,
            is_pinned: c.pinned_at.is_some(),
        }
    }
}

/// Message information returned by the API
#[derive(Debug, Serialize)]
pub struct MessageInfo {
    pub id: String,
    pub timestamp: i64,
    pub is_from_me: bool,
    pub sender_name: Option<String>,
    pub text: Option<String>,
    pub translated_text: Option<String>,
    pub content_type: String,
}

impl From<StoredMessage> for MessageInfo {
    fn from(m: StoredMessage) -> Self {
        // Get the display text (translated for incoming, original for outgoing)
        let text = if m.is_translated && !m.is_from_me {
            m.translated_text.clone()
        } else {
            m.original_text.or_else(|| {
                // Try to extract from content JSON
                m.content.as_ref().and_then(|c| {
                    c.get("body")
                        .and_then(|v| v.as_str().map(String::from))
                        .or_else(|| c.get("caption").and_then(|v| v.as_str().map(String::from)))
                })
            })
        };

        Self {
            id: m.id,
            timestamp: m.timestamp,
            is_from_me: m.is_from_me,
            sender_name: m.sender_name,
            text,
            translated_text: m.translated_text,
            content_type: m.content_type,
        }
    }
}

impl WhatsAppMcpServer {
    pub fn new(
        store: Arc<MessageStore>,
        command_tx: Option<mpsc::Sender<BridgeCommand>>,
        translator: Option<Arc<TranslationService>>,
    ) -> Self {
        Self {
            store,
            command_tx,
            translator,
        }
    }

    fn list_contacts_tool() -> Tool {
        let schema = json!({
            "type": "object",
            "properties": {
                "contact_type": {
                    "type": "string",
                    "description": "Filter by type: 'private', 'group', or 'all' (default: 'all')",
                    "enum": ["all", "private", "group"]
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of contacts to return (default: 50)",
                    "minimum": 1,
                    "maximum": 200
                }
            }
        });
        Tool::new(
            "list_contacts",
            "List all WhatsApp contacts and groups. Returns contact ID, name, phone number, type (private/group), and unread count.",
            schema.as_object().unwrap().clone(),
        )
    }

    fn read_messages_tool() -> Tool {
        let schema = json!({
            "type": "object",
            "properties": {
                "contact_id": {
                    "type": "string",
                    "description": "Contact or group ID (JID) to read messages from"
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of messages to return (default: 50)",
                    "minimum": 1,
                    "maximum": 200
                }
            },
            "required": ["contact_id"]
        });
        Tool::new(
            "read_messages",
            "Read messages from a specific WhatsApp contact or group. Returns message history with timestamps, sender info, and message content.",
            schema.as_object().unwrap().clone(),
        )
    }

    fn send_message_tool() -> Tool {
        let schema = json!({
            "type": "object",
            "properties": {
                "contact_id": {
                    "type": "string",
                    "description": "Contact or group ID (JID) to send the message to"
                },
                "text": {
                    "type": "string",
                    "description": "Message text to send"
                }
            },
            "required": ["contact_id", "text"]
        });
        Tool::new(
            "send_message",
            "Send a text message to a WhatsApp contact or group. The message will be sent through the connected WhatsApp account.",
            schema.as_object().unwrap().clone(),
        )
    }

    async fn handle_list_contacts(
        &self,
        args: serde_json::Value,
    ) -> Result<CallToolResult, McpError> {
        let contact_type = args
            .get("contact_type")
            .and_then(|v| v.as_str())
            .unwrap_or("all");
        let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(50) as usize;

        let contacts = self.store.get_contacts().map_err(|e| {
            McpError::internal_error(format!("Failed to get contacts: {}", e), None)
        })?;

        let filtered: Vec<ContactInfo> = contacts
            .into_iter()
            .filter(|c| contact_type == "all" || c.contact_type.as_deref() == Some(contact_type))
            .take(limit)
            .map(ContactInfo::from)
            .collect();

        let json = serde_json::to_string_pretty(&filtered).map_err(|e| {
            McpError::internal_error(format!("Failed to serialize contacts: {}", e), None)
        })?;

        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    async fn handle_read_messages(
        &self,
        args: serde_json::Value,
    ) -> Result<CallToolResult, McpError> {
        let contact_id = args
            .get("contact_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| McpError::invalid_params("contact_id is required", None))?;
        let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(50) as usize;

        let messages = self.store.get_messages(contact_id).map_err(|e| {
            McpError::internal_error(format!("Failed to get messages: {}", e), None)
        })?;

        // Get the last N messages
        let recent: Vec<MessageInfo> = messages
            .into_iter()
            .rev()
            .take(limit)
            .rev()
            .map(MessageInfo::from)
            .collect();

        let json = serde_json::to_string_pretty(&recent).map_err(|e| {
            McpError::internal_error(format!("Failed to serialize messages: {}", e), None)
        })?;

        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    async fn handle_send_message(
        &self,
        args: serde_json::Value,
    ) -> Result<CallToolResult, McpError> {
        let contact_id = args
            .get("contact_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| McpError::invalid_params("contact_id is required", None))?;
        let text = args
            .get("text")
            .and_then(|v| v.as_str())
            .ok_or_else(|| McpError::invalid_params("text is required", None))?;

        let command_tx = self
            .command_tx
            .as_ref()
            .ok_or_else(|| McpError::internal_error("WhatsApp bridge not connected", None))?;

        // Translate the message if needed based on conversation language
        let (text_to_send, was_translated, target_language) =
            if let Some(translator) = &self.translator {
                match self.store.get_conversation_language(contact_id, 10) {
                    Ok(Some(conv_lang)) => {
                        info!(
                            "MCP: Conversation language for {} is {}",
                            contact_id, conv_lang
                        );
                        match translator.translate_to(text, &conv_lang).await {
                            Ok((translated, usage)) => {
                                // Record usage if there was actual API usage
                                if usage.input_tokens > 0 {
                                    if let Err(e) = self.store.record_usage(
                                        Some(contact_id),
                                        None,
                                        &usage,
                                        "translate_outgoing_mcp",
                                    ) {
                                        warn!("Failed to record usage: {}", e);
                                    }
                                }

                                if translated != text {
                                    info!(
                                        "MCP: Translated outgoing message to {} (cost: ${:.6})",
                                        conv_lang, usage.cost_usd
                                    );
                                    (translated, true, Some(conv_lang))
                                } else {
                                    (text.to_string(), false, None)
                                }
                            }
                            Err(e) => {
                                error!("MCP: Failed to translate outgoing message: {}", e);
                                (text.to_string(), false, None)
                            }
                        }
                    }
                    Ok(None) => (text.to_string(), false, None),
                    Err(e) => {
                        error!("MCP: Failed to get conversation language: {}", e);
                        (text.to_string(), false, None)
                    }
                }
            } else {
                (text.to_string(), false, None)
            };

        // Create the send command
        let cmd = BridgeCommand::Send {
            request_id: None,
            to: contact_id.to_string(),
            text: text_to_send.clone(),
            reply_to: None,
            reply_to_sender: None,
        };

        command_tx.send(cmd).await.map_err(
            |e: tokio::sync::mpsc::error::SendError<BridgeCommand>| {
                McpError::internal_error(format!("Failed to send message: {}", e), None)
            },
        )?;

        // Store the sent message locally
        let timestamp = chrono::Utc::now().timestamp_millis();
        let temp_message_id = format!("mcp_pending_{}", timestamp);

        // Get contact info for the recipient
        let contact_info = self.store.get_contact(contact_id).ok().flatten();
        let contact_name = contact_info.as_ref().and_then(|c| c.name.clone());
        let contact_phone = contact_info.as_ref().and_then(|c| c.phone.clone());
        let chat_type = contact_info
            .as_ref()
            .and_then(|c| c.contact_type.clone())
            .unwrap_or_else(|| "private".to_string());

        // Build content JSON - store what the user typed (English)
        let content = json!({
            "type": "text",
            "body": text
        });

        // Create StoredMessage struct
        let stored_msg = StoredMessage {
            id: temp_message_id,
            contact_id: contact_id.to_string(),
            timestamp,
            is_from_me: true,
            is_forwarded: false,
            sender_name: None,
            sender_phone: None,
            contact_name: contact_name.clone(),
            contact_phone: contact_phone.clone(),
            chat_type: chat_type.clone(),
            content_type: "Text".to_string(),
            content_json: content.to_string(),
            content: Some(content),
            original_text: if was_translated {
                Some(text.to_string())
            } else {
                None
            },
            translated_text: if was_translated {
                Some(text_to_send.clone())
            } else {
                None
            },
            source_language: target_language.clone(),
            is_translated: was_translated,
        };

        // Store the message
        if let Err(e) = self.store.add_message(&stored_msg) {
            warn!("MCP: Failed to store sent message: {}", e);
        }

        // Update contact's last message time
        if let Err(e) = self.store.upsert_contact(
            contact_id,
            contact_name.as_deref(),
            contact_phone.as_deref(),
            Some(&chat_type),
            timestamp,
        ) {
            warn!("MCP: Failed to update contact: {}", e);
        }

        let response = if was_translated {
            format!(
                "Message sent to {} (translated to {}): \"{}\" -> \"{}\"",
                contact_id,
                target_language.unwrap_or_default(),
                text,
                text_to_send
            )
        } else {
            format!("Message sent to {}: \"{}\"", contact_id, text)
        };

        Ok(CallToolResult::success(vec![Content::text(response)]))
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
                "This MCP server provides access to WhatsApp conversations. \
                 Use list_contacts to see available chats, read_messages to get message history, \
                 and send_message to send new messages."
                    .to_string(),
            ),
        }
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParam>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        Ok(ListToolsResult::with_all_items(vec![
            Self::list_contacts_tool(),
            Self::read_messages_tool(),
            Self::send_message_tool(),
        ]))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParam,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let args = request
            .arguments
            .map(serde_json::Value::Object)
            .unwrap_or(serde_json::Value::Null);

        match request.name.as_ref() {
            "list_contacts" => self.handle_list_contacts(args).await,
            "read_messages" => self.handle_read_messages(args).await,
            "send_message" => self.handle_send_message(args).await,
            _ => Err(McpError::invalid_params(
                format!("Unknown tool: {}", request.name),
                None,
            )),
        }
    }
}
