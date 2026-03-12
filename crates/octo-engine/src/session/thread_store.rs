use anyhow::{Context, Result};
use async_trait::async_trait;
use octo_types::{ChatMessage, SessionId};
use uuid::Uuid;

use super::{Thread, ThreadStore, Turn};

/// SQLite-backed ThreadStore for conversation threads and turns.
pub struct SqliteThreadStore {
    conn: tokio_rusqlite::Connection,
}

impl SqliteThreadStore {
    pub fn new(conn: tokio_rusqlite::Connection) -> Self {
        Self { conn }
    }
}

fn now_epoch() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[async_trait]
impl ThreadStore for SqliteThreadStore {
    async fn create_thread(
        &self,
        session_id: &SessionId,
        title: Option<&str>,
    ) -> Result<Thread> {
        let thread = Thread {
            thread_id: Uuid::new_v4().to_string(),
            session_id: session_id.clone(),
            title: title.map(String::from),
            created_at: now_epoch(),
            parent_thread_id: None,
        };
        let t = thread.clone();
        let sid = t.session_id.as_str().to_string();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO threads \
                     (thread_id, session_id, title, parent_thread_id, created_at) \
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    rusqlite::params![
                        t.thread_id,
                        sid,
                        t.title,
                        t.parent_thread_id,
                        t.created_at
                    ],
                )?;
                Ok(())
            })
            .await
            .context("Failed to create thread")?;
        Ok(thread)
    }

    async fn list_threads(&self, session_id: &SessionId) -> Result<Vec<Thread>> {
        let sid = session_id.as_str().to_string();
        let threads = self
            .conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT thread_id, session_id, title, parent_thread_id, created_at \
                     FROM threads WHERE session_id = ?1 ORDER BY created_at ASC",
                )?;
                let rows = stmt
                    .query_map(rusqlite::params![sid], |row| {
                        let sid_str: String = row.get(1)?;
                        Ok(Thread {
                            thread_id: row.get(0)?,
                            session_id: SessionId::from_string(&sid_str),
                            title: row.get(2)?,
                            parent_thread_id: row.get(3)?,
                            created_at: row.get(4)?,
                        })
                    })?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(rows)
            })
            .await
            .context("Failed to list threads")?;
        Ok(threads)
    }

    async fn get_default_thread(&self, session_id: &SessionId) -> Result<Thread> {
        let sid = session_id.as_str().to_string();
        let sid2 = sid.clone();
        let session_id_clone = session_id.clone();

        let existing = self
            .conn
            .call(move |conn| {
                let result = conn
                    .query_row(
                        "SELECT thread_id, session_id, title, parent_thread_id, created_at \
                         FROM threads WHERE session_id = ?1 \
                         ORDER BY created_at ASC LIMIT 1",
                        rusqlite::params![sid],
                        |row| {
                            let sid_str: String = row.get(1)?;
                            Ok(Thread {
                                thread_id: row.get(0)?,
                                session_id: SessionId::from_string(&sid_str),
                                title: row.get(2)?,
                                parent_thread_id: row.get(3)?,
                                created_at: row.get(4)?,
                            })
                        },
                    )
                    .ok();
                Ok(result)
            })
            .await
            .context("Failed to query default thread")?;

        if let Some(thread) = existing {
            return Ok(thread);
        }

        // Auto-create default thread
        let thread = Thread {
            thread_id: Uuid::new_v4().to_string(),
            session_id: session_id_clone,
            title: Some("default".to_string()),
            created_at: now_epoch(),
            parent_thread_id: None,
        };
        let t = thread.clone();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO threads \
                     (thread_id, session_id, title, parent_thread_id, created_at) \
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    rusqlite::params![
                        t.thread_id,
                        sid2,
                        t.title,
                        t.parent_thread_id,
                        t.created_at
                    ],
                )?;
                Ok(())
            })
            .await
            .context("Failed to create default thread")?;
        Ok(thread)
    }

    async fn fork_thread(
        &self,
        thread_id: &str,
        from_turn_id: &str,
    ) -> Result<Thread> {
        let tid = thread_id.to_string();
        let fork_turn = from_turn_id.to_string();
        let new_thread_id = Uuid::new_v4().to_string();
        let now = now_epoch();

        let thread = self
            .conn
            .call(move |conn| {
                // Get parent thread's session_id
                let session_id_str: String = conn.query_row(
                    "SELECT session_id FROM threads WHERE thread_id = ?1",
                    rusqlite::params![tid],
                    |row| row.get(0),
                )?;

                // Get the fork turn's created_at to copy turns up to it
                let fork_created_at: i64 = conn.query_row(
                    "SELECT created_at FROM turns \
                     WHERE turn_id = ?1 AND thread_id = ?2",
                    rusqlite::params![fork_turn, tid],
                    |row| row.get(0),
                )?;

                // Create forked thread
                conn.execute(
                    "INSERT INTO threads \
                     (thread_id, session_id, title, parent_thread_id, created_at) \
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    rusqlite::params![
                        new_thread_id,
                        session_id_str,
                        None::<String>,
                        Some(&tid),
                        now
                    ],
                )?;

                // Copy turns up to and including the fork point
                let mut stmt = conn.prepare(
                    "SELECT user_message_json, assistant_messages_json, created_at \
                     FROM turns WHERE thread_id = ?1 AND created_at <= ?2 \
                     ORDER BY created_at ASC",
                )?;
                let turns: Vec<(String, String, i64)> = stmt
                    .query_map(
                        rusqlite::params![tid, fork_created_at],
                        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                    )?
                    .collect::<rusqlite::Result<Vec<_>>>()?;

                for (user_json, asst_json, turn_created) in &turns {
                    let copy_turn_id = Uuid::new_v4().to_string();
                    conn.execute(
                        "INSERT INTO turns \
                         (turn_id, thread_id, user_message_json, \
                          assistant_messages_json, created_at) \
                         VALUES (?1, ?2, ?3, ?4, ?5)",
                        rusqlite::params![
                            copy_turn_id,
                            new_thread_id,
                            user_json,
                            asst_json,
                            turn_created
                        ],
                    )?;
                }

                Ok(Thread {
                    thread_id: new_thread_id,
                    session_id: SessionId::from_string(&session_id_str),
                    title: None,
                    created_at: now,
                    parent_thread_id: Some(tid),
                })
            })
            .await
            .context("Failed to fork thread")?;

        Ok(thread)
    }

    async fn push_turn(&self, thread_id: &str, turn: Turn) -> Result<()> {
        let tid = thread_id.to_string();
        let user_json = serde_json::to_string(&turn.user_message)
            .context("Failed to serialize user message")?;
        let asst_json = serde_json::to_string(&turn.assistant_messages)
            .context("Failed to serialize assistant messages")?;
        let turn_id = turn.turn_id.clone();
        let created_at = turn.created_at;

        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO turns \
                     (turn_id, thread_id, user_message_json, \
                      assistant_messages_json, created_at) \
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    rusqlite::params![turn_id, tid, user_json, asst_json, created_at],
                )?;
                Ok(())
            })
            .await
            .context("Failed to push turn")?;
        Ok(())
    }

    async fn list_turns(
        &self,
        thread_id: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<Turn>> {
        let tid = thread_id.to_string();
        let turns = self
            .conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT turn_id, thread_id, user_message_json, \
                     assistant_messages_json, created_at \
                     FROM turns WHERE thread_id = ?1 \
                     ORDER BY created_at ASC \
                     LIMIT ?2 OFFSET ?3",
                )?;
                let rows = stmt
                    .query_map(
                        rusqlite::params![tid, limit as i64, offset as i64],
                        |row| {
                            Ok((
                                row.get::<_, String>(0)?,
                                row.get::<_, String>(1)?,
                                row.get::<_, String>(2)?,
                                row.get::<_, String>(3)?,
                                row.get::<_, i64>(4)?,
                            ))
                        },
                    )?
                    .collect::<rusqlite::Result<Vec<_>>>()?;

                let mut turns = Vec::with_capacity(rows.len());
                for (turn_id, thread_id, user_json, asst_json, created_at) in rows {
                    let user_message: ChatMessage = serde_json::from_str(&user_json)
                        .map_err(|e| {
                            rusqlite::Error::ToSqlConversionFailure(Box::new(e))
                        })?;
                    let assistant_messages: Vec<ChatMessage> =
                        serde_json::from_str(&asst_json).map_err(|e| {
                            rusqlite::Error::ToSqlConversionFailure(Box::new(e))
                        })?;
                    turns.push(Turn {
                        turn_id,
                        thread_id,
                        user_message,
                        assistant_messages,
                        created_at,
                    });
                }
                Ok(turns)
            })
            .await
            .context("Failed to list turns")?;
        Ok(turns)
    }

    async fn undo_last_turn(&self, thread_id: &str) -> Result<Option<Turn>> {
        let tid = thread_id.to_string();
        let result = self
            .conn
            .call(move |conn| {
                let last = conn
                    .query_row(
                        "SELECT turn_id, thread_id, user_message_json, \
                         assistant_messages_json, created_at \
                         FROM turns WHERE thread_id = ?1 \
                         ORDER BY created_at DESC LIMIT 1",
                        rusqlite::params![tid],
                        |row| {
                            Ok((
                                row.get::<_, String>(0)?,
                                row.get::<_, String>(1)?,
                                row.get::<_, String>(2)?,
                                row.get::<_, String>(3)?,
                                row.get::<_, i64>(4)?,
                            ))
                        },
                    )
                    .ok();

                let Some((turn_id, thread_id, user_json, asst_json, created_at)) =
                    last
                else {
                    return Ok(None);
                };

                conn.execute(
                    "DELETE FROM turns WHERE turn_id = ?1",
                    rusqlite::params![turn_id],
                )?;

                let user_message: ChatMessage =
                    serde_json::from_str(&user_json).map_err(|e| {
                        rusqlite::Error::ToSqlConversionFailure(Box::new(e))
                    })?;
                let assistant_messages: Vec<ChatMessage> =
                    serde_json::from_str(&asst_json).map_err(|e| {
                        rusqlite::Error::ToSqlConversionFailure(Box::new(e))
                    })?;

                Ok(Some(Turn {
                    turn_id,
                    thread_id,
                    user_message,
                    assistant_messages,
                    created_at,
                }))
            })
            .await
            .context("Failed to undo last turn")?;
        Ok(result)
    }

    async fn get_thread_messages(
        &self,
        thread_id: &str,
    ) -> Result<Vec<ChatMessage>> {
        let tid = thread_id.to_string();
        let messages = self
            .conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT user_message_json, assistant_messages_json \
                     FROM turns WHERE thread_id = ?1 \
                     ORDER BY created_at ASC",
                )?;
                let rows = stmt
                    .query_map(rusqlite::params![tid], |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                        ))
                    })?
                    .collect::<rusqlite::Result<Vec<_>>>()?;

                let mut messages = Vec::new();
                for (user_json, asst_json) in rows {
                    let user_msg: ChatMessage =
                        serde_json::from_str(&user_json).map_err(|e| {
                            rusqlite::Error::ToSqlConversionFailure(Box::new(e))
                        })?;
                    let asst_msgs: Vec<ChatMessage> =
                        serde_json::from_str(&asst_json).map_err(|e| {
                            rusqlite::Error::ToSqlConversionFailure(Box::new(e))
                        })?;
                    messages.push(user_msg);
                    messages.extend(asst_msgs);
                }
                Ok(messages)
            })
            .await
            .context("Failed to get thread messages")?;
        Ok(messages)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    async fn setup() -> (Database, SqliteThreadStore, SessionId) {
        let db = Database::open_in_memory().await.unwrap();
        let store = SqliteThreadStore::new(db.conn().clone());
        let session_id = SessionId::new();

        // Insert a session row so FK constraints pass
        let sid = session_id.as_str().to_string();
        db.conn()
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO sessions (session_id, user_id, sandbox_id) \
                     VALUES (?1, 'test', 'sb1')",
                    rusqlite::params![sid],
                )?;
                Ok(())
            })
            .await
            .unwrap();

        (db, store, session_id)
    }

    fn make_turn(thread_id: &str, user_text: &str, asst_text: &str) -> Turn {
        Turn {
            turn_id: Uuid::new_v4().to_string(),
            thread_id: thread_id.to_string(),
            user_message: ChatMessage::user(user_text),
            assistant_messages: vec![ChatMessage::assistant(asst_text)],
            created_at: now_epoch(),
        }
    }

    #[tokio::test]
    async fn test_create_and_list_threads() {
        let (_db, store, session_id) = setup().await;

        let t1 = store
            .create_thread(&session_id, Some("first"))
            .await
            .unwrap();
        let t2 = store.create_thread(&session_id, None).await.unwrap();

        assert_eq!(t1.session_id.as_str(), session_id.as_str());
        assert_eq!(t1.title.as_deref(), Some("first"));
        assert!(t2.title.is_none());

        let threads = store.list_threads(&session_id).await.unwrap();
        assert_eq!(threads.len(), 2);
        assert_eq!(threads[0].thread_id, t1.thread_id);
        assert_eq!(threads[1].thread_id, t2.thread_id);
    }

    #[tokio::test]
    async fn test_default_thread_auto_creation() {
        let (_db, store, session_id) = setup().await;

        // No threads exist yet; get_default_thread should auto-create one
        let default = store.get_default_thread(&session_id).await.unwrap();
        assert_eq!(default.title.as_deref(), Some("default"));
        assert_eq!(default.session_id.as_str(), session_id.as_str());

        // Calling again returns the same thread
        let default2 = store.get_default_thread(&session_id).await.unwrap();
        assert_eq!(default.thread_id, default2.thread_id);
    }

    #[tokio::test]
    async fn test_push_and_list_turns() {
        let (_db, store, session_id) = setup().await;
        let thread = store.create_thread(&session_id, None).await.unwrap();

        let mut turn1 = make_turn(&thread.thread_id, "hello", "hi there");
        let mut turn2 = make_turn(&thread.thread_id, "how are you", "I'm good");
        // Ensure distinct timestamps for deterministic ordering
        turn1.created_at = now_epoch();
        turn2.created_at = turn1.created_at + 1;

        store
            .push_turn(&thread.thread_id, turn1.clone())
            .await
            .unwrap();
        store
            .push_turn(&thread.thread_id, turn2.clone())
            .await
            .unwrap();

        let turns = store.list_turns(&thread.thread_id, 10, 0).await.unwrap();
        assert_eq!(turns.len(), 2);
        assert_eq!(turns[0].turn_id, turn1.turn_id);
        assert_eq!(turns[1].turn_id, turn2.turn_id);

        // Test pagination
        let page = store.list_turns(&thread.thread_id, 1, 1).await.unwrap();
        assert_eq!(page.len(), 1);
        assert_eq!(page[0].turn_id, turn2.turn_id);
    }

    #[tokio::test]
    async fn test_undo_last_turn() {
        let (_db, store, session_id) = setup().await;
        let thread = store.create_thread(&session_id, None).await.unwrap();

        let mut turn1 = make_turn(&thread.thread_id, "first", "resp1");
        let mut turn2 = make_turn(&thread.thread_id, "second", "resp2");
        // Ensure distinct timestamps so ORDER BY created_at is deterministic
        turn1.created_at = now_epoch();
        turn2.created_at = turn1.created_at + 1;
        store
            .push_turn(&thread.thread_id, turn1.clone())
            .await
            .unwrap();
        store
            .push_turn(&thread.thread_id, turn2.clone())
            .await
            .unwrap();

        // Undo removes the last turn
        let undone = store.undo_last_turn(&thread.thread_id).await.unwrap();
        assert!(undone.is_some());
        assert_eq!(undone.unwrap().turn_id, turn2.turn_id);

        // Only turn1 remains
        let turns = store.list_turns(&thread.thread_id, 10, 0).await.unwrap();
        assert_eq!(turns.len(), 1);
        assert_eq!(turns[0].turn_id, turn1.turn_id);
    }

    #[tokio::test]
    async fn test_fork_thread() {
        let (_db, store, session_id) = setup().await;
        let thread = store
            .create_thread(&session_id, Some("main"))
            .await
            .unwrap();

        let mut turn1 = make_turn(&thread.thread_id, "q1", "a1");
        let base_ts = turn1.created_at;
        let turn1_id = turn1.turn_id.clone();

        let mut turn2 = make_turn(&thread.thread_id, "q2", "a2");
        turn2.created_at = base_ts + 1;

        let mut turn3 = make_turn(&thread.thread_id, "q3", "a3");
        turn3.created_at = base_ts + 2;

        // Ensure turn1 timestamp is deterministic
        turn1.created_at = base_ts;

        store.push_turn(&thread.thread_id, turn1).await.unwrap();
        store.push_turn(&thread.thread_id, turn2).await.unwrap();
        store.push_turn(&thread.thread_id, turn3).await.unwrap();

        // Fork from turn1 -- should copy only turn1
        let forked = store
            .fork_thread(&thread.thread_id, &turn1_id)
            .await
            .unwrap();
        assert_eq!(
            forked.parent_thread_id.as_deref(),
            Some(thread.thread_id.as_str())
        );
        assert_eq!(forked.session_id.as_str(), session_id.as_str());

        let forked_turns =
            store.list_turns(&forked.thread_id, 10, 0).await.unwrap();
        assert_eq!(forked_turns.len(), 1);

        // Original thread still has all 3 turns
        let orig_turns =
            store.list_turns(&thread.thread_id, 10, 0).await.unwrap();
        assert_eq!(orig_turns.len(), 3);
    }

    #[tokio::test]
    async fn test_get_thread_messages_order() {
        let (_db, store, session_id) = setup().await;
        let thread = store.create_thread(&session_id, None).await.unwrap();

        let turn1 = make_turn(&thread.thread_id, "user1", "asst1");
        let mut turn2 = make_turn(&thread.thread_id, "user2", "asst2");
        turn2.created_at = turn1.created_at + 1;

        store.push_turn(&thread.thread_id, turn1).await.unwrap();
        store.push_turn(&thread.thread_id, turn2).await.unwrap();

        let messages =
            store.get_thread_messages(&thread.thread_id).await.unwrap();
        // Each turn produces 2 messages (user + assistant), so 4 total
        assert_eq!(messages.len(), 4);
        // Order: user1, asst1, user2, asst2
        assert_eq!(messages[0].role, octo_types::MessageRole::User);
        assert_eq!(messages[1].role, octo_types::MessageRole::Assistant);
        assert_eq!(messages[2].role, octo_types::MessageRole::User);
        assert_eq!(messages[3].role, octo_types::MessageRole::Assistant);
    }

    #[tokio::test]
    async fn test_multiple_threads_per_session() {
        let (_db, store, session_id) = setup().await;

        let t1 = store
            .create_thread(&session_id, Some("thread-a"))
            .await
            .unwrap();
        let t2 = store
            .create_thread(&session_id, Some("thread-b"))
            .await
            .unwrap();

        // Push turns to different threads
        let turn_a = make_turn(&t1.thread_id, "qa", "aa");
        let turn_b = make_turn(&t2.thread_id, "qb", "ab");
        store.push_turn(&t1.thread_id, turn_a).await.unwrap();
        store.push_turn(&t2.thread_id, turn_b).await.unwrap();

        let turns_a = store.list_turns(&t1.thread_id, 10, 0).await.unwrap();
        let turns_b = store.list_turns(&t2.thread_id, 10, 0).await.unwrap();
        assert_eq!(turns_a.len(), 1);
        assert_eq!(turns_b.len(), 1);

        let threads = store.list_threads(&session_id).await.unwrap();
        assert_eq!(threads.len(), 2);
    }

    #[tokio::test]
    async fn test_undo_empty_thread() {
        let (_db, store, session_id) = setup().await;
        let thread = store.create_thread(&session_id, None).await.unwrap();

        // Undo on empty thread returns None
        let result = store.undo_last_turn(&thread.thread_id).await.unwrap();
        assert!(result.is_none());
    }
}
