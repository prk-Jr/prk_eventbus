pub mod tls;

pub use tls::*;

use async_trait::async_trait;
use tokio::sync::broadcast;
use uuid::Uuid;
use crate::core::Message;

#[async_trait]
pub trait Transport: Send + Sync {
    async fn subscribe(&self, pattern: &str, starting_seq: Option<u64>) -> anyhow::Result<broadcast::Receiver<Message>>;
    async fn send(&self, message: Message, ttl: Option<i64>) -> anyhow::Result<()>;
    async fn receive(&self) -> anyhow::Result<Message>;
    async fn send_batch(&self, messages: Vec<Message>, ttl: Option<i64>) -> anyhow::Result<()>;
    async fn acknowledge(&self, seq: u64, message_id: Uuid) -> anyhow::Result<()>;
}