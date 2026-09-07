use super::*;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutboxEntry {
    pub id: String,
    #[serde(skip_serializing)]
    pub fingerprint: String,
    pub path: String,
    pub state: String,
    pub status_code: Option<u16>,
    pub response: Option<String>,
    pub created_at: i64,
}

impl MessageStore {
    pub fn unfinished_outbox_ids(&self) -> Result<Vec<String>> {
        let conn = self.conn.lock().unwrap();
        let mut query = conn.prepare("SELECT id FROM outbox WHERE state='uncertain'")?;
        let rows = query
            .query_map([], |row| row.get(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }
    pub fn recoverable_send_receipts(&self) -> Result<Vec<(Option<String>, String)>> {
        let conn = self.conn.lock().unwrap();
        let mut query=conn.prepare("SELECT temp_message_id,result FROM outbox_attempts WHERE state='confirmed' AND result IS NOT NULL AND (outbox_id IN (SELECT id FROM outbox WHERE state='uncertain') OR temp_message_id IN (SELECT id FROM messages))")?;
        let rows = query
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }
    pub fn mcp_send_record(&self, id: &str) -> Result<Option<serde_json::Value>> {
        let payload: Option<String> = self
            .conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT payload FROM mcp_send_records WHERE id=?",
                params![id],
                |row| row.get(0),
            )
            .optional()?;
        payload
            .map(|value| serde_json::from_str(&value).map_err(Into::into))
            .transpose()
    }
    pub fn claim_mcp_send(&self, id: &str, payload: &serde_json::Value) -> Result<bool> {
        Ok(self.conn.lock().unwrap().execute(
            "INSERT OR IGNORE INTO mcp_send_records(id,payload) VALUES(?,?)",
            params![id, payload.to_string()],
        )? == 1)
    }
    pub fn update_mcp_send(&self, id: &str, payload: &serde_json::Value) -> Result<()> {
        self.conn.lock().unwrap().execute(
            "UPDATE mcp_send_records SET payload=? WHERE id=?",
            params![payload.to_string(), id],
        )?;
        Ok(())
    }
    pub fn recover_outbox(&self) -> Result<i32> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE outbox SET state='uncertain' WHERE state='sending'",
            [],
        )?;
        conn.execute(
            "UPDATE outbox_attempts SET state='uncertain' WHERE state='sending'",
            [],
        )?;
        conn.execute("UPDATE messages SET delivery_status='uncertain' WHERE id IN (SELECT temp_message_id FROM outbox_attempts WHERE state='uncertain')", [])?;
        Ok(conn.query_row(
            "SELECT COALESCE(MAX(request_id),0)+1 FROM outbox_attempts",
            [],
            |row| row.get(0),
        )?)
    }
    pub fn claim_outbox(&self, id: &str, fingerprint: &str, path: &str) -> Result<bool> {
        Ok(self.conn.lock().unwrap().execute(
            "INSERT OR IGNORE INTO outbox(id,fingerprint,path,created_at) VALUES(?,?,?,?)",
            params![id, fingerprint, path, chrono::Utc::now().timestamp()],
        )? == 1)
    }
    pub fn outbox_entry(&self, id: &str) -> Result<Option<OutboxEntry>> {
        Ok(self.conn.lock().unwrap().query_row("SELECT id,fingerprint,path,state,status_code,response,created_at FROM outbox WHERE id=?", params![id], |row| Ok(OutboxEntry { id:row.get(0)?,fingerprint:row.get(1)?,path:row.get(2)?,state:row.get(3)?,status_code:row.get(4)?,response:row.get(5)?,created_at:row.get(6)? })).optional()?)
    }
    pub fn finish_outbox(&self, id: &str, status: u16, response: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let uncertain: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM outbox_attempts WHERE outbox_id=? AND state IN ('sending','uncertain'))",params![id],|row| row.get(0))?;
        let state = if uncertain {
            "uncertain"
        } else if status < 300 {
            "confirmed"
        } else {
            "failed"
        };
        // A late confirmation may have arrived between the handler timeout and this write.
        conn.execute(
            "UPDATE outbox SET state=?,status_code=?,response=? WHERE id=? AND state!='confirmed'",
            params![state, status, response, id],
        )?;
        Ok(())
    }
    pub fn register_outbox_attempt(
        &self,
        request_id: i32,
        outbox_id: Option<&str>,
        temp_id: Option<&str>,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        if let Some(id) = outbox_id.filter(|id| !id.is_empty()) {
            let exists: bool = conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM outbox WHERE id=?)",
                params![id],
                |row| row.get(0),
            )?;
            anyhow::ensure!(
                exists,
                "Send operation expired after logout; nothing was sent."
            );
        }
        conn.execute(
            "INSERT INTO outbox_attempts(request_id,outbox_id,temp_message_id) VALUES(?,?,?)",
            params![request_id, outbox_id, temp_id],
        )?;
        Ok(())
    }
    pub fn uncertain_attempt(&self, request_id: i32) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE outbox_attempts SET state='uncertain' WHERE request_id=? AND state='sending'",
            params![request_id],
        )?;
        conn.execute("UPDATE outbox SET state='uncertain' WHERE id=(SELECT outbox_id FROM outbox_attempts WHERE request_id=?) AND state='sending'",params![request_id])?;
        let temp: Option<String> = conn
            .query_row(
                "SELECT temp_message_id FROM outbox_attempts WHERE request_id=?",
                params![request_id],
                |row| row.get(0),
            )
            .optional()?
            .flatten();
        if let Some(temp) = &temp {
            conn.execute(
                "UPDATE messages SET delivery_status='uncertain' WHERE id=?",
                params![temp],
            )?;
        }
        Ok(temp)
    }
    pub fn complete_attempt(
        &self,
        request_id: i32,
        success: bool,
        result: &str,
    ) -> Result<Option<(Option<String>, Option<String>)>> {
        let conn = self.conn.lock().unwrap();
        let metadata = conn
            .query_row(
                "SELECT outbox_id,temp_message_id FROM outbox_attempts WHERE request_id=?",
                params![request_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        conn.execute(
            "UPDATE outbox_attempts SET state=?,result=? WHERE request_id=?",
            params![
                if success { "confirmed" } else { "uncertain" },
                result,
                request_id
            ],
        )?;
        Ok(metadata)
    }
    pub fn has_outbox_attempt(&self, request_id: i32) -> Result<bool> {
        Ok(self.conn.lock().unwrap().query_row(
            "SELECT EXISTS(SELECT 1 FROM outbox_attempts WHERE request_id=?)",
            params![request_id],
            |row| row.get(0),
        )?)
    }
    pub fn set_expected_send_count(&self, request_id: i32, count: usize) -> Result<()> {
        self.conn.lock().unwrap().execute(
            "UPDATE outbox_attempts SET expected_count=? WHERE request_id=?",
            params![count, request_id],
        )?;
        Ok(())
    }
    pub fn expected_send_count(&self, request_id: i32) -> Result<usize> {
        Ok(self
            .conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT expected_count FROM outbox_attempts WHERE request_id=?",
                params![request_id],
                |row| row.get(0),
            )
            .optional()?
            .unwrap_or(1))
    }
    pub fn attach_voice_attempt(
        &self,
        request_id: i32,
        note_id: &str,
        original: bool,
    ) -> Result<()> {
        self.conn.lock().unwrap().execute(
            "UPDATE outbox_attempts SET voice_note_id=?,voice_original=? WHERE request_id=?",
            params![note_id, original, request_id],
        )?;
        Ok(())
    }
    pub fn voice_attempt(&self, request_id: i32) -> Result<Option<(String, bool)>> {
        Ok(self.conn.lock().unwrap().query_row("SELECT voice_note_id,voice_original FROM outbox_attempts WHERE request_id=? AND voice_note_id IS NOT NULL",params![request_id],|row|Ok((row.get(0)?,row.get(1)?))).optional()?)
    }
    pub fn cancel_outbox_attempt(&self, request_id: i32) -> Result<()> {
        self.conn.lock().unwrap().execute(
            "UPDATE outbox_attempts SET state='failed' WHERE request_id=?",
            params![request_id],
        )?;
        Ok(())
    }
    pub fn outbox_results(&self, id: &str) -> Result<Vec<serde_json::Value>> {
        let conn = self.conn.lock().unwrap();
        let mut statement = conn
            .prepare("SELECT result FROM outbox_attempts WHERE outbox_id=? ORDER BY request_id")?;
        let rows = statement
            .query_map(params![id], |row| row.get::<_, Option<String>>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows
            .into_iter()
            .filter_map(|row| row.and_then(|row| serde_json::from_str(&row).ok()))
            .collect())
    }
    pub fn confirm_late_outbox(&self, id: &str, response: &str) -> Result<()> {
        self.conn.lock().unwrap().execute(
            "UPDATE outbox SET state='confirmed',status_code=200,response=? WHERE id=?",
            params![response, id],
        )?;
        Ok(())
    }
    pub fn set_delivery_status(&self, id: &str, status: &str) -> Result<()> {
        self.conn.lock().unwrap().execute(
            "UPDATE messages SET delivery_status=? WHERE id=?",
            params![status, id],
        )?;
        Ok(())
    }
    pub fn enqueue_translation(&self, message_id: &str) -> Result<()> {
        self.conn.lock().unwrap().execute(
            "INSERT OR IGNORE INTO translation_jobs(message_id) SELECT id FROM messages WHERE id=? AND source_language IS NULL AND (SELECT COUNT(*) FROM translation_jobs WHERE status IN ('pending','processing')) < 1000",
            params![message_id],
        )?;
        Ok(())
    }
    pub fn recover_translations(&self) -> Result<()> {
        self.conn.lock().unwrap().execute(
            "UPDATE translation_jobs SET status='pending' WHERE status='processing'",
            [],
        )?;
        Ok(())
    }
    pub fn claim_translation(&self) -> Result<Option<String>> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let id = tx.query_row("SELECT message_id FROM translation_jobs WHERE status='pending' AND retry_at<=? ORDER BY rowid LIMIT 1", params![chrono::Utc::now().timestamp()], |row| row.get::<_, String>(0)).optional()?;
        if let Some(id) = &id {
            tx.execute("UPDATE translation_jobs SET status='processing', attempts=attempts+1 WHERE message_id=?", params![id])?;
        }
        tx.commit()?;
        Ok(id)
    }
    pub fn retry_translation(&self, id: &str) -> Result<()> {
        self.conn.lock().unwrap().execute("UPDATE translation_jobs SET status=CASE WHEN attempts>=3 THEN 'failed' ELSE 'pending' END, retry_at=? + attempts*10 WHERE message_id=?", params![chrono::Utc::now().timestamp(),id])?;
        Ok(())
    }
    pub fn finish_translation(
        &self,
        id: &str,
        translated: Option<&str>,
        language: &str,
        needs_translation: bool,
    ) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        tx.execute(
            "UPDATE messages SET translated_text=?, source_language=?, is_translated=? WHERE id=?",
            params![translated, language, needs_translation, id],
        )?;
        let contact: Option<String> = tx
            .query_row(
                "SELECT contact_id FROM messages WHERE id=?",
                params![id],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(contact) = contact {
            Self::refresh_contact_last_message(&tx, &contact)?;
        }
        tx.execute(
            "DELETE FROM translation_jobs WHERE message_id=?",
            params![id],
        )?;
        tx.commit()?;
        Ok(())
    }
}
