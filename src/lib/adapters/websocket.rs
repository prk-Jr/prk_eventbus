use axum::{
    extract::{
        ws::{Message as AxumMessage, WebSocket},
        WebSocketUpgrade,
    },
    response::Response,
    routing::get,
    Router,
};
use bytes::Bytes;
use futures::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, Mutex};
use crate::{core::{EventBus, Message as CoreMessage}, transport::Transport, core::error::EventBusError};
use crate::storage::Storage;

#[derive(Clone)]
pub struct WsConfig {
    pub channel_capacity: usize, // Capacity for mpsc channel
    pub auto_ack: bool,          // Whether to auto-acknowledge messages
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
    receiver_tx: mpsc::Sender<CoreMessage>,
    receiver_rx: Arc<Mutex<mpsc::Receiver<CoreMessage>>>,
}

impl<S: Storage + Send + Sync + 'static> WsTransport<S> {
    pub fn new(storage: Option<Arc<S>>, config: WsConfig) -> Self {
        let bus = Arc::new(EventBus::new(storage.clone(), config.auto_ack));
        let (receiver_tx, receiver_rx) = mpsc::channel(config.channel_capacity);
        Self {
            storage,
            bus,
            receiver_tx,
            receiver_rx: Arc::new(Mutex::new(receiver_rx)),
        }
    }

    pub async fn serve(&self, addr: &str) -> Result<(), EventBusError> {
        let storage = self.storage.clone();
        let bus = self.bus.clone();
        let receiver_tx = self.receiver_tx.clone();
        let app = Router::new().route("/ws", get({
            let storage = storage.clone();
            move |ws| Self::handle_ws(ws, bus.clone(), storage.clone(), receiver_tx.clone())
        }));
        let listener = tokio::net::TcpListener::bind(addr).await?;
        axum::serve(listener, app).await?;
        Ok(())
    }

    async fn handle_ws(
        ws: WebSocketUpgrade,
        bus: Arc<EventBus<S>>,
        storage: Option<Arc<S>>,
        receiver_tx: mpsc::Sender<CoreMessage>,
    ) -> Response {
        ws.on_upgrade(move |socket| Self::handle_connection(socket, bus, storage, receiver_tx))
    }

    async fn handle_connection(
        socket: WebSocket,
        bus: Arc<EventBus<S>>,
        storage: Option<Arc<S>>,
        receiver_tx: mpsc::Sender<CoreMessage>,
    ) {
        let (sender, mut receiver) = socket.split();
        let sender = Arc::new(Mutex::new(sender));

        while let Some(Ok(msg)) = receiver.next().await {
            match msg {
                AxumMessage::Text(text) => {
                    if let Ok(ws_msg) = serde_json::from_str::<WsMessage>(&text) {
                        match ws_msg {
                            WsMessage::Subscribe { pattern, starting_seq } => {
                                let rx = bus.subscribe(&pattern, starting_seq).await;
                                let sender_clone = Arc::clone(&sender);
                                let storage_clone = storage.clone();
                                tokio::spawn(async move {
                                    let mut sender_lock = sender_clone.lock().await;
                                    Self::forward_messages(rx, &mut *sender_lock, storage_clone).await;
                                });
                            }
                            WsMessage::Publish { data, ttl } => {
                                let payload = Bytes::copy_from_slice( &data.payload.into_bytes());
                                let message = CoreMessage {
                                    seq: 0,
                                    message_id: uuid::Uuid::new_v4(),
                                    topic: data.topic.clone(),
                                    payload,
                                    metadata: crate::core::Metadata {
                                        timestamp: chrono::Utc::now().timestamp_millis(),
                                        content_type: "text/plain".to_string(),
                                    },
                                };
                                let _ = bus.publish(&data.topic, message.payload.to_vec(), ttl).await;
                                let _ = receiver_tx.send(message).await;
                            }
                            WsMessage::Acknowledge { seq, message_id } => {
                                if let Some(storage) = &storage {
                                    if let Err(e) = storage.acknowledge_message(seq, message_id).await {
                                        eprintln!("Failed to acknowledge message {}: {}", seq, e);
                                    }
                                }
                            }
                            WsMessage::PublishBulk { messages, ttl } => {
                                let mut msgs = Vec::new();
                                let mut core_messages = Vec::new();
                                for m in messages {
                                    let payload = m.payload.into_bytes();
                                    msgs.push((m.topic.clone(), payload.clone()));
                                let payload = Bytes::copy_from_slice( &payload);

                                    core_messages.push(CoreMessage {
                                        seq: 0,
                                        message_id: uuid::Uuid::new_v4(),
                                        topic: m.topic,
                                        payload,
                                        metadata: crate::core::Metadata {
                                            timestamp: chrono::Utc::now().timestamp_millis(),
                                            content_type: "text/plain".to_string(),
                                        },
                                    });
                                }
                                let _ = bus.publish_batch(msgs, ttl).await;
                                for msg in core_messages {
                                    let _ = receiver_tx.send(msg).await;
                                }
                            }
                        }
                    }
                }
                AxumMessage::Close(_) => break,
                _ => continue,
            }
        }
    }

    async fn forward_messages(
        mut rx: broadcast::Receiver<CoreMessage>,
        sender: &mut (impl SinkExt<AxumMessage> + Unpin),
        storage: Option<Arc<S>>,
    ) {
        while let Ok(msg) = rx.recv().await {
            if let Ok(json_msg) = serde_json::to_string(&msg) {
                let _ = sender.send(AxumMessage::Text(json_msg.into())).await;
                if storage.is_none() {
                    eprintln!("Storage not provided; skipping ack for message {}", msg.seq);
                }
            }
        }
    }
}

#[async_trait::async_trait]
impl<S: Storage + Send + Sync + 'static> Transport for WsTransport<S> {
    async fn send(&self, message: CoreMessage, ttl: Option<i64>) -> Result<(), EventBusError> {
        self.bus.publish(&message.topic, message.payload.to_vec(), ttl).await?;
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
        let mut rx = self.receiver_rx.lock().await;
        rx.recv()
            .await
            .ok_or(EventBusError::ChannelClosed)
    }
}

#[derive(serde::Deserialize)]
#[serde(tag = "type")]
enum WsMessage {
    Subscribe { pattern: String, starting_seq: Option<u64> },
    Publish { data: PublishArgs, ttl: Option<i64> },
    PublishBulk { messages: Vec<PublishArgs>, ttl: Option<i64> },
    Acknowledge { seq: u64, message_id: uuid::Uuid },
}

#[derive(serde::Deserialize)]
struct PublishArgs {
    topic: String,
    payload: String,
}