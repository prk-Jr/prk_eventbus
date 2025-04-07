use std::time::Duration;
use rust_eventbus::{adapters::{WsConfig, WsTransport}, client::{ClientConfig, EventBusClient}, core::error::EventBusError, storage::sqlite::SQLiteStorage};
use tokio::task::JoinHandle;

#[tokio::main]
async fn main() -> Result<(), EventBusError> {
    #[cfg(feature = "tracing")]
    {
        tracing_subscriber::fmt()
            .with_env_filter(std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string()))
            .init();
    }

    let server_handle: JoinHandle<()> = tokio::spawn(async {
        let ws_config = WsConfig {
            channel_capacity: 1000, // Increased capacity
            auto_ack: true,
        };
        let transport: WsTransport<SQLiteStorage> = WsTransport::new(None, ws_config);
        transport.serve("127.0.0.1:3000").await.unwrap();
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    let config = ClientConfig {
        url: "ws://127.0.0.1:3000/ws".to_string(),
        reconnect_interval: Duration::from_secs(2),
        max_retries: 5,
    };

    let mut publisher = EventBusClient::connect(config.clone()).await?;
    let mut subscriber = EventBusClient::connect(config).await?;
    subscriber.subscribe("chat.*", None).await?;
    tokio::time::sleep(Duration::from_millis(100)).await;

    let sub_handle: JoinHandle<Result<(), EventBusError>> = tokio::spawn(async move {
        #[cfg(feature = "tracing")]
        tracing::info!("Subscriber task started");
        let mut received = 0;
        loop {
            let mut messages = subscriber.messages();
            match tokio::time::timeout(Duration::from_secs(5), messages.next()).await {
                Ok(Ok(msg)) => {
                    let payload = String::from_utf8_lossy(&msg.payload);
                    #[cfg(feature = "tracing")]
                    if received % 100 == 0 {
                        tracing::info!(received = received, "Subscriber received message");
                    }
                    subscriber.acknowledge(msg.seq, msg.message_id).await?;
                    received += 1;
                    if received == 1000 { break; }
                }
                Ok(Err(e)) => return Err(e),
                Err(_) => {
                    #[cfg(feature = "tracing")]
                    tracing::warn!(received = received, "Subscriber timed out");
                    break;
                }
            }
        }
        println!("Subscriber received {} messages", received);
        Ok(())
    });

    // Publish 1000 messages
    for i in 0..1000 {
        publisher.publish("chat.stress", &format!("Stress message {}", i),None, Some(3600)).await?;
    }

    tokio::time::sleep(Duration::from_secs(5)).await;

    drop(publisher);
    let _ = sub_handle.await;
    server_handle.abort();

    println!("Stress test completed!");
    Ok(())
}