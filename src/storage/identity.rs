//! One-time repair for conversations previously keyed by a WhatsApp device JID.
use super::*;
use std::collections::BTreeSet;

const REPAIR_KEY: &str = "device_jid_repair_v1";

impl MessageStore {
    pub(super) fn migrate_device_jids(&self, conn: &Connection) -> Result<()> {
        if conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM app_settings WHERE key=?)",
            [REPAIR_KEY],
            |r| r.get::<_, bool>(0),
        )? {
            return Ok(());
        }
        // Include orphan references left by older imports, plus per-chat settings.
        let ids: Vec<String> = {
            let mut stmt = conn.prepare("SELECT id FROM contacts UNION SELECT contact_id FROM messages UNION SELECT contact_id FROM translation_usage WHERE contact_id IS NOT NULL UNION SELECT contact_id FROM style_profiles UNION SELECT substr(key,7) FROM app_settings WHERE key LIKE 'voice:%' UNION SELECT substr(key,27) FROM app_settings WHERE key LIKE 'message-tone:conversation:%'")?;
            let rows = stmt
                .query_map([], |row| row.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            rows
        };
        let aliases: Vec<_> = ids
            .into_iter()
            .filter_map(|id| {
                let canonical = canonical_chat_id(&id).into_owned();
                (canonical != id).then_some((id, canonical))
            })
            .collect();

        // A consistent, private SQLite snapshot must succeed before changing data.
        // Copy SQLite pages, including committed WAL contents, without rebuilding
        // the database in temp_store=MEMORY. Memory use must not scale with its size.
        if !aliases.is_empty() {
            let path = conn
                .path()
                .filter(|p| !p.is_empty())
                .context("Identity repair requires a file-backed database snapshot")?;
            let backup = Path::new(path).with_file_name(format!(
                "messages-before-device-jid-repair-{}.db",
                uuid::Uuid::new_v4()
            ));
            // Reuse only this incomplete destination after an interrupted start.
            // Completed snapshots have unique names and are never overwritten.
            let incomplete =
                Path::new(path).with_file_name("messages-device-jid-repair.incomplete");
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let snapshot_file = options
                .open(&incomplete)
                .context("Create identity repair snapshot")?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                snapshot_file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
            }
            drop(snapshot_file);
            info!(aliases = aliases.len(), "Identity repair snapshot starting");
            let snapshot = (|| -> Result<()> {
                let mut destination = Connection::open(&incomplete)?;
                destination.execute_batch("PRAGMA journal_mode=DELETE; PRAGMA synchronous=FULL; PRAGMA cache_size=-4096; PRAGMA temp_store=FILE;")?;
                {
                    use rusqlite::backup::{Backup, StepResult};
                    use std::time::{Duration, Instant};
                    let copy = Backup::new(conn, &mut destination)?;
                    let started = Instant::now();
                    let mut last_log = started;
                    loop {
                        let result = copy.step(1024)?;
                        if result == StepResult::Done {
                            break;
                        }
                        anyhow::ensure!(
                            started.elapsed() < Duration::from_secs(480),
                            "Identity repair snapshot exceeded eight minutes"
                        );
                        if last_log.elapsed() >= Duration::from_secs(5) {
                            let progress = copy.progress();
                            info!(
                                remaining_pages = progress.remaining,
                                total_pages = progress.pagecount,
                                "Identity repair snapshot progress"
                            );
                            last_log = Instant::now();
                        }
                        if matches!(result, StepResult::Busy | StepResult::Locked) {
                            std::thread::sleep(Duration::from_millis(100));
                        }
                    }
                }
                destination.close().map_err(|(_, error)| error)?;
                std::fs::rename(&incomplete, &backup)?;
                Ok(())
            })();
            if let Err(error) = snapshot {
                // Only remove this attempt's incomplete output. A restart must
                // not consume the volume with a new partial snapshot each time.
                if let Err(cleanup) = std::fs::remove_file(&incomplete) {
                    tracing::warn!(
                        "Could not remove incomplete identity repair snapshot: {cleanup}"
                    );
                }
                return Err(error).context("Snapshot database before identity repair");
            }
            info!("Identity repair snapshot saved at {}", backup.display());
        }

        let tx = conn.unchecked_transaction()?;
        let messages_before: i64 =
            tx.query_row("SELECT COUNT(*) FROM messages", [], |r| r.get(0))?;
        let mut moved_messages = 0;
        let mut repaired = BTreeSet::new();
        for (alias, canonical) in &aliases {
            let existed: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM contacts WHERE id=?)",
                [canonical],
                |r| r.get(0),
            )?;
            // If the account has no row, inherit the first device row's preferences.
            // Otherwise retain the account's explicit settings, including opt-out.
            tx.execute("INSERT OR IGNORE INTO contacts(id,name,phone,type,last_message_time,unread_count,last_message_preview,pinned_at,participant_count,translation_enabled,language_override,translation_style,send_original_follow_up) SELECT ?1,name,phone,type,last_message_time,unread_count,last_message_preview,pinned_at,participant_count,translation_enabled,language_override,translation_style,send_original_follow_up FROM contacts WHERE id=?2", params![canonical, alias])?;
            tx.execute("INSERT OR IGNORE INTO contacts(id,type,last_message_time,unread_count) VALUES (?,'private',0,0)", [canonical])?;
            if existed {
                tx.execute("UPDATE contacts SET unread_count=COALESCE(unread_count,0)+COALESCE((SELECT unread_count FROM contacts WHERE id=?2),0), name=COALESCE(NULLIF(name,''),(SELECT NULLIF(name,'') FROM contacts WHERE id=?2)), last_message_time=MAX(COALESCE(last_message_time,0),COALESCE((SELECT last_message_time FROM contacts WHERE id=?2),0)) WHERE id=?1", params![canonical, alias])?;
            }
            if let Some(phone) = crate::identity::phone_number(canonical) {
                tx.execute(
                    "UPDATE contacts SET phone=?2 WHERE id=?1",
                    params![canonical, phone],
                )?;
            }
            moved_messages += tx.execute(
                "UPDATE messages SET contact_id=?1 WHERE contact_id=?2",
                params![canonical, alias],
            )?;
            tx.execute(
                "UPDATE translation_usage SET contact_id=?1 WHERE contact_id=?2",
                params![canonical, alias],
            )?;
            tx.execute("INSERT OR IGNORE INTO style_profiles(contact_id,profile_text,sample_messages,message_count,updated_at) SELECT ?1,profile_text,sample_messages,message_count,updated_at FROM style_profiles WHERE contact_id=?2", params![canonical, alias])?;
            tx.execute("DELETE FROM style_profiles WHERE contact_id=?", [alias])?;
            for prefix in ["voice:", "message-tone:conversation:"] {
                let old_key = format!("{prefix}{alias}");
                let new_key = format!("{prefix}{canonical}");
                tx.execute("INSERT OR IGNORE INTO app_settings(key,value) SELECT ?1,value FROM app_settings WHERE key=?2", params![new_key, old_key])?;
                tx.execute("DELETE FROM app_settings WHERE key=?", [old_key])?;
            }
            tx.execute("DELETE FROM contacts WHERE id=?", [alias])?;
            repaired.insert(canonical);
        }
        for canonical in &repaired {
            Self::refresh_contact_last_message(&tx, canonical)?;
            // An alias's previous opt-in must not override the account's opt-out.
            tx.execute("DELETE FROM translation_jobs WHERE message_id IN (SELECT m.id FROM messages m JOIN contacts c ON c.id=m.contact_id WHERE c.id=? AND c.translation_enabled=0)", [canonical])?;
            tx.execute("UPDATE pending_notifications SET requires_translation=0 WHERE message_id IN (SELECT m.id FROM messages m JOIN contacts c ON c.id=m.contact_id WHERE c.id=? AND c.translation_enabled=0)", [canonical])?;
        }
        let messages_after: i64 =
            tx.query_row("SELECT COUNT(*) FROM messages", [], |r| r.get(0))?;
        anyhow::ensure!(
            messages_before == messages_after,
            "Identity repair changed the message count"
        );
        tx.execute(
            "INSERT INTO app_settings(key,value) VALUES (?,?)",
            params![REPAIR_KEY, "1"],
        )?;
        tx.commit()?;
        info!(
            aliases = aliases.len(),
            conversations = repaired.len(),
            moved_messages,
            messages_before,
            messages_after,
            "Identity repair complete"
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ACCOUNT: &str = "447700900123@s.whatsapp.net";
    const DEVICE: &str = "447700900123:22@s.whatsapp.net";

    fn fixture() -> (MessageStore, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("jid-repair-{}", uuid::Uuid::new_v4()));
        let store = MessageStore::new(&dir).unwrap();
        {
            let conn = store.conn.lock().unwrap();
            conn.execute("DELETE FROM app_settings WHERE key=?", [REPAIR_KEY])
                .unwrap();
            conn.execute("INSERT INTO contacts(id,name,phone,type,last_message_time,unread_count,translation_enabled,language_override,translation_style,send_original_follow_up,pinned_at) VALUES (?,'Friend','447700900123','private',10,2,0,'French','formal',0,7)", [ACCOUNT]).unwrap();
            for (id, time) in [(DEVICE, 20), ("447700900123:17@s.whatsapp.net", 30)] {
                conn.execute("INSERT INTO contacts(id,name,phone,type,last_message_time,unread_count,translation_enabled,language_override,translation_style,send_original_follow_up) VALUES (?,NULL,'447700900123:22','private',?,3,1,'Spanish','informal',1)", params![id, time]).unwrap();
                conn.execute("INSERT INTO messages(id,contact_id,timestamp,is_from_me,chat_type,content_type,content_json,original_text) VALUES (?,?,?,0,'private','Image',?, 'hello')", params![format!("msg-{time}"),id,time,r#"{"type":"image","caption":"photo","media_data":"YWJj","reply_context":{"stanza_id":"original-id"}}"#]).unwrap();
            }
            conn.execute("INSERT INTO translation_usage(contact_id,timestamp,input_tokens,output_tokens,cost_usd,operation) VALUES (?,20,10,20,0.25,'translate_incoming')", [DEVICE]).unwrap();
            conn.execute("INSERT INTO app_settings(key,value) VALUES (?, 'feminine'), (?, 'masculine'), (?, '\"bell\"')", params![format!("voice:{ACCOUNT}"),format!("voice:{DEVICE}"),format!("message-tone:conversation:{DEVICE}")]).unwrap();
            conn.execute(
                "INSERT INTO style_profiles VALUES (?,'saved style','[]',5,10)",
                [DEVICE],
            )
            .unwrap();
            conn.execute("INSERT INTO pending_notifications(message_id,requires_translation) VALUES ('msg-20',1)", []).unwrap();
            conn.execute(
                "INSERT INTO translation_jobs(message_id) VALUES ('msg-20')",
                [],
            )
            .unwrap();
            conn.execute_batch("PRAGMA foreign_keys=ON").unwrap();
        }
        (store, dir)
    }

    #[test]
    fn repair_preserves_messages_preferences_and_snapshot_and_is_idempotent() {
        let (store, dir) = fixture();
        store.init_schema().unwrap();
        let contact = store.get_contact(DEVICE).unwrap().unwrap();
        assert_eq!(contact.id, ACCOUNT);
        assert_eq!(contact.phone.as_deref(), Some("447700900123"));
        assert_eq!(contact.name.as_deref(), Some("Friend"));
        assert_eq!(contact.unread_count, 8);
        assert_eq!(contact.pinned_at, Some(7));
        assert_eq!(contact.last_message_time, 30);
        let settings = store.get_conversation_settings(DEVICE).unwrap();
        assert!(!settings.translation_enabled);
        assert_eq!(settings.language_override.as_deref(), Some("French"));
        assert_eq!(settings.translation_style.as_deref(), Some("formal"));
        assert!(!settings.send_original_follow_up);
        let messages = store.get_messages(DEVICE).unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].id, "msg-20");
        assert_eq!(messages[0].timestamp, 20);
        assert_eq!(messages[0].contact_id, ACCOUNT);
        assert_eq!(messages[0].content.as_ref().unwrap()["media_data"], "YWJj");
        assert_eq!(
            messages[0].content.as_ref().unwrap()["reply_context"]["stanza_id"],
            "original-id"
        );
        assert_eq!(store.voice_setting(DEVICE).unwrap(), "feminine");
        assert_eq!(
            store
                .get_style_profile(DEVICE)
                .unwrap()
                .unwrap()
                .profile_text,
            "saved style"
        );
        assert_eq!(store.get_conversation_usage(DEVICE).unwrap().cost_usd, 0.25);
        {
            let conn = store.conn.lock().unwrap();
            assert_eq!(
                conn.query_row("SELECT COUNT(*) FROM translation_jobs", [], |r| r
                    .get::<_, i64>(0))
                    .unwrap(),
                0
            );
            assert!(!conn.query_row("SELECT requires_translation FROM pending_notifications WHERE message_id='msg-20'", [], |r| r.get::<_, bool>(0)).unwrap());
            assert_eq!(
                conn.query_row(
                    "SELECT value FROM app_settings WHERE key=?",
                    [format!("message-tone:conversation:{ACCOUNT}")],
                    |r| r.get::<_, String>(0)
                )
                .unwrap(),
                "\"bell\""
            );
        }
        let backups: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("messages-before-device-jid-repair-")
            })
            .collect();
        assert_eq!(backups.len(), 1);
        let snapshot = Connection::open(backups[0].path()).unwrap();
        assert_eq!(
            snapshot
                .query_row(
                    "SELECT COUNT(*) FROM messages WHERE contact_id=?",
                    [DEVICE],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
            1
        );
        store.init_schema().unwrap();
        assert_eq!(store.get_contact(ACCOUNT).unwrap().unwrap().unread_count, 8);
        drop(snapshot);
        drop(store);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn snapshot_copies_large_wal_data_and_reuses_only_incomplete_output() {
        let (store, dir) = fixture();
        let incomplete = dir.join("messages-device-jid-repair.incomplete");
        // A killed previous attempt can leave a valid but outdated destination.
        // Restart must replace its contents, preserving completed snapshots.
        let previous = Connection::open(&incomplete).unwrap();
        previous
            .execute_batch("CREATE TABLE stale(value); INSERT INTO stale VALUES (1);")
            .unwrap();
        drop(previous);
        let preserved = dir.join("messages-before-device-jid-repair-preserved.db");
        std::fs::write(&preserved, b"previous completed snapshot").unwrap();
        {
            let conn = store.conn.lock().unwrap();
            conn.execute_batch(
                "PRAGMA wal_autocheckpoint=0; CREATE TABLE large_snapshot_fixture(payload BLOB);",
            )
            .unwrap();
            // 64 MiB, well beyond the destination cache, written only to the WAL.
            for _ in 0..64 {
                conn.execute(
                    "INSERT INTO large_snapshot_fixture VALUES (zeroblob(1048576))",
                    [],
                )
                .unwrap();
            }
            store.migrate_device_jids(&conn).unwrap();
        }
        assert!(!incomplete.exists());
        assert_eq!(
            std::fs::read(&preserved).unwrap(),
            b"previous completed snapshot"
        );
        let snapshot_path = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|entry| entry.path())
            .find(|path| {
                path != &preserved
                    && path
                        .file_name()
                        .unwrap()
                        .to_string_lossy()
                        .starts_with("messages-before-device-jid-repair-")
            })
            .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            // Reusing an incomplete destination must also enforce private mode.
            assert_eq!(
                std::fs::metadata(&snapshot_path)
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o077,
                0
            );
        }
        let snapshot = Connection::open(snapshot_path).unwrap();
        assert_eq!(
            snapshot
                .query_row(
                    "SELECT SUM(length(payload)) FROM large_snapshot_fixture",
                    [],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
            64 * 1048576
        );
        assert_eq!(
            snapshot
                .query_row("PRAGMA quick_check", [], |r| r.get::<_, String>(0))
                .unwrap(),
            "ok"
        );
        assert_eq!(
            snapshot
                .query_row("SELECT COUNT(*) FROM contacts WHERE id=?", [DEVICE], |r| {
                    r.get::<_, i64>(0)
                })
                .unwrap(),
            1
        );
        assert!(snapshot.prepare("SELECT * FROM stale").is_err());
        drop(snapshot);
        drop(store);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn repair_inherits_device_preferences_and_keeps_lid_and_group_namespaces_separate() {
        let (store, dir) = fixture();
        {
            let conn = store.conn.lock().unwrap();
            conn.execute("DELETE FROM contacts WHERE id=?", [ACCOUNT])
                .unwrap();
            conn.execute("INSERT INTO contacts(id,type,unread_count) VALUES ('447700900123:22@lid','private',1),('447700900123:22@g.us','group',2)", []).unwrap();
            conn.execute("INSERT INTO translation_usage(contact_id,timestamp,input_tokens,output_tokens,cost_usd,operation) VALUES ('447700900999:22@s.whatsapp.net',20,1,2,0.5,'translate_incoming')", []).unwrap();
        }
        store.init_schema().unwrap();
        let settings = store.get_conversation_settings(ACCOUNT).unwrap();
        assert!(settings.translation_enabled);
        assert_eq!(settings.language_override.as_deref(), Some("Spanish"));
        assert_eq!(store.get_contact(ACCOUNT).unwrap().unwrap().unread_count, 6);
        assert_eq!(
            store
                .get_contact("447700900123@lid")
                .unwrap()
                .unwrap()
                .unread_count,
            1
        );
        assert_eq!(
            store
                .get_contact("447700900123:22@g.us")
                .unwrap()
                .unwrap()
                .unread_count,
            2
        );
        assert!(store
            .get_contact("447700900999@s.whatsapp.net")
            .unwrap()
            .is_some());
        assert_eq!(
            store
                .get_conversation_usage("447700900999@s.whatsapp.net")
                .unwrap()
                .cost_usd,
            0.5
        );
        drop(store);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn snapshot_failure_prevents_any_repair() {
        let (store, dir) = fixture();
        // Moving the directory leaves the live connection usable but its snapshot
        // destination unavailable, without relying on OS permission behavior.
        let moved_dir = dir.with_extension("moved");
        std::fs::rename(&dir, &moved_dir).unwrap();
        {
            let conn = store.conn.lock().unwrap();
            assert!(store.migrate_device_jids(&conn).is_err());
            assert_eq!(
                conn.query_row("SELECT COUNT(*) FROM contacts", [], |r| r.get::<_, i64>(0))
                    .unwrap(),
                3
            );
            assert_eq!(
                conn.query_row(
                    "SELECT COUNT(*) FROM messages WHERE contact_id=?",
                    [DEVICE],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
                1
            );
        }
        std::fs::rename(&moved_dir, &dir).unwrap();
        drop(store);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn failed_merge_rolls_back_all_changes() {
        let (store, dir) = fixture();
        {
            let conn = store.conn.lock().unwrap();
            conn.execute_batch("CREATE TRIGGER block_identity_delete BEFORE DELETE ON contacts BEGIN SELECT RAISE(ABORT,'injected failure'); END").unwrap();
            assert!(store.migrate_device_jids(&conn).is_err());
            assert_eq!(
                conn.query_row("SELECT COUNT(*) FROM contacts", [], |r| r.get::<_, i64>(0))
                    .unwrap(),
                3
            );
            assert_eq!(
                conn.query_row(
                    "SELECT COUNT(*) FROM messages WHERE contact_id=?",
                    [DEVICE],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
                1
            );
            assert!(!conn
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM app_settings WHERE key=?)",
                    [REPAIR_KEY],
                    |r| r.get::<_, bool>(0)
                )
                .unwrap());
        }
        drop(store);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn storage_routes_old_device_ids_to_the_account_without_recreating_contacts() {
        let dir = std::env::temp_dir().join(format!("jid-storage-{}", uuid::Uuid::new_v4()));
        let store = MessageStore::new(&dir).unwrap();
        store
            .upsert_contact(
                DEVICE,
                Some("Friend"),
                Some("447700900123:22"),
                Some("private"),
                1,
            )
            .unwrap();
        store.increment_unread(DEVICE).unwrap();
        store.set_voice_setting(DEVICE, "neutral").unwrap();
        assert_eq!(store.get_contacts().unwrap().len(), 1);
        assert_eq!(store.get_contact(ACCOUNT).unwrap().unwrap().unread_count, 1);
        assert_eq!(
            store
                .get_contact(ACCOUNT)
                .unwrap()
                .unwrap()
                .phone
                .as_deref(),
            Some("447700900123")
        );
        assert_eq!(store.voice_setting(ACCOUNT).unwrap(), "neutral");
        store.mark_as_read(DEVICE).unwrap();
        assert_eq!(store.get_contact(ACCOUNT).unwrap().unwrap().unread_count, 0);
        drop(store);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
