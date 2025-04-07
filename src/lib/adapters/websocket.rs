use axum::{
    extract::{
        ws::{Message as AxumMessage, WebSocket},
        WebSocketUpgrade,
    },
    response::Response,
    routing::get,
    Router,
};
use futures::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};
use crate::{core::{EventBus, Message as CoreMessage}, transport::Transport, core::error::EventBusError};
use crate::storage::Storage;
use uuid::Uuid;
use serde_json::Value;

#[cfg(feature = "tracing")]
use tracing::{info, debug, instrument, warn};

#[derive(Clone)]
pub struct WsConfig {
    pub channel_capacity: usize,
    pub auto_ack: bool,
}

impl Default for WsConfig {
    fn default() -> Self {
        Self {
            channel_capacity: 100,
            auto_ack: true,
        }
    }
}

pub struct WsTransport<S: Storage + Send + Sync + 'static> {
    storage: Option<Arc<S>>,
    bus: Arc<EventBus<S>>,
}

impl<S: Storage + Send + Sync + 'static> WsTransport<S> {
    pub fn new(storage: Option<Arc<S>>, config: WsConfig) -> Self {
        let bus = Arc::new(EventBus::new(storage.clone(), config.auto_ack));
        Self { storage, bus }
    }

    pub async fn serve(&self, addr: &str) -> Result<(), EventBusError> {
        let storage = self.storage.clone();
        let bus = self.bus.clone();
        let app = Router::new().route("/ws", get({
            let storage = storage.clone();
            move |ws| Self::handle_ws(ws, bus.clone(), storage.clone())
        }));
        let listener = tokio::net::TcpListener::bind(addr).await?;
        #[cfg(feature = "tracing")]
        info!(addr = %addr, "WebSocket server started");
        axum::serve(listener, app).await?;
        Ok(())
    }

    async fn handle_ws(
        ws: WebSocketUpgrade,
        bus: Arc<EventBus<S>>,
        storage: Option<Arc<S>>,
    ) -> Response {
        ws.on_upgrade(move |socket| Self::handle_connection(socket, bus, storage))
    }

    #[cfg_attr(feature = "tracing", instrument(skip(bus, storage, data, ttl)))]
async fn handle_publish(
    bus: &Arc<EventBus<S>>,
    storage: &Option<Arc<S>>,
    data: Value,
    ttl: Option<i64>,
) -> Result<(), EventBusError> {
    let topic = data.get("topic")
        .and_then(Value::as_str)
        .ok_or_else(|| EventBusError::InvalidMessage("Missing topic".into()))?;
    let payload = data.get("payload")
        .and_then(Value::as_str)
        .ok_or_else(|| EventBusError::InvalidMessage("Missing payload".into()))?;
    let message_id = data.get("message_id")
        .and_then(Value::as_str)
        .map(|id| Uuid::parse_str(id).unwrap_or_else(|_| Uuid::new_v4()))
        .unwrap_or_else(Uuid::new_v4);

    #[cfg(feature = "tracing")]
    debug!(topic = %topic, payload = %payload, message_id = %message_id, "Processing publish request");

    if let Some(storage) = storage {
        if storage.is_message_processed(message_id).await? {
            #[cfg(feature = "tracing")]
            debug!(message_id = %message_id, "Skipping duplicate message");
            return Ok(());
        }
    }

    let payload_bytes = payload.to_string().into_bytes();
    bus.publish(topic, message_id,  payload_bytes, ttl).await?;

    #[cfg(feature = "tracing")]
    debug!(topic = %topic, message_id = %message_id, "Message published successfully");
    Ok(())
}

    #[cfg_attr(feature = "tracing", instrument(skip(socket, bus, storage)))]
    async fn handle_connection(
        socket: WebSocket,
        bus: Arc<EventBus<S>>,
        storage: Option<Arc<S>>,
    ) {
        let (sender, mut receiver) = socket.split();
        let sender = Arc::new(Mutex::new(sender));

        #[cfg(feature = "tracing")]
        info!("New WebSocket connection established");

        while let Some(msg) = receiver.next().await {
            match msg {
                Ok(AxumMessage::Text(text)) => {
                    #[cfg(feature = "tracing")]
                    debug!(message = %text, "Received WebSocket message");
                    match serde_json::from_str::<WsMessage>(&text) {
                        Ok(ws_msg) => {
                            match ws_msg {
                                WsMessage::Subscribe { pattern, starting_seq } => {
                                    #[cfg(feature = "tracing")]
                                    debug!(pattern = %pattern, starting_seq = ?starting_seq, "Processing subscribe");
                                    let rx = bus.subscribe(&pattern, starting_seq).await;
                                    let sender_clone = Arc::clone(&sender);
                                    tokio::spawn(async move {
                                        let mut sender_lock = sender_clone.lock().await;
                                        Self::forward_messages(rx, &mut *sender_lock).await;
                                    });
                                }
                                WsMessage::Publish { data, ttl } => {
                                    if let Err(e) = Self::handle_publish(&bus, &storage, serde_json::to_value(data).unwrap(), ttl).await {
                                        #[cfg(feature = "tracing")]
                                        warn!(error = %e, "Failed to handle publish");
                                    }
                                }
                                WsMessage::Acknowledge { seq, message_id } => {
                                    #[cfg(feature = "tracing")]
                                    debug!(seq = seq, message_id = %message_id, "Received acknowledge");
                                    if let Some(storage) = &storage {
                                        if let Err(e) = storage.acknowledge_message(seq, message_id).await {
                                            eprintln!("Failed to acknowledge message {}: {}", seq, e);
                                        }
                                        #[cfg(feature = "tracing")]
                                        debug!(seq = seq, message_id = %message_id, "Acknowledged message");
                                    }
                                }
                                WsMessage::PublishBulk { messages, ttl } => {
                                    #[cfg(feature = "tracing")]
                                    debug!(count = messages.len(), "Received publish_batch");
                                    let mut msgs = Vec::new();
                                    for m in messages {
                                        let payload = m.payload.into_bytes();
                                        msgs.push((m.topic.clone(), payload));
                                    }
                                    if let Err(e) = bus.publish_batch(msgs, ttl).await {
                                        #[cfg(feature = "tracing")]
                                        warn!(error = %e, "Failed to publish batch");
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            #[cfg(feature = "tracing")]
                            warn!(error = %e, message = %text, "Failed to deserialize WebSocket message");
                        }
                    }
                }
                Ok(AxumMessage::Close(_)) => {
                    #[cfg(feature = "tracing")]
                    info!("WebSocket connection closed");
                    break;
                }
                Ok(_) => continue,
                Err(e) => {
                    #[cfg(feature = "tracing")]
                    warn!(error = %e, "WebSocket message error");
                    break;
                }
            }
        }
    }

    #[cfg_attr(feature = "tracing", instrument(skip(rx, sender)))]
    async fn forward_messages(
        mut rx: broadcast::Receiver<CoreMessage>,
        sender: &mut (impl SinkExt<AxumMessage> + Unpin),
    ) {
        while let Ok(msg) = rx.recv().await {
            #[cfg(feature = "tracing")]
            debug!(topic = %msg.topic, message_id = %msg.message_id, "Received message for forwarding");
            if let Ok(json_msg) = serde_json::to_string(&msg) {
                if let Err(e) = sender.send(AxumMessage::Text(json_msg.into())).await {
                    #[cfg(feature = "tracing")]
                    warn!(target: "forward_messages", "Failed to send message to subscriber");
                    break;
                }
                #[cfg(feature = "tracing")]
                debug!(topic = %msg.topic, message_id = %msg.message_id, "Forwarded message to subscriber");
            }
        }
    }
}

#[async_trait::async_trait]
impl<S: Storage + Send + Sync + 'static> Transport for WsTransport<S> {
    async fn send(&self, message: CoreMessage, ttl: Option<i64>) -> Result<(), EventBusError> {
        self.bus.publish(&message.topic,message.message_id ,message.payload.to_vec(), ttl).await?;
        Ok(())
    }

    async fn send_batch(&self, messages: Vec<CoreMessage>, ttl: Option<i64>) -> Result<(), EventBusError> {
        let message_batch: Vec<(String, Vec<u8>)> = messages
            .into_iter()
            .map(|m| (m.topic, m.payload.to_vec()))
            .collect();
        self.bus.publish_batch(message_batch, ttl).await?;
        Ok(())
    }

    async fn subscribe(&self, pattern: &str, starting_seq: Option<u64>) -> Result<broadcast::Receiver<CoreMessage>, EventBusError> {
        Ok(self.bus.subscribe(pattern, starting_seq).await)
    }

    async fn acknowledge(&self, seq: u64, message_id: uuid::Uuid) -> Result<(), EventBusError> {
        if let Some(storage) = &self.storage {
            storage.acknowledge_message(seq, message_id).await?;
        } else {
            eprintln!("No storage provided; acknowledgment for message {} skipped", seq);
        }
        Ok(())
    }

    async fn receive(&self) -> Result<CoreMessage, EventBusError> {
        Err(EventBusError::NotImplemented)
    }
}

#[derive(serde::Deserialize)]
#[serde(tag = "type")]
enum WsMessage {
    #[serde(rename = "subscribe")]
    Subscribe { pattern: String, starting_seq: Option<u64> },
    #[serde(rename = "publish")]
    Publish { data: PublishArgs, ttl: Option<i64> },
    #[serde(rename = "publish_batch")]
    PublishBulk { messages: Vec<PublishArgs>, ttl: Option<i64> },
    #[serde(rename = "ack")]
    Acknowledge { seq: u64, message_id: uuid::Uuid },
}

#[derive(serde::Deserialize, serde::Serialize)]
struct PublishArgs {
    topic: String,
    payload: String,
    #[serde(default)]
    message_id: Option<String>,
}