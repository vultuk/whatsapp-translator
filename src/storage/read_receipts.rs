use super::*;

#[derive(Debug)]
pub struct PendingReadReceipt {
    pub message_id: String,
    pub contact_id: String,
    pub sender_jid: Option<String>,
    pub viewed_at: i64,
}

impl MessageStore {
    pub(super) fn init_read_receipts(&self) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        tx.execute_batch("CREATE TABLE IF NOT EXISTS viewed_messages (
            message_id TEXT PRIMARY KEY REFERENCES messages(id) ON DELETE CASCADE,
            contact_id TEXT NOT NULL, sender_jid TEXT, viewed_at INTEGER NOT NULL,
            sent_at INTEGER, attempted_at INTEGER NOT NULL DEFAULT 0
        );
        CREATE INDEX IF NOT EXISTS idx_pending_read_receipts ON viewed_messages(sent_at,attempted_at);
        CREATE TABLE IF NOT EXISTS unread_messages (
            message_id TEXT PRIMARY KEY REFERENCES messages(id) ON DELETE CASCADE
        );")?;
        // Preserve the existing unread snapshot once, without treating old history as unread.
        if !tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM app_settings WHERE key='message_read_ledger_v1')",
            [],
            |r| r.get::<_, bool>(0),
        )? {
            tx.execute("INSERT OR IGNORE INTO unread_messages(message_id)
                SELECT id FROM (SELECT m.id, c.unread_count,
                    ROW_NUMBER() OVER(PARTITION BY m.contact_id ORDER BY m.timestamp DESC,m.id DESC) AS position
                    FROM messages m JOIN contacts c ON c.id=m.contact_id
                    WHERE m.is_from_me=0 AND lower(m.content_type) NOT IN ('reaction','revoked','protocol','unknown'))
                WHERE position<=unread_count", [])?;
            tx.execute(
                "INSERT INTO app_settings(key,value) VALUES('message_read_ledger_v1','1')",
                [],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Validate the whole batch before recording any view. IDs alone never come from translation.
    pub fn record_message_views(&self, contact_id: &str, ids: &[String], now: i64) -> Result<i32> {
        anyhow::ensure!(
            !ids.is_empty() && ids.len() <= 200,
            "Supply between 1 and 200 viewed message IDs"
        );
        let contact_id = canonical_chat_id(contact_id);
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let mut targets = Vec::new();
        for id in ids {
            let (chat, own, kind, payload, phone):(String,bool,String,String,Option<String>)=tx.query_row(
                "SELECT contact_id,is_from_me,content_type,content_json,sender_phone FROM messages WHERE id=?", [id],
                |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?)))
                .with_context(|| "Unknown viewed message")?;
            let content: serde_json::Value = serde_json::from_str(&payload)?;
            let kind = content
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or(&kind)
                .to_lowercase();
            anyhow::ensure!(
                canonical_chat_id(&chat) == contact_id
                    && !own
                    && !matches!(
                        kind.as_str(),
                        "reaction" | "revoked" | "protocol" | "unknown"
                    ),
                "Only incoming messages in this conversation can be marked viewed"
            );
            anyhow::ensure!(
                !chat.ends_with("@broadcast") && !chat.ends_with("@newsletter"),
                "Read receipts are supported for chats and groups"
            );
            let sender = content
                .get("receipt_sender_jid")
                .and_then(|v| v.as_str())
                .filter(|v| v.contains('@') && !v.starts_with('@'))
                .map(str::to_owned)
                .or_else(|| {
                    phone.filter(|v| !v.is_empty()).map(|v| {
                        if v.contains('@') {
                            v
                        } else {
                            format!("{v}@s.whatsapp.net")
                        }
                    })
                });
            targets.push((id, sender));
        }
        let mut removed = 0;
        for (id, sender) in targets {
            tx.execute("INSERT OR IGNORE INTO viewed_messages(message_id,contact_id,sender_jid,viewed_at) VALUES (?1,?2,?3,?4)",params![id,contact_id.as_ref(),sender,now])?;
            removed += tx.execute("DELETE FROM unread_messages WHERE message_id=?", [id])?;
            tx.execute("DELETE FROM pending_notifications WHERE message_id=?", [id])?;
        }
        tx.execute(
            "UPDATE contacts SET unread_count=MAX(0,unread_count-?1) WHERE id=?2",
            params![removed, contact_id.as_ref()],
        )?;
        let count = tx.query_row(
            "SELECT unread_count FROM contacts WHERE id=?",
            [contact_id.as_ref()],
            |r| r.get(0),
        )?;
        tx.commit()?;
        Ok(count)
    }

    pub fn pending_read_receipts(&self, now: i64) -> Result<Vec<PendingReadReceipt>> {
        let conn = self.conn.lock().unwrap();
        let mut statement=conn.prepare("SELECT v.message_id,m.contact_id,v.sender_jid,v.viewed_at FROM viewed_messages v JOIN messages m ON m.id=v.message_id
            WHERE v.sent_at IS NULL AND v.attempted_at<=?1 AND (m.contact_id NOT LIKE '%@g.us' OR v.sender_jid IS NOT NULL)
            AND m.is_from_me=0 AND lower(COALESCE(json_extract(m.content_json,'$.type'),m.content_type)) NOT IN ('reaction','revoked','protocol','unknown')
            ORDER BY v.viewed_at,v.message_id LIMIT 50")?;
        let rows = statement.query_map([now - 15], |r| {
            Ok(PendingReadReceipt {
                message_id: r.get(0)?,
                contact_id: r.get(1)?,
                sender_jid: r.get(2)?,
                viewed_at: r.get(3)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }
    pub fn read_receipt_attempted(&self, id: &str, now: i64) -> Result<()> {
        self.conn.lock().unwrap().execute(
            "UPDATE viewed_messages SET attempted_at=?1 WHERE message_id=?2",
            params![now, id],
        )?;
        Ok(())
    }
    pub fn read_receipt_sent(&self, id: &str, now: i64) -> Result<()> {
        self.conn.lock().unwrap().execute(
            "UPDATE viewed_messages SET sent_at=?1 WHERE message_id=?2",
            params![now, id],
        )?;
        Ok(())
    }
}
