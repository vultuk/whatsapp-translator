//! SQLite storage for messages and contacts.

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::{Arc, Mutex};
use tracing::info;

use crate::link_preview::LinkPreview;
use crate::oauth::{
    AccessToken, AuthorizationCode, OAuthClientRegistration, PendingAuthorization, RefreshToken,
};
use crate::translation::UsageInfo;

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
    /// Contact name (other person for private chats, group name for groups)
    #[serde(rename = "contactName")]
    pub contact_name: Option<String>,
    /// Contact phone (for private chats)
    #[serde(rename = "contactPhone")]
    pub contact_phone: Option<String>,
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
    /// Timestamp when pinned (None = not pinned)
    #[serde(rename = "pinnedAt")]
    pub pinned_at: Option<i64>,
    /// Preview of the last message (truncated)
    #[serde(rename = "lastMessagePreview")]
    pub last_message_preview: Option<String>,
}

/// Conversation settings for per-contact customization
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ConversationSettings {
    /// Override the target language for translations in this conversation
    /// e.g., "Spanish", "French", "Japanese"
    pub language_override: Option<String>,
    /// Style instruction for translations in this conversation
    /// e.g., "formal", "informal", "family", "robotic", "geek"
    pub translation_style: Option<String>,
    /// Send the untranslated draft as a second WhatsApp message after a translated send.
    pub send_original_follow_up: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OpenAiSettings {
    pub model: Option<String>,
    pub reasoning_effort: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushDevice {
    pub installation_id: String,
    pub token: String,
    pub environment: String,
}

/// Style profile for AI reply generation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StyleProfile {
    /// Contact ID or "__global__" for overall style
    pub contact_id: String,
    /// AI-generated style description
    pub profile_text: String,
    /// Example messages used for analysis (JSON array)
    pub sample_messages: Vec<String>,
    /// Number of messages analyzed
    pub message_count: i32,
    /// Timestamp of last update
    pub updated_at: i64,
}

impl StyleProfile {
    /// The special contact ID used for the global style profile
    pub const GLOBAL_ID: &'static str = "__global__";

    /// Check if this is the global profile
    pub fn is_global(&self) -> bool {
        self.contact_id == Self::GLOBAL_ID
    }
}

/// Thread-safe message store backed by SQLite
pub struct MessageStore {
    conn: Arc<Mutex<Connection>>,
}

impl MessageStore {
    /// Create a new message store
    pub fn new(data_dir: &Path) -> Result<Self> {
        // Ensure data directory exists with proper permissions
        std::fs::create_dir_all(data_dir)
            .with_context(|| format!("Failed to create data directory: {:?}", data_dir))?;

        let db_path = data_dir.join("messages.db");

        info!("Opening database at {:?}", db_path);

        // Open with explicit create flag
        let conn = Connection::open_with_flags(
            &db_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE
                | rusqlite::OpenFlags::SQLITE_OPEN_CREATE
                | rusqlite::OpenFlags::SQLITE_OPEN_FULL_MUTEX,
        )
        .with_context(|| format!("unable to open database file: {:?}", db_path))?;

        // Tune SQLite for a read-heavy chat workload.
        conn.execute_batch(
            r#"
            PRAGMA journal_mode=WAL;
            PRAGMA synchronous=NORMAL;
            PRAGMA temp_store=MEMORY;
            PRAGMA cache_size=-20000;
            "#,
        )?;

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
                unread_count INTEGER DEFAULT 0,
                last_message_preview TEXT
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
            CREATE INDEX IF NOT EXISTS idx_messages_contact_timestamp_desc ON messages(contact_id, timestamp DESC);
            CREATE INDEX IF NOT EXISTS idx_messages_contact_timestamp_id_desc ON messages(contact_id, timestamp DESC, id DESC);
            CREATE INDEX IF NOT EXISTS idx_contacts_last_message ON contacts(last_message_time DESC);

            -- AI usage tracking
            CREATE TABLE IF NOT EXISTS translation_usage (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                contact_id TEXT,
                message_id TEXT,
                timestamp INTEGER NOT NULL,
                input_tokens INTEGER NOT NULL,
                cached_input_tokens INTEGER NOT NULL DEFAULT 0,
                output_tokens INTEGER NOT NULL,
                cost_usd REAL NOT NULL,
                operation TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS app_settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS push_devices (
                token TEXT PRIMARY KEY,
                installation_id TEXT NOT NULL UNIQUE,
                environment TEXT NOT NULL,
                updated_at INTEGER NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_usage_contact_id ON translation_usage(contact_id);
            CREATE INDEX IF NOT EXISTS idx_usage_timestamp ON translation_usage(timestamp);

            -- Link preview cache
            CREATE TABLE IF NOT EXISTS link_previews (
                url TEXT PRIMARY KEY,
                title TEXT,
                description TEXT,
                image_url TEXT,
                site_name TEXT,
                error TEXT,
                fetched_at INTEGER NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_link_previews_fetched ON link_previews(fetched_at);

            -- OAuth 2.0 tables for MCP authentication
            CREATE TABLE IF NOT EXISTS oauth_clients (
                client_id TEXT PRIMARY KEY,
                client_name TEXT,
                redirect_uris_json TEXT NOT NULL,
                scope TEXT NOT NULL,
                created_at INTEGER NOT NULL
            );
            
            -- Pending authorization requests (before user approves)
            CREATE TABLE IF NOT EXISTS oauth_pending_auth (
                session_key TEXT PRIMARY KEY,
                client_id TEXT NOT NULL,
                redirect_uri TEXT NOT NULL,
                code_challenge TEXT NOT NULL,
                code_challenge_method TEXT NOT NULL,
                scope TEXT NOT NULL,
                state TEXT,
                created_at INTEGER NOT NULL,
                expires_at INTEGER NOT NULL
            );

            -- Authorization codes (after user approves, before token exchange)
            CREATE TABLE IF NOT EXISTS oauth_authorization_codes (
                code TEXT PRIMARY KEY,
                client_id TEXT NOT NULL,
                redirect_uri TEXT NOT NULL,
                code_challenge TEXT NOT NULL,
                code_challenge_method TEXT NOT NULL,
                scope TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                expires_at INTEGER NOT NULL,
                used INTEGER DEFAULT 0
            );

            -- Access tokens
            CREATE TABLE IF NOT EXISTS oauth_access_tokens (
                token TEXT PRIMARY KEY,
                client_id TEXT NOT NULL,
                scope TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                expires_at INTEGER NOT NULL
            );

            -- Refresh tokens
            CREATE TABLE IF NOT EXISTS oauth_refresh_tokens (
                token TEXT PRIMARY KEY,
                client_id TEXT NOT NULL,
                scope TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                expires_at INTEGER NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_oauth_pending_expires ON oauth_pending_auth(expires_at);
            CREATE INDEX IF NOT EXISTS idx_oauth_codes_expires ON oauth_authorization_codes(expires_at);
            CREATE INDEX IF NOT EXISTS idx_oauth_access_expires ON oauth_access_tokens(expires_at);
            CREATE INDEX IF NOT EXISTS idx_oauth_refresh_expires ON oauth_refresh_tokens(expires_at);
            "#,
        )?;

        // Add translation columns if they don't exist (migration for existing databases)
        self.migrate_add_translation_columns(&conn)?;
        self.migrate_add_cached_usage_column(&conn)?;

        // Fix contact types based on JID suffix
        self.migrate_fix_contact_types(&conn)?;

        // Add pinned_at column for pinning contacts
        self.migrate_add_pinned_column(&conn)?;

        self.migrate_add_query_indexes(&conn)?;

        // Add last_message_preview column and backfill it for faster contact list loads
        self.migrate_add_last_message_preview_column(&conn)?;
        self.migrate_refresh_translated_contact_previews(&conn)?;

        // Add style_profiles table for AI reply generation
        self.migrate_add_style_profiles_table(&conn)?;

        // Add conversation settings columns.
        self.migrate_add_conversation_settings_columns(&conn)?;

        Ok(())
    }

    /// Add composite indexes for the most common read paths on existing databases
    fn migrate_add_query_indexes(&self, conn: &Connection) -> Result<()> {
        conn.execute_batch(
            r#"
            CREATE INDEX IF NOT EXISTS idx_messages_contact_timestamp_desc
                ON messages(contact_id, timestamp DESC);
            CREATE INDEX IF NOT EXISTS idx_messages_contact_timestamp_id_desc
                ON messages(contact_id, timestamp DESC, id DESC);
            CREATE INDEX IF NOT EXISTS idx_contacts_pinned_sort
                ON contacts(pinned_at, last_message_time DESC);
            CREATE INDEX IF NOT EXISTS idx_usage_contact_timestamp
                ON translation_usage(contact_id, timestamp DESC);
            CREATE INDEX IF NOT EXISTS idx_messages_contact_language
                ON messages(contact_id, is_from_me, source_language);
            "#,
        )?;

        conn.execute_batch("PRAGMA optimize;")?;

        Ok(())
    }

    /// Add style_profiles table for AI reply generation
    fn migrate_add_style_profiles_table(&self, conn: &Connection) -> Result<()> {
        // Check if table exists
        let table_exists: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='style_profiles'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0)
            > 0;

        if !table_exists {
            info!("Migrating database: creating style_profiles table...");
            conn.execute_batch(
                r#"
                CREATE TABLE style_profiles (
                    contact_id TEXT PRIMARY KEY,
                    profile_text TEXT NOT NULL,
                    sample_messages TEXT NOT NULL,
                    message_count INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL
                );

                CREATE INDEX idx_style_profiles_updated ON style_profiles(updated_at);
                "#,
            )?;
            info!("Database migration complete: created style_profiles table");
        }

        Ok(())
    }

    /// Add conversation settings columns to contacts table
    fn migrate_add_conversation_settings_columns(&self, conn: &Connection) -> Result<()> {
        // Check if language_override column exists
        let has_language_override: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('contacts') WHERE name = 'language_override'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0)
            > 0;

        if !has_language_override {
            info!("Migrating database: adding conversation settings columns...");
            conn.execute("ALTER TABLE contacts ADD COLUMN language_override TEXT", [])?;
        }

        let has_translation_style: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('contacts') WHERE name = 'translation_style'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0)
            > 0;
        if !has_translation_style {
            conn.execute("ALTER TABLE contacts ADD COLUMN translation_style TEXT", [])?;
        }

        let has_send_original_follow_up: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('contacts') WHERE name = 'send_original_follow_up'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0)
            > 0;
        if !has_send_original_follow_up {
            info!("Migrating database: adding original follow-up setting...");
            conn.execute(
                "ALTER TABLE contacts ADD COLUMN send_original_follow_up INTEGER NOT NULL DEFAULT 0",
                [],
            )?;
        }

        if !has_language_override || !has_translation_style || !has_send_original_follow_up {
            info!("Database migration complete: added conversation settings columns");
        }

        Ok(())
    }

    /// Add pinned_at column to contacts table
    fn migrate_add_pinned_column(&self, conn: &Connection) -> Result<()> {
        // Check if column exists
        let has_pinned_at: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('contacts') WHERE name = 'pinned_at'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0)
            > 0;

        if !has_pinned_at {
            info!("Migrating database: adding pinned_at column...");
            conn.execute("ALTER TABLE contacts ADD COLUMN pinned_at INTEGER", [])?;
            info!("Database migration complete: added pinned_at column");
        }

        Ok(())
    }

    /// Add last_message_preview column and backfill previews for existing contacts
    fn migrate_add_last_message_preview_column(&self, conn: &Connection) -> Result<()> {
        let has_last_message_preview: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('contacts') WHERE name = 'last_message_preview'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0)
            > 0;

        if !has_last_message_preview {
            info!("Migrating database: adding last_message_preview column...");
            conn.execute(
                "ALTER TABLE contacts ADD COLUMN last_message_preview TEXT",
                [],
            )?;
            Self::backfill_contact_previews(conn)?;
            info!("Database migration complete: added last_message_preview");
        }

        Ok(())
    }

    fn migrate_refresh_translated_contact_previews(&self, conn: &Connection) -> Result<()> {
        let version: Option<String> = conn
            .query_row(
                "SELECT value FROM app_settings WHERE key = ?",
                params!["translated_contact_previews_version"],
                |row| row.get(0),
            )
            .optional()?;
        if version.as_deref() == Some("1") {
            return Ok(());
        }

        info!("Migrating database: refreshing translated contact previews...");
        Self::backfill_contact_previews(conn)?;
        conn.execute(
            r#"
            INSERT INTO app_settings (key, value) VALUES (?1, ?2)
            ON CONFLICT(key) DO UPDATE SET value = excluded.value
            "#,
            params!["translated_contact_previews_version", "1"],
        )?;
        info!("Database migration complete: refreshed translated contact previews");
        Ok(())
    }

    fn backfill_contact_previews(conn: &Connection) -> Result<()> {
        let mut stmt = conn.prepare(
            r#"
            SELECT
                c.id,
                m.content_json,
                m.content_type,
                m.is_from_me,
                m.translated_text
            FROM contacts c
            LEFT JOIN (
                SELECT contact_id, content_json, content_type, is_from_me, translated_text,
                       ROW_NUMBER() OVER (PARTITION BY contact_id ORDER BY timestamp DESC, rowid DESC) as rn
                FROM messages
            ) m ON m.contact_id = c.id AND m.rn = 1
            "#,
        )?;

        let rows: Vec<(
            String,
            Option<String>,
            Option<String>,
            Option<bool>,
            Option<String>,
        )> = stmt
            .query_map([], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            })?
            .filter_map(|row| row.ok())
            .collect();
        drop(stmt);

        for (contact_id, content_json, content_type, is_from_me, translated_text) in rows {
            let preview = Self::generate_message_preview(
                content_json.as_deref(),
                content_type.as_deref(),
                is_from_me.unwrap_or(false),
                translated_text.as_deref(),
            );
            conn.execute(
                "UPDATE contacts SET last_message_preview = ? WHERE id = ?",
                params![preview, contact_id],
            )?;
        }

        Ok(())
    }

    /// Fix contact types based on JID suffix (groups end with @g.us)
    fn migrate_fix_contact_types(&self, conn: &Connection) -> Result<()> {
        // Update contacts where type doesn't match the JID suffix
        // This fixes both NULL types and incorrectly set types
        let updated = conn.execute(
            r#"
            UPDATE contacts 
            SET type = CASE 
                WHEN id LIKE '%@g.us' THEN 'group'
                WHEN id LIKE '%@s.whatsapp.net' THEN 'private'
                WHEN id LIKE '%@broadcast' THEN 'broadcast'
                ELSE 'private'
            END
            WHERE type IS NULL 
               OR (id LIKE '%@g.us' AND type != 'group')
               OR (id LIKE '%@s.whatsapp.net' AND type != 'private')
               OR (id LIKE '%@broadcast' AND type != 'broadcast')
            "#,
            [],
        )?;

        if updated > 0 {
            info!(
                "Fixed contact types for {} contacts based on JID suffix",
                updated
            );
        }

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

    /// Add cached_input_tokens column to usage tracking
    fn migrate_add_cached_usage_column(&self, conn: &Connection) -> Result<()> {
        let has_cached_input_tokens: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('translation_usage') WHERE name = 'cached_input_tokens'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0)
            > 0;

        if !has_cached_input_tokens {
            info!("Migrating database: adding cached_input_tokens to translation_usage...");
            conn.execute(
                "ALTER TABLE translation_usage ADD COLUMN cached_input_tokens INTEGER NOT NULL DEFAULT 0",
                [],
            )?;
            info!("Database migration complete: added cached_input_tokens");
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

    /// Set unread count for a contact (used for history sync)
    pub fn set_unread_count(&self, contact_id: &str, count: u32) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE contacts SET unread_count = ? WHERE id = ?",
            params![count as i32, contact_id],
        )?;
        Ok(())
    }

    pub fn total_unread_count(&self) -> Result<i32> {
        let conn = self.conn.lock().unwrap();
        let count = conn.query_row(
            "SELECT COALESCE(SUM(unread_count), 0) FROM contacts",
            [],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    pub fn register_push_device(
        &self,
        installation_id: &str,
        token: &str,
        environment: &str,
    ) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        tx.execute(
            "DELETE FROM push_devices WHERE installation_id = ?1 OR token = ?2",
            params![installation_id, token],
        )?;
        tx.execute(
            r#"
            INSERT INTO push_devices (token, installation_id, environment, updated_at)
            VALUES (?1, ?2, ?3, ?4)
            "#,
            params![
                token,
                installation_id,
                environment,
                chrono::Utc::now().timestamp()
            ],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn list_push_devices(&self) -> Result<Vec<PushDevice>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT installation_id, token, environment FROM push_devices ORDER BY installation_id",
        )?;
        let devices = stmt
            .query_map([], |row| {
                Ok(PushDevice {
                    installation_id: row.get(0)?,
                    token: row.get(1)?,
                    environment: row.get(2)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(devices)
    }

    pub fn delete_push_device(&self, token: &str) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        Ok(conn.execute("DELETE FROM push_devices WHERE token = ?", params![token])? > 0)
    }

    pub fn delete_push_installation(&self, installation_id: &str) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        Ok(conn.execute(
            "DELETE FROM push_devices WHERE installation_id = ?",
            params![installation_id],
        )? > 0)
    }

    /// Add a message to the store
    pub fn add_message(&self, msg: &StoredMessage) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let preview = Self::generate_message_preview(
            Some(&msg.content_json),
            Some(&msg.content_type),
            msg.is_from_me,
            msg.translated_text.as_deref(),
        );

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

        conn.execute(
            r#"
            UPDATE contacts
            SET last_message_time = MAX(last_message_time, ?2),
                last_message_preview = ?3
            WHERE id = ?1
            "#,
            params![msg.contact_id, msg.timestamp, preview],
        )?;

        Ok(())
    }

    /// Update the translation for an existing message
    pub fn update_message_translation(
        &self,
        message_id: &str,
        translated_text: Option<&str>,
        source_language: Option<&str>,
    ) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;

        tx.execute(
            r#"
            UPDATE messages 
            SET translated_text = ?1, source_language = ?2, is_translated = 1
            WHERE id = ?3
            "#,
            params![translated_text, source_language, message_id],
        )?;

        let contact_id: Option<String> = tx
            .query_row(
                "SELECT contact_id FROM messages WHERE id = ?",
                params![message_id],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(contact_id) = contact_id {
            Self::refresh_contact_last_message(&tx, &contact_id)?;
        }

        tx.commit()?;

        Ok(())
    }

    /// Replace a temporary outgoing message ID with the bridge-confirmed WhatsApp ID.
    pub fn replace_message_id(
        &self,
        temp_message_id: &str,
        confirmed_message_id: &str,
        confirmed_timestamp: Option<i64>,
    ) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;

        let temp_contact_id: Option<String> = tx
            .query_row(
                "SELECT contact_id FROM messages WHERE id = ?",
                params![temp_message_id],
                |row| row.get(0),
            )
            .optional()?;

        let Some(contact_id) = temp_contact_id else {
            return Ok(());
        };

        let confirmed_exists: bool = tx
            .query_row(
                "SELECT 1 FROM messages WHERE id = ?",
                params![confirmed_message_id],
                |_| Ok(()),
            )
            .optional()?
            .is_some();

        if confirmed_exists && temp_message_id != confirmed_message_id {
            tx.execute(
                "DELETE FROM messages WHERE id = ?",
                params![temp_message_id],
            )?;
        } else {
            tx.execute(
                r#"
                UPDATE messages
                SET id = ?1,
                    timestamp = COALESCE(?2, timestamp)
                WHERE id = ?3
                "#,
                params![confirmed_message_id, confirmed_timestamp, temp_message_id],
            )?;
        }

        Self::refresh_contact_last_message(&tx, &contact_id)?;
        tx.commit()?;
        Ok(())
    }

    /// Delete a message and refresh the owning contact's last-message summary.
    pub fn delete_message(&self, message_id: &str) -> Result<bool> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;

        let contact_id: Option<String> = tx
            .query_row(
                "SELECT contact_id FROM messages WHERE id = ?",
                params![message_id],
                |row| row.get(0),
            )
            .optional()?;

        let Some(contact_id) = contact_id else {
            return Ok(false);
        };

        let deleted = tx.execute("DELETE FROM messages WHERE id = ?", params![message_id])?;
        Self::refresh_contact_last_message(&tx, &contact_id)?;
        tx.commit()?;
        Ok(deleted > 0)
    }

    fn refresh_contact_last_message(
        conn: &rusqlite::Transaction<'_>,
        contact_id: &str,
    ) -> Result<()> {
        let latest: Option<(i64, String, String, bool, Option<String>)> = conn
            .query_row(
                r#"
                SELECT timestamp, content_json, content_type, is_from_me, translated_text
                FROM messages
                WHERE contact_id = ?
                ORDER BY timestamp DESC, id DESC
                LIMIT 1
                "#,
                params![contact_id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .optional()?;

        if let Some((timestamp, content_json, content_type, is_from_me, translated_text)) = latest {
            let preview = Self::generate_message_preview(
                Some(&content_json),
                Some(&content_type),
                is_from_me,
                translated_text.as_deref(),
            );
            conn.execute(
                r#"
                UPDATE contacts
                SET last_message_time = ?2,
                    last_message_preview = ?3
                WHERE id = ?1
                "#,
                params![contact_id, timestamp, preview],
            )?;
        } else {
            conn.execute(
                r#"
                UPDATE contacts
                SET last_message_time = 0,
                    last_message_preview = NULL
                WHERE id = ?1
                "#,
                params![contact_id],
            )?;
        }

        Ok(())
    }

    /// Get all contacts sorted by pinned status first, then last message time
    pub fn get_contacts(&self) -> Result<Vec<StoredContact>> {
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn.prepare(
            r#"
            SELECT
                id, name, phone, type, last_message_time, unread_count, pinned_at, last_message_preview
            FROM contacts
            ORDER BY 
                CASE WHEN pinned_at IS NOT NULL THEN 0 ELSE 1 END,
                pinned_at ASC,
                last_message_time DESC
            "#,
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
                    pinned_at: row.get(6)?,
                    last_message_preview: row.get(7)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(contacts)
    }

    /// Generate a preview string for a message (matching frontend logic)
    fn generate_message_preview(
        content_json: Option<&str>,
        content_type: Option<&str>,
        is_from_me: bool,
        translated_text: Option<&str>,
    ) -> Option<String> {
        let prefix = if is_from_me { "You: " } else { "" };
        let translated_text = (!is_from_me)
            .then_some(translated_text)
            .flatten()
            .map(str::trim)
            .filter(|text| !text.is_empty());

        let content_type = content_type?;
        let content_json = content_json?;

        let content: serde_json::Value = serde_json::from_str(content_json).ok()?;

        // Normalize content_type to lowercase for matching
        let content_type_lower = content_type.to_lowercase();
        let preview = match content_type_lower.as_str() {
            "text" => {
                let body = translated_text.unwrap_or_else(|| {
                    content
                        .get("body")
                        .or_else(|| content.get("text"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                });
                let truncated: String = body.chars().take(50).collect();
                format!("{}{}", prefix, truncated)
            }
            "image" => {
                let caption = translated_text
                    .or_else(|| content.get("caption").and_then(|v| v.as_str()))
                    .map(|c| {
                        let truncated: String = c.chars().take(30).collect();
                        format!(" {}", truncated)
                    })
                    .unwrap_or_default();
                format!("{}[ Image ]{}", prefix, caption)
            }
            "video" => {
                let caption = translated_text
                    .or_else(|| content.get("caption").and_then(|v| v.as_str()))
                    .map(|c| {
                        let truncated: String = c.chars().take(30).collect();
                        format!(" {}", truncated)
                    })
                    .unwrap_or_default();
                format!("{}[ Video ]{}", prefix, caption)
            }
            "audio" => {
                let is_voice = content
                    .get("is_voice_note")
                    .or_else(|| content.get("isVoiceNote"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                if is_voice {
                    format!("{}[ Voice Note ]", prefix)
                } else {
                    format!("{}[ Audio ]", prefix)
                }
            }
            "voice note" => format!("{}[ Voice Note ]", prefix),
            "document" => {
                let file_name = content
                    .get("file_name")
                    .or_else(|| content.get("fileName"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("file");
                format!("{}[ Document: {} ]", prefix, file_name)
            }
            "sticker" => format!("{}[ Sticker ]", prefix),
            "location" => format!("{}[ Location ]", prefix),
            "contact" => {
                let name = content.get("name").and_then(|v| v.as_str()).unwrap_or("");
                format!("{}[ Contact: {} ]", prefix, name)
            }
            "reaction" => {
                let emoji = content.get("emoji").and_then(|v| v.as_str()).unwrap_or("");
                format!("{}{}", prefix, emoji)
            }
            "revoked" => "[ Message deleted ]".to_string(),
            "poll" => {
                let question = content
                    .get("question")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                format!("{}[ Poll: {} ]", prefix, question)
            }
            _ => format!("{}[ Message ]", prefix),
        };

        Some(preview)
    }

    /// Pin or unpin a contact
    pub fn toggle_pin(&self, contact_id: &str) -> Result<bool> {
        let conn = self.conn.lock().unwrap();

        // Check if currently pinned
        let currently_pinned: Option<i64> = conn
            .query_row(
                "SELECT pinned_at FROM contacts WHERE id = ?",
                params![contact_id],
                |row| row.get(0),
            )
            .ok()
            .flatten();

        if currently_pinned.is_some() {
            // Unpin
            conn.execute(
                "UPDATE contacts SET pinned_at = NULL WHERE id = ?",
                params![contact_id],
            )?;
            Ok(false) // Now unpinned
        } else {
            // Pin with current timestamp
            let now = chrono::Utc::now().timestamp_millis();
            conn.execute(
                "UPDATE contacts SET pinned_at = ? WHERE id = ?",
                params![now, contact_id],
            )?;
            Ok(true) // Now pinned
        }
    }

    /// Get conversation settings for a contact
    pub fn get_conversation_settings(&self, contact_id: &str) -> Result<ConversationSettings> {
        let conn = self.conn.lock().unwrap();

        let result = conn.query_row(
            "SELECT language_override, translation_style, send_original_follow_up FROM contacts WHERE id = ?",
            params![contact_id],
            |row| {
                Ok(ConversationSettings {
                    language_override: row.get(0)?,
                    translation_style: row.get(1)?,
                    send_original_follow_up: row.get(2)?,
                })
            },
        );

        match result {
            Ok(settings) => Ok(settings),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(ConversationSettings::default()),
            Err(e) => Err(e.into()),
        }
    }

    /// Update conversation settings for a contact
    pub fn update_conversation_settings(
        &self,
        contact_id: &str,
        settings: &ConversationSettings,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        conn.execute(
            "UPDATE contacts SET language_override = ?, translation_style = ?, send_original_follow_up = ? WHERE id = ?",
            params![
                settings.language_override,
                settings.translation_style,
                settings.send_original_follow_up,
                contact_id
            ],
        )?;

        info!(
            "Updated conversation settings for {}: language={:?}, style={:?}, original_follow_up={}",
            contact_id,
            settings.language_override,
            settings.translation_style,
            settings.send_original_follow_up
        );

        Ok(())
    }

    pub fn get_openai_settings(&self) -> Result<OpenAiSettings> {
        let conn = self.conn.lock().unwrap();
        let value: Option<String> = conn
            .query_row(
                "SELECT value FROM app_settings WHERE key = 'openai'",
                [],
                |row| row.get(0),
            )
            .optional()?;
        value
            .map(|json| serde_json::from_str(&json).context("Invalid stored OpenAI settings"))
            .unwrap_or_else(|| Ok(OpenAiSettings::default()))
    }

    pub fn update_openai_settings(&self, settings: &OpenAiSettings) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let value = serde_json::to_string(settings)?;
        conn.execute(
            "INSERT INTO app_settings (key, value) VALUES ('openai', ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![value],
        )?;
        Ok(())
    }

    /// Get messages for a specific contact (all messages - for MCP/internal use)
    pub fn get_messages(&self, contact_id: &str) -> Result<Vec<StoredMessage>> {
        self.get_messages_paginated(contact_id, None, None, None, false)
    }

    /// Get media data for a specific message
    /// Returns the media_data and mime_type for a message
    pub fn get_message_media(&self, message_id: &str) -> Result<Option<(String, Option<String>)>> {
        let conn = self.conn.lock().unwrap();

        let result = conn.query_row(
            "SELECT content_json FROM messages WHERE id = ?",
            params![message_id],
            |row| row.get::<_, String>(0),
        );

        match result {
            Ok(content_json) => {
                // Parse and extract media_data and mime_type
                if let Ok(content) = serde_json::from_str::<serde_json::Value>(&content_json) {
                    let media_data = content
                        .get("media_data")
                        .or_else(|| content.get("mediaData"))
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());

                    let mime_type = content
                        .get("mime_type")
                        .or_else(|| content.get("mimeType"))
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());

                    if let Some(media) = media_data {
                        return Ok(Some((media, mime_type)));
                    }
                }
                Ok(None)
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Strip media_data from content JSON to reduce payload size
    fn strip_media_from_content(content_json: &str) -> (String, Option<serde_json::Value>) {
        if let Ok(mut content) = serde_json::from_str::<serde_json::Value>(content_json) {
            // Check if this content has media_data
            let has_media =
                content.get("media_data").is_some() || content.get("mediaData").is_some();

            if has_media {
                // Remove media_data from content
                if let Some(obj) = content.as_object_mut() {
                    obj.remove("media_data");
                    obj.remove("mediaData");
                    // Add a flag to indicate media is available
                    obj.insert("has_media".to_string(), serde_json::Value::Bool(true));
                }
                let stripped_json =
                    serde_json::to_string(&content).unwrap_or_else(|_| content_json.to_string());
                return (stripped_json, Some(content));
            }

            (content_json.to_string(), Some(content))
        } else {
            (content_json.to_string(), None)
        }
    }

    /// Get messages for a specific contact with pagination
    /// - limit: max number of messages to return (default: all)
    /// - before_timestamp: only get messages before this timestamp (for loading older messages)
    /// - before_message_id: tie-breaker cursor for messages with the same timestamp
    /// - strip_media: if true, remove media_data from content to reduce payload size
    ///
    /// Returns messages in ascending order by timestamp and ID (oldest first)
    pub fn get_messages_paginated(
        &self,
        contact_id: &str,
        limit: Option<u32>,
        before_timestamp: Option<i64>,
        before_message_id: Option<&str>,
        strip_media: bool,
    ) -> Result<Vec<StoredMessage>> {
        let conn = self.conn.lock().unwrap();

        // First get the contact info to populate contact_name and contact_phone
        let contact_info: Option<(Option<String>, Option<String>)> = conn
            .query_row(
                "SELECT name, phone FROM contacts WHERE id = ?",
                params![contact_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .ok();

        let (contact_name, contact_phone) = contact_info.unwrap_or((None, None));

        let cursor_clause = if before_timestamp.is_some() && before_message_id.is_some() {
            "AND (timestamp < ? OR (timestamp = ? AND id < ?))"
        } else if before_timestamp.is_some() {
            "AND timestamp < ?"
        } else {
            ""
        };
        let order_clause = if limit.is_some() {
            "ORDER BY timestamp DESC, id DESC"
        } else {
            "ORDER BY timestamp ASC, id ASC"
        };
        let limit_clause = limit
            .map(|lim| format!("LIMIT {}", lim))
            .unwrap_or_default();
        let query = format!(
            r#"
            SELECT id, contact_id, timestamp, is_from_me, is_forwarded, sender_name,
                   sender_phone, chat_type, content_type, content_json, original_text,
                   translated_text, source_language, is_translated
            FROM messages
            WHERE contact_id = ? {cursor_clause}
            {order_clause}
            {limit_clause}
            "#
        );

        let mut stmt = conn.prepare(&query)?;

        // Helper to build StoredMessage from row
        let build_message = |row: &rusqlite::Row,
                             contact_name: &Option<String>,
                             contact_phone: &Option<String>,
                             strip: bool|
         -> rusqlite::Result<StoredMessage> {
            let raw_content_json: String = row.get(9)?;
            let (content_json, content) = if strip {
                Self::strip_media_from_content(&raw_content_json)
            } else {
                (
                    raw_content_json.clone(),
                    serde_json::from_str(&raw_content_json).ok(),
                )
            };

            Ok(StoredMessage {
                id: row.get(0)?,
                contact_id: row.get(1)?,
                timestamp: row.get(2)?,
                is_from_me: row.get(3)?,
                is_forwarded: row.get(4)?,
                sender_name: row.get(5)?,
                sender_phone: row.get(6)?,
                contact_name: contact_name.clone(),
                contact_phone: contact_phone.clone(),
                chat_type: row.get(7)?,
                content_type: row.get(8)?,
                content_json,
                content,
                original_text: row.get(10)?,
                translated_text: row.get(11)?,
                source_language: row.get(12)?,
                is_translated: row.get(13)?,
            })
        };

        let messages: Vec<StoredMessage> =
            if let (Some(before), Some(before_id)) = (before_timestamp, before_message_id) {
                stmt.query_map(params![contact_id, before, before, before_id], |row| {
                    build_message(row, &contact_name, &contact_phone, strip_media)
                })?
                .filter_map(|r| r.ok())
                .collect()
            } else if let Some(before) = before_timestamp {
                stmt.query_map(params![contact_id, before], |row| {
                    build_message(row, &contact_name, &contact_phone, strip_media)
                })?
                .filter_map(|r| r.ok())
                .collect()
            } else {
                stmt.query_map(params![contact_id], |row| {
                    build_message(row, &contact_name, &contact_phone, strip_media)
                })?
                .filter_map(|r| r.ok())
                .collect()
            };

        // If we used DESC order with limit, reverse to get chronological order
        if limit.is_some() {
            let mut messages = messages;
            messages.reverse();
            Ok(messages)
        } else {
            Ok(messages)
        }
    }

    /// Get a contact by ID
    pub fn get_contact(&self, contact_id: &str) -> Result<Option<StoredContact>> {
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn.prepare(
            r#"
            SELECT
                id, name, phone, type, last_message_time, unread_count, pinned_at, last_message_preview
            FROM contacts
            WHERE id = ?
            "#,
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
                    pinned_at: row.get(6)?,
                    last_message_preview: row.get(7)?,
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

    /// Record AI usage for a message
    pub fn record_usage(
        &self,
        contact_id: Option<&str>,
        message_id: Option<&str>,
        usage: &UsageInfo,
        operation: &str,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        conn.execute(
            r#"
            INSERT INTO translation_usage 
            (contact_id, message_id, timestamp, input_tokens, cached_input_tokens, output_tokens, cost_usd, operation)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            "#,
            params![
                contact_id,
                message_id,
                timestamp,
                usage.input_tokens,
                usage.cached_input_tokens,
                usage.output_tokens,
                usage.cost_usd,
                operation,
            ],
        )?;

        Ok(())
    }

    /// Get total usage across all conversations
    pub fn get_global_usage(&self) -> Result<UsageInfo> {
        let conn = self.conn.lock().unwrap();

        let result = conn.query_row(
            r#"
            SELECT COALESCE(SUM(input_tokens), 0),
                   COALESCE(SUM(cached_input_tokens), 0),
                   COALESCE(SUM(output_tokens), 0), 
                   COALESCE(SUM(cost_usd), 0.0)
            FROM translation_usage
            "#,
            [],
            |row| {
                Ok(UsageInfo {
                    input_tokens: row.get::<_, i64>(0)? as u32,
                    cached_input_tokens: row.get::<_, i64>(1)? as u32,
                    output_tokens: row.get::<_, i64>(2)? as u32,
                    cost_usd: row.get(3)?,
                })
            },
        )?;

        Ok(result)
    }

    /// Get usage for a specific conversation
    pub fn get_conversation_usage(&self, contact_id: &str) -> Result<UsageInfo> {
        let conn = self.conn.lock().unwrap();

        let result = conn.query_row(
            r#"
            SELECT COALESCE(SUM(input_tokens), 0),
                   COALESCE(SUM(cached_input_tokens), 0),
                   COALESCE(SUM(output_tokens), 0), 
                   COALESCE(SUM(cost_usd), 0.0)
            FROM translation_usage
            WHERE contact_id = ?
            "#,
            params![contact_id],
            |row| {
                Ok(UsageInfo {
                    input_tokens: row.get::<_, i64>(0)? as u32,
                    cached_input_tokens: row.get::<_, i64>(1)? as u32,
                    output_tokens: row.get::<_, i64>(2)? as u32,
                    cost_usd: row.get(3)?,
                })
            },
        )?;

        Ok(result)
    }

    /// Get a cached link preview by URL
    /// Returns None if not cached or if cache is older than max_age_secs
    pub fn get_link_preview(&self, url: &str, max_age_secs: i64) -> Result<Option<LinkPreview>> {
        let conn = self.conn.lock().unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let min_time = now - max_age_secs;

        let result = conn.query_row(
            r#"
            SELECT url, title, description, image_url, site_name, error, fetched_at
            FROM link_previews
            WHERE url = ? AND fetched_at > ?
            "#,
            params![url, min_time],
            |row| {
                Ok(LinkPreview {
                    url: row.get(0)?,
                    title: row.get(1)?,
                    description: row.get(2)?,
                    image_url: row.get(3)?,
                    site_name: row.get(4)?,
                    error: row.get(5)?,
                })
            },
        );

        match result {
            Ok(preview) => Ok(Some(preview)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Save a link preview to cache
    pub fn save_link_preview(&self, preview: &LinkPreview) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        conn.execute(
            r#"
            INSERT OR REPLACE INTO link_previews 
            (url, title, description, image_url, site_name, error, fetched_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#,
            params![
                preview.url,
                preview.title,
                preview.description,
                preview.image_url,
                preview.site_name,
                preview.error,
                now,
            ],
        )?;

        Ok(())
    }

    /// Clear all data from the database (for logout)
    pub fn clear_all(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        conn.execute_batch(
            r#"
            DELETE FROM messages;
            DELETE FROM contacts;
            DELETE FROM translation_usage;
            DELETE FROM link_previews;
            "#,
        )?;

        info!("All data cleared from database");
        Ok(())
    }

    // ==================== OAuth 2.0 Methods ====================

    /// Store a registered OAuth client.
    pub fn oauth_store_client(&self, client: &OAuthClientRegistration) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let redirect_uris_json = serde_json::to_string(&client.redirect_uris)?;

        conn.execute(
            r#"
            INSERT OR REPLACE INTO oauth_clients
            (client_id, client_name, redirect_uris_json, scope, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5)
            "#,
            params![
                client.client_id,
                client.client_name,
                redirect_uris_json,
                client.scope,
                client.created_at,
            ],
        )?;

        Ok(())
    }

    /// Get a registered OAuth client.
    pub fn oauth_get_client(&self, client_id: &str) -> Result<Option<OAuthClientRegistration>> {
        let conn = self.conn.lock().unwrap();

        let result = conn.query_row(
            r#"
            SELECT client_id, client_name, redirect_uris_json, scope, created_at
            FROM oauth_clients
            WHERE client_id = ?
            "#,
            params![client_id],
            |row| {
                let redirect_uris_json: String = row.get(2)?;
                let redirect_uris =
                    serde_json::from_str(&redirect_uris_json).unwrap_or_else(|_| Vec::new());
                Ok(OAuthClientRegistration {
                    client_id: row.get(0)?,
                    client_name: row.get(1)?,
                    redirect_uris,
                    scope: row.get(3)?,
                    created_at: row.get(4)?,
                })
            },
        );

        match result {
            Ok(client) => Ok(Some(client)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// List registered OAuth clients.
    pub fn oauth_list_clients(&self) -> Result<Vec<OAuthClientRegistration>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            r#"
            SELECT client_id, client_name, redirect_uris_json, scope, created_at
            FROM oauth_clients
            ORDER BY created_at DESC, client_id ASC
            "#,
        )?;

        let rows = stmt.query_map([], |row| {
            let redirect_uris_json: String = row.get(2)?;
            let redirect_uris =
                serde_json::from_str(&redirect_uris_json).unwrap_or_else(|_| Vec::new());
            Ok(OAuthClientRegistration {
                client_id: row.get(0)?,
                client_name: row.get(1)?,
                redirect_uris,
                scope: row.get(3)?,
                created_at: row.get(4)?,
            })
        })?;

        Ok(rows.filter_map(|row| row.ok()).collect())
    }

    /// Revoke a registered OAuth client and all tokens/codes issued to it.
    pub fn oauth_revoke_client(&self, client_id: &str) -> Result<bool> {
        let conn = self.conn.lock().unwrap();

        conn.execute(
            "DELETE FROM oauth_pending_auth WHERE client_id = ?",
            params![client_id],
        )?;
        conn.execute(
            "DELETE FROM oauth_authorization_codes WHERE client_id = ?",
            params![client_id],
        )?;
        conn.execute(
            "DELETE FROM oauth_access_tokens WHERE client_id = ?",
            params![client_id],
        )?;
        conn.execute(
            "DELETE FROM oauth_refresh_tokens WHERE client_id = ?",
            params![client_id],
        )?;
        let deleted = conn.execute(
            "DELETE FROM oauth_clients WHERE client_id = ?",
            params![client_id],
        )?;

        Ok(deleted > 0)
    }

    /// Clean up expired OAuth entries (call periodically)
    pub fn oauth_cleanup_expired(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        conn.execute(
            "DELETE FROM oauth_pending_auth WHERE expires_at < ?",
            params![now],
        )?;
        conn.execute(
            "DELETE FROM oauth_authorization_codes WHERE expires_at < ? OR used = 1",
            params![now],
        )?;
        conn.execute(
            "DELETE FROM oauth_access_tokens WHERE expires_at < ?",
            params![now],
        )?;
        conn.execute(
            "DELETE FROM oauth_refresh_tokens WHERE expires_at < ?",
            params![now],
        )?;

        Ok(())
    }

    /// Store a pending authorization request
    pub fn oauth_store_pending_auth(&self, pending: &PendingAuthorization) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        conn.execute(
            r#"
            INSERT OR REPLACE INTO oauth_pending_auth 
            (session_key, client_id, redirect_uri, code_challenge, code_challenge_method, scope, state, created_at, expires_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            "#,
            params![
                pending.session_key,
                pending.client_id,
                pending.redirect_uri,
                pending.code_challenge,
                pending.code_challenge_method,
                pending.scope,
                pending.state,
                pending.created_at,
                pending.expires_at,
            ],
        )?;

        Ok(())
    }

    /// Get and remove a pending authorization
    pub fn oauth_take_pending_auth(
        &self,
        session_key: &str,
    ) -> Result<Option<PendingAuthorization>> {
        let conn = self.conn.lock().unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let result = conn.query_row(
            r#"
            SELECT session_key, client_id, redirect_uri, code_challenge, code_challenge_method, 
                   scope, state, created_at, expires_at
            FROM oauth_pending_auth 
            WHERE session_key = ? AND expires_at > ?
            "#,
            params![session_key, now],
            |row| {
                Ok(PendingAuthorization {
                    session_key: row.get(0)?,
                    client_id: row.get(1)?,
                    redirect_uri: row.get(2)?,
                    code_challenge: row.get(3)?,
                    code_challenge_method: row.get(4)?,
                    scope: row.get(5)?,
                    state: row.get(6)?,
                    created_at: row.get(7)?,
                    expires_at: row.get(8)?,
                })
            },
        );

        match result {
            Ok(pending) => {
                // Delete the pending auth after retrieval
                conn.execute(
                    "DELETE FROM oauth_pending_auth WHERE session_key = ?",
                    params![session_key],
                )?;
                Ok(Some(pending))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Get a pending authorization (without removing it)
    pub fn oauth_get_pending_auth(
        &self,
        session_key: &str,
    ) -> Result<Option<PendingAuthorization>> {
        let conn = self.conn.lock().unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let result = conn.query_row(
            r#"
            SELECT session_key, client_id, redirect_uri, code_challenge, code_challenge_method, 
                   scope, state, created_at, expires_at
            FROM oauth_pending_auth 
            WHERE session_key = ? AND expires_at > ?
            "#,
            params![session_key, now],
            |row| {
                Ok(PendingAuthorization {
                    session_key: row.get(0)?,
                    client_id: row.get(1)?,
                    redirect_uri: row.get(2)?,
                    code_challenge: row.get(3)?,
                    code_challenge_method: row.get(4)?,
                    scope: row.get(5)?,
                    state: row.get(6)?,
                    created_at: row.get(7)?,
                    expires_at: row.get(8)?,
                })
            },
        );

        match result {
            Ok(pending) => Ok(Some(pending)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Store an authorization code
    pub fn oauth_store_authorization_code(&self, code: &AuthorizationCode) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        conn.execute(
            r#"
            INSERT INTO oauth_authorization_codes 
            (code, client_id, redirect_uri, code_challenge, code_challenge_method, scope, created_at, expires_at, used)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            "#,
            params![
                code.code,
                code.client_id,
                code.redirect_uri,
                code.code_challenge,
                code.code_challenge_method,
                code.scope,
                code.created_at,
                code.expires_at,
                code.used,
            ],
        )?;

        Ok(())
    }

    /// Get an authorization code (and mark it as used)
    pub fn oauth_use_authorization_code(&self, code: &str) -> Result<Option<AuthorizationCode>> {
        let conn = self.conn.lock().unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let result = conn.query_row(
            r#"
            SELECT code, client_id, redirect_uri, code_challenge, code_challenge_method, 
                   scope, created_at, expires_at, used
            FROM oauth_authorization_codes 
            WHERE code = ? AND expires_at > ? AND used = 0
            "#,
            params![code, now],
            |row| {
                Ok(AuthorizationCode {
                    code: row.get(0)?,
                    client_id: row.get(1)?,
                    redirect_uri: row.get(2)?,
                    code_challenge: row.get(3)?,
                    code_challenge_method: row.get(4)?,
                    scope: row.get(5)?,
                    created_at: row.get(6)?,
                    expires_at: row.get(7)?,
                    used: row.get::<_, i32>(8)? != 0,
                })
            },
        );

        match result {
            Ok(auth_code) => {
                // Mark as used
                conn.execute(
                    "UPDATE oauth_authorization_codes SET used = 1 WHERE code = ?",
                    params![code],
                )?;
                Ok(Some(auth_code))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Store an access token
    pub fn oauth_store_access_token(&self, token: &AccessToken) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        conn.execute(
            r#"
            INSERT INTO oauth_access_tokens (token, client_id, scope, created_at, expires_at)
            VALUES (?1, ?2, ?3, ?4, ?5)
            "#,
            params![
                token.token,
                token.client_id,
                token.scope,
                token.created_at,
                token.expires_at,
            ],
        )?;

        Ok(())
    }

    /// Validate an access token
    pub fn oauth_validate_access_token(&self, token: &str) -> Result<Option<AccessToken>> {
        let conn = self.conn.lock().unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let result = conn.query_row(
            r#"
            SELECT token, client_id, scope, created_at, expires_at
            FROM oauth_access_tokens 
            WHERE token = ? AND expires_at > ?
              AND EXISTS (
                SELECT 1 FROM oauth_clients
                WHERE oauth_clients.client_id = oauth_access_tokens.client_id
              )
            "#,
            params![token, now],
            |row| {
                Ok(AccessToken {
                    token: row.get(0)?,
                    client_id: row.get(1)?,
                    scope: row.get(2)?,
                    created_at: row.get(3)?,
                    expires_at: row.get(4)?,
                })
            },
        );

        match result {
            Ok(t) => Ok(Some(t)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Store a refresh token
    pub fn oauth_store_refresh_token(&self, token: &RefreshToken) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        conn.execute(
            r#"
            INSERT INTO oauth_refresh_tokens (token, client_id, scope, created_at, expires_at)
            VALUES (?1, ?2, ?3, ?4, ?5)
            "#,
            params![
                token.token,
                token.client_id,
                token.scope,
                token.created_at,
                token.expires_at,
            ],
        )?;

        Ok(())
    }

    /// Validate and get a refresh token
    pub fn oauth_get_refresh_token(&self, token: &str) -> Result<Option<RefreshToken>> {
        let conn = self.conn.lock().unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let result = conn.query_row(
            r#"
            SELECT token, client_id, scope, created_at, expires_at
            FROM oauth_refresh_tokens 
            WHERE token = ? AND expires_at > ?
              AND EXISTS (
                SELECT 1 FROM oauth_clients
                WHERE oauth_clients.client_id = oauth_refresh_tokens.client_id
              )
            "#,
            params![token, now],
            |row| {
                Ok(RefreshToken {
                    token: row.get(0)?,
                    client_id: row.get(1)?,
                    scope: row.get(2)?,
                    created_at: row.get(3)?,
                    expires_at: row.get(4)?,
                })
            },
        );

        match result {
            Ok(t) => Ok(Some(t)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Revoke a token (access or refresh)
    pub fn oauth_revoke_token(&self, token: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        conn.execute(
            "DELETE FROM oauth_access_tokens WHERE token = ?",
            params![token],
        )?;
        conn.execute(
            "DELETE FROM oauth_refresh_tokens WHERE token = ?",
            params![token],
        )?;

        Ok(())
    }

    /// Clear all OAuth tokens (for complete logout)
    pub fn oauth_clear_all(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        conn.execute_batch(
            r#"
            DELETE FROM oauth_pending_auth;
            DELETE FROM oauth_authorization_codes;
            DELETE FROM oauth_access_tokens;
            DELETE FROM oauth_refresh_tokens;
            DELETE FROM oauth_clients;
            "#,
        )?;

        info!("All OAuth tokens cleared from database");
        Ok(())
    }

    // ========== Style Profile Methods ==========

    /// Get a style profile by contact ID (or "__global__" for global profile)
    pub fn get_style_profile(&self, contact_id: &str) -> Result<Option<StyleProfile>> {
        let conn = self.conn.lock().unwrap();

        let result = conn.query_row(
            "SELECT contact_id, profile_text, sample_messages, message_count, updated_at 
             FROM style_profiles WHERE contact_id = ?",
            params![contact_id],
            |row| {
                let sample_messages_json: String = row.get(2)?;
                let sample_messages: Vec<String> =
                    serde_json::from_str(&sample_messages_json).unwrap_or_default();

                Ok(StyleProfile {
                    contact_id: row.get(0)?,
                    profile_text: row.get(1)?,
                    sample_messages,
                    message_count: row.get(3)?,
                    updated_at: row.get(4)?,
                })
            },
        );

        match result {
            Ok(profile) => Ok(Some(profile)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Save or update a style profile
    pub fn save_style_profile(&self, profile: &StyleProfile) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        let sample_messages_json = serde_json::to_string(&profile.sample_messages)?;

        conn.execute(
            r#"
            INSERT INTO style_profiles (contact_id, profile_text, sample_messages, message_count, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(contact_id) DO UPDATE SET
                profile_text = excluded.profile_text,
                sample_messages = excluded.sample_messages,
                message_count = excluded.message_count,
                updated_at = excluded.updated_at
            "#,
            params![
                profile.contact_id,
                profile.profile_text,
                sample_messages_json,
                profile.message_count,
                profile.updated_at,
            ],
        )?;

        info!(
            "Saved style profile for {} ({} messages analyzed)",
            profile.contact_id, profile.message_count
        );

        Ok(())
    }

    /// Get count of outgoing messages (for determining if style profile needs refresh)
    pub fn get_outgoing_message_count(&self, contact_id: Option<&str>) -> Result<i32> {
        let conn = self.conn.lock().unwrap();

        let count: i32 = if let Some(cid) = contact_id {
            conn.query_row(
                "SELECT COUNT(*) FROM messages WHERE is_from_me = 1 AND contact_id = ?",
                params![cid],
                |row| row.get(0),
            )?
        } else {
            conn.query_row(
                "SELECT COUNT(*) FROM messages WHERE is_from_me = 1",
                [],
                |row| row.get(0),
            )?
        };

        Ok(count)
    }

    /// Get outgoing messages for style analysis
    /// If contact_id is None, gets messages across all contacts (for global profile)
    /// Returns messages with text content, ordered by timestamp descending
    pub fn get_outgoing_messages_for_style(
        &self,
        contact_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<StoredMessage>> {
        let conn = self.conn.lock().unwrap();

        let query = if contact_id.is_some() {
            r#"
            SELECT id, contact_id, timestamp, is_from_me, is_forwarded, sender_name,
                   sender_phone, chat_type, content_type, content_json, original_text,
                   translated_text, source_language, is_translated
            FROM messages
            WHERE is_from_me = 1 
              AND contact_id = ?
              AND content_type = 'Text'
              AND (original_text IS NOT NULL OR content_json LIKE '%"body"%')
            ORDER BY timestamp DESC
            LIMIT ?
            "#
        } else {
            r#"
            SELECT id, contact_id, timestamp, is_from_me, is_forwarded, sender_name,
                   sender_phone, chat_type, content_type, content_json, original_text,
                   translated_text, source_language, is_translated
            FROM messages
            WHERE is_from_me = 1 
              AND content_type = 'Text'
              AND (original_text IS NOT NULL OR content_json LIKE '%"body"%')
            ORDER BY timestamp DESC
            LIMIT ?
            "#
        };

        let mut stmt = conn.prepare(query)?;

        let messages: Vec<StoredMessage> = if let Some(cid) = contact_id {
            stmt.query_map(params![cid, limit as i64], |row| {
                Self::row_to_stored_message(row, None, None)
            })?
            .filter_map(|r| r.ok())
            .collect()
        } else {
            stmt.query_map(params![limit as i64], |row| {
                Self::row_to_stored_message(row, None, None)
            })?
            .filter_map(|r| r.ok())
            .collect()
        };

        Ok(messages)
    }

    /// Get message exchange pairs for style learning
    /// Returns pairs of (incoming message, my reply) to show conversation patterns
    /// This helps AI learn not just what you say, but how you respond to different inputs
    pub fn get_message_exchange_pairs(
        &self,
        contact_id: &str,
        limit: usize,
    ) -> Result<Vec<(StoredMessage, StoredMessage)>> {
        let conn = self.conn.lock().unwrap();

        // Get recent messages for this contact, ordered by timestamp
        let query = r#"
            SELECT id, contact_id, timestamp, is_from_me, is_forwarded, sender_name,
                   sender_phone, chat_type, content_type, content_json, original_text,
                   translated_text, source_language, is_translated
            FROM messages
            WHERE contact_id = ?
              AND content_type = 'Text'
              AND (original_text IS NOT NULL OR content_json LIKE '%"body"%')
            ORDER BY timestamp DESC
            LIMIT ?
        "#;

        let mut stmt = conn.prepare(query)?;
        let messages: Vec<StoredMessage> = stmt
            .query_map(params![contact_id, (limit * 3) as i64], |row| {
                Self::row_to_stored_message(row, None, None)
            })?
            .filter_map(|r| r.ok())
            .collect();

        // Find pairs: look for my messages that follow their messages
        let mut pairs = Vec::new();
        let messages_sorted: Vec<_> = messages.into_iter().rev().collect(); // chronological order

        for i in 1..messages_sorted.len() {
            let prev = &messages_sorted[i - 1];
            let curr = &messages_sorted[i];

            // If previous message is from them and current is from me, it's an exchange
            if !prev.is_from_me && curr.is_from_me {
                pairs.push((prev.clone(), curr.clone()));
                if pairs.len() >= limit {
                    break;
                }
            }
        }

        Ok(pairs)
    }

    /// Helper to convert a row to StoredMessage (used by multiple methods)
    fn row_to_stored_message(
        row: &rusqlite::Row,
        contact_name: Option<String>,
        contact_phone: Option<String>,
    ) -> rusqlite::Result<StoredMessage> {
        let content_json: String = row.get(9)?;
        let content = serde_json::from_str(&content_json).ok();

        Ok(StoredMessage {
            id: row.get(0)?,
            contact_id: row.get(1)?,
            timestamp: row.get(2)?,
            is_from_me: row.get::<_, i32>(3)? != 0,
            is_forwarded: row.get::<_, i32>(4).unwrap_or(0) != 0,
            sender_name: row.get(5)?,
            sender_phone: row.get(6)?,
            contact_name,
            contact_phone,
            chat_type: row
                .get::<_, Option<String>>(7)?
                .unwrap_or_else(|| "private".to_string()),
            content_type: row.get(8)?,
            content_json,
            content,
            original_text: row.get(10)?,
            translated_text: row.get(11)?,
            source_language: row.get(12)?,
            is_translated: row.get::<_, i32>(13).unwrap_or(0) != 0,
        })
    }

    /// Get a specific message by ID
    pub fn get_message_by_id(&self, message_id: &str) -> Result<Option<StoredMessage>> {
        let conn = self.conn.lock().unwrap();

        let result = conn.query_row(
            r#"
            SELECT m.id, m.contact_id, m.timestamp, m.is_from_me, m.is_forwarded, m.sender_name,
                   m.sender_phone, m.chat_type, m.content_type, m.content_json, m.original_text,
                   m.translated_text, m.source_language, m.is_translated,
                   c.name as contact_name, c.phone as contact_phone
            FROM messages m
            LEFT JOIN contacts c ON m.contact_id = c.id
            WHERE m.id = ?
            "#,
            params![message_id],
            |row| {
                let contact_name: Option<String> = row.get(14)?;
                let contact_phone: Option<String> = row.get(15)?;
                Self::row_to_stored_message(row, contact_name, contact_phone)
            },
        );

        match result {
            Ok(msg) => Ok(Some(msg)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Get recent messages for a contact (for conversation context)
    pub fn get_recent_messages(
        &self,
        contact_id: &str,
        limit: usize,
    ) -> Result<Vec<StoredMessage>> {
        let conn = self.conn.lock().unwrap();

        // Get contact info
        let contact_info: Option<(Option<String>, Option<String>)> = conn
            .query_row(
                "SELECT name, phone FROM contacts WHERE id = ?",
                params![contact_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .ok();

        let (contact_name, contact_phone) = contact_info.unwrap_or((None, None));

        let mut stmt = conn.prepare(
            r#"
            SELECT id, contact_id, timestamp, is_from_me, is_forwarded, sender_name,
                   sender_phone, chat_type, content_type, content_json, original_text,
                   translated_text, source_language, is_translated
            FROM messages
            WHERE contact_id = ?
            ORDER BY timestamp DESC, id DESC
            LIMIT ?
            "#,
        )?;

        let rows = stmt.query_map(params![contact_id, limit as i64], |row| {
            Self::row_to_stored_message(row, contact_name.clone(), contact_phone.clone())
        })?;

        // Collect and reverse to get chronological order
        let mut messages: Vec<StoredMessage> = rows.filter_map(|r| r.ok()).collect();
        messages.reverse();
        Ok(messages)
    }
}

impl Clone for MessageStore {
    fn clone(&self) -> Self {
        Self {
            conn: Arc::clone(&self.conn),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn test_store() -> (MessageStore, PathBuf) {
        let data_dir = std::env::temp_dir().join(format!(
            "whatsapp-translator-storage-test-{}",
            uuid::Uuid::new_v4()
        ));
        let store = MessageStore::new(&data_dir).expect("test store");
        (store, data_dir)
    }

    fn test_message(id: &str, timestamp: i64) -> StoredMessage {
        let content = serde_json::json!({
            "type": "text",
            "body": id,
        });

        StoredMessage {
            id: id.to_string(),
            contact_id: "chat@example.test".to_string(),
            timestamp,
            is_from_me: false,
            is_forwarded: false,
            sender_name: Some("Tester".to_string()),
            sender_phone: None,
            contact_name: Some("Test Chat".to_string()),
            contact_phone: None,
            chat_type: "private".to_string(),
            content_type: "Text".to_string(),
            content_json: content.to_string(),
            content: Some(content),
            original_text: Some(id.to_string()),
            translated_text: None,
            source_language: None,
            is_translated: false,
        }
    }

    fn message_ids(messages: &[StoredMessage]) -> Vec<String> {
        messages.iter().map(|message| message.id.clone()).collect()
    }

    fn test_oauth_client(client_id: &str) -> OAuthClientRegistration {
        OAuthClientRegistration {
            client_id: client_id.to_string(),
            client_name: Some("Test MCP Client".to_string()),
            redirect_uris: vec!["http://127.0.0.1:9000/callback".to_string()],
            scope: "mcp".to_string(),
            created_at: 1_700_000_000,
        }
    }

    fn insert_test_contact(store: &MessageStore) {
        store
            .upsert_contact(
                "chat@example.test",
                Some("Test Chat"),
                None,
                Some("private"),
                1_700_000_000,
            )
            .expect("insert contact");
    }

    #[test]
    fn conversation_settings_persist_original_follow_up_preference() {
        let (store, data_dir) = test_store();
        insert_test_contact(&store);

        let settings = ConversationSettings {
            language_override: Some("Spanish".to_string()),
            translation_style: Some("friendly".to_string()),
            send_original_follow_up: true,
        };
        store
            .update_conversation_settings("chat@example.test", &settings)
            .expect("save settings");

        let loaded = store
            .get_conversation_settings("chat@example.test")
            .expect("load settings");
        assert!(loaded.send_original_follow_up);

        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[test]
    fn openai_settings_round_trip_globally() {
        let (store, data_dir) = test_store();
        let settings = OpenAiSettings {
            model: Some("gpt-5.6-terra".to_string()),
            reasoning_effort: Some("high".to_string()),
        };

        store
            .update_openai_settings(&settings)
            .expect("save OpenAI settings");

        assert_eq!(
            store.get_openai_settings().expect("load OpenAI settings"),
            settings
        );

        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[test]
    fn push_device_registration_replaces_the_token_for_an_installation() {
        let (store, data_dir) = test_store();

        store
            .register_push_device("installation-1", "token-old", "sandbox")
            .expect("register first token");
        store
            .register_push_device("installation-1", "token-new", "production")
            .expect("replace token");

        let devices = store.list_push_devices().expect("list push devices");
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].installation_id, "installation-1");
        assert_eq!(devices[0].token, "token-new");
        assert_eq!(devices[0].environment, "production");

        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[test]
    fn paginated_messages_use_id_cursor_when_timestamps_match() {
        let (store, data_dir) = test_store();
        insert_test_contact(&store);

        for id in ["m01", "m02", "m03", "m04"] {
            store
                .add_message(&test_message(id, 1_700_000_000))
                .expect("insert message");
        }

        let first_page = store
            .get_messages_paginated("chat@example.test", Some(2), None, None, true)
            .expect("first page");
        assert_eq!(message_ids(&first_page), vec!["m03", "m04"]);

        let second_page = store
            .get_messages_paginated(
                "chat@example.test",
                Some(2),
                Some(first_page[0].timestamp),
                Some(&first_page[0].id),
                true,
            )
            .expect("second page");
        assert_eq!(message_ids(&second_page), vec!["m01", "m02"]);

        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[test]
    fn replace_message_id_reconciles_pending_outgoing_message() {
        let (store, data_dir) = test_store();
        insert_test_contact(&store);
        store
            .add_message(&test_message("pending_1", 1_700_000_000_000))
            .expect("insert pending");

        store
            .replace_message_id("pending_1", "real_1", Some(1_700_000_005_000))
            .expect("replace id");

        let messages = store
            .get_messages_paginated("chat@example.test", None, None, None, true)
            .expect("messages");
        assert_eq!(message_ids(&messages), vec!["real_1"]);
        assert_eq!(messages[0].timestamp, 1_700_000_005_000);

        let contact = store
            .get_contact("chat@example.test")
            .expect("contact query")
            .expect("contact");
        assert_eq!(contact.last_message_time, 1_700_000_005_000);
        assert_eq!(contact.last_message_preview.as_deref(), Some("pending_1"));

        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[test]
    fn delete_message_refreshes_contact_preview_to_previous_message() {
        let (store, data_dir) = test_store();
        insert_test_contact(&store);
        store
            .add_message(&test_message("older", 1_700_000_000_000))
            .expect("insert older");
        store
            .add_message(&test_message("pending_1", 1_700_000_005_000))
            .expect("insert pending");

        assert!(store.delete_message("pending_1").expect("delete"));

        let messages = store
            .get_messages_paginated("chat@example.test", None, None, None, true)
            .expect("messages");
        assert_eq!(message_ids(&messages), vec!["older"]);

        let contact = store
            .get_contact("chat@example.test")
            .expect("contact query")
            .expect("contact");
        assert_eq!(contact.last_message_time, 1_700_000_000_000);
        assert_eq!(contact.last_message_preview.as_deref(), Some("older"));

        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[test]
    fn incoming_translated_message_uses_translation_for_contact_preview() {
        let (store, data_dir) = test_store();
        insert_test_contact(&store);
        let mut message = test_message("Szia, hogy vagy?", 1_700_000_000_000);
        message.translated_text = Some("Hi, how are you?".to_string());
        message.source_language = Some("Hungarian".to_string());
        message.is_translated = true;

        store
            .add_message(&message)
            .expect("insert translated message");

        let contact = store
            .get_contact("chat@example.test")
            .expect("contact query")
            .expect("contact");
        assert_eq!(
            contact.last_message_preview.as_deref(),
            Some("Hi, how are you?")
        );

        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[test]
    fn translating_latest_message_refreshes_contact_preview() {
        let (store, data_dir) = test_store();
        insert_test_contact(&store);
        store
            .add_message(&test_message("Szia, hogy vagy?", 1_700_000_000_000))
            .expect("insert untranslated message");

        store
            .update_message_translation(
                "Szia, hogy vagy?",
                Some("Hi, how are you?"),
                Some("Hungarian"),
            )
            .expect("translate message");

        let contact = store
            .get_contact("chat@example.test")
            .expect("contact query")
            .expect("contact");
        assert_eq!(
            contact.last_message_preview.as_deref(),
            Some("Hi, how are you?")
        );

        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[test]
    fn startup_migration_refreshes_existing_translated_contact_previews() {
        let (store, data_dir) = test_store();
        insert_test_contact(&store);
        let mut message = test_message("Szia, hogy vagy?", 1_700_000_000_000);
        message.translated_text = Some("Hi, how are you?".to_string());
        message.source_language = Some("Hungarian".to_string());
        message.is_translated = true;
        store
            .add_message(&message)
            .expect("insert translated message");
        {
            let conn = store.conn.lock().expect("database lock");
            conn.execute(
                "UPDATE contacts SET last_message_preview = ? WHERE id = ?",
                params!["Szia, hogy vagy?", "chat@example.test"],
            )
            .expect("restore legacy preview");
            conn.execute(
                "DELETE FROM app_settings WHERE key = ?",
                params!["translated_contact_previews_version"],
            )
            .expect("reset migration marker");
        }
        drop(store);

        let reopened = MessageStore::new(&data_dir).expect("reopen store");
        let contact = reopened
            .get_contact("chat@example.test")
            .expect("contact query")
            .expect("contact");
        assert_eq!(
            contact.last_message_preview.as_deref(),
            Some("Hi, how are you?")
        );

        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[test]
    fn replace_message_id_removes_pending_duplicate_when_real_message_exists() {
        let (store, data_dir) = test_store();
        insert_test_contact(&store);
        store
            .add_message(&test_message("pending_1", 1_700_000_000_000))
            .expect("insert pending");
        store
            .add_message(&test_message("real_1", 1_700_000_006_000))
            .expect("insert real");

        store
            .replace_message_id("pending_1", "real_1", Some(1_700_000_005_000))
            .expect("replace duplicate");

        let messages = store
            .get_messages_paginated("chat@example.test", None, None, None, true)
            .expect("messages");
        assert_eq!(message_ids(&messages), vec!["real_1"]);
        assert_eq!(messages[0].timestamp, 1_700_000_006_000);

        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[test]
    fn oauth_tokens_require_persisted_client_and_revoke_with_client() {
        let (store, data_dir) = test_store();
        let access_token = AccessToken {
            token: "access-token".to_string(),
            client_id: "client_test".to_string(),
            scope: "mcp".to_string(),
            created_at: 1_700_000_000,
            expires_at: i64::MAX,
        };
        let refresh_token = RefreshToken {
            token: "refresh-token".to_string(),
            client_id: "client_test".to_string(),
            scope: "mcp".to_string(),
            created_at: 1_700_000_000,
            expires_at: i64::MAX,
        };

        store
            .oauth_store_access_token(&access_token)
            .expect("store access token");
        store
            .oauth_store_refresh_token(&refresh_token)
            .expect("store refresh token");
        assert!(store
            .oauth_validate_access_token("access-token")
            .expect("validate without client")
            .is_none());

        store
            .oauth_store_client(&test_oauth_client("client_test"))
            .expect("store client");
        assert!(store
            .oauth_validate_access_token("access-token")
            .expect("validate with client")
            .is_some());
        assert!(store
            .oauth_get_refresh_token("refresh-token")
            .expect("refresh with client")
            .is_some());

        assert!(store
            .oauth_revoke_client("client_test")
            .expect("revoke client"));
        assert!(store
            .oauth_validate_access_token("access-token")
            .expect("validate after revoke")
            .is_none());
        assert!(store
            .oauth_get_refresh_token("refresh-token")
            .expect("refresh after revoke")
            .is_none());

        let _ = std::fs::remove_dir_all(data_dir);
    }
}
