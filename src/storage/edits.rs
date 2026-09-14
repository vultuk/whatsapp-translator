use super::*;
use rusqlite::Transaction;
use serde_json::Value;

impl StoredMessage {
    pub fn edit_revision(&self) -> i64 {
        serde_json::from_str::<Value>(&self.content_json)
            .ok()
            .and_then(|v| v["edited_at_ms"].as_i64())
            .unwrap_or(0)
    }
}

impl MessageStore {
    /// Keep the latest edit even if reconnect/history delivers it before its target.
    pub fn record_message_edit(
        &self,
        edit: &StoredMessage,
        edited_at_ms: i64,
    ) -> Result<Option<StoredMessage>> {
        let content: Value = serde_json::from_str(&edit.content_json)?;
        if edited_at_ms <= 0
            || !matches!(
                content["type"].as_str(),
                Some("text" | "image" | "video" | "document")
            )
        {
            return Ok(None);
        }
        let contact_id = canonical_chat_id(&edit.contact_id);
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let wrong_target: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM messages WHERE id=?1 AND
             (contact_id<>?2 OR is_from_me<>?3 OR (is_from_me=0 AND sender_phone IS NOT ?4)))",
            params![edit.id, contact_id, edit.is_from_me, edit.sender_phone],
            |row| row.get(0),
        )?;
        if wrong_target {
            return Ok(None);
        }
        let inserted = tx.execute(
            "INSERT INTO message_edits(message_id,contact_id,is_from_me,sender_phone,edited_at_ms,content_json,original_text)
             VALUES (?1,?2,?3,?4,?5,?6,?7)
             ON CONFLICT(message_id) DO UPDATE SET edited_at_ms=excluded.edited_at_ms,content_json=excluded.content_json,original_text=excluded.original_text
             WHERE excluded.edited_at_ms > message_edits.edited_at_ms AND excluded.contact_id=message_edits.contact_id
             AND excluded.is_from_me=message_edits.is_from_me AND excluded.sender_phone IS message_edits.sender_phone",
            params![edit.id, contact_id, edit.is_from_me, edit.sender_phone, edited_at_ms, edit.content_json, edit.original_text],
        )?;
        let applied = inserted > 0 && Self::apply_pending_edit(&tx, &edit.id)?;
        tx.commit()?;
        drop(conn);
        if applied {
            self.get_message_by_id(&edit.id)
        } else {
            Ok(None)
        }
    }

    pub(super) fn apply_pending_edit(tx: &Transaction<'_>, id: &str) -> Result<bool> {
        let pending = tx.query_row(
            "SELECT m.content_json,e.content_json,e.original_text,e.edited_at_ms,m.contact_id,m.is_from_me
             FROM messages m JOIN message_edits e ON e.message_id=m.id
             WHERE m.id=? AND m.contact_id=e.contact_id AND m.is_from_me=e.is_from_me
             AND (m.is_from_me=1 OR m.sender_phone IS e.sender_phone)",
            params![id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, Option<String>>(2)?, row.get::<_, i64>(3)?, row.get::<_, String>(4)?, row.get::<_, bool>(5)?)),
        ).optional()?;
        let Some((original, edit, text, revision, contact, from_me)) = pending else {
            return Ok(false);
        };
        let mut original: Value = serde_json::from_str(&original)?;
        let edit: Value = serde_json::from_str(&edit)?;
        if original["edited_at_ms"].as_i64().unwrap_or(0) >= revision
            || original["type"] != edit["type"]
        {
            return Ok(false);
        }
        // A caption edit must preserve downloaded media, album and reply metadata.
        let field = if edit["type"] == "text" {
            "body"
        } else {
            "caption"
        };
        let Some(object) = original.as_object_mut() else {
            return Ok(false);
        };
        object.insert(field.into(), edit[field].clone());
        object.insert("mentions".into(), edit["mentions"].clone());
        object.insert("edited_at_ms".into(), Value::from(revision));
        tx.execute(
            "UPDATE messages SET content_json=?,original_text=?,translated_text=NULL,source_language=NULL,is_translated=0 WHERE id=?",
            params![original.to_string(), text, id],
        )?;
        tx.execute(
            "DELETE FROM translation_jobs WHERE message_id=?",
            params![id],
        )?;
        if !from_me && text.as_ref().is_some_and(|text| !text.trim().is_empty()) {
            tx.execute(
                "INSERT INTO translation_jobs(message_id) SELECT ?1 FROM contacts WHERE id=?2 AND translation_enabled=1",
                params![id, contact],
            )?;
        } else {
            tx.execute(
                "UPDATE pending_notifications SET requires_translation=0 WHERE message_id=?",
                params![id],
            )?;
        }
        Self::refresh_contact_last_message(tx, &contact)?;
        Ok(true)
    }
}
