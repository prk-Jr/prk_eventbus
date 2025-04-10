use std::time::Duration;
use prk_eventbus::{
    adapters::{WsConfig, WsTransport},
    client::{ClientConfig, EventBusClient},
    core::error::EventBusError,
    storage::sqlite::SQLiteStorage,
};
use tokio::task::JoinHandle;

#[tokio::main]
async fn main() -> Result<(), EventBusError> {
    #[cfg(feature = "tracing")]
    {
        tracing_subscriber::fmt()
            .with_env_filter(std::env::var("RUST_LOG").unwrap_or_else(|_| "debug".to_string()))
            .init();
    }

    // Start server with SQLite persistence
    let server_handle: JoinHandle<()> = tokio::spawn(async {
        let ws_config = WsConfig {
            channel_capacity: 100,
            auto_ack: false,
        };
        let storage = SQLiteStorage::new("eventbus_test.db", 100).await.unwrap();
        let storage = std::sync::Arc::new(storage);
        let transport: WsTransport<SQLiteStorage> = WsTransport::new(Some(storage), ws_config);
        transport.serve("127.0.0.1:3000").await.unwrap();
    });

    tokio::time::sleep(Duration::from_millis(300)).await;

    // Initial client config
    let config = ClientConfig {
        url: "ws://127.0.0.1:3000/ws".to_string(),
        reconnect_interval: Duration::from_secs(2),
        max_retries: 3,
    };

    // Publish a message, then drop the publisher to simulate disconnect
    {
        let mut publisher = EventBusClient::connect(config.clone()).await?;
        publisher.publish("chat.persist", "Persistent message",None, Some(3600)).await?;
        tokio::time::sleep(Duration::from_millis(100)).await;
        drop(publisher);
    }

    // Simulate subscriber starting after message is published
    tokio::time::sleep(Duration::from_millis(500)).await;

    let mut subscriber = EventBusClient::connect(config).await?;
    subscriber.subscribe("chat.*", None).await?;

    let subscriber_handle: JoinHandle<Result<(), EventBusError>> = tokio::spawn(async move {
        #[cfg(feature = "tracing")]
        tracing::debug!("Late subscriber started");

        let mut messages = subscriber.messages();
        match tokio::time::timeout(Duration::from_secs(3), messages.next()).await {
            Ok(Ok(msg)) => {
                let payload = String::from_utf8_lossy(&msg.payload);
                println!("Late subscriber received persisted: [{}] {}", msg.topic, payload);
                subscriber.acknowledge(msg.seq, msg.message_id).await?;
                Ok(())
            }
            Ok(Err(e)) => Err(e),
            Err(_) => {
                println!("Timed out waiting for persisted message");
                Err(EventBusError::Timeout)
            }
        }
    });

    let _ = subscriber_handle.await;
    server_handle.abort();

    println!("Persistence test completed!");
    Ok(())
}
