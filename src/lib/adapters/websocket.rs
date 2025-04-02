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
use crate::core::{EventBus, Message as CoreMessage};
use crate::storage::Storage; // Your Storage trait

pub struct WsTransport<S: Storage + Send + Sync + 'static> {
    storage: Option<Arc<S>>,
}

impl<S: Storage + Send + Sync + 'static> WsTransport<S> {
    // The storage parameter is optional.
    pub fn new(storage: Option<Arc<S>>) -> Self {
        Self { storage }
    }

    pub async fn serve(&self, addr: &str) -> anyhow::Result<()> {
        let storage = self.storage.clone();
        let bus = Arc::new(EventBus::new(self.storage.clone()));
        let app = Router::new().route("/ws", get({
            let storage = storage.clone();
            move |ws| Self::handle_ws(ws, bus.clone(), storage.clone())
        }));
        let listener = tokio::net::TcpListener::bind(addr).await?;
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

    async fn handle_connection(
        socket: WebSocket,
        bus: Arc<EventBus<S>>,
        storage: Option<Arc<S>>,
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
                                // Clone storage Option so it can be moved into the task.
                                let storage_clone = storage.clone();
                                tokio::spawn(async move {
                                    let mut sender_lock = sender_clone.lock().await;
                                    Self::forward_messages(rx, &mut *sender_lock, storage_clone).await;
                                });
                            }
                            WsMessage::Publish { topic, payload } => {
                                let _ = bus.publish(&topic, payload.into_bytes()).await;
                            }
                            WsMessage::Acknowledge { seq } => {
                                if let Some(storage) = &storage {
                                    if let Err(e) = storage.acknowledge_message(seq).await {
                                        eprintln!("Failed to acknowledge message {}: {}", seq, e);
                                    }
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
                // Send the message to the client without auto-acknowledgment.
                let _ = sender.send(AxumMessage::Text(json_msg.into())).await;
                // The client should acknowledge receipt.
                // If you want to auto-acknowledge messages when no storage is provided,
                // you can add custom behavior here.
                if storage.is_none() {
                    eprintln!("Storage not provided; skipping ack for message {}", msg.seq);
                }
            }
        }
    }
}

#[derive(serde::Deserialize)]
#[serde(tag = "type")]
enum WsMessage {
    #[serde(rename = "subscribe")]
    Subscribe { pattern: String, starting_seq: Option<u64> },
    #[serde(rename = "publish")]
    Publish { topic: String, payload: String },
    #[serde(rename = "ack")]
    Acknowledge { seq: u64 },
}
