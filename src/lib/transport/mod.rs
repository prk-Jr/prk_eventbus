pub mod tls;

pub use tls::*;

use async_trait::async_trait;
use tokio::sync::broadcast;
use uuid::Uuid;
use crate::core::{EventBusError, Message};

#[async_trait]
pub trait Transport: Send + Sync {
    async fn subscribe(&self, pattern: &str, starting_seq: Option<u64>) -> Result<broadcast::Receiver<Message>, EventBusError>;
    async fn send(&self, message: Message, ttl: Option<i64>) -> Result<(), EventBusError>;
    async fn receive(&self) -> Result<Message, EventBusError>;
    async fn send_batch(&self, messages: Vec<Message>, ttl: Option<i64>) -> Result<(), EventBusError>;
    async fn acknowledge(&self, seq: u64, message_id: Uuid) -> Result<(), EventBusError>;
}