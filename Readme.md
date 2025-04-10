

# prk_eventbus: Event Bus Service

![Rust](https://img.shields.io/badge/Rust-1.65+-orange.svg)
![License](https://img.shields.io/badge/license-MIT-blue.svg)

The **Event Bus Service** is a versatile, WebSocket-based event bus built in Rust using the `prk_eventbus` library. It enables asynchronous, decoupled communication for distributed systems or within a single application, with SQLite-backed persistent storage. Whether embedded in an Axum server, run as a standalone service, or used for simple pub-sub scenarios, it supports robust publish-subscribe patterns with features like event batching, TTL, and acknowledgment.

## Features
- **WebSocket Interface**: Real-time event handling via a WebSocket endpoint.
- **Persistent Storage**: SQLite storage for event durability and optional replay.
- **Flexible Deployment**: Run standalone, embed in Axum, or use in microservices.
- **Scalable Design**: Handles multiple clients with configurable channel capacity.
- **Event Metadata**: Supports topics, payloads, TTL, message IDs, and more.
- **Batching**: Publish multiple events efficiently in a single operation.

## Prerequisites
- **Rust**: Version 1.65 or higher (with `cargo`).
- **SQLite**: Embedded via `sqlx`; no separate installation needed.

## Installation
1. **Clone the Repository**:
   ```bash
   git clone https://github.com/prk-Jr/prk_eventbus.git
   cd prk_eventbus
   ```

2. **Install Dependencies**:
   ```bash
   cargo build
   ```

3. **Run the Service**:
   - **Standalone**: `cargo run` (see [Standalone Usage](#standalone-usage)).
   - **Axum-Embedded**: Configure as per [Axum Integration](#axum-integration).

## Configuration
- **Port**: Set via `serve` (standalone) or `SocketAddr` (Axum) (e.g., `"127.0.0.1:3000"`).
- **Database**: Adjust SQLite path in `SQLiteStorage::new` (e.g., `"eventbus.db"`).
- **WebSocket Path**: Defaults to `/ws`; customize via `axum_router` nesting.
- **Client Settings**: Tune `ClientConfig` (e.g., `reconnect_interval`, `max_retries`).

## Usage

### Standalone Usage (Server + Pub/Sub)
Run the event bus as a standalone server with a publisher and subscriber in one process, ideal for testing or simple applications.

#### Example: Chat Simulation
- **Server**: Hosts the event bus at `ws://127.0.0.1:3000/ws`.
- **Publisher**: Sends chat messages on `chat.user1`.
- **Subscriber**: Listens to `chat.*`, acknowledges messages, and times out after 3 seconds.

Key Steps:
- Spawn a `WsTransport` server task.
- Connect a publisher and subscriber via `EventBusClient`.
- Publish single messages (`"Hello from User1!"`) and batches with TTL (3600s).
- Subscriber processes messages until timeout, then exits.

Output:
```
Subscriber received: [chat.user1] Hello from User1!
Subscriber received: [chat.user1] How's it going?
Subscriber received: [chat.user1] Batch message 1
Subscriber received: [chat.user1] Batch message 2
Test completed!
```

Run with tracing: `RUST_LOG=debug cargo run`.

## Axum Integration (Producer)
Embed the event bus in an Axum server to host it alongside HTTP routes, publishing events internally.

#### Example: User Management Service
- **Setup**: Runs at `http://127.0.0.1:3000` with event bus at `ws://127.0.0.1:3000/ws`.
- **Routes**: `POST /api/users` creates users and publishes `user.created`.
- **Client**: Internal `EventBusClient` connects lazily to publish events.

Key Function:
```rust
pub fn axum_router<T: Clone + Sync + Send + 'static>(&self, state: T) -> Router<T> {
    let storage = self.storage.clone();
    let bus = self.bus.clone();
    Router::new().route("/ws", get({
        let storage = storage.clone();
        move |ws| Self::handle_ws(ws, bus.clone(), storage.clone())
    })).with_state(state)
}
```

Usage:
- Merge `WsTransport::axum_router` into the Axum router.
- Publish events from endpoints:
  - `curl -X POST http://127.0.0.1:3000/api/users -d '{"id": 1, "username": "alice"}'` → `user.created`.

## Microservice Integration (Consumer)
Connect a separate microservice to the event bus to subscribe and process events, enabling cross-service synchronization.

#### Example: User Sync Service
- **Setup**: Runs at `http://127.0.0.1:5000`, connects to `ws://127.0.0.1:3000/ws`.
- **Subscription**: Listens to `user.*` in a background task.
- **Processing**: On `user.created`, deserializes the payload and saves the user locally.

Workflow:
- Axum server publishes `user.created`.
- Consumer receives it, logs, and syncs the user to its database.

Output:
```
Received message: CoreMessage { topic: "user.created", payload: "{\"id\":1,\"username\":\"alice\"}"... }
User created: User { id: 1, username: "alice" }
```

## Event Format
Events are JSON-serialized:
- `topic`: String (e.g., `"user.created"`, `"chat.user1"`).
- `payload`: Bytes/String (e.g., `{"id": 1, "username": "alice"}`, `"Hello from User1!"`).
- `message_id`: Optional string.
- `ttl`: Optional integer (seconds).
- `seq`: Auto-incremented sequence for acknowledgment.

SQLite (`eventbus.db`):
```sql
SELECT * FROM messages;
-- seq | topic         | payload               | metadata | ttl  | status
-- 1   | user.created  | {"id": 1, "username": "alice"} | {}       | 0    | pending
-- 2   | chat.user1    | Hello from User1!     | {}       | 3600 | processed
```

## Persistence
- Stored in `eventbus.db` with `messages` and `processed_messages` tables.
- Use `acknowledge` to mark events as processed (consumer example).
- Replay events by subscribing with a starting `seq` (if supported).

## Running with Microservices
1. **Standalone Chat**:
   - `cargo run` → Runs server, publisher, and subscriber in one.
2. **Producer (Axum)**:
   - `cargo run` → Hosts at `ws://127.0.0.1:3000/ws`.
   - Test: `curl -X POST http://127.0.0.1:3000/api/users -d '{"id": 1, "username": "alice"}'`.
3. **Consumer (Microservice)**:
   - `cargo run` → Connects to `ws://127.0.0.1:3000/ws`, syncs users.

## Troubleshooting
- **Connection Issues**: Verify WebSocket URL and server status.
- **Event Loss**: Check subscription timing (add delays if needed) or persistence settings.
- **Timeouts**: Adjust `tokio::time::timeout` durations in subscribers.
- **Tracing**: Enable with `RUST_LOG=debug cargo run` for detailed logs.

## Contributing
Fork, branch, commit, and submit a pull request:
1. `git checkout -b feature/your-feature`
2. `git commit -m "Add your feature"`
3. `git push origin feature/your-feature`

## License
MIT License. See [LICENSE](LICENSE).

## Acknowledgments
- SQLite via [sqlx](https://crates.io/crates/sqlx).
- HTTP/WebSocket via [axum](https://crates.io/crates/axum).

---
