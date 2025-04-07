use bytes::Bytes;
use serde::{Serialize, Deserialize};
#[cfg(feature = "storage")]
use sqlx::prelude::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "storage")]
#[derive(FromRow )]

pub struct Message {
    pub seq: u64,
    pub message_id: Uuid,
    pub topic: String,
    pub payload: Bytes, 
    pub metadata: Metadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metadata {
    pub timestamp: i64,
    pub content_type: String,
}