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

    let server_handle: JoinHandle<()> = tokio::spawn(async {
        let ws_config = WsConfig {
            channel_capacity: 100,
            auto_ack: true,
        };
        let transport: WsTransport<SQLiteStorage> = WsTransport::new(None, ws_config);
        #[cfg(feature = "tracing")]
        tracing::debug!("Server task started, about to serve");
        transport.serve("127.0.0.1:3000").await.unwrap();
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    let publisher_config = ClientConfig {
        url: "ws://127.0.0.1:3000/ws".to_string(),
        reconnect_interval: Duration::from_secs(2),
        max_retries: 5,
    };
    let mut publisher = EventBusClient::connect(publisher_config.clone()).await?;
    let mut subscriber = EventBusClient::connect(publisher_config).await?;

    subscriber.subscribe("chat.*", None).await?;
    tokio::time::sleep(Duration::from_millis(100)).await; // Add delay to ensure subscriber task starts

    let subscriber_handle: JoinHandle<Result<(), EventBusError>> = tokio::spawn(async move {
        #[cfg(feature = "tracing")]
        tracing::debug!("Subscriber task started");
        loop {
            let mut messages = subscriber.messages();
            match tokio::time::timeout(Duration::from_secs(3), messages.next()).await {
                Ok(Ok(msg)) => {
                    let payload = String::from_utf8_lossy(&msg.payload);
                    #[cfg(feature = "tracing")]
                    tracing::info!(topic = %msg.topic, payload = %payload, "Subscriber received message");
                    println!("Subscriber received: [{}] {}", msg.topic, payload);
                    subscriber.acknowledge(msg.seq, msg.message_id).await?;
                }
                Ok(Err(e)) => {
                    #[cfg(feature = "tracing")]
                    tracing::error!(error = %e, "Subscriber encountered an error");
                    return Err(e);
                }
                Err(_) => {
                    #[cfg(feature = "tracing")]
                    tracing::warn!("Subscriber timed out waiting for messages after 3 seconds");
                    break;
                }
            }
        }
        #[cfg(feature = "tracing")]
        tracing::debug!("Subscriber task exiting");
        Ok(())
    });

    publisher.publish("chat.user1", "Hello from User1!",None, Some(3600)).await?;
    tokio::time::sleep(Duration::from_millis(100)).await;

    publisher.publish("chat.user1", "How's it going?",None, Some(3600)).await?;
    tokio::time::sleep(Duration::from_millis(100)).await;

    let batch = vec![
        ("chat.user1".to_string(), "Batch message 1".to_string()),
        ("chat.user1".to_string(), "Batch message 2".to_string()),
    ];
    publisher.publish_batch(batch, Some(3600)).await?;

    tokio::time::sleep(Duration::from_secs(2)).await;

    #[cfg(feature = "tracing")]
    tracing::debug!("Dropping publisher to close its connection");
    drop(publisher);

    #[cfg(feature = "tracing")]
    tracing::debug!("Waiting for subscriber task to complete");
    let _ = subscriber_handle.await;
    // subscriber_result?;

    #[cfg(feature = "tracing")]
    tracing::debug!("Aborting server task");
    server_handle.abort();

    println!("Test completed!");
    Ok(())
}