use thiserror::Error;

#[derive(Error, Debug)]
pub enum EventBusError {
    #[error("Topic {0} not found")]
    TopicNotFound(String),
    #[error("Serialization error: {0}")]
    SerializationError(String),
    #[error("Storage error: {0}")]
    StorageError(String),
}