
use crate::application::services;
use crate::domain::error::AppError;
use crate::domain::*;
use crate::infrastructure::*;
use anyhow::Context;
use axum::{
    extract::*,
    http::StatusCode,
    routing::{get, post},
    Router,
};
use prk_eventbus::adapters::WsConfig;
use prk_eventbus::adapters::WsTransport;
use prk_eventbus::client::ClientConfig;
use prk_eventbus::client::EventBusClient;
use prk_eventbus::storage::sqlite::SQLiteStorage;
use serde::*;
use tokio::sync::Mutex;
use std::time::Duration;
use std::{net::SocketAddr, sync::Arc};
use tokio::net;
use tower_http::{cors::CorsLayer, trace::TraceLayer};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpServerConfig<'a> {
    pub port: &'a str,
}

#[derive(Clone)]
pub struct AppState {
    pub user_service: Arc<services::UserService<SqlxUserRepository>>,
    pub todo_service: Arc<services::TodoService<SqlxTodoRepository>>,
    pub event_bus_client: Arc<tokio::sync::Mutex<Option<EventBusClient>>>,
}

pub async fn get_users(State(state): State<AppState>) -> Result<Json<Vec<User>>, AppError> {
    Ok(Json(state.user_service.get_all().await?))

}

pub async fn post_users(
    State(state): State<AppState>,
    Json(body): Json<User>,
) -> Result<Json<User>, AppError> {
    init_bus_client(state.clone()).await?;
    let mut bus = state.event_bus_client.lock().await;
    if bus.is_none() {
        return Err(AppError::EventBusClientError(prk_eventbus::core::EventBusError::Connection("Connection failed".to_string())));
    }
    let bus = bus.as_mut().unwrap();
    tracing::info!("Publishing user.created event");
    println!("Publishing user.created event");
    bus.publish("user.created", &serde_json::to_string(&body).unwrap(), None, None).await?;
    Ok(Json(state.user_service.create(body).await?))

}

    pub async fn get_todos(State(state): State<AppState>) -> Result<Json<Vec<Todo>>, AppError> {
    let items = state.todo_service.get_all().await?;
    Ok(Json(items))
}

pub async fn post_todos(
    State(state): State<AppState>,
    Json(body): Json<Todo>,
) -> Result<Json<Todo>, AppError> {
    let item = state.todo_service.create(body).await?;
    Ok(Json(item))
}

#[derive(Deserialize)]
pub struct TodoPathParams {
    pub id: i32,
}

pub async fn get_todos_by_id(
    State(state): State<AppState>,
    Path(params): Path<TodoPathParams>,
) -> Result<Json<Todo>, AppError> {
    let item = state
        .todo_service
        .get_by_id(params.id)
        .await?
        .ok_or(AppError::NotFound(format!("Not found")))?;
    Ok(Json(item))
}

pub struct HttpServer {
    router: Router,
    listener: net::TcpListener,
}

impl HttpServer {
    pub async fn new(
        user_service: services::UserService<SqlxUserRepository>,
        todo_service: services::TodoService<SqlxTodoRepository>,
        config: HttpServerConfig<'_>,
    ) -> anyhow::Result<Self> {
        let trace_layer =
            TraceLayer::new_for_http().make_span_with(|request: &axum::extract::Request<_>| {
                let uri = request.uri().to_string();
                tracing::info_span!("http_request", method = ?request.method(), uri)
            });

       
        let state = AppState {
            user_service: Arc::new(user_service),
            todo_service: Arc::new(todo_service),
            event_bus_client: Arc::new(Mutex::new(None)),
        };

        let storage = Arc::new(SQLiteStorage::new("eventbus.db", 100).await.unwrap());
        let storage = Some(storage);
        let auto_ack = true;
        let ws_transport = WsTransport::new(storage, WsConfig { channel_capacity: 100, auto_ack });
        let ws_transport = ws_transport.axum_router::<AppState>( state.clone(),);


        let router = Router::new()
            .layer(trace_layer)
            .merge( ws_transport)
            .route("/health", get(health_route))
            .nest("/api", api_routes())
            .layer(CorsLayer::permissive())
            .with_state(state);

        let addr = SocketAddr::from((
            [0, 0, 0, 0, 0, 0, 0, 0],
            config.port.parse::<u16>().unwrap_or(3000),
        ));

        let listener = net::TcpListener::bind(&addr)
            .await
            .with_context(|| format!("failed to listen on port {}", config.port))?;

        Ok(Self { router, listener })
    }

    pub async fn run(self) -> anyhow::Result<()> {
        tracing::debug!("listening on {}", self.listener.local_addr().unwrap());
        axum::serve(self.listener, self.router)
            .await
            .context("received error from running server")?;
        Ok(())
    }
}

fn api_routes() -> Router<AppState> {
    Router::new()
        .route("/users", get(get_users))
        .route("/users", post(post_users))
        .route("/todos", get(get_todos))
        .route("/todos", post(post_todos))
        .route("/todos/{id}", get(get_todos_by_id))
}

async fn health_route() -> (StatusCode, &'static str) {
    (StatusCode::OK, "OK")
}


async fn init_bus_client(state: AppState) -> Result<(), AppError> {
    // Check already initialzed
    if state.event_bus_client.lock().await.is_some() {
        tracing::info!("Event bus client already initialized");
        return Ok(());
    }
    let client_config = ClientConfig {
        url: "ws://localhost:3000/ws".to_string(),
        reconnect_interval: Duration::from_secs(5),
        max_retries: 5,
    };

    let connection = EventBusClient::connect_with_retry(client_config).await?;
    
    state.event_bus_client.lock().await.replace(connection);
    tracing::info!("Event bus client initialized");
    Ok(())
}