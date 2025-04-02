use tokio_tungstenite::{connect_async, tungstenite::protocol::Message as WsMessage};
use futures_util::{SinkExt, StreamExt};
use serde_json::{self, json};
use crate::core::{Message as CoreMessage, Metadata};
use anyhow::{Result, Context};
use tokio::net::TcpStream;
use tokio_tungstenite::WebSocketStream;
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

    pub async fn publish(&mut self, topic: &str, payload: &[u8]) -> Result<()> {
        let publish_msg = json!({
            "type": "publish",
            "topic": topic,
            "payload": String::from_utf8_lossy(payload).to_string()
        });
        
        self.sender
            .send(WsMessage::Text(publish_msg.to_string()))
            .await
            .context("Failed to send publish message")?;
        
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

    pub async fn acknowledge(&mut self, starting_seq: u64) -> Result<()> {
        let subscribe_msg = json!({
            "type": "ack",
            "seq": starting_seq
        }); 
        
        self.sender
            .send(WsMessage::Text(subscribe_msg.to_string()))
            .await
            .context("Failed to send subscribe message")?;
        
        Ok(())
    }    

    pub async fn next_message(&mut self) -> Option<Result<CoreMessage>> {
        loop {
            match self.receiver.next().await {
                Some(Ok(WsMessage::Text(text))) => {
                    if let Ok(msg) = serde_json::from_str::<CoreMessage>(&text) {
                        return Some(Ok(msg));
                    }
                    continue; 
                }
                Some(Ok(_)) => continue, // Ignore non-text messages
                Some(Err(e)) => return Some(Err(e.into())),
                None => return None,
            }
        }
    }
}