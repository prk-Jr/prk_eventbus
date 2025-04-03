use std::time::Duration;
use rust_eventbus::{adapters::{WsConfig, WsTransport}, client::{ClientConfig, EventBusClient}, core::error::EventBusError, storage::{sqlite::SQLiteStorage, Storage}, transport};

#[tokio::main]
async fn main() -> Result<(), EventBusError> {
    // Start the server
    let server_handle = tokio::spawn(async {
        let ws_config = WsConfig {
            channel_capacity: 100,
            auto_ack: true,
        };
        let transport: WsTransport<SQLiteStorage> = WsTransport::new(None, ws_config);
        transport.serve("127.0.0.1:3000").await.unwrap();
    });

    // Wait briefly for the server to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Client 1: Publisher
    let publisher_config = ClientConfig {
        url: "ws://127.0.0.1:3000/ws".to_string(),
        reconnect_interval: Duration::from_secs(2),
        max_retries: 5,
    };
    let mut publisher = EventBusClient::connect(publisher_config.clone()).await?;
    
    // Client 2: Subscriber
    let mut subscriber = EventBusClient::connect(publisher_config).await?;

    // Subscribe to all messages on "chat.*"
    subscriber.subscribe("chat.*", None).await?;

    // Spawn a task to handle subscriber messages
    let subscriber_handle = tokio::spawn(async move {
        loop {
            let mut messages = subscriber.messages();
            if let msg = messages.next().await? {
                let payload = String::from_utf8_lossy(&msg.payload);
                println!("Subscriber received: [{}] {}", msg.topic, payload);
                subscriber.acknowledge(msg.seq, msg.message_id).await?;
            } else {
                break;
            }
        }
        Ok::<(), EventBusError>(())
    });

    // Publish some messages
    publisher.publish("chat.user1", "Hello from User1!", Some(3600)).await?;
    tokio::time::sleep(Duration::from_millis(100)).await;

    publisher.publish("chat.user1", "How's it going?", Some(3600)).await?;
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Publish a batch of messages
    let batch = vec![
        ("chat.user1".to_string(), "Batch message 1".to_string()),
        ("chat.user1".to_string(), "Batch message 2".to_string()),
    ];
    publisher.publish_batch(batch, Some(3600)).await?;

    // Wait for messages to be processed
    tokio::time::sleep(Duration::from_secs(1)).await;


    // Clean up
    server_handle.abort();
    subscriber_handle.abort();

    println!("Test completed!");
    Ok(())
}