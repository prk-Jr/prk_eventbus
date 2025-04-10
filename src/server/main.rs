use prk_eventbus::adapters::websocket::WsTransport;
use prk_eventbus::adapters::WsConfig;
use prk_eventbus::storage::sqlite::SQLiteStorage;
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let storage = Arc::new(SQLiteStorage::new("eventbus.db", 100).await?);
    let storage = Some( storage);
    let auto_ack = true;
    let ws_transport = WsTransport::new(storage, WsConfig { channel_capacity: 100, auto_ack });
    ws_transport.serve("0.0.0.0:8080").await?;
    Ok(())
}