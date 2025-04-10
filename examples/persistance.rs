use std::time::Duration;
use prk_eventbus::{adapters::{WsConfig, WsTransport}, client::{ClientConfig, EventBusClient}, core::error::EventBusError, storage::sqlite::SQLiteStorage};
use tokio::task::JoinHandle;

#[tokio::main]
async fn main() -> Result<(), EventBusError> {
    #[cfg(feature = "tracing")]
    {
        tracing_subscriber::fmt()
            .with_env_filter(std::env::var("RUST_LOG").unwrap_or_else(|_| "debug".to_string()))
            .init();
    }

    let storage = SQLiteStorage::new("test.db", 100).await?;
    let storage = std::sync::Arc::new(storage);

    // First run: Publish messages
    let server_handle: JoinHandle<()> = tokio::spawn({
        let storage = storage.clone();
        async move {
            let ws_config = WsConfig {
                channel_capacity: 100,
                auto_ack: false, // Manual acknowledgment for persistence
            };
            let transport: WsTransport<SQLiteStorage> = WsTransport::new(Some(storage), ws_config);
            transport.serve("127.0.0.1:3000").await.unwrap();
        }
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    let config = ClientConfig {
        url: "ws://127.0.0.1:3000/ws".to_string(),
        reconnect_interval: Duration::from_secs(2),
        max_retries: 5,
    };

    let mut publisher = EventBusClient::connect(config.clone()).await?;
    publisher.publish("chat.user1", "Persistent message 1",None, Some(3600)).await?;
    publisher.publish("chat.user1", "Persistent message 2",None, Some(3600)).await?;
    tokio::time::sleep(Duration::from_millis(100)).await;
    drop(publisher);
    server_handle.abort();

    // Second run: Retrieve persisted messages
    let server_handle: JoinHandle<()> = tokio::spawn({
        let storage = storage.clone();
        async move {
            let ws_config = WsConfig {
                channel_capacity: 100,
                auto_ack: false,
            };
            let transport: WsTransport<SQLiteStorage> = WsTransport::new(Some(storage), ws_config);
            transport.serve("127.0.0.1:3000").await.unwrap();
        }
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    let mut subscriber = EventBusClient::connect(config).await?;
    subscriber.subscribe("chat.user1", Some(0)).await?; // Start from seq 0
    let sub_handle: JoinHandle<Result<(), EventBusError>> = tokio::spawn(async move {
        #[cfg(feature = "tracing")]
        tracing::debug!("Subscriber task started");
        let mut received = 0;
        loop {
            let mut messages = subscriber.messages();
            match tokio::time::timeout(Duration::from_secs(3), messages.next()).await {
                Ok(Ok(msg)) => {
                    let payload = String::from_utf8_lossy(&msg.payload);
                    println!("Subscriber received: [{}] {} (seq: {})", msg.topic, payload, msg.seq);
                    subscriber.acknowledge(msg.seq, msg.message_id).await?;
                    received += 1;
                    if received == 2 { break; }
                }
                Ok(Err(e)) => return Err(e),
                Err(_) => break,
            }
        }
        Ok(())
    });

    tokio::time::sleep(Duration::from_secs(2)).await;
    let  _ =  sub_handle.await;
    server_handle.abort();

    println!("Persistence test completed!");
    std::fs::remove_file("test.db")?; // Clean up
    Ok(())
}