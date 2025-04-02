pub mod sqlite;

use async_trait::async_trait;
use crate::core::Message;

#[async_trait]
pub trait Storage: Send + Sync {
    async fn insert_message(&self, msg: &Message) -> anyhow::Result<u64>;
    async fn insert_messages(&self, messages: &[Message]) -> anyhow::Result<Vec<u64>>;
    async fn get_messages_after(&self, seq: u64, pattern: &str) -> anyhow::Result<Vec<Message>>;
    async fn acknowledge_message(&self, seq: u64) -> anyhow::Result<()>;
}