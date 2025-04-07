use std::time::Duration;
use rust_eventbus::{adapters::{WsConfig, WsTransport}, client::{ClientConfig, EventBusClient}, core::error::EventBusError, storage::sqlite::SQLiteStorage};
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
        transport.serve("127.0.0.1:3000").await.unwrap();
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    let config = ClientConfig {
        url: "ws://127.0.0.1:3000/ws".to_string(),
        reconnect_interval: Duration::from_secs(2),
        max_retries: 5,
    };

    // Publisher
    let mut publisher = EventBusClient::connect(config.clone()).await?;

    // Subscriber 1: Broad subscription ("chat.*")
    let mut sub1 = EventBusClient::connect(config.clone()).await?;
    sub1.subscribe("chat.*", None).await?;
    let sub1_handle: JoinHandle<Result<(), EventBusError>> = tokio::spawn(async move {
        #[cfg(feature = "tracing")]
        tracing::debug!("Subscriber 1 task started (chat.*)");
        let mut received = 0;
        loop {
            let mut messages = sub1.messages();
            match tokio::time::timeout(Duration::from_secs(3), messages.next()).await {
                Ok(Ok(msg)) => {
                    let payload = String::from_utf8_lossy(&msg.payload);
                    println!("Sub1 received: [{}] {}", msg.topic, payload);
                    sub1.acknowledge(msg.seq, msg.message_id).await?;
                    received += 1;
                    if received == 3 { break; }
                }
                Ok(Err(e)) => return Err(e),
                Err(_) => break,
            }
        }
        Ok(())
    });

    // Subscriber 2: Specific subscription ("chat.user2")
    let mut sub2 = EventBusClient::connect(config.clone()).await?;
    sub2.subscribe("chat.user2", None).await?;
    let sub2_handle: JoinHandle<Result<(), EventBusError>> = tokio::spawn(async move {
        #[cfg(feature = "tracing")]
        tracing::debug!("Subscriber 2 task started (chat.user2)");
        let mut received = 0;
        loop {
            let mut messages = sub2.messages();
            match tokio::time::timeout(Duration::from_secs(3), messages.next()).await {
                Ok(Ok(msg)) => {
                    let payload = String::from_utf8_lossy(&msg.payload);
                    println!("Sub2 received: [{}] {}", msg.topic, payload);
                    sub2.acknowledge(msg.seq, msg.message_id).await?;
                    received += 1;
                    if received == 1 { break; }
                }
                Ok(Err(e)) => return Err(e),
                Err(_) => break,
            }
        }
        Ok(())
    });

    tokio::time::sleep(Duration::from_millis(100)).await; // Ensure subscribers are ready

    // Publish messages
    publisher.publish("chat.user1", "Message for user1",None, Some(3600)).await?;
    publisher.publish("chat.user2", "Message for user2",None, Some(3600)).await?;
    publisher.publish("chat.user1", "Another for user1",None, Some(3600)).await?;

    tokio::time::sleep(Duration::from_secs(2)).await;

    drop(publisher);
    let _ = sub1_handle.await;
    let _=  sub2_handle.await;
    server_handle.abort();

    println!("Multiple subscribers test completed!");
    Ok(())
}