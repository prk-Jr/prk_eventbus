
use uuid::Uuid;
use async_trait::async_trait;
use anyhow::Result;

use crate::core::Message;

use super::Storage;

pub struct NoStorage;

#[async_trait]
impl Storage for NoStorage {
    async fn insert_message(&self, _msg: &Message, _ttl: Option<i64>) -> Result<u64> {
        Ok(0) // Dummy sequence number
    }

    async fn insert_messages(&self, _messages: &[Message], _ttl: Option<i64>) -> Result<Vec<u64>> {
        Ok(vec![0; _messages.len()]) // Dummy sequence numbers
    }

    async fn acknowledge_message(&self, _seq: u64, _message_id: Uuid) -> Result<()> {
        Ok(()) // No-op
    }

    async fn get_messages_after(&self, _seq: u64, _pattern: &str) -> Result<Vec<Message>> {
        Ok(vec![]) // No messages stored
    }

    async fn is_message_processed(&self, _message_id: Uuid) -> Result<bool> {
        Ok(false) // Nothing is processed
    }
}