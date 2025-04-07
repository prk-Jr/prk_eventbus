use std::sync::Arc;
use bytes::Bytes;
use tokio::sync::Semaphore;
use sqlx::{migrate::MigrateDatabase, sqlite::SqlitePoolOptions, Sqlite, SqlitePool};
use uuid::Uuid;
use async_trait::async_trait;
use anyhow::Result;

use crate::core::Message;

use super::Storage;

pub struct SQLiteStorage {
    pool: SqlitePool,
    semaphore: Arc<Semaphore>,
}

impl SQLiteStorage {
    pub async fn new(path: &str, max_concurrent_ops: usize) -> Result<Self> {
        if !Sqlite::database_exists(path).await.unwrap_or(false) {
            println!("Creating database {}", path);
            match Sqlite::create_database(path).await {
                Ok(_) => println!("Create db success"),
                Err(error) => panic!("error: {}", error),
            }
        } else {
            println!("Database already exists");
        }
        Self::migrate(path,max_concurrent_ops).await
    }
    pub async fn new_memory(max_concurrent_ops: usize) -> Result<Self> {
        Self::migrate("sqlite::memory:",max_concurrent_ops).await
    }

    async fn migrate(path: &str, max_concurrent_ops: usize) -> Result<Self> {
        let pool = SqlitePoolOptions::new()
            .max_connections(max_concurrent_ops as u32)
            .connect(path)
            .await?;

        // Initialize the database schema
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS messages (
                seq INTEGER PRIMARY KEY AUTOINCREMENT,
                topic TEXT NOT NULL,
                payload BLOB NOT NULL,
                metadata TEXT NOT NULL,
                ttl INTEGER NOT NULL,
                status TEXT NOT NULL DEFAULT 'pending'
            )"
        )
        .execute(&pool)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS processed_messages (
                message_id TEXT PRIMARY KEY
            )"
        )
        .execute(&pool)
        .await?;

        Ok(Self {
            pool,
            semaphore: Arc::new(Semaphore::new(max_concurrent_ops)),
        })
    }

    // async fn is_message_processed(&self, message_id: Uuid) -> Result<bool> {
    //     let _permit = self.semaphore.acquire().await?;
    //     let exists = sqlx::query_scalar::<_, bool>(
    //         "SELECT EXISTS(SELECT 1 FROM processed_messages WHERE message_id = ?)"
    //     )
    //     .bind(message_id.to_string())
    //     .fetch_one(&self.pool)
    //     .await?;
    //     Ok(exists)
    // }
}

#[async_trait]
impl Storage for SQLiteStorage {
    async fn insert_message(&self, msg: &Message, ttl: Option<i64>) -> Result<u64> {
        self.insert_messages(&[msg.clone()], ttl).await.map(|seqs| seqs[0])
    }

    async fn insert_messages(&self, messages: &[Message], ttl: Option<i64>) -> Result<Vec<u64>> {
        let _permit = self.semaphore.acquire().await?;
        let ttl = ttl.unwrap_or(0);
        let mut tx = self.pool.begin().await?;
        let mut seqs = Vec::new();

        for msg in messages {
            let seq = sqlx::query(
                "INSERT INTO messages (topic, payload, metadata, ttl, status) 
                 VALUES (?, ?, ?, ?, 'pending')"
            )
            .bind(&msg.topic)
            .bind(&msg.payload[..])
            .bind(serde_json::to_string(&msg.metadata)?)
            .bind(ttl)
            .execute(&mut *tx)
            .await?
            .last_insert_rowid();
            seqs.push(seq as u64);
        }

        tx.commit().await?;
        Ok(seqs)
    }

    async fn is_message_processed(&self, message_id: Uuid) -> Result<bool> {
        let _permit = self.semaphore.acquire().await?;
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM processed_messages WHERE message_id = ?"
        )
        .bind(message_id.to_string())
        .fetch_one(&self.pool)
        .await?;
        Ok(count > 0)
    }

    async fn acknowledge_message(&self, seq: u64, message_id: Uuid) -> Result<()> {
        let _permit = self.semaphore.acquire().await?;
        let mut tx = self.pool.begin().await?;

        sqlx::query(
            "UPDATE messages SET status = 'delivered' WHERE seq = ?"
        )
        .bind(seq as i64)
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            "INSERT OR IGNORE INTO processed_messages (message_id) VALUES (?)"
        )
        .bind(message_id.to_string())
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(())
    }

    async fn get_messages_after(&self, seq: u64, pattern: &str) -> Result<Vec<Message>> {
        let _permit = self.semaphore.acquire().await?;
        let rows = sqlx::query_as::<_, MessageRow>(
            "SELECT seq, message_id, topic, payload, metadata 
             FROM messages 
             WHERE seq >= ? AND status = 'pending' 
             AND (ttl = 0 OR strftime('%s', 'now') - CAST(json_extract(metadata, '$.timestamp') AS INTEGER) < ttl)"
        )
        .bind(seq as i64)
        .fetch_all(&self.pool)
        .await?;
        
    
        let messages = rows.into_iter()
            .filter(|row| pattern_matches(&row.topic, pattern))
            .map(|row| row.into_message())
            .collect::<Result<Vec<_>>>()?;
        Ok(messages)
    }
}

// Helper struct for SQLx row mapping
#[derive(sqlx::FromRow)]
struct MessageRow {
    seq: i64,
    message_id: String,
    topic: String,
    payload: Vec<u8>,
    metadata: String,
}

impl MessageRow {
    fn into_message(self) -> Result<Message> {
        Ok(Message {
            seq: self.seq as u64,
            message_id: Uuid::parse_str(&self.message_id)?,
            topic: self.topic,
            payload: Bytes::from(self.payload),
            metadata: serde_json::from_str(&self.metadata)?,
        })
    }
}

// Same pattern matching logic as your original
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