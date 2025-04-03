use std::io;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum EventBusError {
    #[error("WebSocket error: {0}")]
    WebSocket(#[from] tokio_tungstenite::tungstenite::Error),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("Storage error: {0}")]
    Storage(#[from] anyhow::Error),
    #[error("Connection failed: {0}")]
    IoError(#[from] io::Error),
    #[error("IO Error: {0}")]
    Connection(String),
    #[error("Invalid message format")]
    InvalidMessage,
    #[error("Channel closed")]
    ChannelClosed,
}