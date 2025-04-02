use std::sync::Arc;
use tokio::sync::Mutex;
use rusqlite::{Connection, params};
use crate::core::Message;
use crate::storage::Storage;
use async_trait::async_trait;
use anyhow::Result;

pub struct SQLiteStorage {
    conn: Arc<Mutex<Connection>>,
}

impl SQLiteStorage {
    pub fn new(path: &str) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS messages (
                  seq INTEGER PRIMARY KEY AUTOINCREMENT,
                  topic TEXT,
                  payload BLOB,
                  metadata TEXT,
                  status TEXT DEFAULT 'pending');
                ",
            [],
        )?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }
}

#[async_trait]
impl Storage for SQLiteStorage {
    async fn insert_message(&self, msg: &Message) -> Result<u64> {
        let conn = self.conn.clone();
        let msg = msg.clone();
        let seq = tokio::task::spawn_blocking(move || {
            let conn = conn.blocking_lock();
            conn.execute(
                "INSERT INTO messages (topic, payload, metadata, status) VALUES (?, ?, ?, 'pending')",
                params![msg.topic, msg.payload, serde_json::to_string(&msg.metadata)?],
            )?;
            Ok::<u64, anyhow::Error>(conn.last_insert_rowid() as u64)
        })
        .await??;
        Ok(seq)
    }

    async fn acknowledge_message(&self, seq: u64) -> Result<()> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.blocking_lock();
            conn.execute("UPDATE messages SET status = 'delivered' WHERE seq = ?", params![seq])?;
            Ok::<(), anyhow::Error>(())
        })
        .await??;
        Ok(())
    }

    async fn get_messages_after(&self, seq: u64, pattern: &str) -> Result<Vec<Message>> {
        let conn = self.conn.clone();
        let pattern = pattern.to_string();
        let messages = tokio::task::spawn_blocking(move || {
            let conn = conn.blocking_lock();
            let mut stmt = conn.prepare(
                "SELECT seq, topic, payload, metadata FROM messages WHERE seq > ?",
            )?;
            let rows = stmt.query_map([seq], |row| {
                Ok(Message {
                    seq: row.get(0)?,
                    topic: row.get(1)?,
                    payload: row.get(2)?,
                    metadata: serde_json::from_str(&row.get::<_, String>(3)?)
                        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?,
                })
            })?;

            let mut messages = Vec::new();
            for row in rows {
                let msg = row.map_err(|e| anyhow::anyhow!(e))?;
                if pattern_matches(&msg.topic, &pattern) {
                    messages.push(msg);
                }
            }
            Ok::<Vec<Message>, anyhow::Error>(messages)
        })
        .await??;
        Ok(messages)
    }
}

fn pattern_matches(topic: &str, pattern: &str) -> bool {
    let topic_segments = topic.split('.').collect::<Vec<_>>();
    let pattern_segments = pattern.split('.').collect::<Vec<_>>();
    if pattern_segments.contains(&"#") {
        let index = pattern_segments.iter().position(|&s| s == "#").unwrap();
        let prefix = &pattern_segments[0..index];
        let suffix = if index + 1 < pattern_segments.len() {
            &pattern_segments[index + 1..]
        } else {
            &[]
        };
        if topic_segments.len() < prefix.len() + suffix.len() {
            return false;
        }
        for (t, p) in topic_segments.iter().take(prefix.len()).zip(prefix.iter()) {
            if *p != "*" && p != t {
                return false;
            }
        }
        for (t, p) in topic_segments.iter().rev().take(suffix.len()).zip(suffix.iter().rev()) {
            if *p != "*" && p != t {
                return false;
            }
        }
        true
    } else if pattern_segments.len() == topic_segments.len() {
        for (t, p) in topic_segments.iter().zip(pattern_segments.iter()) {
            if *p != "*" && p != t {
                return false;
            }
        }
        true
    } else {
        false
    }
}