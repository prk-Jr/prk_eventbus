use std::time::Duration;

use prk_eventbus::client::{ClientConfig, EventBusClient};

mod adapters;
mod application;
mod database_connection;
mod domain;
mod infrastructure;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
            .with_env_filter(std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string()))
            .init();
    let database_connection = database_connection::connect_to_database()
        .await
        .expect("Could not connect to database");
    let config = adapters::http::http::HttpServerConfig {
        port: "5000".into(),
    };
    let user_repo =
        infrastructure::repositories::SqlxUserRepository::new(database_connection.clone());
    let todo_repo =
        infrastructure::repositories::SqlxTodoRepository::new(database_connection.clone());
    let user_service = application::services::UserService::new(user_repo.clone());
    let user_service_clone = application::services::UserService::new(user_repo);
    let todo_service = application::services::TodoService::new(todo_repo);

    tokio::spawn(async move {

        let client_config = ClientConfig {
            url: "ws://localhost:3000/ws".to_string(),
            reconnect_interval: Duration::from_secs(5),
            max_retries: 5,
        };
        
        let mut bus = EventBusClient::connect_with_retry(client_config).await.expect("Failed to connect to event bus");

        bus.subscribe("user.*", None).await.expect("Failed to subscribe to event bus");
        while let Ok(msg) = bus.next_message().await {
            tracing::info!("Received message: {:?}", msg);
            println!("Received message: {:?}", msg);
            if msg.topic == "user.created" {
                let body = String::from_utf8(msg.payload.to_vec()).unwrap();
                let user = serde_json::from_str::<domain::user::User>(&body);
            match user {
                Ok(user) => {
                    tracing::info!("User created: {:?}", user);
                    println!("User created: {:?}", user);
                    user_service_clone.create(user).await.expect("Failed to create user");
                }
                Err(e) => {
                    tracing::error!("Failed to deserialize user: {:?}", e);
                    println!("Failed to deserialize user: {:?}", e);
                }
            } 
            }
            

        }
    });
    


    let http_server = adapters::http::http::HttpServer::new(user_service, todo_service, config)
        .await
        .expect("Failed to create HTTP server");
    http_server.run().await.expect("Failed to run HTTP server");
}
