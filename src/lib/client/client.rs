use tokio_tungstenite::{connect_async, tungstenite::protocol::Message as WsMessage};
use futures_util::{SinkExt, StreamExt};
use serde_json::{self, json};
use crate::core::Message as CoreMessage;
use crate::core::error::EventBusError;
use tokio::net::TcpStream;
use tokio_tungstenite::WebSocketStream;
use uuid::Uuid;
use std::time::Duration;

type WsConnection = WebSocketStream<tokio_tungstenite::MaybeTlsStream<TcpStream>>;

#[cfg(feature = "tracing")]
use tracing::{info, debug, instrument};

#[derive(Clone)]
pub struct ClientConfig {
    pub url: String,
    pub reconnect_interval: Duration,
    pub max_retries: usize,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            url: "ws://127.0.0.1:3000/ws".to_string(),
            reconnect_interval: Duration::from_secs(5),
            max_retries: 3,
        }
    }
}

pub struct EventBusClient {
    sender: futures_util::stream::SplitSink<WsConnection, WsMessage>,
    receiver: futures_util::stream::SplitStream<WsConnection>,
    config: ClientConfig,
}

impl EventBusClient {
    #[cfg_attr(feature = "tracing", instrument(skip(config)))]
    pub async fn connect(config: ClientConfig) -> Result<Self, EventBusError> {
        let (ws_stream, _) = connect_async(&config.url)
            .await
            .map_err(|e| EventBusError::Connection(e.to_string()))?;
        
        let (sender, receiver) = ws_stream.split();
        #[cfg(feature = "tracing")]
        info!(url = %config.url, "Client connected");
        Ok(Self { sender, receiver, config })
    }

    #[cfg_attr(feature = "tracing", instrument(skip(config)))]
    pub async fn connect_with_retry(config: ClientConfig) -> Result<EventBusClient, Box<dyn std::error::Error>> {
        let mut retries = config.max_retries;
        while retries > 0 {
            match EventBusClient::connect(config.clone()).await {
                Ok(client) => return Ok(client),
                Err(_) => {
                    tokio::time::sleep(config.reconnect_interval).await;
                    retries -= 1;
                }
            }
        }
        Err("Failed to connect after retries".into())
    }

    #[cfg_attr(feature = "tracing", instrument(skip(self)))]
    async fn reconnect(&mut self) -> Result<(), EventBusError> {
        let mut retries = 0;
        loop {
            match connect_async(&self.config.url).await {
                Ok((ws_stream, _)) => {
                    let (sender, receiver) = ws_stream.split();
                    self.sender = sender;
                    self.receiver = receiver;
                    #[cfg(feature = "tracing")]
                    info!(url = %self.config.url, "Client reconnected");
                    return Ok(());
                }
                Err(e) => {
                    retries += 1;
                    if retries >= self.config.max_retries {
                        return Err(EventBusError::Connection(format!("Max retries exceeded: {}", e)));
                    }
                    #[cfg(feature = "tracing")]
                    debug!(retry = retries, max = self.config.max_retries, error = %e, "Reconnection attempt failed");
                    tokio::time::sleep(self.config.reconnect_interval).await;
                }
            }
        }
    }

    // Write a function to disconnect the client
    #[cfg_attr(feature = "tracing", instrument(skip(self)))]
    pub async fn disconnect(&mut self) -> Result<(), EventBusError> {
        self.sender.close().await.map_err(EventBusError::WebSocket)?;
        #[cfg(feature = "tracing")]
        info!("Client disconnected");
        Ok(())
    }

    #[cfg_attr(feature = "tracing", instrument(skip(self)))]
    pub async fn publish(
        &mut self,
        topic: &str,
        payload: &str,
        message_id: Option<String>,
        ttl: Option<i64>,
    ) -> Result<(), EventBusError> {
        let message_id = message_id.unwrap_or(Uuid::new_v4().to_string());
        let publish_msg = json!({
            "type": "publish",
            "data": {
                "topic": topic,
                "payload": payload,
                "message_id": message_id
            },
            "ttl": ttl
        });
    
        self.sender
            .send(WsMessage::Text(publish_msg.to_string()))
            .await
            .map_err(EventBusError::WebSocket)?;
        #[cfg(feature = "tracing")]
        debug!(topic = %topic, payload = %payload, message_id = ?message_id, "Published message");
        Ok(())
    }

    #[cfg_attr(feature = "tracing", instrument(skip(self)))]
    pub async fn publish_batch(&mut self, messages: Vec<(String, String)>, ttl: Option<i64>) -> Result<(), EventBusError> {
        let messages_json = messages.into_iter().map(|(topic, payload)| {
            json!({
                "topic": topic,
                "payload": payload
            })
        }).collect::<Vec<_>>();
        
        let publish_msg = json!({
            "type": "publish_batch",
            "messages": messages_json,
            "ttl": ttl
        });
        
        self.sender
            .send(WsMessage::Text(publish_msg.to_string()))
            .await
            .map_err(EventBusError::WebSocket)?;
        #[cfg(feature = "tracing")]
        debug!(count = messages_json.len(), "Published batch of messages");
        Ok(())
    }

    #[cfg_attr(feature = "tracing", instrument(skip(self)))]
    pub async fn subscribe(&mut self, pattern: &str, starting_seq: Option<u64>) -> Result<(), EventBusError> {
        let subscribe_msg = json!({
            "type": "subscribe",
            "pattern": pattern,
            "starting_seq": starting_seq
        }); 
        
        self.sender
            .send(WsMessage::Text(subscribe_msg.to_string()))
            .await
            .map_err(EventBusError::WebSocket)?;
        
        #[cfg(feature = "tracing")]
        info!(pattern = %pattern, "Subscribed to pattern");
        Ok(())
    }    

    #[cfg_attr(feature = "tracing", instrument(skip(self)))]
    pub async fn acknowledge(&mut self, seq: u64, message_id: Uuid) -> Result<(), EventBusError> {
        let ack_msg = json!({
            "type": "ack",
            "seq": seq,
            "message_id": message_id.to_string()
        }); 
        
        self.sender
            .send(WsMessage::Text(ack_msg.to_string()))
            .await
            .map_err(EventBusError::WebSocket)?;
        #[cfg(feature = "tracing")]
        debug!(seq = seq, message_id = %message_id, "Acknowledged message");
        Ok(())
    }    

    #[cfg_attr(feature = "tracing", instrument(skip(self)))]
    pub async fn next_message(&mut self) -> Result<CoreMessage, EventBusError> {
        loop {
            match self.receiver.next().await {
                Some(Ok(WsMessage::Text(text))) => {
                    match serde_json::from_str::<CoreMessage>(&text) {
                        Ok(msg) => {
                            #[cfg(feature = "tracing")]
                            debug!(topic = %msg.topic, message_id = %msg.message_id, "Received message");
                            return Ok(msg);
                        }
                        Err(e) => return Err(EventBusError::Serialization(e)),
                    }
                }
                Some(Ok(_)) => continue,
                Some(Err(e)) => {
                    self.reconnect().await?;
                    return Err(EventBusError::WebSocket(e))
                }
                None => {
                    self.reconnect().await?;
                    continue;
                }
            }
        }
    }

    pub fn messages(&mut self) -> MessageIterator<'_> {
        MessageIterator { client: self }
    }
}

pub struct MessageIterator<'a> {
    client: &'a mut EventBusClient,
}

impl<'a> MessageIterator<'a> {
    pub async fn next(&mut self) -> Result<CoreMessage, EventBusError> {
        self.client.next_message().await
    }
}