use rust_eventbus::adapters::websocket::WsTransport;
use rust_eventbus::storage::sqlite::SQLiteStorage;
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let storage = Arc::new(SQLiteStorage::new("eventbus.db")?);
    let storage = Some( storage);
    let ws_transport = WsTransport::new(storage);
    ws_transport.serve("0.0.0.0:8080").await?;
    Ok(())
}