use super::*;
use std::collections::BTreeMap;

/// Latest choice per actor, including empty-emoji removals. Keeping its clock
/// lets clients reconcile a fresh snapshot with events received during loading.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReactionState {
    pub id: String,
    pub timestamp: i64,
    pub emoji: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PresentedMessage {
    #[serde(flatten)]
    pub message: StoredMessage,
    pub reactions: BTreeMap<String, Vec<String>>,
    pub reaction_states: BTreeMap<String, ReactionState>,
}

impl MessageStore {
    pub fn present_messages(&self, messages: Vec<StoredMessage>) -> Result<Vec<PresentedMessage>> {
        let conn = self.conn.lock().unwrap();
        let mut query = conn.prepare_cached(
            "SELECT id,timestamp,is_from_me,sender_phone,sender_name,content_json
             FROM messages WHERE contact_id=?1 AND lower(content_type)='reaction'
             AND json_extract(content_json,'$.target_message_id')=?2
             ORDER BY timestamp DESC,id DESC",
        )?;
        messages
            .into_iter()
            .map(|message| {
                let mut reaction_states = BTreeMap::new();
                if !message.content_type.eq_ignore_ascii_case("reaction") {
                    let rows = query.query_map(params![message.contact_id, message.id], |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, i64>(1)?,
                            row.get::<_, bool>(2)?,
                            row.get::<_, Option<String>>(3)?,
                            row.get::<_, Option<String>>(4)?,
                            row.get::<_, String>(5)?,
                        ))
                    })?;
                    for row in rows {
                        let (id, timestamp, from_me, phone, name, json) = row?;
                        let actor = if from_me {
                            "me".to_string()
                        } else {
                            phone
                                .filter(|s| !s.is_empty())
                                .or(name)
                                .unwrap_or_else(|| "unknown".into())
                        };
                        if reaction_states.contains_key(&actor) {
                            continue;
                        }
                        let content: serde_json::Value = serde_json::from_str(&json)?;
                        let emoji = content
                            .get("emoji")
                            .and_then(|v| v.as_str())
                            .unwrap_or_default()
                            .to_string();
                        reaction_states.insert(
                            actor,
                            ReactionState {
                                id,
                                timestamp,
                                emoji,
                            },
                        );
                    }
                }
                let mut reactions: BTreeMap<String, Vec<String>> = BTreeMap::new();
                for (actor, state) in &reaction_states {
                    if !state.emoji.is_empty() {
                        reactions
                            .entry(state.emoji.clone())
                            .or_default()
                            .push(actor.clone());
                    }
                }
                Ok(PresentedMessage {
                    message,
                    reactions,
                    reaction_states,
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(
        id: &str,
        chat: &str,
        time: i64,
        emoji: Option<&str>,
        from_me: bool,
    ) -> StoredMessage {
        let content = match emoji {
            Some(emoji) => {
                serde_json::json!({"type":"reaction","target_message_id":"target","emoji":emoji})
            }
            None => serde_json::json!({"type":"text","body":"Hello"}),
        };
        StoredMessage {
            id: id.into(),
            contact_id: chat.into(),
            timestamp: time,
            is_from_me: from_me,
            is_forwarded: false,
            sender_name: None,
            sender_phone: Some("447700900123".into()),
            contact_name: None,
            contact_phone: None,
            chat_type: "group".into(),
            content_type: if emoji.is_some() { "Reaction" } else { "Text" }.into(),
            content_json: content.to_string(),
            content: Some(content),
            original_text: None,
            translated_text: None,
            source_language: None,
            is_translated: false,
            delivery_status: None,
        }
    }

    #[test]
    fn paginated_snapshots_include_latest_reactions_outside_the_message_page() {
        let dir = std::env::temp_dir().join(format!("reaction-snapshot-{}", uuid::Uuid::new_v4()));
        let store = MessageStore::new(&dir).unwrap();
        for chat in ["family@g.us", "other@g.us"] {
            store
                .upsert_contact(chat, None, None, Some("group"), 0)
                .unwrap();
        }
        for item in [
            message("target", "family@g.us", 100, None, true),
            message("a", "family@g.us", 1100, Some("❤️"), false),
            message("z", "family@g.us", 1200, Some("👍"), false),
            message("mine", "family@g.us", 1300, Some("🙏"), true),
            message("other-chat", "other@g.us", 1400, Some("😂"), false),
        ] {
            store.add_message(&item).unwrap();
        }
        let feed = store.get_unified_messages(1, None, None).unwrap();
        let snapshot = store.present_messages(feed).unwrap();
        assert_eq!(
            snapshot[0].reactions,
            BTreeMap::from([
                ("👍".into(), vec!["447700900123".into()]),
                ("🙏".into(), vec!["me".into()])
            ])
        );
        assert_eq!(snapshot[0].reaction_states["447700900123"].id, "z");
        // The reaction is newer than this older-message cursor, yet belongs to its target.
        let page = store
            .get_messages_paginated("family@g.us", Some(1), Some(200), None, true)
            .unwrap();
        assert_eq!(
            store.present_messages(page).unwrap()[0].reactions,
            snapshot[0].reactions
        );
        store
            .add_message(&message("remove", "family@g.us", 1250, Some(""), false))
            .unwrap();
        let snapshot = store
            .present_messages(store.get_unified_messages(1, None, None).unwrap())
            .unwrap();
        assert_eq!(
            snapshot[0].reactions,
            BTreeMap::from([("🙏".into(), vec!["me".into()])])
        );
        assert_eq!(snapshot[0].reaction_states["447700900123"].emoji, "");
        assert_eq!(snapshot[0].reaction_states["447700900123"].timestamp, 1250);
        drop(store);
        let reopened = MessageStore::new(&dir).unwrap();
        assert_eq!(
            reopened
                .present_messages(reopened.get_unified_messages(1, None, None).unwrap())
                .unwrap()[0]
                .reactions,
            snapshot[0].reactions
        );
        drop(reopened);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
