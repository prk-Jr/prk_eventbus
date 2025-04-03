use std::sync::Arc;
use bytes::Bytes;
use tokio::sync::{Mutex, Semaphore};
use rusqlite::{Connection, params};
use uuid::Uuid;
use crate::core::Message;
use crate::storage::Storage;
use async_trait::async_trait;
use anyhow::Result;

pub struct SQLiteStorage {
    conn: Arc<Mutex<Connection>>,
    semaphore: Arc<Semaphore>,
}

impl SQLiteStorage {
    pub fn new(path: &str, max_concurrent_ops: usize) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS messages (
                  seq INTEGER PRIMARY KEY AUTOINCREMENT,
                  topic TEXT,
                  payload BLOB,
                  metadata TEXT,
                  ttl INTEGER,
                  status TEXT DEFAULT 'pending')
                  ;
                ",
            [],
        )?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS processed_messages (
                message_id TEXT PRIMARY KEY
            )",
            [],
        )?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            semaphore: Arc::new(Semaphore::new(max_concurrent_ops)),
        })
    }
}

#[async_trait]
impl Storage for SQLiteStorage {
    async fn insert_message(&self, msg: &Message, ttl: Option<i64>) -> Result<u64> {
          self.insert_messages(&[msg.clone()], ttl).await.map(|seqs| seqs[0])
    }

    async fn insert_messages(&self, messages: &[Message], ttl: Option<i64>) -> Result<Vec<u64>> {
        let permit = self.semaphore.acquire().await?;
        let conn = self.conn.clone();
        let ttl = ttl.unwrap_or(0);
        let messages = messages.to_vec();
        let seqs = tokio::task::spawn_blocking(move || {
            let mut conn = conn.blocking_lock();
            let mut seqs = Vec::new();
            let tx = conn.transaction()?;
            {
                let mut stmt = tx.prepare(
                    "INSERT OR IGNORE INTO messages (message_id, topic, payload, metadata, status, ttl) 
                 VALUES (?, ?, ?, ?, 'pending', ?)",
                )?;
                for msg in &messages {
                    stmt.execute(params![
                        msg.topic,
                        &msg.payload[..],
                        serde_json::to_string(&msg.metadata)?,
                        ttl
                    ])?;
                    seqs.push(tx.last_insert_rowid() as u64);
                }
            }
            tx.commit()?;
            Ok::<Vec<u64>, anyhow::Error>(seqs)
        })
        .await??;
    drop(permit);

        Ok(seqs)
    }

    async fn is_message_processed(&self, message_id: Uuid) -> anyhow::Result<bool> {
        let permit = self.semaphore.acquire().await?;
        let conn = self.conn.clone();
        let is_processed = tokio::task::spawn_blocking(move || {
            let conn = conn.blocking_lock();
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM processed_messages WHERE message_id = ?",
                params![message_id.to_string()],
                |row| row.get(0),
            )?;
            Ok::<bool, anyhow::Error>(count > 0)
        })
        .await??;
        drop(permit);
        Ok(is_processed)
    }


    async fn acknowledge_message(&self, seq: u64, message_id: Uuid) -> anyhow::Result<()> {
        let permit = self.semaphore.acquire().await?;
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.blocking_lock();
            conn.execute(
                "UPDATE messages SET status = 'delivered' WHERE seq = ?",
                params![seq],
            )?;
            conn.execute(
                "INSERT OR IGNORE INTO processed_messages (message_id) VALUES (?)",
                params![message_id.to_string()],
            )?;
            Ok::<(), anyhow::Error>(())
        })
        .await??;
        drop(permit);
        Ok(())
    }

    async fn get_messages_after(&self, seq: u64, pattern: &str) -> anyhow::Result<Vec<Message>> {
        let permit = self.semaphore.acquire().await?;
        let conn = self.conn.clone();
        let pattern = pattern.to_string();
        let messages = tokio::task::spawn_blocking(move || {
            let conn = conn.blocking_lock();
            let mut stmt = conn.prepare(
                "SELECT seq, message_id, topic, payload, metadata 
                 FROM messages 
                 WHERE seq > ? AND status = 'pending' 
                 AND (ttl = 0 OR strftime('%s', 'now') - json_extract(metadata, '$.timestamp') < ttl)",
            )?;
            let rows = stmt.query_map([seq], |row| {
                Ok(Message {
                    seq: row.get(0)?,
                    message_id: Uuid::parse_str(&row.get::<_, String>(1)?).map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?,
                    topic: row.get(2)?,
                    payload: Bytes::from(row.get::<_, Vec<u8>>(3)?),
                    metadata: serde_json::from_str(&row.get::<_, String>(4)?).map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?,
                })
            })?;
            let mut messages = Vec::new();
            for row in rows {
                let msg = row?;
                if pattern_matches(&msg.topic, &pattern) {
                    messages.push(msg);
                }
            }
            Ok::<Vec<Message>, anyhow::Error>(messages)
        })
        .await??;
        drop(permit);
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