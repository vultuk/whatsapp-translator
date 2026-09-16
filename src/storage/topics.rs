use super::*;
use crate::topics::{
    revision, valid_title, ChatTopic, TopicAssignment, TopicBatch, TopicLabel, TopicSetting,
    BATCH_SIZE, INITIAL_MESSAGES, LABEL_BATCH_SIZE,
};
use serde_json::{json, Value};

impl MessageStore {
    /// The manual initial import uses only locally stored text and captions in the last seven days.
    /// Already assigned messages are excluded, and an import never expands to older history.
    pub fn topic_import_preview(&self, now_ms: i64) -> Result<crate::topics::TopicImportSummary> {
        let conn = self.conn.lock().unwrap();
        Self::topic_import_summary(&conn, now_ms)
    }

    fn topic_import_summary(
        conn: &rusqlite::Connection,
        now_ms: i64,
    ) -> Result<crate::topics::TopicImportSummary> {
        let since = now_ms - crate::topics::IMPORT_DAYS * 24 * 60 * 60 * 1000;
        Ok(conn.query_row(r#"SELECT count(DISTINCT m.contact_id),count(*) FROM messages m
            JOIN contacts c ON c.id=m.contact_id
            WHERE m.timestamp>=?1 AND m.timestamp<=?2 AND m.contact_id!='status@broadcast'
              AND length(trim(COALESCE(m.original_text,'')))>0
              AND lower(m.content_type)!='reaction' AND lower(COALESCE(json_extract(m.content_json,'$.type'),''))!='reaction'
              AND NOT EXISTS(SELECT 1 FROM topic_assignments a WHERE a.message_id=m.id)"#,
            params![since,now_ms], |r|Ok(crate::topics::TopicImportSummary { days:crate::topics::IMPORT_DAYS,chat_count:r.get(0)?,message_count:r.get(1)? }))?)
    }

    pub fn import_recent_topics(&self, now_ms: i64) -> Result<crate::topics::TopicImportSummary> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let summary = Self::topic_import_summary(&tx, now_ms)?;
        let since = now_ms - crate::topics::IMPORT_DAYS * 24 * 60 * 60 * 1000;
        let candidates: Vec<(String, String)> = {
            let mut stmt=tx.prepare(r#"SELECT m.id,m.contact_id FROM messages m JOIN contacts c ON c.id=m.contact_id
                WHERE m.timestamp>=?1 AND m.timestamp<=?2 AND m.contact_id!='status@broadcast'
                  AND length(trim(COALESCE(m.original_text,'')))>0
                  AND lower(m.content_type)!='reaction' AND lower(COALESCE(json_extract(m.content_json,'$.type'),''))!='reaction'
                  AND NOT EXISTS(SELECT 1 FROM topic_assignments a WHERE a.message_id=m.id)"#)?;
            let rows = stmt
                .query_map(params![since, now_ms], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<rusqlite::Result<_>>()?;
            rows
        };
        let chats: std::collections::HashSet<_> =
            candidates.iter().map(|(_, id)| id.as_str()).collect();
        for id in chats {
            // Preserve an enabled chat's epoch so its current batch remains valid.
            tx.execute("INSERT INTO topic_settings VALUES (?1,1,?2) ON CONFLICT(contact_id) DO UPDATE SET enabled=1,epoch=CASE WHEN topic_settings.enabled=1 THEN topic_settings.epoch ELSE excluded.epoch END",params![id,uuid::Uuid::new_v4().to_string()])?;
        }
        for (id, _) in candidates {
            tx.execute("INSERT INTO topic_jobs(message_id) VALUES(?) ON CONFLICT(message_id) DO UPDATE SET attempts=0,retry_at=0",[id])?;
        }
        tx.commit()?;
        Ok(summary)
    }

    pub(super) fn init_topics(&self) -> Result<()> {
        self.conn.lock().unwrap().execute_batch(r#"
            CREATE TABLE IF NOT EXISTS topic_settings(contact_id TEXT PRIMARY KEY, enabled INTEGER NOT NULL, epoch TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS chat_topics(id TEXT PRIMARY KEY, contact_id TEXT NOT NULL, title TEXT NOT NULL, name_key TEXT NOT NULL, UNIQUE(contact_id,name_key));
            CREATE TABLE IF NOT EXISTS topic_assignments(message_id TEXT PRIMARY KEY, topic_id TEXT NOT NULL);
            CREATE INDEX IF NOT EXISTS topic_assignments_topic ON topic_assignments(topic_id);
            CREATE TABLE IF NOT EXISTS topic_jobs(message_id TEXT PRIMARY KEY, attempts INTEGER NOT NULL DEFAULT 0, retry_at INTEGER NOT NULL DEFAULT 0);
            CREATE TABLE IF NOT EXISTS topic_label_jobs(topic_id TEXT PRIMARY KEY, attempts INTEGER NOT NULL DEFAULT 0, retry_at INTEGER NOT NULL DEFAULT 0);
            CREATE TRIGGER IF NOT EXISTS topics_insert AFTER INSERT ON messages
            WHEN EXISTS(SELECT 1 FROM topic_settings WHERE contact_id=NEW.contact_id AND enabled=1)
                AND length(trim(COALESCE(NEW.original_text,'')))>0
                AND lower(NEW.content_type)!='reaction' AND lower(COALESCE(json_extract(NEW.content_json,'$.type'),''))!='reaction'
            BEGIN INSERT OR IGNORE INTO topic_jobs(message_id) VALUES(NEW.id); END;
            CREATE TRIGGER IF NOT EXISTS topics_edit AFTER UPDATE OF original_text,content_json ON messages
            WHEN OLD.original_text IS NOT NEW.original_text OR COALESCE(json_extract(OLD.content_json,'$.edited_at_ms'),0)!=COALESCE(json_extract(NEW.content_json,'$.edited_at_ms'),0)
            BEGIN
                DELETE FROM topic_assignments WHERE message_id=NEW.id;
                DELETE FROM topic_jobs WHERE message_id=NEW.id;
                INSERT OR IGNORE INTO topic_jobs(message_id) SELECT NEW.id
                  WHERE EXISTS(SELECT 1 FROM topic_settings WHERE contact_id=NEW.contact_id AND enabled=1)
                    AND length(trim(COALESCE(NEW.original_text,'')))>0
                    AND lower(NEW.content_type)!='reaction' AND lower(COALESCE(json_extract(NEW.content_json,'$.type'),''))!='reaction';
            END;
            CREATE TRIGGER IF NOT EXISTS topics_delete AFTER DELETE ON messages
            BEGIN DELETE FROM topic_assignments WHERE message_id=OLD.id; DELETE FROM topic_jobs WHERE message_id=OLD.id; END;
            CREATE TRIGGER IF NOT EXISTS topics_confirm_id AFTER UPDATE OF id ON messages WHEN OLD.id!=NEW.id
            BEGIN
                UPDATE OR IGNORE topic_jobs SET message_id=NEW.id WHERE message_id=OLD.id;
                DELETE FROM topic_jobs WHERE message_id=OLD.id;
                UPDATE OR IGNORE topic_assignments SET message_id=NEW.id WHERE message_id=OLD.id;
                DELETE FROM topic_assignments WHERE message_id=OLD.id;
            END;
        "#)?;
        self.recover_topic_jobs("topic_json_input_v1", "json_input")?;
        self.recover_topic_jobs("topic_background_timeout_v1", "background_timeout")?;
        self.recover_topic_jobs("topic_compact_batches_v1", "compact_batches")?;
        self.queue_legacy_topic_labels()?;
        self.recover_isolated_topic_retries()?;
        self.recover_outgoing_topics(chrono::Utc::now().timestamp_millis())?;
        Ok(())
    }

    /// Repair only recent, unassigned outgoing messages in opted-in chats.
    /// A restart cannot repeatedly import older history or replace saved categories.
    fn recover_outgoing_topics(&self, now_ms: i64) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        if tx.execute(
            "INSERT OR IGNORE INTO app_settings(key,value) VALUES('topic_outgoing_context_v1','1')",
            [],
        )? == 0
        {
            return Ok(());
        }
        let since = now_ms - crate::topics::IMPORT_DAYS * 24 * 60 * 60 * 1000;
        let restored = tx.execute(r#"UPDATE messages SET original_text=CASE lower(json_extract(content_json,'$.type'))
                WHEN 'text' THEN json_extract(content_json,'$.body') ELSE json_extract(content_json,'$.caption') END
            WHERE is_from_me=1 AND timestamp>=?1 AND timestamp<=?2 AND length(trim(COALESCE(original_text,'')))=0
                AND lower(json_extract(content_json,'$.type')) IN ('text','image','video','document')
                AND length(trim(COALESCE(CASE lower(json_extract(content_json,'$.type')) WHEN 'text' THEN json_extract(content_json,'$.body') ELSE json_extract(content_json,'$.caption') END,'')))>0
                AND EXISTS(SELECT 1 FROM topic_settings s WHERE s.contact_id=messages.contact_id AND s.enabled=1)
                AND NOT EXISTS(SELECT 1 FROM topic_assignments a WHERE a.message_id=messages.id)"#, params![since,now_ms])?;
        let queued = tx.execute(r#"INSERT OR IGNORE INTO topic_jobs(message_id)
            SELECT m.id FROM messages m JOIN topic_settings s ON s.contact_id=m.contact_id AND s.enabled=1
            WHERE m.is_from_me=1 AND m.timestamp>=?1 AND m.timestamp<=?2 AND length(trim(COALESCE(m.original_text,'')))>0
                AND lower(m.content_type)!='reaction' AND lower(COALESCE(json_extract(m.content_json,'$.type'),''))!='reaction'
                AND NOT EXISTS(SELECT 1 FROM topic_assignments a WHERE a.message_id=m.id)"#,params![since,now_ms])?;
        tx.execute("DELETE FROM topic_jobs WHERE NOT EXISTS(SELECT 1 FROM messages m WHERE m.id=topic_jobs.message_id)", [])?;
        tx.execute("DELETE FROM topic_assignments WHERE NOT EXISTS(SELECT 1 FROM messages m WHERE m.id=topic_assignments.message_id)", [])?;
        tx.commit()?;
        tracing::info!(restored, queued, "Recovered recent outgoing topic messages");
        Ok(())
    }

    fn queue_legacy_topic_labels(&self) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        if tx.execute(
            "INSERT OR IGNORE INTO app_settings(key,value) VALUES('topic_broad_labels_v1','1')",
            [],
        )? > 0
        {
            tx.execute(
                "INSERT OR IGNORE INTO topic_label_jobs(topic_id) SELECT id FROM chat_topics",
                [],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn next_topic_labels(&self) -> Result<Vec<TopicLabel>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(r#"SELECT t.id,t.contact_id,s.epoch,t.title FROM topic_label_jobs j
            JOIN chat_topics t ON t.id=j.topic_id JOIN topic_settings s ON s.contact_id=t.contact_id AND s.enabled=1
            WHERE j.attempts<3 AND j.retry_at<=?1 AND EXISTS(SELECT 1 FROM topic_assignments a JOIN messages m ON m.id=a.message_id AND m.contact_id=t.contact_id WHERE a.topic_id=t.id)
            ORDER BY j.retry_at,t.name_key,t.id LIMIT ?2"#)?;
        let labels = stmt
            .query_map(
                params![chrono::Utc::now().timestamp(), LABEL_BATCH_SIZE],
                |r| {
                    Ok(TopicLabel {
                        id: r.get(0)?,
                        contact_id: r.get(1)?,
                        epoch: r.get(2)?,
                        title: r.get(3)?,
                    })
                },
            )?
            .collect::<rusqlite::Result<_>>()?;
        Ok(labels)
    }

    pub fn finish_topic_labels(
        &self,
        labels: &[TopicLabel],
        assignments: &[TopicAssignment],
    ) -> Result<usize> {
        let mut seen = std::collections::HashSet::new();
        anyhow::ensure!(
            assignments.len() == labels.len(),
            "Incomplete topic label assignments"
        );
        for assignment in assignments {
            anyhow::ensure!(
                seen.insert(&assignment.message_id)
                    && labels.iter().any(|label| label.id == assignment.message_id)
                    && valid_title(&assignment.topic),
                "Invalid topic label assignment"
            );
        }
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let mut moves = Vec::new();
        for assignment in assignments {
            let label = labels
                .iter()
                .find(|label| label.id == assignment.message_id)
                .unwrap();
            // Opt-out, reset, and stale responses cannot change a conversation's assignments.
            if !tx.query_row(r#"SELECT EXISTS(SELECT 1 FROM chat_topics t JOIN topic_settings s ON s.contact_id=t.contact_id AND s.enabled=1
                JOIN topic_label_jobs j ON j.topic_id=t.id WHERE t.id=?1 AND t.contact_id=?2 AND t.title=?3 AND s.epoch=?4)"#,
                params![label.id,label.contact_id,label.title,label.epoch], |r| r.get::<_,bool>(0))? { continue; }
            let title = assignment.topic.trim();
            let key = title.to_lowercase();
            tx.execute("INSERT OR IGNORE INTO chat_topics(id,contact_id,title,name_key) VALUES(?1,?2,?3,?4)",params![uuid::Uuid::new_v4().to_string(),label.contact_id,title,key])?;
            let target: String = tx.query_row(
                "SELECT id FROM chat_topics WHERE contact_id=?1 AND name_key=?2",
                params![label.contact_id, key],
                |r| r.get(0),
            )?;
            moves.push(json!({"source":label.id,"target":target,"chat":label.contact_id}));
            tx.execute("DELETE FROM topic_label_jobs WHERE topic_id=?", [&label.id])?;
        }
        // Apply all mappings in one statement so an A -> B mapping cannot be moved again by B -> C.
        tx.execute(r#"UPDATE topic_assignments AS a SET topic_id=(SELECT json_extract(value,'$.target') FROM json_each(?1) WHERE json_extract(value,'$.source')=a.topic_id)
            WHERE EXISTS(SELECT 1 FROM json_each(?1) j JOIN messages m ON m.id=a.message_id
                WHERE json_extract(j.value,'$.source')=a.topic_id AND json_extract(j.value,'$.chat')=m.contact_id)"#, [serde_json::to_string(&moves)?])?;
        tx.commit()?;
        Ok(moves.len())
    }

    pub fn retry_topic_labels(&self, labels: &[TopicLabel]) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        for label in labels {
            tx.execute(r#"UPDATE topic_label_jobs SET attempts=attempts+1,retry_at=?1+(30*(attempts+1)*(attempts+1)) WHERE topic_id=?2
                AND EXISTS(SELECT 1 FROM chat_topics t JOIN topic_settings s ON s.contact_id=t.contact_id AND s.enabled=1
                    WHERE t.id=?2 AND t.contact_id=?3 AND t.title=?4 AND s.epoch=?5)"#,
                params![chrono::Utc::now().timestamp(),label.id,label.contact_id,label.title,label.epoch])?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Retry existing opted-in jobs once after a known request failure is fixed.
    fn recover_topic_jobs(&self, marker: &str, reason: &str) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let first_run = tx.execute(
            "INSERT OR IGNORE INTO app_settings(key,value) VALUES(?1,'1')",
            [marker],
        )? > 0;
        let recovered = if first_run {
            Some((tx.execute(r#"UPDATE topic_jobs SET attempts=0,retry_at=0 WHERE attempts>0
                AND message_id IN (SELECT m.id FROM messages m JOIN topic_settings s ON s.contact_id=m.contact_id AND s.enabled=1)
                AND NOT EXISTS(SELECT 1 FROM topic_assignments a WHERE a.message_id=topic_jobs.message_id)"#, [])?,
                tx.execute(r#"UPDATE topic_label_jobs SET attempts=0,retry_at=0 WHERE attempts>0 AND topic_id IN
                    (SELECT t.id FROM chat_topics t JOIN topic_settings s ON s.contact_id=t.contact_id AND s.enabled=1)"#,[])?))
        } else {
            None
        };
        tx.commit()?;
        if let Some((messages, topics)) = recovered {
            tracing::info!(
                messages,
                topics,
                reason,
                "Recovered topic jobs after request fix"
            );
        }
        Ok(())
    }

    /// Older batches could consume every retry together. Give only exhausted,
    /// unassigned, opted-in messages a bounded retry through the isolated path.
    fn recover_isolated_topic_retries(&self) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let first_run = tx.execute(
            "INSERT OR IGNORE INTO app_settings(key,value) VALUES('topic_isolated_retries_v1','1')",
            [],
        )? > 0;
        let recovered = if first_run {
            tx.execute(r#"UPDATE topic_jobs SET attempts=1,retry_at=0 WHERE attempts>=3
                AND message_id IN (SELECT m.id FROM messages m JOIN topic_settings s ON s.contact_id=m.contact_id AND s.enabled=1)
                AND NOT EXISTS(SELECT 1 FROM topic_assignments a WHERE a.message_id=topic_jobs.message_id)"#, [])?
        } else {
            0
        };
        tx.commit()?;
        if recovered > 0 {
            tracing::info!(
                messages = recovered,
                "Topic recovery queued individual retries"
            );
        }
        Ok(())
    }

    pub fn set_topics_enabled(&self, id: &str, enabled: bool) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        anyhow::ensure!(
            tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM contacts WHERE id=?)",
                [id],
                |r| r.get::<_, bool>(0)
            )?,
            "Conversation not found"
        );
        tx.execute("INSERT INTO topic_settings VALUES (?1,?2,?3) ON CONFLICT(contact_id) DO UPDATE SET enabled=excluded.enabled,epoch=excluded.epoch", params![id,enabled,uuid::Uuid::new_v4().to_string()])?;
        if enabled {
            tx.execute("UPDATE topic_label_jobs SET attempts=0,retry_at=0 WHERE topic_id IN (SELECT id FROM chat_topics WHERE contact_id=?)", [id])?;
            tx.execute("UPDATE topic_jobs SET attempts=0,retry_at=0 WHERE message_id IN (SELECT id FROM messages WHERE contact_id=?)", [id])?;
            tx.execute(r#"INSERT OR IGNORE INTO topic_jobs(message_id)
                SELECT id FROM (SELECT id, original_text,content_type,content_json FROM messages WHERE contact_id=?1
                    AND lower(content_type)!='reaction' AND lower(COALESCE(json_extract(content_json,'$.type'),''))!='reaction'
                    AND length(trim(COALESCE(original_text,'')))>0 ORDER BY timestamp DESC,id DESC LIMIT ?2)
                WHERE id NOT IN (SELECT message_id FROM topic_assignments)"#, params![id,INITIAL_MESSAGES])?;
        } else {
            tx.execute("DELETE FROM topic_jobs WHERE message_id IN (SELECT id FROM messages WHERE contact_id=?)", [id])?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn topic_settings(&self) -> Result<Vec<TopicSetting>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt=conn.prepare(r#"SELECT s.contact_id,s.enabled,
            CASE WHEN s.enabled=1 THEN
            (SELECT count(*) FROM topic_jobs j JOIN messages m ON m.id=j.message_id WHERE m.contact_id=s.contact_id AND j.attempts<3) +
            (SELECT count(*) FROM topic_assignments a JOIN messages m ON m.id=a.message_id JOIN chat_topics t ON t.id=a.topic_id AND t.contact_id=m.contact_id JOIN topic_label_jobs j ON j.topic_id=t.id WHERE t.contact_id=s.contact_id AND j.attempts<3) ELSE 0 END,
            CASE WHEN s.enabled=1 THEN
            (SELECT count(*) FROM topic_jobs j JOIN messages m ON m.id=j.message_id WHERE m.contact_id=s.contact_id AND j.attempts>=3) +
            (SELECT count(*) FROM topic_assignments a JOIN messages m ON m.id=a.message_id JOIN chat_topics t ON t.id=a.topic_id AND t.contact_id=m.contact_id JOIN topic_label_jobs j ON j.topic_id=t.id WHERE t.contact_id=s.contact_id AND j.attempts>=3) ELSE 0 END
            FROM topic_settings s JOIN contacts c ON c.id=s.contact_id ORDER BY s.contact_id"#)?;
        let result = stmt
            .query_map([], |r| {
                Ok(TopicSetting {
                    contact_id: r.get(0)?,
                    enabled: r.get(1)?,
                    pending_count: r.get(2)?,
                    failed_count: r.get(3)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(result)
    }

    /// Counts only: no chat identifiers, text, or topic titles in operational logs.
    pub fn log_topic_queue_health(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let (ready, delayed, failed): (i64,i64,i64) = conn.query_row(r#"SELECT
            COALESCE(sum(j.attempts<3 AND j.retry_at<=?1),0), COALESCE(sum(j.attempts<3 AND j.retry_at>?1),0), COALESCE(sum(j.attempts>=3),0)
            FROM topic_jobs j JOIN messages m ON m.id=j.message_id JOIN topic_settings s ON s.contact_id=m.contact_id AND s.enabled=1"#, [chrono::Utc::now().timestamp()], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
        let (label_ready, label_delayed, label_failed, disabled_label_messages, invalid_label_assignments): (i64,i64,i64,i64,i64) = conn.query_row(r#"SELECT
            (SELECT count(*) FROM topic_label_jobs j JOIN chat_topics t ON t.id=j.topic_id JOIN topic_settings s ON s.contact_id=t.contact_id AND s.enabled=1 WHERE j.attempts<3 AND j.retry_at<=?1 AND EXISTS(SELECT 1 FROM topic_assignments a JOIN messages m ON m.id=a.message_id AND m.contact_id=t.contact_id WHERE a.topic_id=t.id)),
            (SELECT count(*) FROM topic_label_jobs j JOIN chat_topics t ON t.id=j.topic_id JOIN topic_settings s ON s.contact_id=t.contact_id AND s.enabled=1 WHERE j.attempts<3 AND j.retry_at>?1 AND EXISTS(SELECT 1 FROM topic_assignments a JOIN messages m ON m.id=a.message_id AND m.contact_id=t.contact_id WHERE a.topic_id=t.id)),
            (SELECT count(*) FROM topic_label_jobs j JOIN chat_topics t ON t.id=j.topic_id JOIN topic_settings s ON s.contact_id=t.contact_id AND s.enabled=1 WHERE j.attempts>=3 AND EXISTS(SELECT 1 FROM topic_assignments a JOIN messages m ON m.id=a.message_id AND m.contact_id=t.contact_id WHERE a.topic_id=t.id)),
            (SELECT count(*) FROM topic_assignments a JOIN chat_topics t ON t.id=a.topic_id JOIN topic_label_jobs j ON j.topic_id=t.id JOIN topic_settings s ON s.contact_id=t.contact_id AND s.enabled=0 WHERE j.attempts<3),
            (SELECT count(*) FROM topic_assignments a JOIN chat_topics t ON t.id=a.topic_id JOIN topic_label_jobs j ON j.topic_id=t.id WHERE j.attempts<3 AND NOT EXISTS(SELECT 1 FROM messages m WHERE m.id=a.message_id AND m.contact_id=t.contact_id))"#, [chrono::Utc::now().timestamp()], |r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?)))?;
        tracing::info!(
            ready,
            delayed,
            failed,
            label_ready,
            label_delayed,
            label_failed,
            disabled_label_messages,
            invalid_label_assignments,
            "Topic queue health"
        );
        Ok(())
    }

    pub fn message_topics(&self, ids: &[String]) -> Result<Vec<crate::topics::MessageTopic>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(r#"SELECT m.id,m.contact_id,COALESCE(json_extract(m.content_json,'$.edited_at_ms'),0),t.title,
            CASE WHEN t.id IS NOT NULL THEN 'assigned' WHEN COALESCE(s.enabled,0)=0 THEN 'off'
                 WHEN j.attempts>=3 THEN 'failed' WHEN j.message_id IS NOT NULL THEN 'pending' ELSE 'unassigned' END
            FROM messages m LEFT JOIN topic_settings s ON s.contact_id=m.contact_id
            LEFT JOIN topic_assignments a ON a.message_id=m.id
            LEFT JOIN chat_topics t ON t.id=a.topic_id AND t.contact_id=m.contact_id
            LEFT JOIN topic_jobs j ON j.message_id=m.id
            WHERE m.id IN (SELECT value FROM json_each(?1)) ORDER BY m.id"#)?;
        let result = stmt
            .query_map([serde_json::to_string(ids)?], |r| {
                Ok(crate::topics::MessageTopic {
                    message_id: r.get(0)?,
                    contact_id: r.get(1)?,
                    revision: r.get(2)?,
                    title: r.get(3)?,
                    state: r.get(4)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(result)
    }

    pub fn list_topics(&self) -> Result<Vec<ChatTopic>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt=conn.prepare(r#"SELECT t.id,t.contact_id,COALESCE(c.name,c.phone,t.contact_id),t.title,count(*),max(m.timestamp),t.name_key
            FROM chat_topics t JOIN topic_assignments a ON a.topic_id=t.id JOIN messages m ON m.id=a.message_id AND m.contact_id=t.contact_id
            JOIN topic_settings s ON s.contact_id=t.contact_id AND s.enabled=1 JOIN contacts c ON c.id=t.contact_id
            GROUP BY t.id ORDER BY max(m.timestamp) DESC,t.id"#)?;
        let result = stmt
            .query_map([], |r| {
                Ok(ChatTopic {
                    id: r.get(0)?,
                    category_id: crate::topics::category_id(&r.get::<_, String>(6)?),
                    contact_id: r.get(1)?,
                    contact_name: r.get(2)?,
                    title: r.get(3)?,
                    message_count: r.get(4)?,
                    last_message_time: r.get(5)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(result)
    }

    pub fn topic_names(&self, contact_id: &str) -> Result<Vec<String>> {
        let mut topics = self.list_topics()?;
        // Reuse labels across chats while keeping this chat's useful labels first.
        topics.sort_by_key(|t| t.contact_id != contact_id);
        let mut seen = std::collections::HashSet::new();
        Ok(topics
            .into_iter()
            .filter(|t| seen.insert(t.category_id.clone()))
            .take(60)
            .map(|t| t.title)
            .collect())
    }

    pub fn topic_message_input(&self, batch: &TopicBatch) -> Result<Vec<Value>> {
        let conn = self.conn.lock().unwrap();
        // Include unclassified neighbours too, but never another chat or future
        // messages. Keep each target's context separate during history imports.
        let columns = r#"SELECT m.id,COALESCE(NULLIF(m.original_text,''),json_extract(m.content_json,'$.body'),json_extract(m.content_json,'$.caption'),''),t.title,m.timestamp,m.is_from_me
            FROM messages m LEFT JOIN topic_assignments a ON a.message_id=m.id
            LEFT JOIN chat_topics t ON t.id=a.topic_id AND t.contact_id=m.contact_id"#;
        let mut previous = conn.prepare(&format!(r#"{columns} WHERE m.contact_id=?1 AND (m.timestamp<?2 OR (m.timestamp=?2 AND m.id<?3)) AND m.timestamp>=?4
            AND lower(m.content_type)!='reaction' AND lower(COALESCE(json_extract(m.content_json,'$.type'),''))!='reaction'
            AND length(trim(COALESCE(NULLIF(m.original_text,''),json_extract(m.content_json,'$.body'),json_extract(m.content_json,'$.caption'),'')))>0
            ORDER BY m.timestamp DESC,m.id DESC LIMIT 6"#))?;
        let mut quoted = conn.prepare(&format!(
            "{columns} WHERE m.contact_id=?1 AND m.id=?2 AND m.id!=?3 AND m.timestamp<=?4"
        ))?;
        let context_row = |r: &rusqlite::Row<'_>| -> rusqlite::Result<Value> {
            Ok(
                json!({"messageId":r.get::<_,String>(0)?,"text":r.get::<_,String>(1)?.chars().take(600).collect::<String>(),
                "topic":r.get::<_,Option<String>>(2)?,"timestamp":r.get::<_,i64>(3)?,"isFromMe":r.get::<_,bool>(4)?}),
            )
        };
        batch.messages.iter().map(|message| {
            anyhow::ensure!(message.contact_id == batch.contact_id, "Invalid topic context ownership");
            let since = message.timestamp.saturating_sub(crate::topics::IMPORT_DAYS * 24 * 60 * 60 * 1000);
            let mut context = previous.query_map(params![batch.contact_id,message.timestamp,message.id,since],context_row)?.collect::<rusqlite::Result<Vec<_>>>()?;
            context.reverse();
            let content: Value = serde_json::from_str(&message.content_json).unwrap_or(Value::Null);
            let quote = content.get("reply_context").filter(|v| !v.is_null()).or_else(|| content.get("reply_to")).or_else(|| content.get("replyTo"));
            let quote_id = quote.and_then(|q| q.as_str().or_else(|| q.get("messageId").or_else(|| q.get("message_id")).or_else(|| q.get("id")).and_then(Value::as_str)));
            let resolved = match quote_id {
                Some(id) => quoted.query_row(params![batch.contact_id,id,message.id,message.timestamp],context_row).optional()?,
                None => None,
            };
            let reply = resolved.or_else(|| quote.map(|q| json!({"messageId":quote_id.map(|id|id.chars().take(512).collect::<String>()),
                "text":q.get("text").or_else(||q.get("body")).and_then(Value::as_str).unwrap_or("").chars().take(600).collect::<String>()})));
            Ok(json!({"messageId":message.id,"text":message.original_text.as_deref().unwrap_or("").chars().take(2000).collect::<String>(),
                "timestamp":message.timestamp,"isFromMe":message.is_from_me,"replyTo":reply,"previousMessages":context}))
        }).collect()
    }

    pub fn next_topic_batch(&self) -> Result<Option<TopicBatch>> {
        let conn = self.conn.lock().unwrap();
        let now = chrono::Utc::now().timestamp();
        let next:Option<(String,String,i64)>=conn.query_row(r#"SELECT m.contact_id,s.epoch,j.attempts FROM topic_jobs j JOIN messages m ON m.id=j.message_id
            JOIN topic_settings s ON s.contact_id=m.contact_id AND s.enabled=1 WHERE j.attempts<3 AND j.retry_at<=? ORDER BY j.retry_at,m.timestamp,m.id LIMIT 1"#, [now], |r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
        let Some((contact_id, epoch, attempts)) = next else {
            return Ok(None);
        };
        // A slow or invalid message must not repeatedly exhaust a whole batch.
        let limit = if attempts == 0 { BATCH_SIZE } else { 1 };
        let mut stmt=conn.prepare(r#"SELECT m.id,m.contact_id,m.timestamp,m.is_from_me,m.is_forwarded,m.sender_name,m.sender_phone,m.chat_type,m.content_type,
            m.content_json,m.original_text,m.translated_text,m.source_language,m.is_translated,m.delivery_status FROM messages m JOIN topic_jobs j ON j.message_id=m.id
            WHERE m.contact_id=?1 AND j.attempts=?3 AND j.retry_at<=?2 ORDER BY m.timestamp,m.id LIMIT ?4"#)?;
        let mut messages = stmt
            .query_map(params![contact_id, now, attempts, limit], |r| {
                Self::row_to_stored_message(r, None, None)
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for m in &mut messages {
            let (raw, parsed) = Self::strip_media_from_content(&m.content_json);
            m.content_json = raw;
            m.content = parsed;
        }
        Ok(Some(TopicBatch {
            contact_id,
            epoch,
            messages,
        }))
    }

    pub fn finish_topic_batch(
        &self,
        batch: &TopicBatch,
        assignments: &[TopicAssignment],
    ) -> Result<usize> {
        // Validate ownership and completeness again at the persistence boundary.
        let mut seen = std::collections::HashSet::new();
        anyhow::ensure!(
            assignments.len() == batch.messages.len(),
            "Incomplete topic assignments"
        );
        for a in assignments {
            anyhow::ensure!(
                seen.insert(&a.message_id)
                    && batch
                        .messages
                        .iter()
                        .any(|m| m.id == a.message_id && m.contact_id == batch.contact_id),
                "Invalid topic ownership"
            );
        }
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        if !tx.query_row("SELECT EXISTS(SELECT 1 FROM topic_settings WHERE contact_id=? AND enabled=1 AND epoch=?)",params![batch.contact_id,batch.epoch],|r|r.get::<_,bool>(0))? {return Ok(0)}
        let mut assigned = 0;
        for a in assignments {
            let message = batch
                .messages
                .iter()
                .find(|m| m.id == a.message_id)
                .unwrap();
            let current:Option<(Option<String>,i64)>=tx.query_row("SELECT original_text,COALESCE(json_extract(content_json,'$.edited_at_ms'),0) FROM messages WHERE id=? AND contact_id=?",params![message.id,batch.contact_id],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
            if current != Some((message.original_text.clone(), revision(message))) {
                continue;
            }
            let title = a.topic.trim();
            anyhow::ensure!(valid_title(title), "Invalid topic title");
            let key = title.to_lowercase();
            tx.execute(
                "INSERT OR IGNORE INTO chat_topics(id,contact_id,title,name_key) VALUES(?,?,?,?)",
                params![
                    uuid::Uuid::new_v4().to_string(),
                    batch.contact_id,
                    title,
                    key
                ],
            )?;
            let topic_id: String = tx.query_row(
                "SELECT id FROM chat_topics WHERE contact_id=? AND name_key=?",
                params![batch.contact_id, key],
                |r| r.get(0),
            )?;
            tx.execute("INSERT INTO topic_assignments VALUES(?,?) ON CONFLICT(message_id) DO UPDATE SET topic_id=excluded.topic_id",params![message.id,topic_id])?;
            tx.execute("DELETE FROM topic_jobs WHERE message_id=?", [&message.id])?;
            assigned += 1;
        }
        tx.commit()?;
        Ok(assigned)
    }

    pub fn retry_topic_batch(&self, batch: &TopicBatch) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        if !tx.query_row("SELECT EXISTS(SELECT 1 FROM topic_settings WHERE contact_id=? AND enabled=1 AND epoch=?)",params![batch.contact_id,batch.epoch],|r|r.get::<_,bool>(0))? {return Ok(())}
        for m in &batch.messages {
            tx.execute(r#"UPDATE topic_jobs SET attempts=attempts+1,retry_at=?1+(30*(attempts+1)*(attempts+1)) WHERE message_id=?2
                AND EXISTS(SELECT 1 FROM messages WHERE id=?2 AND contact_id=?3 AND original_text IS ?4 AND COALESCE(json_extract(content_json,'$.edited_at_ms'),0)=?5)"#,params![chrono::Utc::now().timestamp(),m.id,batch.contact_id,m.original_text,revision(m)])?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn topic_messages(
        &self,
        topic_id: &str,
        limit: u32,
        before: Option<i64>,
        before_id: Option<&str>,
    ) -> Result<Option<Vec<StoredMessage>>> {
        let conn = self.conn.lock().unwrap();
        let category = crate::topics::category_key(topic_id);
        let is_category = category.is_some();
        let key = category.as_deref().unwrap_or(topic_id);
        if !conn.query_row("SELECT EXISTS(SELECT 1 FROM chat_topics t JOIN topic_settings s ON s.contact_id=t.contact_id AND s.enabled=1 WHERE (?2=1 AND t.name_key=?1) OR (?2=0 AND t.id=?1))",params![key,is_category],|r|r.get::<_,bool>(0))? {return Ok(None)}
        let mut stmt=conn.prepare(r#"SELECT m.id,m.contact_id,m.timestamp,m.is_from_me,m.is_forwarded,m.sender_name,m.sender_phone,m.chat_type,m.content_type,
            m.content_json,m.original_text,m.translated_text,m.source_language,m.is_translated,m.delivery_status,c.name,c.phone
            FROM messages m JOIN topic_assignments a ON a.message_id=m.id JOIN chat_topics t ON t.id=a.topic_id AND t.contact_id=m.contact_id
            JOIN topic_settings s ON s.contact_id=m.contact_id AND s.enabled=1
            LEFT JOIN contacts c ON c.id=m.contact_id WHERE ((?5=1 AND t.name_key=?1) OR (?5=0 AND t.id=?1)) AND (?2 IS NULL OR m.timestamp<?2 OR(m.timestamp=?2 AND ?3 IS NOT NULL AND m.id<?3))
            ORDER BY m.timestamp DESC,m.id DESC LIMIT ?4"#)?;
        let mut messages = stmt
            .query_map(params![key, before, before_id, limit, is_category], |r| {
                Self::row_to_stored_message(r, r.get(15)?, r.get(16)?)
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for m in &mut messages {
            let (raw, parsed) = Self::strip_media_from_content(&m.content_json);
            m.content_json = raw;
            m.content = parsed;
        }
        messages.reverse();
        Ok(Some(messages))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::tests::{test_message, test_store};

    fn add(store: &MessageStore, chat: &str, id: &str, time: i64) {
        store
            .upsert_contact(chat, Some(chat), None, Some("group"), time)
            .unwrap();
        let mut m = test_message(id, time);
        m.contact_id = chat.into();
        m.chat_type = "group".into();
        store.add_message(&m).unwrap();
    }
    fn finish(store: &MessageStore, batch: &TopicBatch, title: &str) {
        let assignments = batch
            .messages
            .iter()
            .map(|m| TopicAssignment {
                message_id: m.id.clone(),
                topic: title.into(),
            })
            .collect::<Vec<_>>();
        store.finish_topic_batch(batch, &assignments).unwrap();
    }

    fn legacy_topic(store: &MessageStore, chat: &str, id: &str, title: &str) {
        add(store, chat, id, 100);
        store.set_topics_enabled(chat, true).unwrap();
        finish(
            store,
            &store.next_topic_batch().unwrap().unwrap(),
            "Temporary",
        );
        store.conn.lock().unwrap().execute("UPDATE chat_topics SET title=?1,name_key=?2 WHERE contact_id=?3 AND name_key='temporary'",params![title,title.to_lowercase(),chat]).unwrap();
    }

    fn seed_legacy_migration(store: &MessageStore) {
        store
            .conn
            .lock()
            .unwrap()
            .execute(
                "DELETE FROM app_settings WHERE key='topic_broad_labels_v1'",
                [],
            )
            .unwrap();
        store.queue_legacy_topic_labels().unwrap();
    }

    #[test]
    fn outgoing_topic_text_and_jobs_survive_send_confirmation_and_echo() {
        let (store, path) = test_store();
        store
            .upsert_contact("chat@g.us", None, None, Some("group"), 1)
            .unwrap();
        store.set_topics_enabled("chat@g.us", true).unwrap();
        let mut message = test_message("pending", 100);
        message.contact_id = "chat@g.us".into();
        message.is_from_me = true;
        message.original_text = None;
        message.content_json =
            json!({"type":"text","body":"Yes, the hospital appointment works."}).to_string();
        store.add_message(&message).unwrap();
        let pending = store.next_topic_batch().unwrap().unwrap();
        assert_eq!(
            pending.messages[0].original_text.as_deref(),
            Some("Yes, the hospital appointment works.")
        );
        store
            .replace_message_id("pending", "confirmed", Some(101))
            .unwrap();
        // A response for the temporary ID cannot attach to the wrong message.
        let assignment = TopicAssignment {
            message_id: "pending".into(),
            topic: "Hospital".into(),
        };
        assert_eq!(
            store.finish_topic_batch(&pending, &[assignment]).unwrap(),
            0
        );
        let confirmed = store.next_topic_batch().unwrap().unwrap();
        assert_eq!(confirmed.messages[0].id, "confirmed");
        finish(&store, &confirmed, "Hospital");
        store
            .replace_message_id("confirmed", "final-id", None)
            .unwrap();
        assert_eq!(
            store.message_topics(&["final-id".into()]).unwrap()[0]
                .title
                .as_deref(),
            Some("Hospital")
        );
        assert!(store.next_topic_batch().unwrap().is_none());
        // WhatsApp can echo the real message before send confirmation arrives.
        message.id = "pending-echo".into();
        store.add_message(&message).unwrap();
        message.id = "real-echo".into();
        store.add_message(&message).unwrap();
        store
            .replace_message_id("pending-echo", "real-echo", None)
            .unwrap();
        let echo = store.next_topic_batch().unwrap().unwrap();
        assert_eq!(echo.messages.len(), 1);
        assert_eq!(echo.messages[0].id, "real-echo");
        finish(&store, &echo, "Hospital");
        let conn = store.conn.lock().unwrap();
        assert_eq!(
            conn.query_row("SELECT count(*) FROM topic_jobs", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
        drop(conn);
        drop(store);
        std::fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn topic_context_resolves_quotes_and_only_earlier_same_chat_messages() {
        let (store, path) = test_store();
        legacy_topic(&store, "chat@g.us", "hospital", "Hospital");
        // The quote remains useful beyond the six-message neighbourhood.
        for i in 0..8 {
            add(&store, "chat@g.us", &format!("earlier-{i}"), 110 + i);
        }
        add(&store, "other@g.us", "private-other-chat", 119);
        add(&store, "chat@g.us", "future", 200);
        let mut target = test_message("outgoing", 120);
        target.contact_id = "chat@g.us".into();
        target.is_from_me = true;
        target.original_text = Some("Yes, I can take you.".into());
        target.content_json = json!({"type":"text","body":"Yes, I can take you.","reply_context":{"message_id":"hospital","text":"old quote","media_data":"must-not-send"}}).to_string();
        let batch = TopicBatch {
            contact_id: target.contact_id.clone(),
            epoch: "test".into(),
            messages: vec![target.clone()],
        };
        let input = store.topic_message_input(&batch).unwrap();
        assert_eq!(input[0]["isFromMe"], true);
        assert_eq!(input[0]["replyTo"]["topic"], "Hospital");
        assert_eq!(input[0]["replyTo"]["messageId"], "hospital");
        let context = input[0]["previousMessages"].as_array().unwrap();
        assert_eq!(context.len(), 6);
        assert_eq!(context[0]["messageId"], "earlier-2");
        assert_eq!(context[5]["messageId"], "earlier-7");
        assert!(context.iter().all(|m| m["topic"].is_null())); // Unclassified neighbours are included.
        let serialized = serde_json::to_string(&input).unwrap();
        assert!(!serialized.contains("private-other-chat"));
        assert!(!serialized.contains("future"));
        assert!(!serialized.contains("must-not-send"));
        // Embedded quote text still works if the quoted message is not stored;
        // an ID from another chat cannot resolve to that chat's text or topic.
        target.content_json = json!({"type":"text","reply_context":{"messageId":"private-other-chat","text":"x".repeat(2000),"media_data":"must-not-send"}}).to_string();
        let fallback = store
            .topic_message_input(&TopicBatch {
                messages: vec![target],
                ..batch
            })
            .unwrap();
        assert_eq!(fallback[0]["replyTo"]["text"].as_str().unwrap().len(), 600);
        assert!(fallback[0]["replyTo"]["topic"].is_null());
        assert!(!fallback[0]["replyTo"].to_string().contains("must-not-send"));
        drop(store);
        std::fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn outgoing_topic_recovery_is_recent_opted_in_and_once_only() {
        let (store, path) = test_store();
        let now = 1_800_000_000_000;
        for chat in ["enabled@g.us", "paused@g.us"] {
            store
                .upsert_contact(chat, None, None, Some("group"), now)
                .unwrap();
            store
                .set_topics_enabled(chat, chat.starts_with("enabled"))
                .unwrap();
        }
        for (id, chat, time) in [
            ("recent", "enabled@g.us", now),
            ("lost-id", "enabled@g.us", now),
            ("old", "enabled@g.us", now - 8 * 86_400_000),
            ("future", "enabled@g.us", now + 1),
            ("paused", "paused@g.us", now),
            ("saved", "enabled@g.us", now),
        ] {
            let mut m = test_message(id, time);
            m.contact_id = chat.into();
            m.is_from_me = true;
            m.content_json = json!({"type":"text","body":"See you at the hospital."}).to_string();
            store.add_message(&m).unwrap();
        }
        while let Some(batch) = store.next_topic_batch().unwrap() {
            finish(&store, &batch, "Hospital");
        }
        {
            let conn = store.conn.lock().unwrap();
            conn.execute("UPDATE messages SET original_text=NULL WHERE id IN ('recent','old','future','paused')", []).unwrap();
            conn.execute(
                "DELETE FROM topic_assignments WHERE message_id='lost-id'",
                [],
            )
            .unwrap();
            conn.execute("DELETE FROM topic_jobs", []).unwrap();
            conn.execute(
                "DELETE FROM app_settings WHERE key='topic_outgoing_context_v1'",
                [],
            )
            .unwrap();
        }
        store.recover_outgoing_topics(now).unwrap();
        let batch = store.next_topic_batch().unwrap().unwrap();
        let ids: Vec<_> = batch.messages.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, vec!["lost-id", "recent"]);
        assert_eq!(
            batch.messages[1].original_text.as_deref(),
            Some("See you at the hospital.")
        );
        assert_eq!(
            store.message_topics(&["saved".into()]).unwrap()[0]
                .title
                .as_deref(),
            Some("Hospital")
        );
        finish(&store, &batch, "Hospital");
        store
            .recover_outgoing_topics(now + 10 * 86_400_000)
            .unwrap();
        assert!(store.next_topic_batch().unwrap().is_none());
        drop(store);
        std::fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn topic_retries_isolate_messages_and_recover_exhausted_work_only_once() {
        let (store, path) = test_store();
        for i in 0..8 {
            add(&store, "family@g.us", &format!("message-{i}"), i);
        }
        store.set_topics_enabled("family@g.us", true).unwrap();
        let initial = store.next_topic_batch().unwrap().unwrap();
        assert_eq!(initial.messages.len(), 8);
        store.retry_topic_batch(&initial).unwrap();
        store
            .conn
            .lock()
            .unwrap()
            .execute("UPDATE topic_jobs SET retry_at=0", [])
            .unwrap();
        let retry = store.next_topic_batch().unwrap().unwrap();
        assert_eq!(retry.messages.len(), 1);
        finish(&store, &retry, "Hospital");
        assert_eq!(store.topic_settings().unwrap()[0].pending_count, 7);
        store.conn.lock().unwrap().execute_batch("UPDATE topic_jobs SET attempts=3; DELETE FROM app_settings WHERE key='topic_isolated_retries_v1';").unwrap();
        drop(store);
        let store = MessageStore::new(&path).unwrap();
        assert_eq!(store.topic_settings().unwrap()[0].pending_count, 7);
        assert_eq!(store.topic_settings().unwrap()[0].failed_count, 0);
        let recovered = store.next_topic_batch().unwrap().unwrap();
        assert_eq!(recovered.messages.len(), 1);
        for _ in 0..2 {
            store.retry_topic_batch(&recovered).unwrap();
        }
        drop(store);
        let store = MessageStore::new(&path).unwrap();
        assert_eq!(store.topic_settings().unwrap()[0].failed_count, 1); // restart cannot give unlimited retries
        while let Some(batch) = store.next_topic_batch().unwrap() {
            assert_eq!(batch.messages.len(), 1);
            finish(&store, &batch, "Hospital");
        }
        assert_eq!(store.topic_settings().unwrap()[0].pending_count, 0);
        assert_eq!(store.list_topics().unwrap()[0].message_count, 7);
        drop(store);
        std::fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn organising_counts_exclude_disabled_and_invalid_legacy_assignments() {
        let (store, path) = test_store();
        for id in ["a", "b", "c"] {
            add(&store, "disabled@g.us", id, 100);
        }
        store.set_topics_enabled("disabled@g.us", true).unwrap();
        finish(
            &store,
            &store.next_topic_batch().unwrap().unwrap(),
            "Hospital",
        );
        seed_legacy_migration(&store);
        assert_eq!(store.topic_settings().unwrap()[0].pending_count, 3);
        store.set_topics_enabled("disabled@g.us", false).unwrap();
        assert!(store.next_topic_labels().unwrap().is_empty());
        assert_eq!(store.topic_settings().unwrap()[0].pending_count, 0);
        // Retaining paused work is intentional: opting back in resumes it.
        store.set_topics_enabled("disabled@g.us", true).unwrap();
        assert_eq!(store.topic_settings().unwrap()[0].pending_count, 3);
        store
            .upsert_contact("wrong@g.us", None, None, Some("group"), 100)
            .unwrap();
        store
            .conn
            .lock()
            .unwrap()
            .execute(
                "UPDATE messages SET contact_id='wrong@g.us' WHERE id='a'",
                [],
            )
            .unwrap();
        assert_eq!(store.topic_settings().unwrap()[0].pending_count, 2);
        store
            .conn
            .lock()
            .unwrap()
            .execute("UPDATE topic_label_jobs SET attempts=3", [])
            .unwrap();
        assert_eq!(store.topic_settings().unwrap()[0].failed_count, 2);
        store.set_topics_enabled("disabled@g.us", false).unwrap();
        assert_eq!(store.topic_settings().unwrap()[0].failed_count, 0);
        store.log_topic_queue_health().unwrap();
        drop(store);
        std::fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn message_topic_inspection_tracks_saved_assignment_edit_retry_and_opt_out() {
        let (store, path) = test_store();
        add(&store, "family@g.us", "a", 100);
        let ids = vec!["a".into(), "missing".into()];
        let status = || store.message_topics(&ids).unwrap().remove(0);
        assert_eq!(store.message_topics(&ids).unwrap().len(), 1);
        assert_eq!(status().state, "off");
        store.set_topics_enabled("family@g.us", true).unwrap();
        assert_eq!(status().state, "pending");
        finish(
            &store,
            &store.next_topic_batch().unwrap().unwrap(),
            "Hospital",
        );
        assert_eq!(status().title.as_deref(), Some("Hospital"));
        store.set_topics_enabled("family@g.us", false).unwrap();
        assert_eq!(status().title.as_deref(), Some("Hospital"));
        store.set_topics_enabled("family@g.us", true).unwrap();
        store.conn.lock().unwrap().execute("UPDATE messages SET original_text='Shopping now',content_json=json_set(content_json,'$.edited_at_ms',200) WHERE id='a'", []).unwrap();
        assert_eq!(status().state, "pending");
        assert!(status().title.is_none());
        assert_eq!(status().revision, 200);
        let batch = store.next_topic_batch().unwrap().unwrap();
        for _ in 0..3 {
            store.retry_topic_batch(&batch).unwrap();
        }
        assert_eq!(status().state, "failed");
        store.set_topics_enabled("family@g.us", true).unwrap();
        finish(
            &store,
            &store.next_topic_batch().unwrap().unwrap(),
            "Shopping",
        );
        assert_eq!(status().title.as_deref(), Some("Shopping"));
        // A corrupt cross-chat assignment must never leak a misleading title.
        store
            .conn
            .lock()
            .unwrap()
            .execute(
                "UPDATE chat_topics SET contact_id='other@g.us' WHERE title='Shopping'",
                [],
            )
            .unwrap();
        assert!(status().title.is_none());
        assert_eq!(status().state, "unassigned");
        drop(store);
        std::fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn broad_labels_merge_existing_topics_and_preserve_messages_across_restart() {
        let (store, path) = test_store();
        legacy_topic(
            &store,
            "family@g.us",
            "a",
            "Hospital support during recovery",
        );
        legacy_topic(
            &store,
            "family@g.us",
            "b",
            "Procedure delay and discharge plans",
        );
        legacy_topic(
            &store,
            "friend@s.whatsapp.net",
            "c",
            "Hospital recovery and surgery",
        );
        legacy_topic(
            &store,
            "family@g.us",
            "d",
            "Grocery shopping and meal requests",
        );
        seed_legacy_migration(&store);
        let batch = store.next_topic_labels().unwrap();
        assert_eq!(batch.len(), 4);
        let before = store.get_message_by_id("b").unwrap().unwrap().content_json;
        let assignments: Vec<_> = batch
            .iter()
            .map(|label| TopicAssignment {
                message_id: label.id.clone(),
                topic: if label.title.starts_with("Grocery") {
                    "Shopping"
                } else {
                    "Hospital"
                }
                .into(),
            })
            .collect();
        assert_eq!(store.finish_topic_labels(&batch, &assignments).unwrap(), 4);
        assert!(store.next_topic_labels().unwrap().is_empty());
        assert_eq!(
            store.get_message_by_id("b").unwrap().unwrap().content_json,
            before
        );
        let topics = store.list_topics().unwrap();
        assert_eq!(topics.len(), 3);
        let category = crate::topics::category_id("hospital");
        let messages = store
            .topic_messages(&category, 10, None, None)
            .unwrap()
            .unwrap();
        assert_eq!(
            messages.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
            ["a", "b", "c"]
        );
        assert_eq!(messages[2].contact_id, "friend@s.whatsapp.net");
        assert!(store
            .topic_settings()
            .unwrap()
            .iter()
            .all(|s| s.pending_count == 0));
        drop(store);
        let store = MessageStore::new(&path).unwrap();
        assert!(store.next_topic_labels().unwrap().is_empty());
        assert_eq!(
            store
                .topic_messages(&category, 10, None, None)
                .unwrap()
                .unwrap()
                .len(),
            3
        );
        drop(store);
        std::fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn broad_label_migration_rejects_invalid_output_and_in_flight_opt_out() {
        let (store, path) = test_store();
        legacy_topic(&store, "family@g.us", "a", "Hospital support");
        legacy_topic(&store, "disabled@g.us", "b", "Hospital recovery");
        store.set_topics_enabled("disabled@g.us", false).unwrap();
        seed_legacy_migration(&store);
        let batch = store.next_topic_labels().unwrap();
        assert_eq!(batch.len(), 1);
        let bad = [TopicAssignment {
            message_id: batch[0].id.clone(),
            topic: "A label with too many words".into(),
        }];
        assert!(store.finish_topic_labels(&batch, &bad).is_err());
        assert_eq!(store.list_topics().unwrap()[0].title, "Hospital support");
        let valid = [TopicAssignment {
            message_id: batch[0].id.clone(),
            topic: "Hospital".into(),
        }];
        store.set_topics_enabled("family@g.us", false).unwrap();
        assert_eq!(store.finish_topic_labels(&batch, &valid).unwrap(), 0);
        store.set_topics_enabled("family@g.us", true).unwrap();
        assert_eq!(store.finish_topic_labels(&batch, &valid).unwrap(), 0); // epoch changed
        let current = store.next_topic_labels().unwrap();
        assert_eq!(store.finish_topic_labels(&current, &valid).unwrap(), 1);
        store.set_topics_enabled("disabled@g.us", true).unwrap();
        let retry = store.next_topic_labels().unwrap();
        assert_eq!(retry.len(), 1);
        for _ in 0..3 {
            store.retry_topic_labels(&retry).unwrap();
            store
                .conn
                .lock()
                .unwrap()
                .execute("UPDATE topic_label_jobs SET retry_at=0", [])
                .unwrap();
        }
        assert!(store.next_topic_labels().unwrap().is_empty());
        assert_eq!(
            store
                .topic_settings()
                .unwrap()
                .iter()
                .find(|s| s.contact_id == "disabled@g.us")
                .unwrap()
                .failed_count,
            1
        );
        store.clear_all().unwrap();
        assert!(store.next_topic_labels().unwrap().is_empty());
        let count: i64 = store
            .conn
            .lock()
            .unwrap()
            .query_row("SELECT count(*) FROM topic_label_jobs", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
        drop(store);
        std::fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn label_mappings_apply_simultaneously_without_cascading_between_sources() {
        let (store, path) = test_store();
        legacy_topic(&store, "family@g.us", "a", "Travel");
        legacy_topic(&store, "family@g.us", "b", "Holidays");
        seed_legacy_migration(&store);
        let batch = store.next_topic_labels().unwrap();
        let assignments: Vec<_> = batch
            .iter()
            .map(|label| TopicAssignment {
                message_id: label.id.clone(),
                topic: if label.title == "Travel" {
                    "Holidays"
                } else {
                    "Weekend plans"
                }
                .into(),
            })
            .collect();
        store.finish_topic_labels(&batch, &assignments).unwrap();
        for (key, id) in [("holidays", "a"), ("weekend plans", "b")] {
            let messages = store
                .topic_messages(&crate::topics::category_id(key), 10, None, None)
                .unwrap()
                .unwrap();
            assert_eq!(messages.len(), 1);
            assert_eq!(messages[0].id, id);
        }
        drop(store);
        std::fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn unified_categories_merge_people_and_groups_with_stable_pagination_and_opt_out() {
        let (store, path) = test_store();
        for (chat, id, title) in [
            ("family@g.us", "a", "Birthday wishes"),
            ("447700900123@s.whatsapp.net", "b", "birthday wishes"),
            ("friends@g.us", "c", "Birthday wishes"),
        ] {
            add(&store, chat, id, 100);
            store.set_topics_enabled(chat, true).unwrap();
            finish(&store, &store.next_topic_batch().unwrap().unwrap(), title);
        }
        let topics = store.list_topics().unwrap();
        let category = crate::topics::category_id("birthday wishes");
        assert!(topics.iter().all(|t| t.category_id == category));
        assert_eq!(store.topic_names("new@g.us").unwrap().len(), 1);
        let recent = store
            .topic_messages(&category, 2, None, None)
            .unwrap()
            .unwrap();
        assert_eq!(
            recent.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
            ["b", "c"]
        );
        let earlier = store
            .topic_messages(&category, 2, Some(100), Some("b"))
            .unwrap()
            .unwrap();
        assert_eq!(earlier[0].id, "a");
        assert_eq!(earlier[0].contact_id, "family@g.us");
        let per_chat = topics
            .iter()
            .find(|t| t.contact_id == "family@g.us")
            .unwrap();
        assert_eq!(
            store
                .topic_messages(&per_chat.id, 10, None, None)
                .unwrap()
                .unwrap()
                .len(),
            1
        );
        store.set_topics_enabled("family@g.us", false).unwrap();
        assert_eq!(
            store
                .topic_messages(&category, 10, None, None)
                .unwrap()
                .unwrap()
                .len(),
            2
        );
        assert!(store
            .topic_messages(&per_chat.id, 10, None, None)
            .unwrap()
            .is_none());
        assert!(store
            .topic_messages("category:not-valid!", 10, None, None)
            .unwrap()
            .is_none());
        drop(store);
        let store = MessageStore::new(&path).unwrap();
        assert_eq!(
            store
                .topic_messages(&category, 10, None, None)
                .unwrap()
                .unwrap()
                .len(),
            2
        );
        drop(store);
        std::fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn compact_batch_fix_recovers_only_enabled_label_jobs_once() {
        let (store, path) = test_store();
        legacy_topic(&store, "family@g.us", "a", "Hospital support");
        legacy_topic(&store, "disabled@g.us", "b", "Hospital recovery");
        seed_legacy_migration(&store);
        {
            let conn = store.conn.lock().unwrap();
            conn.execute(
                "UPDATE topic_label_jobs SET attempts=3,retry_at=9999999999",
                [],
            )
            .unwrap();
            conn.execute(
                "UPDATE topic_settings SET enabled=0 WHERE contact_id='disabled@g.us'",
                [],
            )
            .unwrap();
            conn.execute(
                "DELETE FROM app_settings WHERE key='topic_compact_batches_v1'",
                [],
            )
            .unwrap();
        }
        drop(store);
        let store = MessageStore::new(&path).unwrap();
        let batch = store.next_topic_labels().unwrap();
        assert_eq!(batch.len(), 1);
        assert_eq!(batch[0].contact_id, "family@g.us");
        assert_eq!(
            store
                .topic_settings()
                .unwrap()
                .iter()
                .find(|s| s.contact_id == "disabled@g.us")
                .unwrap()
                .failed_count,
            0
        );
        assert_eq!(store.conn.lock().unwrap().query_row("SELECT attempts FROM topic_label_jobs j JOIN chat_topics t ON t.id=j.topic_id WHERE t.contact_id='disabled@g.us'", [], |r| r.get::<_,i64>(0)).unwrap(), 3);
        assert_eq!(store.list_topics().unwrap()[0].title, "Hospital support");
        for _ in 0..3 {
            store.retry_topic_labels(&batch).unwrap();
        }
        drop(store);
        let store = MessageStore::new(&path).unwrap();
        assert!(store.next_topic_labels().unwrap().is_empty());
        assert_eq!(
            store
                .topic_settings()
                .unwrap()
                .iter()
                .find(|s| s.contact_id == "family@g.us")
                .unwrap()
                .failed_count,
            1
        );
        drop(store);
        std::fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn compact_batch_recovery_completes_the_queue_in_bounded_chunks() {
        let (store, path) = test_store();
        for n in 0..19 {
            add(&store, "family@g.us", &format!("m{n:02}"), n);
        }
        store.set_topics_enabled("family@g.us", true).unwrap();
        {
            let conn = store.conn.lock().unwrap();
            conn.execute("UPDATE topic_jobs SET attempts=3,retry_at=9999999999", [])
                .unwrap();
            conn.execute(
                "DELETE FROM app_settings WHERE key='topic_compact_batches_v1'",
                [],
            )
            .unwrap();
        }
        drop(store);
        let store = MessageStore::new(&path).unwrap();
        let mut sizes = Vec::new();
        let mut ids = std::collections::HashSet::new();
        while let Some(batch) = store.next_topic_batch().unwrap() {
            sizes.push(batch.messages.len());
            for message in &batch.messages {
                assert!(ids.insert(message.id.clone()));
            }
            finish(&store, &batch, "Hospital");
        }
        assert_eq!(sizes, [8, 8, 3]);
        assert_eq!(ids.len(), 19);
        assert_eq!(store.list_topics().unwrap()[0].message_count, 19);
        assert_eq!(store.topic_settings().unwrap()[0].pending_count, 0);
        add(&store, "family@g.us", "later", 100);
        let later = store.next_topic_batch().unwrap().unwrap();
        for _ in 0..3 {
            store.retry_topic_batch(&later).unwrap();
        }
        drop(store);
        let store = MessageStore::new(&path).unwrap();
        assert!(store.next_topic_batch().unwrap().is_none());
        assert_eq!(store.topic_settings().unwrap()[0].failed_count, 1);
        drop(store);
        std::fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn timeout_fix_recovers_exhausted_jobs_once_without_enabling_other_chats() {
        let (store, path) = test_store();
        add(&store, "family@g.us", "pending", 100);
        add(&store, "friends@g.us", "disabled", 100);
        store.set_topics_enabled("family@g.us", true).unwrap();
        let batch = store.next_topic_batch().unwrap().unwrap();
        for _ in 0..3 {
            store.retry_topic_batch(&batch).unwrap();
        }
        assert!(store.next_topic_batch().unwrap().is_none());
        store
            .conn
            .lock()
            .unwrap()
            .execute(
                "DELETE FROM app_settings WHERE key='topic_background_timeout_v1'",
                [],
            )
            .unwrap();
        drop(store);
        let store = MessageStore::new(&path).unwrap();
        let batch = store.next_topic_batch().unwrap().unwrap();
        assert_eq!(batch.messages.len(), 1);
        assert_eq!(batch.messages[0].id, "pending");
        assert_eq!(store.topic_settings().unwrap().len(), 1);
        for _ in 0..3 {
            store.retry_topic_batch(&batch).unwrap();
        }
        drop(store);
        let store = MessageStore::new(&path).unwrap();
        assert!(store.next_topic_batch().unwrap().is_none());
        drop(store);
        std::fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn topic_json_fix_recovers_only_existing_enabled_jobs_once_across_restarts() {
        let (store, path) = test_store();
        add(&store, "enabled@g.us", "assigned", 100);
        store.set_topics_enabled("enabled@g.us", true).unwrap();
        finish(
            &store,
            &store.next_topic_batch().unwrap().unwrap(),
            "Saved topic",
        );
        add(&store, "enabled@g.us", "failed", 200);
        add(&store, "enabled@g.us", "waiting", 300);
        add(&store, "disabled@g.us", "disabled", 400);
        store.set_topics_enabled("disabled@g.us", true).unwrap();
        add(&store, "never-enabled@g.us", "untouched", 500);
        {
            let conn = store.conn.lock().unwrap();
            conn.execute(
                "DELETE FROM app_settings WHERE key='topic_json_input_v1'",
                [],
            )
            .unwrap();
            conn.execute("UPDATE topic_jobs SET attempts=3,retry_at=9999999999", [])
                .unwrap();
            conn.execute(
                "UPDATE topic_jobs SET attempts=1 WHERE message_id='waiting'",
                [],
            )
            .unwrap();
            // A stale disabled-chat job is deliberately retained to verify the recovery scope.
            conn.execute(
                "UPDATE topic_settings SET enabled=0 WHERE contact_id='disabled@g.us'",
                [],
            )
            .unwrap();
        }
        drop(store);
        let store = MessageStore::new(&path).unwrap();
        let enabled = store
            .topic_settings()
            .unwrap()
            .into_iter()
            .find(|s| s.contact_id == "enabled@g.us")
            .unwrap();
        assert_eq!((enabled.pending_count, enabled.failed_count), (2, 0));
        assert_eq!(store.list_topics().unwrap()[0].message_count, 1);
        let batch = store.next_topic_batch().unwrap().unwrap();
        assert_eq!(
            batch
                .messages
                .iter()
                .map(|m| m.id.as_str())
                .collect::<Vec<_>>(),
            vec!["failed", "waiting"]
        );
        {
            let conn = store.conn.lock().unwrap();
            assert_eq!(
                conn.query_row(
                    "SELECT attempts FROM topic_jobs WHERE message_id='disabled'",
                    [],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
                3
            );
            assert!(!conn
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM topic_jobs WHERE message_id='untouched')",
                    [],
                    |r| r.get::<_, bool>(0)
                )
                .unwrap());
        }
        for _ in 0..3 {
            store.retry_topic_batch(&batch).unwrap();
        }
        drop(store);
        let store = MessageStore::new(&path).unwrap();
        assert!(store.next_topic_batch().unwrap().is_none()); // Restart must not create endless retries.
        let enabled = store
            .topic_settings()
            .unwrap()
            .into_iter()
            .find(|s| s.contact_id == "enabled@g.us")
            .unwrap();
        assert_eq!((enabled.pending_count, enabled.failed_count), (0, 2));
        assert_eq!(store.list_topics().unwrap()[0].title, "Saved topic");
        drop(store);
        std::fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn initial_topic_import_covers_seven_days_across_chats_without_reclassifying_or_expanding_history(
    ) {
        let (store, path) = test_store();
        let now = 1_800_000_000_000_i64;
        let since = now - 7 * 24 * 60 * 60 * 1000;
        for i in 0..205 {
            add(&store, "one@g.us", &format!("recent-{i:03}"), since + i);
        }
        add(&store, "two@s.whatsapp.net", "direct", now);
        add(&store, "old@g.us", "too-old", since - 1);
        add(&store, "future@g.us", "future", now + 1);
        add(&store, "status@broadcast", "status", now);
        add(&store, "empty@g.us", "empty", now);
        add(&store, "one@g.us", "reaction", now);
        store
            .conn
            .lock()
            .unwrap()
            .execute("UPDATE messages SET original_text=' ' WHERE id='empty'", [])
            .unwrap();
        store
            .conn
            .lock()
            .unwrap()
            .execute(
                "UPDATE messages SET content_type='reaction' WHERE id='reaction'",
                [],
            )
            .unwrap();
        let preview = store.topic_import_preview(now).unwrap();
        assert_eq!(
            (preview.days, preview.chat_count, preview.message_count),
            (7, 2, 206)
        );
        assert!(store.topic_settings().unwrap().is_empty()); // A preview does not opt in or enqueue anything.
        let imported = store.import_recent_topics(now).unwrap();
        assert_eq!((imported.chat_count, imported.message_count), (2, 206));
        assert_eq!(
            store
                .topic_settings()
                .unwrap()
                .iter()
                .map(|s| s.pending_count)
                .sum::<i64>(),
            206
        );
        let in_flight = store.next_topic_batch().unwrap().unwrap();
        store.import_recent_topics(now).unwrap();
        finish(&store, &in_flight, "Plans"); // A repeated import preserves a running batch's epoch.
        while let Some(batch) = store.next_topic_batch().unwrap() {
            finish(&store, &batch, "Plans");
        }
        assert_eq!(
            store
                .list_topics()
                .unwrap()
                .iter()
                .map(|t| t.message_count)
                .sum::<i64>(),
            206
        );
        assert_eq!(store.topic_import_preview(now).unwrap().message_count, 0);
        assert_eq!(store.import_recent_topics(now).unwrap().message_count, 0);
        add(&store, "two@s.whatsapp.net", "new-after-import", now + 1000);
        assert_eq!(
            store.next_topic_batch().unwrap().unwrap().messages[0].id,
            "new-after-import"
        );
        assert_eq!(store.topic_settings().unwrap().len(), 2);
        drop(store);
        std::fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn topics_are_opt_in_bounded_persistent_and_separate_even_when_titles_match() {
        let (store, path) = test_store();
        for i in 0..205 {
            add(&store, "one@g.us", &format!("a{i:03}"), i);
        }
        add(&store, "two@g.us", "second-chat", 300);
        assert!(store.next_topic_batch().unwrap().is_none());
        store.set_topics_enabled("one@g.us", true).unwrap();
        assert_eq!(store.topic_settings().unwrap()[0].pending_count, 200);
        while let Some(batch) = store.next_topic_batch().unwrap() {
            finish(&store, &batch, "Weekend plans");
        }
        let first = store.list_topics().unwrap().remove(0);
        assert_eq!(first.message_count, 200);
        store.set_topics_enabled("two@g.us", true).unwrap();
        finish(
            &store,
            &store.next_topic_batch().unwrap().unwrap(),
            "Weekend plans",
        );
        let topics = store.list_topics().unwrap();
        assert_eq!(topics.len(), 2);
        assert_ne!(topics[0].id, topics[1].id);
        assert_eq!(
            store
                .topic_messages(&first.id, 250, None, None)
                .unwrap()
                .unwrap()
                .iter()
                .map(|m| m.contact_id.as_str())
                .collect::<std::collections::HashSet<_>>(),
            std::collections::HashSet::from(["one@g.us"])
        );
        drop(store);
        let store = MessageStore::new(&path).unwrap();
        assert_eq!(store.list_topics().unwrap().len(), 2);
        assert!(store.next_topic_batch().unwrap().is_none());
        store.clear_all().unwrap();
        assert!(store.list_topics().unwrap().is_empty());
        assert!(store.topic_settings().unwrap().is_empty());
        drop(store);
        std::fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn topic_edits_requeue_and_stale_results_cannot_restore_membership() {
        let (store, path) = test_store();
        add(&store, "one@g.us", "a", 100);
        store.set_topics_enabled("one@g.us", true).unwrap();
        let original = store.next_topic_batch().unwrap().unwrap();
        finish(&store, &original, "Food");
        assert_eq!(store.list_topics().unwrap().len(), 1);
        store
            .conn
            .lock()
            .unwrap()
            .execute(
                "UPDATE messages SET original_text='Football',content_json=? WHERE id='a'",
                [r#"{"type":"text","body":"Football","edited_at_ms":200}"#],
            )
            .unwrap();
        assert!(store.list_topics().unwrap().is_empty());
        finish(&store, &original, "Food");
        assert!(store.list_topics().unwrap().is_empty());
        let edited = store.next_topic_batch().unwrap().unwrap();
        assert_eq!(
            edited.messages[0].original_text.as_deref(),
            Some("Football")
        );
        finish(&store, &edited, "Sport");
        assert_eq!(store.list_topics().unwrap()[0].title, "Sport");
        // A reaction/translation-only update does not incur another classification.
        store.conn.lock().unwrap().execute("UPDATE messages SET translated_text='Sport',content_json=json_set(content_json,'$.reactions',json('{}')) WHERE id='a'",[]).unwrap();
        assert!(store.next_topic_batch().unwrap().is_none());
        drop(store);
        std::fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn disabling_or_reenabling_a_chat_invalidates_inflight_results_and_retries_are_bounded() {
        let (store, path) = test_store();
        add(&store, "one@g.us", "a", 100);
        store.set_topics_enabled("one@g.us", true).unwrap();
        let old = store.next_topic_batch().unwrap().unwrap();
        store.set_topics_enabled("one@g.us", false).unwrap();
        finish(&store, &old, "Plans");
        assert!(store.list_topics().unwrap().is_empty());
        assert!(store.next_topic_batch().unwrap().is_none());
        store.set_topics_enabled("one@g.us", true).unwrap();
        finish(&store, &old, "Plans");
        assert!(store.list_topics().unwrap().is_empty());
        let current = store.next_topic_batch().unwrap().unwrap();
        for _ in 0..3 {
            store.retry_topic_batch(&current).unwrap();
        }
        assert!(store.next_topic_batch().unwrap().is_none());
        assert_eq!(store.topic_settings().unwrap()[0].failed_count, 1);
        store.set_topics_enabled("one@g.us", true).unwrap();
        let retried = store.next_topic_batch().unwrap().unwrap();
        finish(&store, &retried, "Plans");
        let id = store.list_topics().unwrap()[0].id.clone();
        store.set_topics_enabled("one@g.us", false).unwrap();
        assert!(store.topic_messages(&id, 50, None, None).unwrap().is_none());
        drop(store);
        std::fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn topic_pagination_preserves_ties_and_rejects_cross_chat_assignments() {
        let (store, path) = test_store();
        for id in ["a", "b", "c"] {
            add(&store, "one@g.us", id, 100);
        }
        add(&store, "two@g.us", "foreign", 100);
        store.set_topics_enabled("one@g.us", true).unwrap();
        let batch = store.next_topic_batch().unwrap().unwrap();
        for invalid in [
            r#"{"assignments":[{"messageId":"foreign","topic":"Oops"}]}"#,
            r#"{"assignments":[{"messageId":"a","topic":"X"},{"messageId":"a","topic":"X"},{"messageId":"c","topic":"X"}]}"#,
        ] {
            assert!(crate::topics::parse_assignments(invalid, &batch).is_err());
        }
        finish(&store, &batch, "Plans");
        let topic = &store.list_topics().unwrap()[0];
        let page = store
            .topic_messages(&topic.id, 2, None, None)
            .unwrap()
            .unwrap();
        assert_eq!(
            page.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
            vec!["b", "c"]
        );
        let older = store
            .topic_messages(&topic.id, 2, Some(100), Some("b"))
            .unwrap()
            .unwrap();
        assert_eq!(older[0].id, "a");
        assert!(store.reply_is_latest("one@g.us", "c").unwrap());
        assert!(!store.reply_is_latest("one@g.us", "a").unwrap());
        store.delete_message("b").ok();
        drop(store);
        std::fs::remove_dir_all(path).unwrap();
    }
}
