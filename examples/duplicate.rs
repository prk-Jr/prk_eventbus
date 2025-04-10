use std::sync::Arc;
use prk_eventbus::{
    adapters::{WsConfig, WsTransport},
    client::{ClientConfig, EventBusClient},
    storage::sqlite::SQLiteStorage,
};
use uuid::Uuid;

async fn connect_with_retry(config: ClientConfig) -> Result<EventBusClient, Box<dyn std::error::Error>> {
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(feature = "tracing")]
    {
        tracing_subscriber::fmt()
            .with_env_filter(std::env::var("RUST_LOG").unwrap_or_else(|_| "debug".to_string()))
            .init();
    }
    let db_path = "test_duplicate_message_handling.db";
    let storage = SQLiteStorage::new(db_path, 100).await?;
    let transport = Arc::new(WsTransport::new(Some(Arc::new(storage)), WsConfig { auto_ack: false, ..Default::default() }));
    let transport_clone = transport.clone();
    let server_handle = tokio::spawn(async move {
        transport_clone.serve("127.0.0.1:3003").await.unwrap(); // Unique port
    });

    let config = ClientConfig {
        url: "ws://127.0.0.1:3003/ws".to_string(),
        reconnect_interval: std::time::Duration::from_millis(200),
        max_retries: 5,
    };

    let mut subscriber = connect_with_retry(config.clone()).await?;
    subscriber.subscribe("chat.dedupe", None).await?;

    let mut publisher = connect_with_retry(config.clone()).await?;
    let message_id = Some(Uuid::new_v4().to_string());
    publisher.publish("chat.dedupe", "First send", message_id.clone(), None).await?;

    let msg = subscriber.next_message().await?;
    assert_eq!(String::from_utf8_lossy(&msg.payload), "First send");
    subscriber.acknowledge(msg.seq, msg.message_id).await?;

    publisher.publish("chat.dedupe", "Duplicate send", message_id, None).await?;

    let received = tokio::time::timeout(
        std::time::Duration::from_millis(500),
        subscriber.next_message(),
    ).await;

    assert!(received.is_err(), "Duplicate message should not be delivered");

    server_handle.abort();
    let _ = std::fs::remove_file(db_path);
    Ok(())
}