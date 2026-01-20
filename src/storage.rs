//! SQLite storage for messages and contacts.

use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::{Arc, Mutex};
use tracing::info;

/// Stored message with translation info
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredMessage {
    pub id: String,
    #[serde(rename = "contactId")]
    pub contact_id: String,
    pub timestamp: i64,
    #[serde(rename = "isFromMe")]
    pub is_from_me: bool,
    #[serde(rename = "isForwarded")]
    pub is_forwarded: bool,
    #[serde(rename = "senderName")]
    pub sender_name: Option<String>,
    #[serde(rename = "senderPhone")]
    pub sender_phone: Option<String>,
    #[serde(rename = "chatType")]
    pub chat_type: String,
    #[serde(rename = "contentType")]
    pub content_type: String,
    /// Raw JSON string stored in database
    #[serde(skip_serializing)]
    pub content_json: String,
    /// Parsed content for API responses
    #[serde(skip_deserializing)]
    pub content: Option<serde_json::Value>,
    // Translation fields
    #[serde(rename = "originalText")]
    pub original_text: Option<String>,
    #[serde(rename = "translatedText")]
    pub translated_text: Option<String>,
    #[serde(rename = "sourceLanguage")]
    pub source_language: Option<String>,
    #[serde(rename = "isTranslated")]
    pub is_translated: bool,
}

/// Stored contact
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredContact {
    pub id: String,
    pub name: Option<String>,
    pub phone: Option<String>,
    #[serde(rename = "type")]
    pub contact_type: Option<String>,
    #[serde(rename = "lastMessageTime")]
    pub last_message_time: i64,
    #[serde(rename = "unreadCount")]
    pub unread_count: i32,
}

/// Thread-safe message store backed by SQLite
pub struct MessageStore {
    conn: Arc<Mutex<Connection>>,
}

impl MessageStore {
    /// Create a new message store
    pub fn new(data_dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(data_dir)?;
        let db_path = data_dir.join("messages.db");

        let conn = Connection::open(&db_path).context("Failed to open messages database")?;

        // Enable WAL mode for better performance
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;

        let store = Self {
            conn: Arc::new(Mutex::new(conn)),
        };

        store.init_schema()?;

        info!("Message store initialized at {:?}", db_path);

        Ok(store)
    }

    /// Initialize database schema
    fn init_schema(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        conn.execute_batch(
            r#"
            -- Contacts table
            CREATE TABLE IF NOT EXISTS contacts (
                id TEXT PRIMARY KEY,
                name TEXT,
                phone TEXT,
                type TEXT,
                last_message_time INTEGER DEFAULT 0,
                unread_count INTEGER DEFAULT 0
            );

            -- Messages table
            CREATE TABLE IF NOT EXISTS messages (
                id TEXT PRIMARY KEY,
                contact_id TEXT NOT NULL,
                timestamp INTEGER NOT NULL,
                is_from_me INTEGER NOT NULL,
                is_forwarded INTEGER DEFAULT 0,
                sender_name TEXT,
                sender_phone TEXT,
                chat_type TEXT,
                content_type TEXT NOT NULL,
                content_json TEXT NOT NULL,
                FOREIGN KEY (contact_id) REFERENCES contacts(id)
            );

            -- Indexes
            CREATE INDEX IF NOT EXISTS idx_messages_contact_id ON messages(contact_id);
            CREATE INDEX IF NOT EXISTS idx_messages_timestamp ON messages(timestamp);
            CREATE INDEX IF NOT EXISTS idx_contacts_last_message ON contacts(last_message_time DESC);
            "#,
        )?;

        // Add translation columns if they don't exist (migration for existing databases)
        self.migrate_add_translation_columns(&conn)?;

        Ok(())
    }

    /// Add translation columns to existing database
    fn migrate_add_translation_columns(&self, conn: &Connection) -> Result<()> {
        // Check if columns exist by querying table info
        let has_original_text: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('messages') WHERE name = 'original_text'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0)
            > 0;

        if !has_original_text {
            tracing::info!("Migrating database: adding translation columns...");
            conn.execute_batch(
                r#"
                ALTER TABLE messages ADD COLUMN original_text TEXT;
                ALTER TABLE messages ADD COLUMN translated_text TEXT;
                ALTER TABLE messages ADD COLUMN source_language TEXT;
                ALTER TABLE messages ADD COLUMN is_translated INTEGER DEFAULT 0;
                "#,
            )?;
            tracing::info!("Database migration complete");
        }

        Ok(())
    }

    /// Add or update a contact
    pub fn upsert_contact(
        &self,
        id: &str,
        name: Option<&str>,
        phone: Option<&str>,
        contact_type: Option<&str>,
        last_message_time: i64,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        conn.execute(
            r#"
            INSERT INTO contacts (id, name, phone, type, last_message_time, unread_count)
            VALUES (?1, ?2, ?3, ?4, ?5, 0)
            ON CONFLICT(id) DO UPDATE SET
                name = COALESCE(
                    CASE WHEN excluded.name IS NOT NULL AND excluded.name != excluded.phone 
                         THEN excluded.name ELSE NULL END,
                    contacts.name
                ),
                phone = COALESCE(excluded.phone, contacts.phone),
                type = COALESCE(excluded.type, contacts.type),
                last_message_time = MAX(contacts.last_message_time, excluded.last_message_time)
            "#,
            params![id, name, phone, contact_type, last_message_time],
        )?;

        Ok(())
    }

    /// Increment unread count for a contact
    pub fn increment_unread(&self, contact_id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE contacts SET unread_count = unread_count + 1 WHERE id = ?",
            params![contact_id],
        )?;
        Ok(())
    }

    /// Reset unread count for a contact
    pub fn mark_as_read(&self, contact_id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE contacts SET unread_count = 0 WHERE id = ?",
            params![contact_id],
        )?;
        Ok(())
    }

    /// Add a message to the store
    pub fn add_message(&self, msg: &StoredMessage) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        conn.execute(
            r#"
            INSERT OR IGNORE INTO messages 
            (id, contact_id, timestamp, is_from_me, is_forwarded, sender_name, sender_phone, 
             chat_type, content_type, content_json, original_text, translated_text, 
             source_language, is_translated)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
            "#,
            params![
                msg.id,
                msg.contact_id,
                msg.timestamp,
                msg.is_from_me,
                msg.is_forwarded,
                msg.sender_name,
                msg.sender_phone,
                msg.chat_type,
                msg.content_type,
                msg.content_json,
                msg.original_text,
                msg.translated_text,
                msg.source_language,
                msg.is_translated,
            ],
        )?;

        Ok(())
    }

    /// Get all contacts sorted by last message time
    pub fn get_contacts(&self) -> Result<Vec<StoredContact>> {
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn.prepare(
            "SELECT id, name, phone, type, last_message_time, unread_count 
             FROM contacts ORDER BY last_message_time DESC",
        )?;

        let contacts = stmt
            .query_map([], |row| {
                Ok(StoredContact {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    phone: row.get(2)?,
                    contact_type: row.get(3)?,
                    last_message_time: row.get(4)?,
                    unread_count: row.get(5)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(contacts)
    }

    /// Get messages for a specific contact
    pub fn get_messages(&self, contact_id: &str) -> Result<Vec<StoredMessage>> {
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn.prepare(
            r#"
            SELECT id, contact_id, timestamp, is_from_me, is_forwarded, sender_name, 
                   sender_phone, chat_type, content_type, content_json, original_text,
                   translated_text, source_language, is_translated
            FROM messages 
            WHERE contact_id = ?
            ORDER BY timestamp ASC
            "#,
        )?;

        let messages = stmt
            .query_map(params![contact_id], |row| {
                let content_json: String = row.get(9)?;
                let content = serde_json::from_str(&content_json).ok();
                Ok(StoredMessage {
                    id: row.get(0)?,
                    contact_id: row.get(1)?,
                    timestamp: row.get(2)?,
                    is_from_me: row.get(3)?,
                    is_forwarded: row.get(4)?,
                    sender_name: row.get(5)?,
                    sender_phone: row.get(6)?,
                    chat_type: row.get(7)?,
                    content_type: row.get(8)?,
                    content_json,
                    content,
                    original_text: row.get(10)?,
                    translated_text: row.get(11)?,
                    source_language: row.get(12)?,
                    is_translated: row.get(13)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(messages)
    }

    /// Get a contact by ID
    pub fn get_contact(&self, contact_id: &str) -> Result<Option<StoredContact>> {
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn.prepare(
            "SELECT id, name, phone, type, last_message_time, unread_count 
             FROM contacts WHERE id = ?",
        )?;

        let contact = stmt
            .query_row(params![contact_id], |row| {
                Ok(StoredContact {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    phone: row.get(2)?,
                    contact_type: row.get(3)?,
                    last_message_time: row.get(4)?,
                    unread_count: row.get(5)?,
                })
            })
            .ok();

        Ok(contact)
    }

    /// Get database statistics
    pub fn get_stats(&self) -> Result<(i64, i64)> {
        let conn = self.conn.lock().unwrap();

        let message_count: i64 =
            conn.query_row("SELECT COUNT(*) FROM messages", [], |row| row.get(0))?;

        let contact_count: i64 =
            conn.query_row("SELECT COUNT(*) FROM contacts", [], |row| row.get(0))?;

        Ok((message_count, contact_count))
    }

    /// Get the predominant language used by a contact in recent messages.
    /// Returns the most common source_language from the last N incoming messages.
    pub fn get_conversation_language(
        &self,
        contact_id: &str,
        _limit: usize,
    ) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();

        // Get the most common source language from recent incoming (not from me) messages
        let mut stmt = conn.prepare(
            r#"
            SELECT source_language, COUNT(*) as cnt
            FROM messages 
            WHERE contact_id = ? 
              AND is_from_me = 0 
              AND source_language IS NOT NULL
              AND source_language != ''
            GROUP BY source_language
            ORDER BY cnt DESC
            LIMIT 1
            "#,
        )?;

        let language: Option<String> = stmt.query_row(params![contact_id], |row| row.get(0)).ok();

        Ok(language)
    }
}

impl Clone for MessageStore {
    fn clone(&self) -> Self {
        Self {
            conn: Arc::clone(&self.conn),
        }
    }
}
