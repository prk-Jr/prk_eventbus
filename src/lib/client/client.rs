use tokio_tungstenite::{connect_async, tungstenite::protocol::Message as WsMessage};
use futures_util::{SinkExt, StreamExt};
use serde_json::{self, json};
use crate::core::Message as CoreMessage;
use anyhow::{Result, Context};
use tokio::net::TcpStream;
use tokio_tungstenite::WebSocketStream;
use uuid::Uuid;

type WsConnection = WebSocketStream<tokio_tungstenite::MaybeTlsStream<TcpStream>>;

pub struct EventBusClient {
    sender: futures_util::stream::SplitSink<WsConnection, WsMessage>,
    receiver: futures_util::stream::SplitStream<WsConnection>,
}

impl EventBusClient {
    pub async fn connect(url: &str) -> Result<Self> {
        let (ws_stream, _) = connect_async(url)
            .await
            .context("Failed to connect to WebSocket server")?;
        
        let (sender, receiver) = ws_stream.split();
        Ok(Self { sender, receiver })
    }

    pub async fn publish(&mut self, topic: &str, payload: &[u8], ttl: Option<i64>) -> Result<()> {
        let publish_msg = json!({
            "type": "publish",
            "data": {
                "topic": topic,
                "payload": payload
            },
            "ttl": ttl
        });
        
        self.sender
            .send(WsMessage::Text(publish_msg.to_string()))
            .await
            .context("Failed to send publish message")?;
        
        Ok(())
    }

    pub async fn publish_batch(&mut self, messages: Vec<(String, Vec<u8>)>, ttl: Option<i64>) -> Result<()> {
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
            .context("Failed to send publish_batch message")?;
        
        Ok(())
    }

    pub async fn subscribe(&mut self, pattern: &str, starting_seq: Option<u64>) -> Result<()> {
        let subscribe_msg = json!({
            "type": "subscribe",
            "pattern": pattern,
            "starting_seq": starting_seq
        }); 
        
        self.sender
            .send(WsMessage::Text(subscribe_msg.to_string()))
            .await
            .context("Failed to send subscribe message")?;
        
        Ok(())
    }    

    pub async fn acknowledge(&mut self, seq: u64, message_id: Uuid) -> Result<()> {
        let ack_msg = json!({
            "type": "ack",
            "seq": seq,
            "message_id": message_id.to_string()
        }); 
        
        self.sender
            .send(WsMessage::Text(ack_msg.to_string()))
            .await
            .context("Failed to send acknowledge message")?;
        
        Ok(())
    }    

    pub async fn next_message(&mut self) -> Option<Result<CoreMessage>> {
        loop {
            match self.receiver.next().await {
                Some(Ok(WsMessage::Text(text))) => {
                    match serde_json::from_str::<CoreMessage>(&text) {
                        Ok(mut msg) => {
                            // Note: Assuming server sends payload as base64-encoded string in JSON.
                            // If server sends raw binary in the future, handle WsMessage::Binary here.
                            return Some(Ok(msg));
                        }
                        Err(e) => return Some(Err(anyhow::anyhow!("Failed to parse message: {}", e))),
                    }
                }
                Some(Ok(WsMessage::Binary(_))) => {
                    // Placeholder for future raw binary support if server switches from JSON
                    eprintln!("Binary messages not yet supported; ignoring");
                    continue;
                }
                Some(Ok(_)) => continue, // Ignore other message types
                Some(Err(e)) => return Some(Err(e.into())),
                None => return None,
            }
        }
    }
}