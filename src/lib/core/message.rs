use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub seq: u64,           // Added for sequence-based persistence and replay
    pub topic: String,
    pub payload: Vec<u8>,
    pub metadata: Metadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metadata {
    pub timestamp: i64,
    pub content_type: String,
}