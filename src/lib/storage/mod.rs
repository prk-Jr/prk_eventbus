#[cfg(feature = "storage")]
pub mod sqlite;
pub mod dummy_storage;

use async_trait::async_trait;
use uuid::Uuid;
use crate::core::Message;

#[async_trait]
pub trait Storage: Send + Sync {
    async fn insert_message(&self, msg: &Message,ttl: Option<i64>) -> anyhow::Result<u64>;
    async fn insert_messages(&self, messages: &[Message],ttl: Option<i64>) -> anyhow::Result<Vec<u64>>;
    async fn get_messages_after(&self, seq: u64, pattern: &str) -> anyhow::Result<Vec<Message>>;
    async fn acknowledge_message(&self, seq: u64, message_id: Uuid) -> anyhow::Result<()>;
    async fn is_message_processed(&self, message_id: Uuid) -> anyhow::Result<bool>;
}