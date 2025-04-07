use std::sync::Arc;
use tokio::sync::mpsc;
use uuid::Uuid;
use crate::adapters::{WsConfig, WsTransport};
use crate::client::{ClientConfig, EventBusClient};
use crate::storage::sqlite::SQLiteStorage;

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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_message_persistence() -> Result<(), Box<dyn std::error::Error>> {
    let db_path = "test_message_persistence.db";
    let storage = SQLiteStorage::new(db_path, 100).await?;
    let storage = Arc::new(storage);
    let ws_config = WsConfig { channel_capacity: 100, auto_ack: false };
    let transport: WsTransport<SQLiteStorage> = WsTransport::new(Some(storage.clone()), ws_config);
    let server_handle = tokio::spawn(async move {
        transport.serve("127.0.0.1:3000").await.unwrap();
    });

    let config = ClientConfig {
        url: "ws://127.0.0.1:3000/ws".to_string(),
        reconnect_interval: std::time::Duration::from_millis(200),
        max_retries: 5,
    };

    let (tx, mut rx) = mpsc::channel(1);
    let mut subscriber = connect_with_retry(config.clone()).await?;
    subscriber.subscribe("chat.*", Some(0)).await?;
    tokio::spawn({
        let tx = tx.clone();
        async move {
            if let Ok(msg) = subscriber.next_message().await {
                tx.send(msg).await.unwrap();
            }
        }
    });

    let mut publisher = connect_with_retry(config.clone()).await?;
    publisher.publish("chat.persist", "Persistent message",None, Some(0)).await?;

    let received_msg = rx.recv().await.expect("Failed to receive persisted message");
    assert_eq!(received_msg.topic, "chat.persist");
    assert_eq!(String::from_utf8_lossy(&received_msg.payload), "Persistent message");

    server_handle.abort();
    let _ = std::fs::remove_file(db_path);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_reconnection() -> Result<(), Box<dyn std::error::Error>> {
    let db_path = "test_reconnection.db";
    let storage = SQLiteStorage::new(db_path, 100).await?;
    let transport = Arc::new(WsTransport::new(Some(Arc::new(storage)), WsConfig::default()));
    let transport_clone = transport.clone();
    let server_handle = tokio::spawn(async move {
        transport_clone.serve("127.0.0.1:3001").await.unwrap(); // Unique port
    });

    let config = ClientConfig {
        url: "ws://127.0.0.1:3001/ws".to_string(),
        reconnect_interval: std::time::Duration::from_millis(200),
        max_retries: 5,
    };

    let mut subscriber = connect_with_retry(config.clone()).await?;
    subscriber.subscribe("chat.reconnect", None).await?;

    let mut publisher = connect_with_retry(config.clone()).await?;
    publisher.publish("chat.reconnect", "Before disconnect",None, None).await?;
    let msg1 = subscriber.next_message().await?;
    assert_eq!(msg1.topic, "chat.reconnect");
    assert_eq!(String::from_utf8_lossy(&msg1.payload), "Before disconnect");

    subscriber.disconnect().await?;
    subscriber = connect_with_retry(config.clone()).await?;
    subscriber.subscribe("chat.reconnect", None).await?;

    publisher.publish("chat.reconnect", "After reconnect",None, None).await?;
    let msg2 = subscriber.next_message().await?;
    assert_eq!(msg2.topic, "chat.reconnect");
    assert_eq!(String::from_utf8_lossy(&msg2.payload), "After reconnect");

    server_handle.abort();
    let _ = std::fs::remove_file(db_path);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_ttl_expiration() -> Result<(), Box<dyn std::error::Error>> {
    let db_path = "test_ttl_expiration.db";
    let storage = SQLiteStorage::new(db_path, 100).await?;
    let transport = Arc::new(WsTransport::new(Some(Arc::new(storage)), WsConfig::default()));
    let transport_clone = transport.clone();
    let server_handle = tokio::spawn(async move {
        transport_clone.serve("127.0.0.1:3002").await.unwrap(); // Unique port
    });

    let config = ClientConfig {
        url: "ws://127.0.0.1:3002/ws".to_string(),
        reconnect_interval: std::time::Duration::from_millis(200),
        max_retries: 5,
    };

    let mut publisher = connect_with_retry(config.clone()).await?;
    publisher.publish("chat.ttl", "Short-lived message",None,  Some(1)).await?;

    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    let mut subscriber = connect_with_retry(config).await?;
    subscriber.subscribe("chat.ttl", Some(0)).await?;

    let received = tokio::time::timeout(
        std::time::Duration::from_millis(500),
        subscriber.next_message(),
    ).await;

    assert!(received.is_err(), "No message should be received after TTL expiration");

    server_handle.abort();
    let _ = std::fs::remove_file(db_path);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_duplicate_message_handling() -> Result<(), Box<dyn std::error::Error>> {
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_multi_subscriber() -> Result<(), Box<dyn std::error::Error>> {
    let storage = SQLiteStorage::new_memory(100).await?;
    let transport = Arc::new(WsTransport::new(Some(Arc::new(storage)), WsConfig::default()));
    let transport_clone = transport.clone();
    let server_handle = tokio::spawn(async move {
        transport_clone.serve("127.0.0.1:3004").await.unwrap(); // Unique port
    });

    let config = ClientConfig {
        url: "ws://127.0.0.1:3004/ws".to_string(),
        reconnect_interval: std::time::Duration::from_millis(200),
        max_retries: 5,
    };

    let mut subscriber1 = connect_with_retry(config.clone()).await?;
    let mut subscriber2 = connect_with_retry(config.clone()).await?;
    subscriber1.subscribe("chat.multi", None).await?;
    subscriber2.subscribe("chat.multi", None).await?;

    let mut publisher = connect_with_retry(config).await?;
    publisher.publish("chat.multi", "Broadcast message",None, None).await?;

    let msg1 = subscriber1.next_message().await?;
    let msg2 = subscriber2.next_message().await?;

    assert_eq!(msg1.topic, "chat.multi");
    assert_eq!(String::from_utf8_lossy(&msg1.payload), "Broadcast message");
    assert_eq!(msg2.topic, "chat.multi");
    assert_eq!(String::from_utf8_lossy(&msg2.payload), "Broadcast message");

    server_handle.abort();
    Ok(())
}